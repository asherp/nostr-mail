use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use lettre::message::{MultiPart, SinglePart, Attachment};
use anyhow::Result;
use mailparse::{MailHeaderMap, parse_mail};
use lettre::message::header::{Header, HeaderName, HeaderValue, ContentType};
use std::error::Error;
use std::collections::HashSet;
use crate::crypto;
use crate::database::{Database, Email as DbEmail};
use crate::types::{EmailConfig, EmailAttachment};
use tokio::task;
use tokio::time::timeout;
use std::time::Duration;
use crate::types::{EmailMessage, TransportAuthVerdict, TransportAuthMethod};
use std::net::TcpStream;
#[cfg(not(target_os = "android"))]
use native_tls::TlsConnector;
#[cfg(target_os = "android")]
use rustls::{ClientConfig, ClientConnection, RootCertStore, pki_types::ServerName};
#[cfg(target_os = "android")]
use std::sync::Arc;
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose};

/// Macro to create a TLS-wrapped IMAP client connection
/// Uses native-tls on desktop and rustls on Android
#[cfg(not(target_os = "android"))]
macro_rules! create_imap_tls_client {
    ($host:expr, $addr:expr) => {{
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect($addr)?;
        let tls_stream: native_tls::TlsStream<TcpStream> = tls.connect($host, tcp_stream)?;
        Ok::<imap::Client<native_tls::TlsStream<TcpStream>>, anyhow::Error>(imap::Client::new(tls_stream))
    }};
}

#[cfg(target_os = "android")]
macro_rules! create_imap_tls_client {
    ($host:expr, $addr:expr) => {{
        // Initialize rustls crypto provider if not already initialized (required for rustls 0.23+)
        {
            use std::sync::Once;
            static INIT: Once = Once::new();
            INIT.call_once(|| {
                // Try to install default provider, but don't fail if already installed
                let _ = rustls::crypto::ring::default_provider().install_default();
            });
        }
        
        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        
        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        
        let server_name = ServerName::try_from($host.to_string())
            .map_err(|_| anyhow::anyhow!("Invalid server name"))?;
        
        let tcp_stream = TcpStream::connect($addr)?;
        let client = ClientConnection::new(Arc::new(config), server_name)?;
        let tls_stream = rustls::StreamOwned::new(client, tcp_stream);
        Ok::<imap::Client<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>, anyhow::Error>(imap::Client::new(tls_stream))
    }};
}

/// Decode RFC 2047 encoded header value and fix UTF-8 encoding issues
/// mailparse should handle RFC 2047 automatically, but this fixes common UTF-8 misinterpretations
fn decode_header_value(value: &str) -> String {
    // Fix common UTF-8 encoding issues where UTF-8 bytes are interpreted as Latin-1
    // These patterns occur when UTF-8 sequences are read as single-byte characters
    // The pattern "â€™" represents UTF-8 bytes E2 80 99 (right single quotation mark U+2019)
    // being misinterpreted as three Latin-1 characters
    
    let apostrophe = "'";
    let result = value
        // Fix right single quotation mark (most common apostrophe issue)
        .replace("\u{00E2}\u{0080}\u{0099}", apostrophe)  // â€™ -> '
        .replace("\u{00E2}\u{0080}\u{009C}", "\"")        // â€œ -> "
        .replace("\u{00E2}\u{0080}\u{009D}", "\"")        // â€ -> "
        .replace("\u{00E2}\u{0080}\u{0094}", "—")         // â€" -> —
        .replace("\u{00E2}\u{0080}\u{0093}", "–")         // â€" -> –
        .replace('\u{FFFD}', apostrophe)                   // Replacement character -> apostrophe
        // Handle common contractions where â appears before t
        .replace("doesn\u{00E2}", "doesn't")
        .replace("won\u{00E2}", "won't")
        .replace("can\u{00E2}", "can't")
        .replace("isn\u{00E2}", "isn't")
        .replace("aren\u{00E2}", "aren't")
        .replace("wasn\u{00E2}", "wasn't")
        .replace("weren\u{00E2}", "weren't")
        .replace("haven\u{00E2}", "haven't")
        .replace("hasn\u{00E2}", "hasn't")
        .replace("hadn\u{00E2}", "hadn't")
        .replace("wouldn\u{00E2}", "wouldn't")
        .replace("couldn\u{00E2}", "couldn't")
        .replace("shouldn\u{00E2}", "shouldn't")
        .replace("mustn\u{00E2}", "mustn't")
        .replace("mightn\u{00E2}", "mightn't")
        .replace("needn\u{00E2}", "needn't")
        .replace("daren\u{00E2}", "daren't")
        .replace("mayn\u{00E2}", "mayn't")
        .replace("shan\u{00E2}", "shan't");
    
    // Also handle the pattern where â appears before t (common in contractions)
    result.replace("\u{00E2} t", "'t")
}

#[derive(Debug, Clone)]
struct XNostrPubkey(String);

impl Header for XNostrPubkey {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str("X-Nostr-Pubkey")
    }
    fn parse(s: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        Ok(XNostrPubkey(s.to_string()))
    }
    fn display(&self) -> HeaderValue {
        HeaderValue::new(Self::name(), self.0.clone())
    }
}

#[derive(Debug, Clone)]
struct XNostrSig(String);

impl Header for XNostrSig {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str("X-Nostr-Sig")
    }
    fn parse(s: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        Ok(XNostrSig(s.to_string()))
    }
    fn display(&self) -> HeaderValue {
        HeaderValue::new(Self::name(), self.0.clone())
    }
}

/// Construct email headers without sending the email
pub fn construct_email_headers(
    config: &EmailConfig,
    to_address: &str,
    subject: &str,
    body: &str,
    _nostr_npub: Option<&str>,
    message_id: Option<&str>,
    attachments: Option<&Vec<EmailAttachment>>,
) -> Result<String> {
    println!("[RUST] construct_email_headers: Constructing email headers");
    println!("[RUST] construct_email_headers: From: {}, To: {}", config.email_address, to_address);
    
    let mut builder = Message::builder()
        .from(config.email_address.parse()?)
        .to(to_address.parse()?)
        .subject(subject);
    
    // Add custom message ID if provided
    if let Some(msg_id) = message_id {
        println!("[RUST] construct_email_headers: Setting message ID: {}", msg_id);
        // Try using the builder's message_id method
        builder = builder.message_id(Some(msg_id.to_string()));
        
        // Also try manually adding the header as a fallback
        // Note: This might not work with lettre's builder pattern, but worth trying
        // builder = builder.header(("Message-ID", msg_id));
    } else {
        println!("[RUST] construct_email_headers: No message ID provided");
    }
    
    // Add the sender's public key to the headers (not the receiver's)
    // This allows the receiver to derive the shared secret using their private key
    if let Some(private_key) = &config.private_key {
        // Extract public key from private key
        match crypto::get_public_key_from_private(private_key) {
            Ok(sender_pubkey) => {
                println!("[RUST] construct_email_headers: Adding sender pubkey to headers: {}", sender_pubkey);
                builder = builder.header(XNostrPubkey(sender_pubkey));
                
                // Sign the email body and add signature to headers
                match crypto::sign_data(private_key, body) {
                    Ok(signature) => {
                        println!("[RUST] construct_email_headers: Signing email body, signature length: {}", signature.len());
                        builder = builder.header(XNostrSig(signature));
                    }
                    Err(e) => {
                        println!("[RUST] construct_email_headers: Failed to sign email body: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("[RUST] construct_email_headers: Failed to get public key from private key: {}", e);
            }
        }
    }
    
    // Build email with or without attachments (for header construction)
    let email = if let Some(attachments) = attachments {
        if attachments.is_empty() {
            // No attachments, simple text email
            builder.body(body.to_string())?
        } else {
            println!("[RUST] construct_email_headers: Building multipart email with {} attachments", attachments.len());
            
            // Create multipart email with text body and attachments
            let mut multipart = MultiPart::mixed()
                .singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(body.to_string()));
            
            // Add each attachment (for header construction, we don't need the actual data)
            for attachment in attachments {
                println!("[RUST] construct_email_headers: Adding attachment header: {}", attachment.filename);
                
                // Parse content type
                let content_type = attachment.content_type.parse::<ContentType>()
                    .unwrap_or(ContentType::parse("application/octet-stream").unwrap());
                
                // Create attachment part with empty data for header construction
                let attachment_part = Attachment::new(attachment.filename.clone())
                    .body(Vec::new(), content_type);
                
                multipart = multipart.singlepart(attachment_part);
            }
            
            builder.multipart(multipart)?
        }
    } else {
        // No attachments, simple text email
        builder.body(body.to_string())?
    };
    
    // Convert the email to a string to get the raw headers
    let email_bytes = email.formatted();
    let email_string = String::from_utf8(email_bytes)?;
    
    println!("[RUST] construct_email_headers: Full email string:");
    println!("{}", email_string);
    
    // Extract headers from the email string
    let lines: Vec<&str> = email_string.lines().collect();
    let mut headers = Vec::new();
    let in_body = false;
    
    for line in lines {
        if line.is_empty() {
            break;
        }
        if !in_body {
            headers.push(line);
        }
    }
    
    let final_headers = headers.join("\n");
    println!("[RUST] construct_email_headers: Final headers:");
    println!("{}", final_headers);
    
    // Check if Message-ID is present in the headers
    if final_headers.to_lowercase().contains("message-id:") {
        println!("[RUST] construct_email_headers: Message-ID found in headers");
    } else {
        println!("[RUST] construct_email_headers: Message-ID NOT found in headers");
        // If Message-ID is not present, manually add it
        if let Some(msg_id) = message_id {
            println!("[RUST] construct_email_headers: Manually adding Message-ID: {}", msg_id);
            let headers_with_message_id = format!("Message-ID: {}\n{}", msg_id, final_headers);
            println!("[RUST] construct_email_headers: Headers with manually added Message-ID:");
            println!("{}", headers_with_message_id);
            return Ok(headers_with_message_id);
        }
    }
    
    Ok(final_headers)
}

pub async fn send_email(
    config: &EmailConfig,
    to_address: &str,
    subject: &str,
    body: &str,
    _nostr_npub: Option<&str>,
    message_id: Option<&str>,
    attachments: Option<&Vec<EmailAttachment>>,
) -> Result<String> {
    println!("[RUST] send_email: Starting email send process");
    println!("[RUST] send_email: SMTP Host: {}, Port: {}", config.smtp_host, config.smtp_port);
    println!("[RUST] send_email: From: {}, To: {}", config.email_address, to_address);
    println!("[RUST] send_email: Use TLS: {}", config.use_tls);
    
    let mut builder = Message::builder()
        .from(config.email_address.parse()?)
        .to(to_address.parse()?)
        .subject(subject);
    
    // Add custom message ID if provided
    if let Some(msg_id) = message_id {
        // Pass the message ID as Option<String> to the builder
        builder = builder.message_id(Some(msg_id.to_string()));
    }
    
    // Add the sender's public key to the headers (not the receiver's)
    // This allows the receiver to derive the shared secret using their private key
    if let Some(private_key) = &config.private_key {
        // Extract public key from private key
        match crypto::get_public_key_from_private(private_key) {
            Ok(sender_pubkey) => {
                println!("[RUST] send_email: Adding sender pubkey to headers: {}", sender_pubkey);
                builder = builder.header(XNostrPubkey(sender_pubkey));
                
                // Sign the email body and add signature to headers
                match crypto::sign_data(private_key, body) {
                    Ok(signature) => {
                        println!("[RUST] send_email: Signing email body, signature length: {}", signature.len());
                        builder = builder.header(XNostrSig(signature));
                    }
                    Err(e) => {
                        println!("[RUST] send_email: Failed to sign email body: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("[RUST] send_email: Failed to get public key from private key: {}", e);
            }
        }
    }
    
    // Build email with or without attachments
    let email = if let Some(attachments) = attachments {
        if attachments.is_empty() {
            // No attachments, simple text email
            builder.body(body.to_string())?
        } else {
            println!("[RUST] send_email: Building multipart email with {} attachments", attachments.len());
            
            // Create multipart email with text body and attachments
            let mut multipart = MultiPart::mixed()
                .singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(body.to_string()));
            
            // Add each attachment
            for attachment in attachments {
                println!("[RUST] send_email: Adding attachment: {} ({})", attachment.filename, attachment.size);
                
                // Decode base64 data
                let attachment_data = match general_purpose::STANDARD.decode(&attachment.data) {
                    Ok(data) => data,
                    Err(e) => {
                        println!("[RUST] send_email: Failed to decode base64 attachment data for {}: {}", attachment.filename, e);
                        continue;
                    }
                };
                
                // Parse content type
                let content_type = attachment.content_type.parse::<ContentType>()
                    .unwrap_or(ContentType::parse("application/octet-stream").unwrap());
                
                // Create attachment part
                let attachment_part = Attachment::new(attachment.filename.clone())
                    .body(attachment_data, content_type);
                
                multipart = multipart.singlepart(attachment_part);
            }
            
            builder.multipart(multipart)?
        }
    } else {
        // No attachments, simple text email
        builder.body(body.to_string())?
    };

    let creds = Credentials::new(config.email_address.clone(), config.password.clone());

    // Build the mailer with proper TLS configuration
    let mut mailer_builder = SmtpTransport::relay(&config.smtp_host)?
        .port(config.smtp_port)
        .credentials(creds);

    // Configure TLS based on the use_tls setting
    if config.use_tls {
        // For Gmail and most modern providers, use STARTTLS
        let tls_params = lettre::transport::smtp::client::TlsParameters::new(config.smtp_host.clone())?;
        mailer_builder = mailer_builder.tls(lettre::transport::smtp::client::Tls::Required(tls_params));
    }

    let mailer = mailer_builder.build();

    println!("[RUST] send_email: Mailer built, attempting to send...");
    
    // Run the blocking SMTP send operation in a separate thread with a 60-second timeout
    let mailer_clone = mailer.clone();
    let email_clone = email.clone();
    
    let send_future = task::spawn_blocking(move || {
        println!("[RUST] send_email: Executing SMTP send in blocking thread");
        mailer_clone.send(&email_clone)
    });
    
    match timeout(Duration::from_secs(60), send_future).await {
        Ok(join_res) => match join_res {
            Ok(send_res) => match send_res {
                Ok(_) => {
                    println!("[RUST] send_email: Email sent successfully");
                    Ok(format!("Email sent successfully to {}", to_address))
                }
                Err(e) => {
                    println!("[RUST] send_email: Failed to send email: {}", e);
                    let error_msg = if e.to_string().to_lowercase().contains("authentication") {
                        "Authentication failed. For Gmail, make sure you're using an App Password, not your regular password.".to_string()
                    } else if e.to_string().to_lowercase().contains("connection") || e.to_string().to_lowercase().contains("host") {
                        "SMTP client error. Check your SMTP host and port settings.".to_string()
                    } else if e.is_transient() {
                        "Temporary SMTP error. Please try again.".to_string()
                    } else if e.is_permanent() {
                        "Permanent SMTP error. Check your email configuration.".to_string()
                    } else {
                        format!("SMTP error: {}", e)
                    };
                    Err(anyhow::anyhow!("Failed to send email: {}", error_msg))
                }
            },
            Err(e) => {
                println!("[RUST] send_email: Task join error: {}", e);
                Err(anyhow::anyhow!("Task join error: {}", e))
            }
        },
        Err(_) => {
            println!("[RUST] send_email: SMTP send operation timed out after 60 seconds");
            Err(anyhow::anyhow!("SMTP send operation timed out after 60 seconds. Check your internet connection and SMTP settings."))
        }
    }
}

/// Delete a sent email from the IMAP server by moving it to Trash
/// For Gmail, moves to [Gmail]/Trash
/// For other providers, tries common trash folder names
pub async fn delete_sent_email_from_server(config: &EmailConfig, message_id: &str) -> Result<()> {
    use std::net::TcpStream;
    
    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    let is_gmail = host.contains("gmail.com");
    
    println!("[RUST] delete_sent_email_from_server: Attempting to delete email with Message-ID: {}", message_id);
    
    // Handle TLS and non-TLS connections separately due to type differences
    // Use a block to ensure session is dropped/logged out properly
    let result = if use_tls {
        let client = create_imap_tls_client!(host, &addr)?;
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        
        let result = delete_sent_email_from_session(&mut session, is_gmail, message_id).await;
        // Close the session properly - ignore errors on logout
        let _ = session.logout();
        println!("[RUST] delete_sent_email_from_server: Session closed");
        result
    } else {
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        
        let result = delete_sent_email_from_session(&mut session, is_gmail, message_id).await;
        // Close the session properly - ignore errors on logout
        let _ = session.logout();
        println!("[RUST] delete_sent_email_from_server: Session closed");
        result
    };
    
    result
}

/// Helper function to ensure the nostr-mail folder exists on the IMAP server
/// Returns the folder name if successful, or an error if folder creation/access fails
fn ensure_nostr_mail_folder_exists(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    _is_gmail: bool,
) -> Result<String> {
    // Use "nostr-mail" for all providers (simpler and more consistent)
    let folder_name = "nostr-mail";
    
    println!("[RUST] ensure_nostr_mail_folder_exists: Attempting to create/verify folder: {}", folder_name);
    
    // Try to create the folder (will fail silently if it already exists)
    let _ = session.create(folder_name);
    
    // Verify it exists by trying to select it
    match session.select(folder_name) {
        Ok(_) => {
            println!("[RUST] ensure_nostr_mail_folder_exists: Successfully verified folder: {}", folder_name);
            Ok(folder_name.to_string())
        }
        Err(_) => {
            // Try alternative folder names
            let alternatives = vec!["Nostr-Mail", "NostrMail", "nostrmail"];
            
            for alt in alternatives {
                println!("[RUST] ensure_nostr_mail_folder_exists: Trying alternative folder name: {}", alt);
                let _ = session.create(alt);
                if session.select(alt).is_ok() {
                    println!("[RUST] ensure_nostr_mail_folder_exists: Successfully verified alternative folder: {}", alt);
                    return Ok(alt.to_string());
                }
            }
            
            Err(anyhow::anyhow!("Could not create or access nostr-mail folder"))
        }
    }
}

/// Helper function to move email to nostr-mail folder
/// Uses MOVE command if available, falls back to COPY + DELETE
/// Note: This function assumes the session is already in the correct folder context (e.g., INBOX)
fn move_email_to_nostr_folder(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    message_seq: u32,
    is_gmail: bool,
    current_folder: &str,
) -> Result<()> {
    // Ensure folder exists (this will select the nostr-mail folder temporarily)
    let nostr_folder = match ensure_nostr_mail_folder_exists(session, is_gmail) {
        Ok(folder) => folder,
        Err(e) => {
            println!("[RUST] move_email_to_nostr_folder: Failed to ensure folder exists: {}, continuing without move", e);
            return Err(e);
        }
    };
    
    // Re-select the current folder (e.g., INBOX) so we can move from it
    // This is necessary because ensure_nostr_mail_folder_exists selects the nostr-mail folder
    if let Err(e) = session.select(current_folder) {
        println!("[RUST] move_email_to_nostr_folder: Failed to re-select {}: {}, attempting move anyway", current_folder, e);
        // Try to continue anyway - the move might still work
    }
    
    let message_seq_str = format!("{}", message_seq);
    println!("[RUST] move_email_to_nostr_folder: Moving message {} from {} to folder: {}", message_seq_str, current_folder, nostr_folder);
    
    // Try MOVE command first (preferred, supported by Gmail and modern IMAP servers)
    match session.mv(&message_seq_str, &nostr_folder) {
        Ok(_) => {
            println!("[RUST] move_email_to_nostr_folder: Successfully moved email using MOVE command");
            return Ok(());
        }
        Err(e) => {
            println!("[RUST] move_email_to_nostr_folder: MOVE command failed: {}, trying COPY + DELETE", e);
        }
    }
    
    // Fallback: Use COPY + STORE + EXPUNGE if MOVE is not supported
    match session.copy(&message_seq_str, &nostr_folder) {
        Ok(_) => {
            println!("[RUST] move_email_to_nostr_folder: Successfully copied email to {}", nostr_folder);
            // Mark original as deleted
            session.store(&message_seq_str, "+FLAGS (\\Deleted)")?;
            // Expunge to actually delete
            session.expunge()?;
            println!("[RUST] move_email_to_nostr_folder: Successfully moved email using COPY + DELETE");
            Ok(())
        }
        Err(e) => {
            println!("[RUST] move_email_to_nostr_folder: Failed to move email: {}", e);
            Err(anyhow::anyhow!("Failed to move email to nostr-mail folder: {}", e))
        }
    }
}

/// Helper function to delete email from IMAP session (works with both TLS and non-TLS)
async fn delete_sent_email_from_session(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    is_gmail: bool,
    message_id: &str,
) -> Result<()> {
    
    // Select the sent folder
    let sent_folder = if is_gmail {
        "[Gmail]/Sent Mail"
    } else {
        "Sent"
    };
    
    println!("[RUST] delete_sent_email_from_session: Selecting sent folder: {}", sent_folder);
    
    // Try to select the sent folder, fallback to common variations
    let folder_selected = session.select(sent_folder).is_ok() || 
                         session.select("Sent Mail").is_ok() || 
                         session.select("Sent Items").is_ok() ||
                         session.select("Sent").is_ok();
    
    if !folder_selected {
        println!("[RUST] delete_sent_email_from_session: Could not select sent folder, aborting server deletion");
        return Err(anyhow::anyhow!("Could not select sent folder"));
    }
    
    // Search for the email by Message-ID header
    // The message_id might be just the UUID or the full <uuid@domain> format
    // Try both formats to ensure we find it
    let normalized_msg_id = message_id.trim().trim_start_matches('<').trim_end_matches('>');
    
    // Try searching with the full Message-ID format first (with angle brackets)
    let full_msg_id = if normalized_msg_id.contains('@') {
        format!("<{}>", normalized_msg_id)
    } else {
        // If it's just a UUID, add the @nostr-mail domain
        format!("<{}@nostr-mail>", normalized_msg_id)
    };
    
    // Try multiple search formats
    let search_queries = vec![
        format!("HEADER Message-ID \"{}\"", full_msg_id),
        format!("HEADER Message-ID \"{}\"", normalized_msg_id),
        format!("HEADER Message-ID \"{}\"", message_id.trim()),
    ];
    
    let mut matching_messages = std::collections::HashSet::new();
    for search_query in &search_queries {
        println!("[RUST] delete_sent_email_from_session: Searching for email with query: {}", search_query);
        match session.search(search_query) {
            Ok(results) => {
                let result_count = results.len();
                if !results.is_empty() {
                    matching_messages.extend(results);
                    println!("[RUST] delete_sent_email_from_session: Found {} matching message(s) with query: {}", result_count, search_query);
                    break; // Found results, no need to try other formats
                }
            }
            Err(e) => {
                println!("[RUST] delete_sent_email_from_session: Search query failed: {} - {}", search_query, e);
            }
        }
    }
    
    if matching_messages.is_empty() {
        println!("[RUST] delete_sent_email_from_session: No email found with Message-ID (tried: {}, {}, {})", full_msg_id, normalized_msg_id, message_id.trim());
        return Err(anyhow::anyhow!("Email not found on server"));
    }
    
    println!("[RUST] delete_sent_email_from_session: Found {} matching message(s)", matching_messages.len());
    
    // Get the message sequence number (should be just one)
    // Convert HashSet to Vec to get the first element
    let message_seq = *matching_messages.iter().next().ok_or_else(|| anyhow::anyhow!("No message sequence found"))?;
    
    // Determine trash folder name
    let trash_folder = if is_gmail {
        "[Gmail]/Trash"
    } else {
        // Try common trash folder names
        "Trash"
    };
    
    println!("[RUST] delete_sent_email_from_session: Moving message {} to trash folder: {}", message_seq, trash_folder);
    
    // Use MOVE command (mv method) to move the message to trash
    // This is supported by Gmail and other modern IMAP servers
    let message_seq_str = format!("{}", message_seq);
    match session.mv(&message_seq_str, trash_folder) {
        Ok(_) => {
            println!("[RUST] delete_sent_email_from_session: Successfully moved email to trash using MOVE command");
            return Ok(());
        }
        Err(e) => {
            println!("[RUST] delete_sent_email_from_session: MOVE command failed: {}, trying COPY + DELETE", e);
        }
    }
    
    // Fallback: Use COPY + STORE + EXPUNGE if MOVE is not supported
    // First, try to copy to trash
    let copy_result = session.copy(&message_seq_str, trash_folder);
    match copy_result {
        Ok(_) => {
            println!("[RUST] delete_sent_email_from_session: Successfully copied email to trash");
            // Mark original as deleted
            session.store(&message_seq_str, "+FLAGS (\\Deleted)")?;
            // Expunge to actually delete
            session.expunge()?;
            println!("[RUST] delete_sent_email_from_session: Successfully deleted email from sent folder");
            Ok(())
        }
        Err(e) => {
            // If COPY fails, try alternative trash folder names
            let alternative_trash_folders = if is_gmail {
                vec!["[Gmail]/Trash"]
            } else {
                vec!["Trash", "Deleted", "Deleted Items", "Junk"]
            };
            
            for alt_trash in alternative_trash_folders {
                println!("[RUST] delete_sent_email_from_session: Trying alternative trash folder: {}", alt_trash);
                if session.copy(&message_seq_str, alt_trash).is_ok() {
                    session.store(&message_seq_str, "+FLAGS (\\Deleted)")?;
                    session.expunge()?;
                    println!("[RUST] delete_sent_email_from_session: Successfully moved email to {} using COPY", alt_trash);
                    return Ok(());
                }
            }
            
            Err(anyhow::anyhow!("Failed to move email to trash: {}", e))
        }
    }
}

/// Test IMAP connection with the given config. Returns Ok(()) if successful, Err otherwise.
pub async fn test_imap_connection(config: &EmailConfig) -> Result<()> {
    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;

    let addr = format!("{}:{}", host, port);
    
    println!("[RUST] Testing IMAP connection to: {}", addr);
    println!("[RUST] Host: {}, Port: {}, Use TLS: {}", host, port, use_tls);

    if use_tls {
        let client = create_imap_tls_client!(host, &addr)?;
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        session.logout()?;
    } else {
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        session.logout()?;
    }
    println!("[RUST] IMAP connection test successful");
    Ok(())
}

pub async fn fetch_emails(config: &EmailConfig, limit: usize, search_query: Option<String>, only_nostr: bool, latest: Option<chrono::DateTime<chrono::Utc>>, sync_cutoff_days: Option<i64>) -> Result<Vec<EmailMessage>> {
    println!("[RUST] fetch_emails: Starting to fetch emails with limit: {}, search: {:?}, only_nostr: {}, latest: {:?}, sync_cutoff_days: {:?}", limit, search_query, only_nostr, latest, sync_cutoff_days);
    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    println!("[RUST] fetch_emails: Connecting to IMAP server: {}", addr);
    
    // Check if this is Gmail and we're only looking for Nostr emails
    let is_gmail = host.contains("gmail.com");
    let use_gmail_optimization = is_gmail && only_nostr;
    
    let emails = if use_tls {
        let client = create_imap_tls_client!(host, &addr)?;
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if use_gmail_optimization {
            let (emails_result, _attachments) = fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest, sync_cutoff_days)?;
            emails_result
        } else {
            fetch_emails_from_session(&mut session, config, limit, search_query, only_nostr, latest, sync_cutoff_days)?
        }
    } else {
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if use_gmail_optimization {
            let (emails_result, _attachments) = fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest, sync_cutoff_days)?;
            emails_result
        } else {
            fetch_emails_from_session(&mut session, config, limit, search_query, only_nostr, latest, sync_cutoff_days)?
        }
    };
    
    println!("[RUST] fetch_emails: Successfully fetched {} emails", emails.len());
    // Sort emails by date (newest first)
    let mut sorted_emails = emails;
    sorted_emails.sort_by(|a, b| b.date.cmp(&a.date));
    
    // Apply limit if not using Gmail optimization (Gmail optimization already handles this)
    if !use_gmail_optimization && sorted_emails.len() > limit {
        sorted_emails.truncate(limit);
    }
    
    // After collecting all emails, filter for Nostr if needed (only for non-Gmail or when not using optimization)
    if only_nostr && !use_gmail_optimization {
        sorted_emails.retain(|email| {
            email.raw_headers.contains("X-Nostr-Pubkey:")
        });
    }
    
    Ok(sorted_emails)
}

fn fetch_emails_from_session(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, limit: usize, search_query: Option<String>, _only_nostr: bool, latest: Option<chrono::DateTime<chrono::Utc>>, sync_cutoff_days: Option<i64>) -> Result<Vec<EmailMessage>> {
    use chrono::Utc;
    println!("[RUST] fetch_emails_from_session: Successfully connected to IMAP server");
    session.select("INBOX")?;
    println!("[RUST] fetch_emails_from_session: Selected INBOX");
    
    // Calculate cutoff date for filtering
    // If latest is provided, use it directly
    // If latest is None (new device), use sync_cutoff_days setting (default 365 days / 1 year)
    // If sync_cutoff_days is 0 or None, fetch all emails
    let cutoff = if let Some(latest_date) = latest {
        latest_date
    } else {
        // New device - use user's sync cutoff setting
        let cutoff_days = sync_cutoff_days.unwrap_or(365); // Default to 1 year
        if cutoff_days <= 0 {
            // 0 means fetch all emails - use a very old date
            Utc::now() - chrono::Duration::days(365 * 100) // 100 years ago
        } else {
            Utc::now() - chrono::Duration::days(cutoff_days)
        }
    };
    
    // Subtract a small buffer (1 hour) to account for timing edge cases
    let cutoff_with_buffer = cutoff - chrono::Duration::hours(1);
    let since_date = cutoff_with_buffer.format("%d-%b-%Y").to_string();
    println!("[RUST] fetch_emails_from_session: Using SINCE filter for date: {} (latest: {:?}, sync_cutoff_days: {:?})", since_date, latest, sync_cutoff_days);
    // Gmail's IMAP does not support searching for custom headers like X-Nostr-Pubkey.
    // So we must fetch all emails and filter for Nostr emails client-side after download.
    let search_criteria = if let Some(query) = &search_query {
        if query.trim().is_empty() {
            format!("ALL SINCE {}", since_date)
        } else {
            format!("TO \"{}\" SINCE {}", query, since_date)
        }
    } else {
        format!("ALL SINCE {}", since_date)
    };
    // Search for messages matching criteria
    let matching_messages = session.search(&search_criteria)?;
    let total_messages = matching_messages.len();
    println!("[RUST] fetch_emails_from_session: Found {} messages matching criteria", total_messages);

    if total_messages == 0 {
        println!("[RUST] fetch_emails_from_session: No messages found matching criteria");
        return Ok(vec![]);
    }

    // Calculate the range of messages to fetch (most recent 'limit' messages)
    let start = if total_messages > limit {
        total_messages - limit + 1
    } else {
        1
    };
    let end = total_messages;

    println!("[RUST] fetch_emails_from_session: Fetching messages {} to {} of {} matching messages (most recent)", start, end, total_messages);

    // Get the message numbers for the range we want (most recent messages)
    let message_numbers: Vec<u32> = matching_messages.iter().skip(start - 1).take(end - start + 1).cloned().collect();
    
    if message_numbers.is_empty() {
        println!("[RUST] fetch_emails_from_session: No message numbers to fetch");
        return Ok(vec![]);
    }

    // Fetch the messages
    let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
    println!("[RUST] fetch_emails_from_session: Fetched {} message objects", messages.len());
    
    let mut emails = Vec::new();
    let mut email_id = 0;

    for (idx, message) in messages.iter().enumerate() {
        email_id += 1;
        // Get the actual message sequence number from message_numbers
        let message_seq = message_numbers.get(idx).copied().unwrap_or(0);
        println!("[RUST] Processing message {} (sequence: {})", email_id, message_seq);
        
        if let Some(body) = message.body() {
            println!("[RUST] Message {} has body, length: {}", email_id, body.len());
            
            if let Ok(email) = parse_mail(body) {
                println!("[RUST] Successfully parsed email {}", email_id);
                
                // Extract header values before moving email
                let from = email.headers
                    .get_first_value("From")
                    .unwrap_or_else(|| "Unknown".to_string());
                
                let to = email.headers
                    .get_first_value("To")
                    .unwrap_or_else(|| config.email_address.clone());
                
                let subject_raw = email.headers
                    .get_first_value("Subject")
                    .unwrap_or_else(|| "No Subject".to_string());
                let subject = decode_header_value(&subject_raw);
                
                let date_str = email.headers
                    .get_first_value("Date")
                    .unwrap_or_else(|| Utc::now().to_rfc2822());
                
                // Parse the date, fallback to current time if parsing fails
                let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                
                // Extract body text - try multiple approaches
                let body_text = if let Some(body_part) = email.subparts.first() {
                    if let Ok(body_content) = body_part.get_body() {
                        body_content
                    } else {
                        // Try to get the main body if subpart fails
                        email.get_body().unwrap_or_else(|_| "No body content".to_string())
                    }
                } else {
                    // No subparts, try to get the main body
                    email.get_body().unwrap_or_else(|_| "No body content".to_string())
                };

                println!("[RUST] Email {} - From: {}, Subject: {}, Body length: {}", 
                    email_id, from, subject, body_text.len());

                // Extract raw headers as a string
                let raw_headers = email.headers.iter()
                    .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
                    .collect::<Vec<_>>().join("\n");

                // Check if this is a Nostr email and try to decrypt it
                let (final_subject, final_body) = if raw_headers.contains("X-Nostr-Pubkey:") {
                    // Move email to nostr-mail folder before processing
                    let is_gmail = config.imap_host.contains("gmail.com");
                    if message_seq > 0 {
                        if let Err(e) = move_email_to_nostr_folder(session, message_seq, is_gmail, "INBOX") {
                            println!("[RUST] fetch_emails_from_session: Failed to move email {} (seq {}) to nostr-mail folder: {}, continuing", email_id, message_seq, e);
                            // Continue processing even if move fails
                        }
                    }
                    
                    match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                        Ok((dec_subject, dec_body)) => {
                            println!("[RUST] fetch_emails_from_session: Email {} decryption completed", email_id);
                            (dec_subject, dec_body)
                        }
                        Err(e) => {
                            println!("[RUST] fetch_emails_from_session: Email {} decryption failed: {}, using original content", email_id, e);
                            (subject.clone(), body_text.clone())
                        }
                    }
                } else {
                    (subject.clone(), body_text.clone())
                };

                let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                
                // Verify signature if present
                let signature_valid = if let Some(pubkey) = &sender_pubkey {
                    if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                        Some(verify_email_signature(pubkey, &sig, &final_body))
                    } else {
                        None // No signature present
                    }
                } else {
                    None // No pubkey, can't verify
                };
                
                // Verify transport authentication
                let transport_auth = verify_transport_authentication(Some(body), Some(&email))
                    .unwrap_or_else(|e| TransportAuthVerdict {
                        transport_verified: false,
                        method: TransportAuthMethod::None,
                        reason: format!("Error verifying transport auth: {}", e),
                    });
                
                // Skip emails that fail transport authentication
                if !transport_auth.transport_verified {
                    println!("[RUST] fetch_emails_from_session: Email {} failed transport authentication: {}", email_id, transport_auth.reason);
                    continue;
                }
                
                let email_message = EmailMessage {
                    id: email_id.to_string(),
                    from,
                    to,
                    subject: final_subject,
                    body: final_body.clone(),
                    raw_body: final_body.clone(),
                    date,
                    is_read: true, // We'll assume all fetched emails are read for now
                    raw_headers: raw_headers.clone(),
                    sender_pubkey: sender_pubkey.clone(),
                    recipient_pubkey: None, // Inbox emails don't have recipient_pubkey
                    message_id: extract_message_id_from_headers(&raw_headers),
                    signature_valid,
                    transport_auth_verified: Some(transport_auth.transport_verified),
                };

                emails.push(email_message);
            } else {
                println!("[RUST] Failed to parse email {}", email_id);
            }
        } else {
            println!("[RUST] Message {} has no body", email_id);
        }
    }

    session.logout()?;
    println!("[RUST] fetch_emails_from_session: Successfully processed {} emails", emails.len());
    Ok(emails)
}

fn fetch_emails_from_session_last_24h(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, latest: Option<chrono::DateTime<chrono::Utc>>, sync_cutoff_days: Option<i64>) -> Result<Vec<EmailMessage>> {
    use chrono::{Duration, Utc};
    session.select("INBOX")?;
    
    // Calculate cutoff date for filtering
    // If latest is provided, use it directly
    // If latest is None (new device), use sync_cutoff_days setting (default 365 days / 1 year)
    // If sync_cutoff_days is 0 or None, fetch all emails
    let cutoff = if let Some(latest_date) = latest {
        latest_date
    } else {
        // New device - use user's sync cutoff setting
        let cutoff_days = sync_cutoff_days.unwrap_or(365); // Default to 1 year
        if cutoff_days <= 0 {
            // 0 means fetch all emails - use a very old date
            Utc::now() - Duration::days(365 * 100) // 100 years ago
        } else {
            Utc::now() - Duration::days(cutoff_days)
        }
    };
    
    // Subtract a small buffer (1 hour) to account for timing edge cases
    let cutoff_with_buffer = cutoff - Duration::hours(1);
    let since_date = cutoff_with_buffer.format("%d-%b-%Y").to_string();
    println!("[RUST] fetch_emails_from_session_last_24h: Using SINCE filter for date: {} (latest: {:?}, sync_cutoff_days: {:?})", since_date, latest, sync_cutoff_days);
    
    let search_criteria = format!("ALL SINCE {}", since_date);
    let matching_messages = session.search(&search_criteria)?;
    let total_messages = matching_messages.len();
    if total_messages == 0 {
        return Ok(vec![]);
    }
    // Fetch all matching messages
    let message_numbers: Vec<u32> = matching_messages.iter().cloned().collect();
    if message_numbers.is_empty() {
        return Ok(vec![]);
    }
    let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
    let mut emails = Vec::new();
    let mut _email_id = 0;
    for (idx, message) in messages.iter().enumerate() {
        _email_id += 1;
        // Get the actual message sequence number from message_numbers
        let message_seq = message_numbers.get(idx).copied().unwrap_or(0);
        if let Some(body) = message.body() {
            if let Ok(email) = parse_mail(body) {
                let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                let subject = decode_header_value(&subject_raw);
                let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                // Only keep emails after cutoff date
                if date < cutoff_with_buffer {
                    continue;
                }
                let body_text = if let Some(body_part) = email.subparts.first() {
                    if let Ok(body_content) = body_part.get_body() {
                        body_content
                    } else {
                        email.get_body().unwrap_or_else(|_| "No body content".to_string())
                    }
                } else {
                    email.get_body().unwrap_or_else(|_| "No body content".to_string())
                };
                let raw_headers = email.headers.iter().map(|h| format!("{}: {}", h.get_key(), h.get_value())).collect::<Vec<_>>().join("\n");
                // Check if this is a Nostr email and try to decrypt it
                let (_final_subject, _final_body) = if raw_headers.contains("X-Nostr-Pubkey:") {
                    // Move email to nostr-mail folder before processing
                    let is_gmail = config.imap_host.contains("gmail.com");
                    if message_seq > 0 {
                        if let Err(e) = move_email_to_nostr_folder(session, message_seq, is_gmail, "INBOX") {
                            println!("[RUST] fetch_emails_from_session_last_24h: Failed to move email {} (seq {}) to nostr-mail folder: {}, continuing", _email_id, message_seq, e);
                            // Continue processing even if move fails
                        }
                    }
                    
                    match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                        Ok((dec_subject, dec_body)) => {
                            println!("[RUST] fetch_emails_from_session_last_24h: Email {} decryption completed", _email_id);
                            (dec_subject, dec_body)
                        }
                        Err(e) => {
                            println!("[RUST] fetch_emails_from_session_last_24h: Email {} decryption failed: {}, using original content", _email_id, e);
                            (subject.clone(), body_text.clone())
                        }
                    }
                } else {
                    (subject.clone(), body_text.clone())
                };
                
                let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                
                // Verify signature if present - signature is created on the encrypted body, so verify against body_text
                let signature_valid = if let Some(pubkey) = &sender_pubkey {
                    if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                        Some(verify_email_signature(pubkey, &sig, &body_text))
                    } else {
                        None // No signature present
                    }
                } else {
                    None // No pubkey, can't verify
                };
                
                // Verify transport authentication
                let transport_auth = verify_transport_authentication(Some(body), Some(&email))
                    .unwrap_or_else(|e| TransportAuthVerdict {
                        transport_verified: false,
                        method: TransportAuthMethod::None,
                        reason: format!("Error verifying transport auth: {}", e),
                    });
                
                // Skip emails that fail transport authentication
                if !transport_auth.transport_verified {
                    println!("[RUST] fetch_emails_from_session_last_24h: Email {} failed transport authentication: {}", _email_id, transport_auth.reason);
                    continue;
                }
                
                let email_message = EmailMessage {
                    id: _email_id.to_string(),
                    from,
                    to,
                    subject: _final_subject,
                    body: _final_body.clone(),
                    raw_body: _final_body.clone(),
                    date,
                    is_read: true,
                    raw_headers: raw_headers.clone(),
                    sender_pubkey: sender_pubkey.clone(),
                    recipient_pubkey: None, // Inbox emails don't have recipient_pubkey
                    message_id: extract_message_id_from_headers(&raw_headers),
                    signature_valid,
                    transport_auth_verified: Some(transport_auth.transport_verified),
                };
                emails.push(email_message);
            }
        }
    }
    session.logout()?;
    Ok(emails)
}

/// Test SMTP connection with the given config. Returns Ok(()) if successful, Err otherwise.
pub async fn test_smtp_connection(config: &EmailConfig) -> Result<()> {
    println!("[RUST] test_smtp_connection: Starting SMTP connection test");
    println!("[RUST] test_smtp_connection: SMTP Host: {}, Port: {}", config.smtp_host, config.smtp_port);
    println!("[RUST] test_smtp_connection: Email: {}, Use TLS: {}", config.email_address, config.use_tls);
    
    let creds = Credentials::new(config.email_address.clone(), config.password.clone());

    // Build the mailer with proper TLS configuration
    let mut mailer_builder = SmtpTransport::relay(&config.smtp_host)?
        .port(config.smtp_port)
        .credentials(creds);

    // Configure TLS based on the use_tls setting
    if config.use_tls {
        let tls_params = lettre::transport::smtp::client::TlsParameters::new(config.smtp_host.clone())?;
        mailer_builder = mailer_builder.tls(lettre::transport::smtp::client::Tls::Required(tls_params));
    }

    let mailer = mailer_builder.build();

    println!("[RUST] test_smtp_connection: Mailer built, testing connection...");
    
    // Test the connection with a timeout
    let mailer_clone = mailer.clone();
    let test_future = task::spawn_blocking(move || {
        println!("[RUST] test_smtp_connection: Executing connection test in blocking thread");
        mailer_clone.test_connection()
    });
    
    match timeout(Duration::from_secs(30), test_future).await {
        Ok(join_res) => match join_res {
            Ok(test_res) => match test_res {
                Ok(_) => {
                    println!("[RUST] test_smtp_connection: SMTP connection test successful");
                    Ok(())
                }
                Err(e) => {
                    println!("[RUST] test_smtp_connection: SMTP connection test failed: {}", e);
                    let error_msg = if e.to_string().to_lowercase().contains("authentication") {
                        "Authentication failed. For Gmail, make sure you're using an App Password, not your regular password.".to_string()
                    } else if e.to_string().to_lowercase().contains("connection") || e.to_string().to_lowercase().contains("host") {
                        "SMTP client error. Check your SMTP host and port settings.".to_string()
                    } else if e.is_transient() {
                        "Temporary SMTP error. Please try again.".to_string()
                    } else if e.is_permanent() {
                        "Permanent SMTP error. Check your email configuration.".to_string()
                    } else {
                        format!("SMTP connection error: {}", e)
                    };
                    Err(anyhow::anyhow!("SMTP connection failed: {}", error_msg))
                }
            },
            Err(e) => {
                println!("[RUST] test_smtp_connection: Task join error: {}", e);
                Err(anyhow::anyhow!("SMTP connection join error: {}", e))
            }
        },
        Err(_) => {
            println!("[RUST] test_smtp_connection: SMTP connection test timed out after 30 seconds");
            Err(anyhow::anyhow!("SMTP connection test timed out after 30 seconds. Check your internet connection and SMTP settings."))
        }
    }
}

/// Fetch up to 100 emails from the last 24 hours that have the X-Nostr-Pubkey header
pub async fn fetch_nostr_emails_last_24h(config: &EmailConfig) -> Result<Vec<EmailMessage>> {
    use chrono::{Utc, Duration};
    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    println!("[RUST] fetch_nostr_emails_last_24h: Connecting to IMAP server: {}", addr);
    
    // Check if this is Gmail to use optimized search
    let is_gmail = host.contains("gmail.com");
    
    // Calculate latest timestamp for 24 hours ago
    let latest_24h = Some(Utc::now() - Duration::hours(24));
    
    let emails = if use_tls {
        let client = create_imap_tls_client!(host, &addr)?;
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            let (emails_result, _attachments) = fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest_24h, None)?;
            emails_result
        } else {
            fetch_emails_from_session_last_24h(&mut session, config, latest_24h, None)?
        }
    } else {
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            let (emails_result, _attachments) = fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest_24h, None)?;
            emails_result
        } else {
            fetch_emails_from_session_last_24h(&mut session, config, latest_24h, None)?
        }
    };
    
    println!("[RUST] fetch_nostr_emails_last_24h: Fetched {} emails from last 24h", emails.len());
    
    // For non-Gmail providers, we still need to filter for Nostr emails
    let mut nostr_emails = if is_gmail {
        emails // Already filtered by Gmail search
    } else {
        emails.into_iter()
            .filter(|email| email.raw_headers.contains("X-Nostr-Pubkey:"))
            .collect()
    };
    
    // If no emails found in last 24h and this is Gmail, try searching last 7 days
    if nostr_emails.is_empty() && is_gmail {
        println!("[RUST] fetch_nostr_emails_last_24h: No emails found in last 24h, trying last 7 days");
        // Calculate latest timestamp for 7 days ago
        let latest_7d = Some(Utc::now() - Duration::days(7));
        let fallback_emails = if use_tls {
            let client = create_imap_tls_client!(host, &addr)?;
            let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
            let (emails_result, _attachments) = fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest_7d, None)?;
            emails_result
        } else {
            let tcp_stream = TcpStream::connect(&addr)?;
            let client = imap::Client::new(tcp_stream);
            let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
            let (emails_result, _attachments) = fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest_7d, None)?;
            emails_result
        };
        nostr_emails = fallback_emails;
        println!("[RUST] fetch_nostr_emails_last_24h: Found {} emails from last 7 days", nostr_emails.len());
    }
    
    // Sort by date, newest first
    nostr_emails.sort_by(|a, b| b.date.cmp(&a.date));
    // Limit to 100
    nostr_emails.truncate(100);
    Ok(nostr_emails)
}

/// Optimized function that uses Gmail's X-GM-RAW search to find Nostr encrypted emails
/// 
/// This function leverages Gmail's powerful X-GM-RAW search operator to efficiently find
/// Nostr-related emails without having to download and filter all emails client-side.
/// 
/// Search Strategy:
/// 1. Search for any "BEGIN NOSTR NIP-<number> ENCRYPTED MESSAGE" - finds emails with NIP-based encrypted content
/// 2. Search for "X-Nostr-Pubkey:" - finds emails with Nostr public key headers
/// 3. Search for any "END NOSTR NIP-<number> ENCRYPTED MESSAGE"
/// 4. Combine results and remove duplicates
/// 5. Fetch only the matching emails (much more efficient than fetching all emails)
/// 
/// Benefits:
/// - Dramatically reduces bandwidth usage
/// - Faster email fetching (only relevant emails downloaded)
/// - Reduces server load on Gmail
/// - Better user experience with faster loading times
fn fetch_nostr_emails_from_gmail_optimized(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, latest: Option<chrono::DateTime<chrono::Utc>>, sync_cutoff_days: Option<i64>) -> Result<(Vec<EmailMessage>, Vec<(String, Vec<crate::database::Attachment>)>)> {
    use chrono::Utc;
    
    session.select("INBOX")?;
    
    // Calculate cutoff date for filtering
    // If latest is provided, use it directly
    // If latest is None (new device), use sync_cutoff_days setting (default 365 days / 1 year)
    // If sync_cutoff_days is 0 or None, fetch all emails
    let cutoff = if let Some(latest_date) = latest {
        latest_date
    } else {
        // New device - use user's sync cutoff setting
        let cutoff_days = sync_cutoff_days.unwrap_or(365); // Default to 1 year
        if cutoff_days <= 0 {
            // 0 means fetch all emails - use a very old date
            Utc::now() - chrono::Duration::days(365 * 100) // 100 years ago
        } else {
            Utc::now() - chrono::Duration::days(cutoff_days)
        }
    };
    
    // Use Gmail's date filtering in X-GM-RAW search to reduce server load
    // IMPORTANT: Gmail interprets "after:YYYY/MM/DD" as midnight (00:00) Pacific Time, not UTC!
    // To avoid timezone issues, we use Unix timestamps instead, which are timezone-independent
    // Format: X-GM-RAW "content search AND after:unix_timestamp"
    // Unix timestamps are in seconds since epoch (1970-01-01 00:00:00 UTC)
    let date_filter = if latest.is_some() {
        // Convert cutoff timestamp to Unix timestamp (seconds)
        let unix_timestamp = cutoff.timestamp();
        // Subtract a small buffer (1 hour) to account for any timing edge cases
        let search_timestamp = unix_timestamp - 3600;
        println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Filtering for emails after: {} (using Unix timestamp: {} to avoid timezone issues)", cutoff, search_timestamp);
        format!(" AND after:{}", search_timestamp)
    } else {
        String::new()
    };
    
    // Use more specific search terms with date filtering to avoid fetching old emails
    // Try multiple search strategies to catch all Nostr emails:
    let mut search_terms = vec![
        format!("X-GM-RAW \"BEGIN NOSTR NIP-{}\"", date_filter),
        format!("X-GM-RAW \"END NOSTR NIP-{}\"", date_filter),
    ];
    
    // Add header searches - try multiple approaches
    if !date_filter.is_empty() {
        // With date filter, try different header search syntaxes
        search_terms.push(format!("X-GM-RAW \"has:X-Nostr-Pubkey{}\"", date_filter));
        // Also search for "npub" which is in the header value - this might be more reliable
        search_terms.push(format!("X-GM-RAW \"npub{}\"", date_filter));
    } else {
        // Without date filter
        search_terms.push("X-GM-RAW \"has:X-Nostr-Pubkey\"".to_string());
        search_terms.push("X-GM-RAW \"npub\"".to_string());
    }
    
    let mut all_message_numbers: HashSet<u32> = HashSet::new();
    
    // Search with each term and collect results
    for search_term in &search_terms {
        println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Searching with: {}", search_term);
        match session.search(search_term) {
            Ok(messages) => {
                let count = messages.len();
                println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Found {} messages with search '{}'", count, search_term);
                all_message_numbers.extend(messages.iter().cloned());
            }
            Err(e) => {
                println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Search '{}' failed: {}", search_term, e);
            }
        }
    }
    
    let mut message_numbers: Vec<u32> = all_message_numbers.into_iter().collect();
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Total unique messages to fetch: {}", message_numbers.len());
    
    // If no results and we have a latest date that's recent (within last 7 days), try searching without date filter
    // This handles cases where Gmail's date filtering might be too strict
    if message_numbers.is_empty() && latest.is_some() {
        let days_since_latest = Utc::now().signed_duration_since(cutoff).num_days();
        if days_since_latest <= 7 {
            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: No results with date filter, trying without date filter (latest was {} days ago)", days_since_latest);
            let fallback_search_terms = vec![
                "X-GM-RAW \"BEGIN NOSTR NIP-\"".to_string(),
                "X-GM-RAW \"END NOSTR NIP-\"".to_string(),
                "X-GM-RAW \"has:X-Nostr-Pubkey\"".to_string(),
                "X-GM-RAW \"npub\"".to_string(),
            ];
            
            let mut fallback_message_numbers: HashSet<u32> = HashSet::new();
            for search_term in fallback_search_terms {
                match session.search(&search_term) {
                    Ok(messages) => {
                        let count = messages.len();
                        println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Fallback search '{}' found {} messages", search_term, count);
                        fallback_message_numbers.extend(messages.iter().cloned());
                    }
                    Err(e) => {
                        println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Fallback search '{}' failed: {}", search_term, e);
                    }
                }
            }
            message_numbers = fallback_message_numbers.into_iter().collect();
            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Fallback search found {} total messages", message_numbers.len());
        }
    }
    
    if message_numbers.is_empty() {
        println!("[RUST] fetch_nostr_emails_from_gmail_optimized: No messages found, returning empty result");
        return Ok((vec![], vec![]));
    }
    
    // Fetch all matching messages
    let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Successfully fetched {} message objects", messages.len());
    
    let mut emails = Vec::new();
    let mut email_id = 0;
    
    // Store parsed emails with attachments for later use (keyed by message_id)
    let mut emails_with_attachments: Vec<(String, Vec<crate::database::Attachment>)> = Vec::new();
    
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Filtering for emails after: {}", cutoff);
    
    for (idx, message) in messages.iter().enumerate() {
        email_id += 1;
        // Get the actual message sequence number from message_numbers
        let message_seq = message_numbers.get(idx).copied().unwrap_or(0);
        if let Some(body) = message.body() {
            if let Ok(email) = parse_mail(body) {
                let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                let subject = decode_header_value(&subject_raw);
                let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                
                println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Processing email {} - From: {}, Subject: {}, Date: {}", 
                    email_id, from, subject, date);
                
                // Only keep emails after the cutoff (use <= to include emails on the exact cutoff timestamp)
                if date <= cutoff {
                    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} is too old ({} <= {}), skipping", 
                        email_id, date, cutoff);
                    continue;
                }
                
                let body_text = if let Some(body_part) = email.subparts.first() {
                    if let Ok(body_content) = body_part.get_body() {
                        body_content
                    } else {
                        email.get_body().unwrap_or_else(|_| "No body content".to_string())
                    }
                } else {
                    email.get_body().unwrap_or_else(|_| "No body content".to_string())
                };
                
                let raw_headers = email.headers.iter()
                    .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                // Check for Nostr indicators
                let has_nostr_header = raw_headers.contains("X-Nostr-Pubkey:");
                // Check for both NIP-04 and NIP-44 encrypted message markers
                // Require BOTH begin and end markers to avoid false positives
                let has_nip04_begin = body_text.contains("BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE");
                let has_nip04_end = body_text.contains("END NOSTR NIP-04 ENCRYPTED MESSAGE");
                let has_nip44_begin = body_text.contains("BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE");
                let has_nip44_end = body_text.contains("END NOSTR NIP-44 ENCRYPTED MESSAGE");
                let has_encrypted_content = (has_nip04_begin && has_nip04_end) || (has_nip44_begin && has_nip44_end);
                
                println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} - Has Nostr header: {}, Has encrypted content (with both markers): {}", 
                    email_id, has_nostr_header, has_encrypted_content);
                
                // Accept if it has the header OR has both begin and end encrypted content markers
                let is_nostr_email = has_nostr_header || has_encrypted_content;
                
                if is_nostr_email {
                    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} is confirmed as Nostr email", email_id);
                    
                    // Move email to nostr-mail folder before processing
                    if message_seq > 0 {
                        if let Err(e) = move_email_to_nostr_folder(session, message_seq, true, "INBOX") {
                            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Failed to move email {} (seq {}) to nostr-mail folder: {}, continuing", email_id, message_seq, e);
                            // Continue processing even if move fails
                        }
                    }
                    
                    // Check if this is a manifest-encrypted email by looking at the full email body
                    let full_email_body = email.get_body_raw().unwrap_or_default();
                    let full_email_body_str = String::from_utf8_lossy(&full_email_body);
                    let is_manifest_encrypted = body_text.contains("\"attachments\"") && 
                                               (body_text.contains("\"cipher_sha256\"") || body_text.contains("\"key_wrap\"")) ||
                                               full_email_body_str.contains("\"attachments\"") &&
                                               (full_email_body_str.contains("\"cipher_sha256\"") || full_email_body_str.contains("\"key_wrap\""));
                    
                    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} is_manifest_encrypted: {}", email_id, is_manifest_encrypted);
                    
                    // Extract attachments from the parsed email (in encrypted form)
                    let mut extracted_attachments = extract_attachments_from_parsed_email(&email, &body_text);
                    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Extracted {} attachments from inbox email {}", extracted_attachments.len(), email_id);
                    
                    // For Nostr emails with attachments and encrypted content, mark as manifest-encrypted
                    if has_encrypted_content && !extracted_attachments.is_empty() {
                        println!("[RUST] Marking {} attachments as manifest_aes (Nostr email with encrypted content and attachments)", extracted_attachments.len());
                        for att in &mut extracted_attachments {
                            att.is_encrypted = true;
                            att.encryption_method = Some("manifest_aes".to_string());
                            att.algorithm = Some("AES-256".to_string());
                            println!("[RUST] Marked attachment {} as manifest_aes encrypted", att.filename);
                        }
                    } else if is_manifest_encrypted && !extracted_attachments.is_empty() {
                        // Also check if we detected manifest markers
                        for att in &mut extracted_attachments {
                            if att.encryption_method.is_none() {
                                att.is_encrypted = true;
                                att.encryption_method = Some("manifest_aes".to_string());
                                att.algorithm = Some("AES-256".to_string());
                                println!("[RUST] Updated attachment {} to manifest_aes", att.filename);
                            }
                        }
                    }
                    
                    // Store attachments keyed by message_id
                    let message_id = extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| email_id.to_string());
                    if !extracted_attachments.is_empty() {
                        emails_with_attachments.push((message_id.clone(), extracted_attachments));
                    }
                    
                    // Try to decrypt the email content
                    let (_final_subject, _final_body) = match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                        Ok((dec_subject, dec_body)) => {
                            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} decryption completed", email_id);
                            (dec_subject, dec_body)
                        }
                        Err(e) => {
                            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} decryption failed: {}, using original content", email_id, e);
                            (subject.clone(), body_text.clone())
                        }
                    };
                    
                    let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                    
                    // Verify signature if present
                    let signature_valid = if let Some(pubkey) = &sender_pubkey {
                        if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                            Some(verify_email_signature(pubkey, &sig, &body_text))
                        } else {
                            None // No signature present
                        }
                    } else {
                        None // No pubkey, can't verify
                    };
                    
                    // Verify transport authentication
                    let transport_auth = verify_transport_authentication(Some(body), Some(&email))
                        .unwrap_or_else(|e| TransportAuthVerdict {
                            transport_verified: false,
                            method: TransportAuthMethod::None,
                            reason: format!("Error verifying transport auth: {}", e),
                        });
                    
                    // Skip emails that fail transport authentication
                    if !transport_auth.transport_verified {
                        println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} failed transport authentication: {}", email_id, transport_auth.reason);
                        continue;
                    }
                    
                    let email_message = EmailMessage {
                        id: email_id.to_string(),
                        from,
                        to,
                        subject,
                        body: body_text.clone(),
                        raw_body: body_text.clone(),
                        date,
                        is_read: true,
                        raw_headers: raw_headers.clone(),
                        sender_pubkey: sender_pubkey.clone(),
                        recipient_pubkey: None, // Inbox emails don't have recipient_pubkey
                        message_id: Some(message_id),
                        signature_valid,
                        transport_auth_verified: Some(transport_auth.transport_verified),
                    };
                    emails.push(email_message);
                } else {
                    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} is not a Nostr email, skipping", email_id);
                }
            } else {
                println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Failed to parse email {}", email_id);
            }
        } else {
            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Message {} has no body", email_id);
        }
    }
    
    session.logout()?;
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Successfully processed {} Nostr emails with {} attachment sets", emails.len(), emails_with_attachments.len());
    Ok((emails, emails_with_attachments))
}

fn fetch_sent_emails_from_gmail_optimized(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, latest: Option<chrono::DateTime<chrono::Utc>>) -> Result<(Vec<EmailMessage>, Vec<(String, Vec<crate::database::Attachment>)>)> {
    use chrono::Utc;
    
    // Try to select the sent folder
    let sent_folder = "[Gmail]/Sent Mail";
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Selecting sent folder: {}", sent_folder);
    session.select(sent_folder)?;
    
    // Calculate cutoff date for filtering
    // If latest is provided, use it directly; otherwise search all emails
    let cutoff = latest.unwrap_or_else(|| Utc::now() - chrono::Duration::days(365 * 5));
    
    // Use Gmail's date filtering in X-GM-RAW search to reduce server load
    // IMPORTANT: Gmail interprets "after:YYYY/MM/DD" as midnight (00:00) Pacific Time, not UTC!
    // To avoid timezone issues, we use Unix timestamps instead, which are timezone-independent
    // Format: X-GM-RAW "content search AND after:unix_timestamp"
    // Unix timestamps are in seconds since epoch (1970-01-01 00:00:00 UTC)
    let date_filter = if latest.is_some() {
        // Convert cutoff timestamp to Unix timestamp (seconds)
        let unix_timestamp = cutoff.timestamp();
        // Subtract a small buffer (1 hour) to account for any timing edge cases
        let search_timestamp = unix_timestamp - 3600;
        println!("[RUST] fetch_sent_emails_from_gmail_optimized: Filtering for emails after: {} (using Unix timestamp: {} to avoid timezone issues)", cutoff, search_timestamp);
        format!(" AND after:{}", search_timestamp)
    } else {
        String::new()
    };
    
    // Use more specific search terms with date filtering to avoid fetching old emails
    // Gmail X-GM-RAW syntax supports various search operators
    // Try multiple search strategies to catch all Nostr emails:
    // 1. Search body content for encrypted message markers (partial match should work)
    // 2. Search for header using has: operator (Gmail-specific)
    // 3. Search for "npub" which appears in X-Nostr-Pubkey header values
    let mut search_terms = vec![
        format!("X-GM-RAW \"BEGIN NOSTR NIP-{}\"", date_filter),
        format!("X-GM-RAW \"END NOSTR NIP-{}\"", date_filter),
    ];
    
    // Add header searches - try multiple approaches
    if !date_filter.is_empty() {
        // With date filter, try different header search syntaxes
        // Note: Gmail's has: operator searches for header existence
        search_terms.push(format!("X-GM-RAW \"has:X-Nostr-Pubkey{}\"", date_filter));
        // Also search for "npub" which is in the header value - this might be more reliable
        search_terms.push(format!("X-GM-RAW \"npub{}\"", date_filter));
    } else {
        // Without date filter
        search_terms.push("X-GM-RAW \"has:X-Nostr-Pubkey\"".to_string());
        search_terms.push("X-GM-RAW \"npub\"".to_string());
    }
    
    let mut all_message_numbers: HashSet<u32> = HashSet::new();
    
    // Search with each term and collect results
    for search_term in search_terms {
        println!("[RUST] fetch_sent_emails_from_gmail_optimized: Searching with: {}", search_term);
        match session.search(&search_term) {
            Ok(messages) => {
                let count = messages.len();
                println!("[RUST] fetch_sent_emails_from_gmail_optimized: Found {} messages with search '{}'", count, search_term);
                all_message_numbers.extend(messages.iter().cloned());
            }
            Err(e) => {
                println!("[RUST] fetch_sent_emails_from_gmail_optimized: Search '{}' failed: {}", search_term, e);
            }
        }
    }
    
    let mut message_numbers: Vec<u32> = all_message_numbers.into_iter().collect();
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Total unique sent messages to fetch: {}", message_numbers.len());
    
    // If no results and we have a latest date that's recent (within last 7 days), try searching without date filter
    // This handles cases where Gmail's date filtering might be too strict
    if message_numbers.is_empty() && latest.is_some() {
        let days_since_latest = Utc::now().signed_duration_since(cutoff).num_days();
        if days_since_latest <= 7 {
            println!("[RUST] fetch_sent_emails_from_gmail_optimized: No results with date filter, trying without date filter (latest was {} days ago)", days_since_latest);
            let fallback_search_terms = vec![
                "X-GM-RAW \"BEGIN NOSTR NIP-\"".to_string(),
                "X-GM-RAW \"END NOSTR NIP-\"".to_string(),
                "X-GM-RAW \"has:X-Nostr-Pubkey\"".to_string(),
                "X-GM-RAW \"npub\"".to_string(), // Search for npub in header values
            ];
            
            let mut fallback_message_numbers: HashSet<u32> = HashSet::new();
            for search_term in fallback_search_terms {
                match session.search(&search_term) {
                    Ok(messages) => {
                        let count = messages.len();
                        println!("[RUST] fetch_sent_emails_from_gmail_optimized: Fallback search '{}' found {} messages", search_term, count);
                        fallback_message_numbers.extend(messages.iter().cloned());
                    }
                    Err(e) => {
                        println!("[RUST] fetch_sent_emails_from_gmail_optimized: Fallback search '{}' failed: {}", search_term, e);
                    }
                }
            }
            message_numbers = fallback_message_numbers.into_iter().collect();
            println!("[RUST] fetch_sent_emails_from_gmail_optimized: Fallback search found {} total messages", message_numbers.len());
        }
    }
    
    if message_numbers.is_empty() {
        println!("[RUST] fetch_sent_emails_from_gmail_optimized: No sent messages found, returning empty result");
        return Ok((vec![], vec![]));
    }
    
    // Fetch all matching messages
    let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Successfully fetched {} sent message objects", messages.len());
    
    let mut emails = Vec::new();
    let mut email_id = 0;
    
    // Filter for emails after the cutoff timestamp (client-side filtering since Gmail only supports dates)
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Filtering for sent emails after: {}", cutoff);
    
    // Store parsed emails with attachments for later use (keyed by message_id)
    let mut emails_with_attachments: Vec<(String, Vec<crate::database::Attachment>)> = Vec::new();
    
    for (idx, message) in messages.iter().enumerate() {
        email_id += 1;
        // Get the actual message sequence number from message_numbers
        let message_seq = message_numbers.get(idx).copied().unwrap_or(0);
        if let Some(body) = message.body() {
            if let Ok(email) = parse_mail(body) {
                let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                let subject = decode_header_value(&subject_raw);
                let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                
                println!("[RUST] fetch_sent_emails_from_gmail_optimized: Processing sent email {} - From: {}, Subject (raw): {:?}, Subject (decoded): {}, Date: {}", 
                    email_id, from, subject_raw, subject, date);
                
                // Only keep emails after the cutoff timestamp (strict comparison)
                if date <= cutoff {
                    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent email {} is not newer than cutoff ({} <= {}), skipping", 
                        email_id, date, cutoff);
                    continue;
                }
                
                let body_text = if let Some(body_part) = email.subparts.first() {
                    if let Ok(body_content) = body_part.get_body() {
                        body_content
                    } else {
                        email.get_body().unwrap_or_else(|_| "No body content".to_string())
                    }
                } else {
                    email.get_body().unwrap_or_else(|_| "No body content".to_string())
                };
                
                let raw_headers = email.headers.iter()
                    .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                // Check for Nostr indicators
                let has_nostr_header = raw_headers.contains("X-Nostr-Pubkey:");
                // Check for both NIP-04 and NIP-44 encrypted message markers
                // Require BOTH begin and end markers to avoid false positives
                let has_nip04_begin = body_text.contains("BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE");
                let has_nip04_end = body_text.contains("END NOSTR NIP-04 ENCRYPTED MESSAGE");
                let has_nip44_begin = body_text.contains("BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE");
                let has_nip44_end = body_text.contains("END NOSTR NIP-44 ENCRYPTED MESSAGE");
                let has_encrypted_content = (has_nip04_begin && has_nip04_end) || (has_nip44_begin && has_nip44_end);
                
                println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent email {} - Has Nostr header: {}, Has encrypted content (with both markers): {}", 
                    email_id, has_nostr_header, has_encrypted_content);
                
                // Accept if it has the header OR has both begin and end encrypted content markers
                let is_nostr_email = has_nostr_header || has_encrypted_content;
                
                if is_nostr_email {
                    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent email {} is confirmed as Nostr email", email_id);
                    
                    // Move email to nostr-mail folder before processing
                    // Note: We're in the sent folder context, so we need to ensure we're still there
                    if message_seq > 0 {
                        let sent_folder = "[Gmail]/Sent Mail";
                        if let Err(e) = move_email_to_nostr_folder(session, message_seq, true, sent_folder) {
                            println!("[RUST] fetch_sent_emails_from_gmail_optimized: Failed to move email {} (seq {}) to nostr-mail folder: {}, continuing", email_id, message_seq, e);
                            // Continue processing even if move fails
                        }
                    }
                    
                    // Check if this is a manifest-encrypted email by looking at the full email body
                    // The body_text might just be the text part, so we need to check the raw email body
                    // or check if the body contains manifest markers
                    let full_email_body = email.get_body_raw().unwrap_or_default();
                    let full_email_body_str = String::from_utf8_lossy(&full_email_body);
                    let is_manifest_encrypted = body_text.contains("\"attachments\"") && 
                                               (body_text.contains("\"cipher_sha256\"") || body_text.contains("\"key_wrap\"")) ||
                                               full_email_body_str.contains("\"attachments\"") &&
                                               (full_email_body_str.contains("\"cipher_sha256\"") || full_email_body_str.contains("\"key_wrap\""));
                    
                    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Email {} is_manifest_encrypted: {}", email_id, is_manifest_encrypted);
                    
                    // Extract attachments from the parsed email (in encrypted form)
                    let mut extracted_attachments = extract_attachments_from_parsed_email(&email, &body_text);
                    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Extracted {} attachments from sent email {}", extracted_attachments.len(), email_id);
                    
                    // For Nostr emails with attachments and encrypted content, mark as manifest-encrypted
                    // This is because Nostr emails with attachments use manifest-based encryption
                    // The manifest is encrypted in the body, so we can't detect it from the body text
                    // But we know that if it's a Nostr email with encrypted content and attachments, it's manifest-encrypted
                    if has_encrypted_content && !extracted_attachments.is_empty() {
                        println!("[RUST] Marking {} attachments as manifest_aes (Nostr email with encrypted content and attachments)", extracted_attachments.len());
                        for att in &mut extracted_attachments {
                            att.is_encrypted = true;
                            att.encryption_method = Some("manifest_aes".to_string());
                            att.algorithm = Some("AES-256".to_string());
                            println!("[RUST] Marked attachment {} as manifest_aes encrypted", att.filename);
                        }
                    } else if is_manifest_encrypted && !extracted_attachments.is_empty() {
                        // Also check if we detected manifest markers
                        for att in &mut extracted_attachments {
                            if att.encryption_method.is_none() {
                                att.is_encrypted = true;
                                att.encryption_method = Some("manifest_aes".to_string());
                                att.algorithm = Some("AES-256".to_string());
                                println!("[RUST] Updated attachment {} to manifest_aes", att.filename);
                            }
                        }
                    }
                    
                    // For sent emails, don't decrypt during sync - the subject is encrypted with recipient's pubkey
                    // which we don't have access to here. The frontend will decrypt it when displaying.
                    // Just save the encrypted content as-is.
                    let (_final_subject, _final_body) = (subject.clone(), body_text.clone());
                    
                    let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                    
                    // Verify signature if present
                    let signature_valid = if let Some(pubkey) = &sender_pubkey {
                        if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                            Some(verify_email_signature(pubkey, &sig, &body_text))
                        } else {
                            None // No signature present
                        }
                    } else {
                        None // No pubkey, can't verify
                    };
                    
                    let email_message = EmailMessage {
                        id: email_id.to_string(),
                        from,
                        to,
                        subject,
                        body: body_text.clone(),
                        raw_body: body_text.clone(),
                        date,
                        is_read: true,
                        raw_headers: raw_headers.clone(),
                        sender_pubkey: sender_pubkey.clone(), // Sent emails include sender's pubkey in headers
                        recipient_pubkey: None, // Will be populated during sync if contact exists
                        message_id: extract_message_id_from_headers(&raw_headers),
                        signature_valid,
                        transport_auth_verified: None, // Sent emails don't have transport auth verification
                    };
                    
                    // Store email with attachments (keyed by message_id for lookup)
                    if let Some(msg_id) = &email_message.message_id {
                        emails_with_attachments.push((msg_id.clone(), extracted_attachments));
                    }
                    emails.push(email_message);
                } else {
                    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent email {} is not a Nostr email, skipping", email_id);
                }
            } else {
                println!("[RUST] fetch_sent_emails_from_gmail_optimized: Failed to parse sent email {}", email_id);
            }
        } else {
            println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent message {} has no body", email_id);
        }
    }
    
    // Don't logout here - let the caller handle session cleanup
    // session.logout()?;
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Successfully processed {} sent Nostr emails with {} attachment sets", emails.len(), emails_with_attachments.len());
    Ok((emails, emails_with_attachments))
}

/// Extract attachments from a parsed email (in encrypted form as they appear in the email)
/// Recursively checks all subparts to find attachments
fn extract_attachments_from_parsed_email(email: &mailparse::ParsedMail, body_text: &str) -> Vec<crate::database::Attachment> {
    
    let mut attachments = Vec::new();
    
    // Check if this is a manifest-encrypted email
    // Check both the body text and try to get the full raw body
    let mut is_manifest_encrypted = body_text.contains("\"attachments\"") && 
                                   (body_text.contains("\"cipher_sha256\"") || body_text.contains("\"key_wrap\""));
    
    // Also check the raw email body if available
    if let Ok(raw_body) = email.get_body_raw() {
        let raw_body_str = String::from_utf8_lossy(&raw_body);
        if raw_body_str.contains("\"attachments\"") && 
           (raw_body_str.contains("\"cipher_sha256\"") || raw_body_str.contains("\"key_wrap\"")) {
            is_manifest_encrypted = true;
        }
    }
    
    println!("[RUST] extract_attachments_from_parsed_email: Checking email with {} subparts, manifest_encrypted: {}", email.subparts.len(), is_manifest_encrypted);
    
    // Recursively extract attachments from all subparts
    extract_attachments_recursive(email, &mut attachments, is_manifest_encrypted, 0);
    
    println!("[RUST] extract_attachments_from_parsed_email: Extracted {} total attachments", attachments.len());
    
    attachments
}

/// Recursively extract attachments from email parts
fn extract_attachments_recursive(
    part: &mailparse::ParsedMail,
    attachments: &mut Vec<crate::database::Attachment>,
    is_manifest_encrypted: bool,
    depth: usize
) {
    use base64::{Engine as _, engine::general_purpose};
    use chrono::Utc;
    
    let indent = "  ".repeat(depth);
    println!("{}[RUST] Checking part at depth {}: {} subparts", indent, depth, part.subparts.len());
    
    // Check Content-Type of this part
    let content_type = part.headers.get_first_value("Content-Type").unwrap_or_default();
    let content_disposition = part.headers.get_first_value("Content-Disposition").unwrap_or_default();
    
    println!("{}[RUST] Part Content-Type: {}, Content-Disposition: {}", indent, content_type, content_disposition);
    
    // Check if this part itself is an attachment
    let is_attachment = content_disposition.to_lowercase().contains("attachment") || 
                       content_disposition.to_lowercase().contains("filename");
    
    let is_multipart = content_type.to_lowercase().starts_with("multipart/");
    let is_text = content_type.to_lowercase().starts_with("text/");
    
    // If this is a multipart container, recurse into subparts
    if is_multipart {
        println!("{}[RUST] Part is multipart, recursing into {} subparts", indent, part.subparts.len());
        for (_idx, subpart) in part.subparts.iter().enumerate() {
            extract_attachments_recursive(subpart, attachments, is_manifest_encrypted, depth + 1);
        }
    } else if is_attachment || (!is_text && !content_type.is_empty()) {
        // This part is an attachment (has Content-Disposition: attachment or is non-text)
        println!("{}[RUST] Part looks like attachment: is_attachment={}, is_text={}, content_type={}", 
            indent, is_attachment, is_text, content_type);
        
        // Extract filename from Content-Disposition or Content-Type
        let filename = extract_filename_from_headers(&content_disposition, &content_type)
            .unwrap_or_else(|| format!("attachment_{}.dat", attachments.len()));
        
        // Get attachment data
        if let Ok(attachment_data) = part.get_body_raw() {
            println!("{}[RUST] Extracting attachment: {} ({} bytes)", indent, filename, attachment_data.len());
            
            // Encode as base64 for storage
            let data_base64 = general_purpose::STANDARD.encode(&attachment_data);
            
            let db_attachment = crate::database::Attachment {
                id: None,
                email_id: 0, // Will be set when saving
                filename: filename.clone(),
                content_type: content_type.clone(),
                data: data_base64,
                size: attachment_data.len(),
                is_encrypted: is_manifest_encrypted,
                encryption_method: if is_manifest_encrypted { Some("manifest_aes".to_string()) } else { None },
                algorithm: if is_manifest_encrypted { Some("AES-256".to_string()) } else { None },
                original_filename: None, // Will be extracted from manifest when decrypted
                original_type: None,
                original_size: None,
                created_at: Utc::now(),
            };
            
            attachments.push(db_attachment);
            println!("{}[RUST] Successfully extracted attachment: {} ({} bytes, encrypted: {})", 
                indent, filename, attachment_data.len(), is_manifest_encrypted);
        } else {
            println!("{}[RUST] Failed to get attachment data for {}", indent, filename);
        }
    } else {
        println!("{}[RUST] Part is not an attachment (text or empty), skipping", indent);
    }
}

/// Extract filename from Content-Disposition or Content-Type header
fn extract_filename_from_headers(content_disposition: &str, content_type: &str) -> Option<String> {
    // Try Content-Disposition first: filename="file.txt" or filename=file.txt
    if let Some(start) = content_disposition.find("filename=") {
        let after_filename = &content_disposition[start + 9..];
        let filename = if after_filename.starts_with('"') {
            // Quoted filename
            if let Some(end) = after_filename[1..].find('"') {
                Some(after_filename[1..end+1].to_string())
            } else {
                None
            }
        } else {
            // Unquoted filename
            let end = after_filename.find(';').unwrap_or(after_filename.len());
            Some(after_filename[..end].trim().to_string())
        };
        if filename.is_some() {
            return filename;
        }
    }
    
    // Try Content-Type: name="file.txt"
    if let Some(start) = content_type.find("name=") {
        let after_name = &content_type[start + 5..];
        if after_name.starts_with('"') {
            if let Some(end) = after_name[1..].find('"') {
                return Some(after_name[1..end+1].to_string());
            }
        } else {
            let end = after_name.find(';').unwrap_or(after_name.len());
            return Some(after_name[..end].trim().to_string());
        }
    }
    
    None
}

/// Extract Nostr public key from email headers
pub fn extract_nostr_pubkey_from_headers(raw_headers: &str) -> Option<String> {
    for line in raw_headers.lines() {
        if line.to_lowercase().starts_with("x-nostr-pubkey:") {
            return Some(line.split_once(':').unwrap_or(("", "")).1.trim().to_string());
        }
    }
    None
}

pub fn extract_nostr_sig_from_headers(raw_headers: &str) -> Option<String> {
    for line in raw_headers.lines() {
        if line.to_lowercase().starts_with("x-nostr-sig:") {
            return Some(line.split_once(':').unwrap_or(("", "")).1.trim().to_string());
        }
    }
    None
}

/// Normalize email body for signature verification
/// This ensures the body matches what was signed, handling:
/// - Line ending differences (\r\n vs \n)
/// - Trailing whitespace
fn normalize_body_for_verification(body: &str) -> String {
    // Replace \r\n with \n to normalize line endings
    let normalized = body.replace("\r\n", "\n");
    // Remove trailing whitespace from each line (but preserve structure)
    normalized.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Verify email signature
pub fn verify_email_signature(sender_pubkey: &str, signature: &str, body: &str) -> bool {
    // Normalize the body to match what was signed (handle line ending differences)
    let normalized_body = normalize_body_for_verification(body);
    println!("[RUST] verify_email_signature: Verifying signature for pubkey: {}, original body length: {}, normalized body length: {}, signature: {}", 
        sender_pubkey, body.len(), normalized_body.len(), signature);
    println!("[RUST] verify_email_signature: Original body first 100 chars: {}", body.chars().take(100).collect::<String>());
    println!("[RUST] verify_email_signature: Normalized body first 100 chars: {}", normalized_body.chars().take(100).collect::<String>());
    match crypto::verify_signature(sender_pubkey, signature, &normalized_body) {
        Ok(valid) => {
            println!("[RUST] verify_email_signature: Signature verification result: {}", valid);
            valid
        },
        Err(e) => {
            println!("[RUST] verify_email_signature: Error verifying signature: {}", e);
            false
        }
    }
}

/// Extract message ID from email headers
pub fn extract_message_id_from_headers(raw_headers: &str) -> Option<String> {
    for line in raw_headers.lines() {
        if line.to_lowercase().starts_with("message-id:") {
            return Some(line.split_once(':').unwrap_or(("", "")).1.trim().to_string());
        }
    }
    None
}

/// Extract domain from RFC5322 From: header
/// Handles formats like: "Name <email@domain.com>", "email@domain.com", etc.
fn extract_domain_from_email_address(from_header: &str) -> Option<String> {
    // Try to find email address in angle brackets first
    if let Some(start) = from_header.find('<') {
        if let Some(end) = from_header[start+1..].find('>') {
            let email = &from_header[start+1..start+1+end];
            if let Some(at_pos) = email.find('@') {
                return Some(email[at_pos+1..].trim().to_lowercase());
            }
        }
    }
    
    // Try to find @ symbol directly
    if let Some(at_pos) = from_header.find('@') {
        // Extract domain part after @
        let after_at = &from_header[at_pos+1..];
        // Find end of domain (space, comma, or end of string)
        let end = after_at.find(|c: char| c.is_whitespace() || c == ',' || c == '>')
            .unwrap_or(after_at.len());
        return Some(after_at[..end].trim().to_lowercase());
    }
    
    None
}

/// Get the last Authentication-Results header (trusted final MTA)
fn get_last_authentication_results_header(email: &mailparse::ParsedMail) -> Option<String> {
    // Get all Authentication-Results headers
    let mut auth_results_headers: Vec<String> = email.headers
        .get_all_values("Authentication-Results")
        .into_iter()
        .collect();
    
    // Return the last one (most recent/final MTA)
    auth_results_headers.pop()
}

/// Parsed authentication results from Authentication-Results header
#[derive(Debug, Clone)]
struct AuthResults {
    dmarc: Option<String>,  // "pass", "fail", "none", etc.
    dkim: Option<String>,   // "pass", "fail", "none", etc.
    dkim_domain: Option<String>, // The header.d domain from DKIM
    spf: Option<String>,    // "pass", "fail", "none", etc.
}

/// Parse Authentication-Results header value
fn parse_authentication_results(header_value: &str) -> AuthResults {
    let mut auth_results = AuthResults {
        dmarc: None,
        dkim: None,
        dkim_domain: None,
        spf: None,
    };
    
    // Authentication-Results format: authserv-id; method1=result1 reason1; method2=result2 reason2; ...
    // Example: "mail.example.com; dmarc=pass header.from=example.com; dkim=pass header.d=example.com; spf=pass smtp.mailfrom=example.com"
    
    // Split by semicolon to get individual results
    let parts: Vec<&str> = header_value.split(';').collect();
    
    for part in parts.iter().skip(1) { // Skip first part (authserv-id)
        let part = part.trim();
        
        // Check for DMARC
        if part.starts_with("dmarc=") {
            let rest = &part[6..].trim();
            // Extract result (before space or end)
            let result = rest.split_whitespace().next().unwrap_or("").to_lowercase();
            auth_results.dmarc = Some(result);
        }
        
        // Check for DKIM
        if part.starts_with("dkim=") {
            let rest = &part[5..].trim();
            // Extract result (before space or semicolon)
            let result = rest.split_whitespace().next().unwrap_or("").to_lowercase();
            auth_results.dkim = Some(result);
            
            // Look for header.d=domain in the same part
            if let Some(d_pos) = rest.find("header.d=") {
                let after_d = &rest[d_pos+9..];
                let domain = after_d.split_whitespace()
                    .next()
                    .unwrap_or("")
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_lowercase();
                if !domain.is_empty() {
                    auth_results.dkim_domain = Some(domain);
                }
            }
        }
        
        // Check for SPF
        if part.starts_with("spf=") {
            let rest = &part[4..].trim();
            // Extract result (before space or semicolon)
            let result = rest.split_whitespace().next().unwrap_or("").to_lowercase();
            auth_results.spf = Some(result);
        }
    }
    
    auth_results
}

/// Check DKIM alignment: header.from domain must match DKIM header.d domain
fn check_dkim_alignment(from_domain: &str, dkim_domain: &str) -> bool {
    from_domain.to_lowercase() == dkim_domain.to_lowercase()
}

/// Verify transport authentication (DMARC/DKIM/SPF) from RFC 5322 email
/// Accepts either raw RFC 5322 bytes or a parsed mailparse::ParsedMail struct
pub fn verify_transport_authentication(
    raw_bytes: Option<&[u8]>,
    parsed_email: Option<&mailparse::ParsedMail>
) -> Result<TransportAuthVerdict> {
    // Parse email if not already parsed - need to handle lifetime by parsing into owned value
    let parsed_owned: Option<mailparse::ParsedMail> = if parsed_email.is_some() {
        None
    } else if let Some(bytes) = raw_bytes {
        match parse_mail(bytes) {
            Ok(parsed) => Some(parsed),
            Err(e) => {
                return Ok(TransportAuthVerdict {
                    transport_verified: false,
                    method: TransportAuthMethod::None,
                    reason: format!("Failed to parse email: {}", e),
                });
            }
        }
    } else {
        return Ok(TransportAuthVerdict {
            transport_verified: false,
            method: TransportAuthMethod::None,
            reason: "No email data provided".to_string(),
        });
    };
    
    // Use parsed_email if provided, otherwise use parsed_owned
    let email = if let Some(parsed) = parsed_email {
        parsed
    } else if let Some(ref parsed) = parsed_owned {
        parsed
    } else {
        unreachable!()
    };
    
    // Extract RFC5322 From: domain
    let from_header = email.headers
        .get_first_value("From")
        .unwrap_or_else(|| "".to_string());
    
    let from_domain = match extract_domain_from_email_address(&from_header) {
        Some(domain) => domain,
        None => {
            return Ok(TransportAuthVerdict {
                transport_verified: false,
                method: TransportAuthMethod::None,
                reason: format!("Could not extract domain from From: header: {}", from_header),
            });
        }
    };
    
    // Find the last Authentication-Results header (trusted final MTA)
    let auth_results_header = match get_last_authentication_results_header(email) {
        Some(header) => header,
        None => {
            return Ok(TransportAuthVerdict {
                transport_verified: false,
                method: TransportAuthMethod::None,
                reason: "No Authentication-Results header found".to_string(),
            });
        }
    };
    
    // Parse Authentication-Results header
    let auth_results = parse_authentication_results(&auth_results_header);
    
    // Evaluate in priority order: DMARC > DKIM > SPF
    
    // 1. Check DMARC
    if let Some(ref dmarc_result) = auth_results.dmarc {
        if dmarc_result == "pass" {
            return Ok(TransportAuthVerdict {
                transport_verified: true,
                method: TransportAuthMethod::Dmarc,
                reason: format!("DMARC verification passed for domain {}", from_domain),
            });
        } else if dmarc_result == "fail" {
            return Ok(TransportAuthVerdict {
                transport_verified: false,
                method: TransportAuthMethod::Dmarc,
                reason: format!("DMARC verification failed for domain {}", from_domain),
            });
        }
    }
    
    // 2. Check DKIM (must pass AND have alignment)
    if let Some(ref dkim_result) = auth_results.dkim {
        if dkim_result == "pass" {
            // Check alignment
            if let Some(ref dkim_domain) = auth_results.dkim_domain {
                if check_dkim_alignment(&from_domain, dkim_domain) {
                    return Ok(TransportAuthVerdict {
                        transport_verified: true,
                        method: TransportAuthMethod::Dkim,
                        reason: format!("DKIM verification passed with alignment: header.from={}, header.d={}", from_domain, dkim_domain),
                    });
                } else {
                    return Ok(TransportAuthVerdict {
                        transport_verified: false,
                        method: TransportAuthMethod::Dkim,
                        reason: format!("DKIM verification passed but alignment failed: header.from={}, header.d={}", from_domain, dkim_domain),
                    });
                }
            } else {
                return Ok(TransportAuthVerdict {
                    transport_verified: false,
                    method: TransportAuthMethod::Dkim,
                    reason: "DKIM verification passed but no header.d domain found".to_string(),
                });
            }
        } else if dkim_result == "fail" {
            return Ok(TransportAuthVerdict {
                transport_verified: false,
                method: TransportAuthMethod::Dkim,
                reason: format!("DKIM verification failed for domain {}", from_domain),
            });
        }
    }
    
    // 3. Check SPF
    if let Some(ref spf_result) = auth_results.spf {
        if spf_result == "pass" {
            return Ok(TransportAuthVerdict {
                transport_verified: true,
                method: TransportAuthMethod::None, // SPF is not a separate method in our enum, use "none"
                reason: format!("SPF verification passed for domain {}", from_domain),
            });
        } else if spf_result == "fail" {
            return Ok(TransportAuthVerdict {
                transport_verified: false,
                method: TransportAuthMethod::None,
                reason: format!("SPF verification failed for domain {}", from_domain),
            });
        }
    }
    
    // No authentication method passed
    Ok(TransportAuthVerdict {
        transport_verified: false,
        method: TransportAuthMethod::None,
        reason: format!("No authentication method passed. DMARC: {:?}, DKIM: {:?}, SPF: {:?}", 
            auth_results.dmarc, auth_results.dkim, auth_results.spf),
    })
}

/// Decrypt email content if it's a Nostr encrypted email
/// For inbox emails: shared secret = user's private key × sender's public key
/// So we use sender's pubkey (from headers) for decryption
pub fn decrypt_nostr_email_content(config: &EmailConfig, raw_headers: &str, subject: &str, body: &str) -> Result<(String, String)> {
    // Check if we have a private key to decrypt with
    let private_key = match &config.private_key {
        Some(key) => key,
        None => {
            println!("[RUST] No private key available for decryption");
            return Ok((subject.to_string(), body.to_string()));
        }
    };
    
    // Extract the sender's public key from headers
    // For inbox emails: shared secret = user's private key × sender's public key
    let sender_pubkey = match extract_nostr_pubkey_from_headers(raw_headers) {
        Some(pubkey) => pubkey,
        None => {
            println!("[RUST] No X-Nostr-Pubkey header found");
            return Ok((subject.to_string(), body.to_string()));
        }
    };
    
    println!("[RUST] Attempting to decrypt inbox email using sender_pubkey (shared secret: user_privkey × sender_pubkey): {}", sender_pubkey);
    
    // Try to decrypt subject - encrypted subjects are typically just the raw encrypted content
    // without ASCII armor, and are usually base64 encoded
    let decrypted_subject = if is_likely_encrypted_content(subject) {
        match crypto::decrypt_message(private_key, &sender_pubkey, subject) {
            Ok(decrypted) => {
                println!("[RUST] Successfully decrypted subject");
                decrypted
            }
            Err(e) => {
                println!("[RUST] Failed to decrypt subject: {}", e);
                subject.to_string()
            }
        }
    } else {
        subject.to_string()
    };
    
    // Try to decrypt body
    let decrypted_body = if body.contains("BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE") {
        // Remove the ASCII armor if present
        let clean_body = body
            .replace("-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
            .replace("-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
            .trim()
            .to_string();
        match crypto::decrypt_message(private_key, &sender_pubkey, &clean_body) {
            Ok(decrypted) => {
                println!("[RUST] Successfully decrypted body: '{}'", decrypted); // <-- Print decrypted body
                decrypted
            }
            Err(e) => {
                println!("[RUST] Failed to decrypt body: {}", e);
                body.to_string()
            }
        }
    } else {
        body.to_string()
    };
    
    Ok((decrypted_subject, decrypted_body))
}

/// Check if content is likely encrypted (base64-like pattern, reasonable length)
pub fn is_likely_encrypted_content(content: &str) -> bool {
    // Skip empty or very short content
    if content.len() < 20 {
        return false;
    }
    
    // Check if it looks like base64 encoded content (typical for encrypted data)
    // Base64 contains A-Z, a-z, 0-9, +, /, and = for padding
    let base64_chars = content.chars().all(|c| {
        c.is_ascii_alphabetic() || c.is_ascii_digit() || c == '+' || c == '/' || c == '='
    });
    
    // Also check that it doesn't contain typical email subject patterns
    let has_email_patterns = content.contains('@') || 
                            content.contains("Re:") || 
                            content.contains("Fwd:") ||
                            content.contains("FW:") ||
                            content.contains("Subject:") ||
                            content.contains("From:") ||
                            content.contains("To:");
    
    base64_chars && !has_email_patterns
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_email_config_creation() {
        let config = EmailConfig {
            email_address: "test@example.com".to_string(),
            password: "password".to_string(),
            smtp_host: "smtp.gmail.com".to_string(),
            smtp_port: 587,
            imap_host: "imap.gmail.com".to_string(),
            imap_port: 993,
            use_tls: true,
        };
        
        assert_eq!(config.email_address, "test@example.com");
        assert_eq!(config.smtp_port, 587);
    }
} 

pub async fn fetch_nostr_emails_smart(config: &EmailConfig, db: &crate::database::Database) -> Result<Vec<EmailMessage>> {
    use chrono::Utc;
    use crate::email::extract_nostr_pubkey_from_headers;

    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    let is_gmail = host.contains("gmail.com");

    // 1. Get latest nostr email date from DB
    let latest = db.get_latest_nostr_email_received_at()?;
    
    // 2. Get sync cutoff setting (default to 365 days / 1 year)
    let sync_cutoff_days = Some(365);

    // 3. If None, fetch all Nostr emails from IMAP (no date filter)
    // 4. If Some(date), fetch Nostr emails from IMAP since that date
    let (email_msgs, _attachments_map) = if use_tls {
        let client = create_imap_tls_client!(host, &addr)?;
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            // For Gmail, pass latest timestamp directly (or None for all emails)
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest, sync_cutoff_days)?
        } else {
            // For non-Gmail, use SINCE in IMAP search
            let since_date = match latest {
                Some(dt) => dt.format("%d-%b-%Y").to_string(),
                None => "01-Jan-1970".to_string(),
            };
            let search_criteria = format!("ALL SINCE {}", since_date);
            let matching_messages = session.search(&search_criteria)?;
            let message_numbers: Vec<u32> = matching_messages.iter().cloned().collect();
            if message_numbers.is_empty() {
                return Ok(vec![]);
            }
            let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
            let mut emails = Vec::new();
            let mut _email_id = 0;
            for message in messages.iter() {
                _email_id += 1;
                if let Some(body) = message.body() {
                    if let Ok(email) = parse_mail(body) {
                        let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                        let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                        let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                let subject = decode_header_value(&subject_raw);
                        let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                        let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                        let body_text = if let Some(body_part) = email.subparts.first() {
                            if let Ok(body_content) = body_part.get_body() {
                                body_content
                            } else {
                                email.get_body().unwrap_or_else(|_| "No body content".to_string())
                            }
                        } else {
                            email.get_body().unwrap_or_else(|_| "No body content".to_string())
                        };
                        let raw_headers = email.headers.iter().map(|h| format!("{}: {}", h.get_key(), h.get_value())).collect::<Vec<_>>().join("\n");
                        // Only keep Nostr emails
                        if raw_headers.contains("X-Nostr-Pubkey:") {
                            let (_final_subject, _final_body) = match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                                Ok((dec_subject, dec_body)) => (dec_subject, dec_body),
                                Err(_) => (subject.clone(), body_text.clone()),
                            };
                            let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                            
                            // Verify signature if present - signature is created on the encrypted body, so verify against body_text
                            let signature_valid = if let Some(pubkey) = &sender_pubkey {
                                if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                                    Some(verify_email_signature(pubkey, &sig, &body_text))
                                } else {
                                    None // No signature present
                                }
                            } else {
                                None // No pubkey, can't verify
                            };
                            
                            let email_message = EmailMessage {
                                id: _email_id.to_string(),
                                from,
                                to,
                                subject,
                                body: _final_body.clone(),
                                raw_body: _final_body.clone(),
                                date,
                                is_read: true,
                                raw_headers: raw_headers.clone(),
                                sender_pubkey: sender_pubkey.clone(),
                                recipient_pubkey: None, // Inbox emails don't have recipient_pubkey
                                message_id: extract_message_id_from_headers(&raw_headers),
                                signature_valid,
                                transport_auth_verified: None, // Not verified in this path
                            };
                            emails.push(email_message);
                        }
                    }
                }
            }
            session.logout()?;
            (emails, vec![]) // Return empty attachments vector for non-Gmail
        }
    } else {
        // Not using TLS
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            // For Gmail, pass latest timestamp directly (or None for all emails)
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest, sync_cutoff_days)?
        } else {
            let since_date = match latest {
                Some(dt) => dt.format("%d-%b-%Y").to_string(),
                None => "01-Jan-1970".to_string(),
            };
            let search_criteria = format!("ALL SINCE {}", since_date);
            let matching_messages = session.search(&search_criteria)?;
            let message_numbers: Vec<u32> = matching_messages.iter().cloned().collect();
            if message_numbers.is_empty() {
                (vec![], vec![])
            } else {
                let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
                let mut emails = Vec::new();
                let mut _email_id = 0;
                for message in messages.iter() {
                    _email_id += 1;
                    if let Some(body) = message.body() {
                        if let Ok(email) = parse_mail(body) {
                            let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                            let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                            let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                    let subject = decode_header_value(&subject_raw);
                            let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                            let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now());
                            let body_text = if let Some(body_part) = email.subparts.first() {
                                if let Ok(body_content) = body_part.get_body() {
                                    body_content
                                } else {
                                    email.get_body().unwrap_or_else(|_| "No body content".to_string())
                                }
                            } else {
                                email.get_body().unwrap_or_else(|_| "No body content".to_string())
                            };
                            let raw_headers = email.headers.iter().map(|h| format!("{}: {}", h.get_key(), h.get_value())).collect::<Vec<_>>().join("\n");
                            if raw_headers.contains("X-Nostr-Pubkey:") {
                                let (_final_subject, _final_body) = match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                                    Ok((dec_subject, dec_body)) => (dec_subject, dec_body),
                                    Err(_) => (subject.clone(), body_text.clone()),
                                };
                                let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                                
                                // Verify signature if present
                                let signature_valid = if let Some(pubkey) = &sender_pubkey {
                                    if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                                        Some(verify_email_signature(pubkey, &sig, &_final_body))
                                    } else {
                                        None // No signature present
                                    }
                                } else {
                                    None // No pubkey, can't verify
                                };
                                
                                let email_message = EmailMessage {
                                    id: _email_id.to_string(),
                                    from,
                                    to,
                                    subject,
                                    body: _final_body.clone(),
                                    raw_body: _final_body.clone(),
                                    date,
                                    is_read: true,
                                    raw_headers: raw_headers.clone(),
                                    sender_pubkey: sender_pubkey.clone(),
                                    recipient_pubkey: None, // Inbox emails don't have recipient_pubkey
                                    message_id: extract_message_id_from_headers(&raw_headers),
                                    signature_valid,
                                    transport_auth_verified: None, // Not verified in this path
                                };
                                emails.push(email_message);
                            }
                        }
                    }
                }
                session.logout()?;
                (emails, vec![]) // Return empty attachments vector for non-Gmail
            }
        }
    };
    
    // Return just the emails (attachments are ignored in this function - they're handled in fetch_nostr_emails_smart_raw)
    Ok(email_msgs)
} 

pub async fn sync_nostr_emails_to_db(config: &EmailConfig, db: &Database) -> anyhow::Result<usize> {
    
    // Fetch latest nostr email date from DB
    let latest = db.get_latest_nostr_email_received_at()?;
    
    // Get sync cutoff setting from database (default to 365 days / 1 year)
    // Find pubkey(s) associated with this email address
    let sync_cutoff_days = match db.find_pubkeys_by_email_setting(&config.email_address) {
        Ok(pubkeys) => {
            // Try to get sync_cutoff_days from the first matching pubkey
            let mut cutoff = 365; // Default
            for pubkey in pubkeys {
                if let Ok(Some(value)) = db.get_setting(&pubkey, "sync_cutoff_days") {
                    if let Ok(parsed) = value.parse::<i64>() {
                        cutoff = parsed;
                        break; // Use first found setting
                    }
                }
            }
            cutoff
        }
        Err(_) => 365, // Default if we can't find pubkey
    };
    
    // Sync from most recent email onward - use latest directly (no cutoff window)
    // sync_cutoff_days is only used when latest is None (initial sync)
    println!("[RUST] sync_nostr_emails_to_db: Latest: {:?}, sync_cutoff_days: {} (only used if latest is None)", latest, sync_cutoff_days);
    
    // Get require_signature setting (default: true)
    let require_signature = match db.find_pubkeys_by_email_setting(&config.email_address) {
        Ok(pubkeys) => {
            let mut req_sig = true; // Default to true
            for pubkey in pubkeys {
                if let Ok(Some(value)) = db.get_setting(&pubkey, "require_signature") {
                    req_sig = value == "true";
                    break; // Use first found setting
                }
            }
            req_sig
        }
        Err(_) => true, // Default if we can't find pubkey
    };
    
    // Fetch new Nostr emails from IMAP inbox (raw, not decrypted)
    // Sync from most recent email onward
    let mut raw_nostr_emails = fetch_nostr_emails_smart_raw(config, latest, Some(sync_cutoff_days)).await?;
    
    // Also fetch from nostr-mail folder if it exists
    let mut nostr_folder_emails = fetch_nostr_emails_from_folder(config, latest, Some(sync_cutoff_days), "nostr-mail").await.unwrap_or_else(|e| {
        println!("[RUST] sync_nostr_emails_to_db: Could not fetch from nostr-mail folder (folder may not exist): {}", e);
        vec![]
    });
    
    // Combine emails from inbox and nostr-mail folder
    raw_nostr_emails.append(&mut nostr_folder_emails);
    
    // Filter emails based on transport authentication - always filter out unauthenticated emails
    raw_nostr_emails.retain(|email| {
        if let Some(false) = email.transport_auth_verified {
            println!("[RUST] sync_nostr_emails_to_db: Filtering out email {} - transport authentication failed", email.message_id);
            false
        } else {
            true
        }
    });
    
    // Filter emails based on signature requirement
    if require_signature {
        raw_nostr_emails.retain(|email| {
            // If email has sender_pubkey, it must have a valid signature
            if email.sender_pubkey.is_some() {
                // Email has pubkey, check signature
                if let Some(valid) = email.signature_valid {
                    valid // Only keep if signature is valid
                } else {
                    false // Reject emails without signature when require_signature is true
                }
            } else {
                // No pubkey header, allow (not a nostr email, but we're syncing nostr emails so this shouldn't happen)
                true
            }
        });
    }
    // If require_signature is false, accept all emails regardless of signature

    let mut new_count = 0;
    for email in raw_nostr_emails {
        // Check if already in DB by message_id
        let existing_email = match db.get_email(&email.message_id) {
            Ok(Some(existing)) => Some(existing),
            Ok(None) => None,
            Err(e) => {
                println!("[RUST] ERROR: Failed to check if email exists: {}", e);
                return Err(anyhow::anyhow!("Failed to check email {} in DB: {}", email.message_id, e));
            }
        };
        
        if let Some(existing_email) = existing_email {
            // Email already exists - update it with IMAP data (but preserve attachments)
            println!("[RUST] Email with message_id {} already exists (id: {:?}), updating with IMAP data (preserving attachments)", 
                email.message_id, existing_email.id);
            let updated_email = DbEmail {
                id: existing_email.id,
                message_id: existing_email.message_id.clone(),
                from_address: email.from.clone(),
                to_address: email.to.clone(),
                subject: email.subject.clone(), // still encrypted
                body: email.body.clone(),       // still encrypted
                body_plain: None,
                body_html: None,
                received_at: email.date,
                is_nostr_encrypted: true,
                sender_pubkey: email.sender_pubkey.clone(),
                recipient_pubkey: email.recipient_pubkey.clone(),
                raw_headers: Some(email.raw_headers.clone()),
                is_draft: false,
                is_read: existing_email.is_read, // Preserve read status
                updated_at: Some(chrono::Utc::now()),
                created_at: existing_email.created_at, // Preserve original creation date
                signature_valid: email.signature_valid,
                transport_auth_verified: email.transport_auth_verified,
            };
            db.save_email(&updated_email)?;
            println!("[RUST] Updated existing email in DB: message_id={}", email.message_id);
        } else {
            // Save raw email to DB
            // For inbox emails, sender_pubkey comes from headers (already extracted)
            let db_email = DbEmail {
                id: None,
                message_id: email.message_id.clone(),
                from_address: email.from.clone(),
                to_address: email.to.clone(),
                subject: email.subject.clone(), // still encrypted
                body: email.body.clone(),       // still encrypted
                body_plain: None,
                body_html: None,
                received_at: email.date,
                is_nostr_encrypted: true,
                sender_pubkey: email.sender_pubkey.clone(),
                recipient_pubkey: email.recipient_pubkey.clone(),
                raw_headers: Some(email.raw_headers.clone()),
                is_draft: false,
                is_read: false,
                updated_at: None,
                created_at: chrono::Utc::now(),
                signature_valid: email.signature_valid,
                transport_auth_verified: email.transport_auth_verified,
            };
            println!("[RUST] Saving email to DB: message_id={}", email.message_id);
            let email_id = db.save_email(&db_email)?;
            println!("[RUST] Saved email to DB: message_id={}, id={}", email.message_id, email_id);
            
            // Save attachments for this email
            println!("[RUST] sync_nostr_emails_to_db: Email {} has {} attachments in RawNostrEmail", email.message_id, email.attachments.len());
            if !email.attachments.is_empty() {
                println!("[RUST] Saving {} attachments for email {} (id: {})", email.attachments.len(), email.message_id, email_id);
                for mut attachment in email.attachments.iter().cloned() {
                    attachment.email_id = email_id;
                    println!("[RUST] Saving attachment: filename={}, size={}, encrypted={}, email_id={}",
                        attachment.filename, attachment.size, attachment.is_encrypted, email_id);
                    match db.save_attachment(&attachment) {
                        Ok(att_id) => {
                            println!("[RUST] Successfully saved attachment {} (id: {}) for email {}", attachment.filename, att_id, email_id);
                        }
                        Err(e) => {
                            println!("[RUST] ERROR: Failed to save attachment {}: {}", attachment.filename, e);
                        }
                    }
                }
            } else {
                println!("[RUST] sync_nostr_emails_to_db: Email {} has no attachments in RawNostrEmail, trying to extract from body", email.message_id);
                // Try to extract attachments by parsing the raw RFC822 email body
                if let Ok(parsed_email) = mailparse::parse_mail(email.body.as_bytes()) {
                    let extracted_attachments = extract_attachments_from_parsed_email(&parsed_email, &email.body);
                    if !extracted_attachments.is_empty() {
                        println!("[RUST] Extracted {} attachments from email body for email {}", extracted_attachments.len(), email_id);
                        for mut attachment in extracted_attachments {
                            attachment.email_id = email_id;
                            match db.save_attachment(&attachment) {
                                Ok(att_id) => {
                                    println!("[RUST] Saved extracted attachment {} (id: {}) for email {}", attachment.filename, att_id, email_id);
                                }
                                Err(e) => {
                                    println!("[RUST] ERROR: Failed to save extracted attachment {}: {}", attachment.filename, e);
                                }
                            }
                        }
                    }
                } else {
                    println!("[RUST] Could not parse email body to extract attachments for email {}", email_id);
                }
            }
            
            new_count += 1;
        }
    }
    println!("[RUST] sync_nostr_emails_to_db: Completed sync, {} new emails saved", new_count);
    Ok(new_count)
} 

pub async fn sync_sent_emails_to_db(config: &EmailConfig, db: &Database) -> anyhow::Result<usize> {
    
    println!("[RUST] sync_sent_emails_to_db: Starting sync for email: {}", config.email_address);
    // Fetch latest sent email date from DB using the user's email address
    let latest = db.get_latest_sent_email_received_at(Some(&config.email_address))?;
    println!("[RUST] sync_sent_emails_to_db: Latest sent email date: {:?}", latest);
    
    // Fetch new sent Nostr emails from IMAP (raw, not decrypted)
    // Sync from most recent email sent onward
    println!("[RUST] sync_sent_emails_to_db: Fetching emails from IMAP...");
    let raw_sent_emails = fetch_sent_emails_smart_raw(config, latest).await?;
    println!("[RUST] sync_sent_emails_to_db: Fetched {} emails from IMAP", raw_sent_emails.len());

    let mut new_count = 0;
    println!("[RUST] sync_sent_emails_to_db: Processing {} emails for saving", raw_sent_emails.len());
    for (idx, email) in raw_sent_emails.iter().enumerate() {
        println!("[RUST] sync_sent_emails_to_db: Processing email {} of {}: message_id={}, from={}, date={}", 
            idx + 1, raw_sent_emails.len(), email.message_id, email.from, email.date);
        // Skip emails that failed transport authentication
        if let Some(false) = email.transport_auth_verified {
            println!("[RUST] sync_nostr_emails_to_db: Skipping email {} - transport authentication failed", email.message_id);
            continue;
        }
        
        // Check if already in DB by message_id (only check, don't save yet)
        let existing_email = match db.get_email(&email.message_id) {
            Ok(Some(existing)) => Some(existing),
            Ok(None) => None,
            Err(e) => {
                println!("[RUST] ERROR: Failed to check if email exists: {}", e);
                return Err(anyhow::anyhow!("Failed to check email {} in DB: {}", email.message_id, e));
            }
        };
        
        if let Some(existing_email) = existing_email {
                // Email already exists - update it with IMAP data (but preserve attachments)
                // Only update fields that might have changed from IMAP, don't overwrite attachment data
                println!("[RUST] Email with message_id {} already exists (id: {:?}), updating with IMAP data (preserving attachments)", 
                    email.message_id, existing_email.id);
                let updated_email = DbEmail {
                    id: existing_email.id,
                    message_id: existing_email.message_id.clone(),
                    from_address: email.from.clone(),
                    to_address: email.to.clone(),
                    subject: email.subject.clone(), // Update with IMAP subject (might be more recent)
                    body: email.body.clone(),       // Update with IMAP body (might be more recent)
                    body_plain: existing_email.body_plain.clone(), // Preserve decrypted body if exists
                    body_html: existing_email.body_html.clone(),   // Preserve HTML if exists
                    received_at: email.date, // Update with IMAP date
                    is_nostr_encrypted: true,
                    sender_pubkey: email.sender_pubkey.clone(),
                    recipient_pubkey: email.recipient_pubkey.clone(),
                    raw_headers: Some(email.raw_headers.clone()), // Update with IMAP headers
                    is_draft: false,
                    is_read: existing_email.is_read, // Preserve read status
                    updated_at: Some(chrono::Utc::now()),
                    created_at: existing_email.created_at, // Preserve original creation time
                    signature_valid: email.signature_valid,
                    transport_auth_verified: email.transport_auth_verified,
                };
                match db.save_email(&updated_email) {
                    Ok(id) => println!("[RUST] Updated existing email with IMAP data, id: {}", id),
                    Err(e) => {
                        println!("[RUST] ERROR: Failed to update email {}: {}", email.message_id, e);
                        return Err(anyhow::anyhow!("Failed to update email {}: {}", email.message_id, e));
                    }
                }
        } else {
            // New email - save raw email to DB directly without checking again
            println!("[RUST] Email is new, inserting directly to DB (skipping redundant get_email check)");
            // For sent emails, try to find recipient_pubkey from contacts
            let recipient_pubkey = db.find_pubkeys_by_email(&email.to).ok()
                .and_then(|pubkeys| pubkeys.first().cloned());
            let db_email = DbEmail {
                id: None,
                message_id: email.message_id.clone(),
                from_address: email.from.clone(),
                to_address: email.to.clone(),
                subject: email.subject.clone(), // still encrypted
                body: email.body.clone(),       // still encrypted
                body_plain: None,
                body_html: None,
                received_at: email.date,
                is_nostr_encrypted: true,
                sender_pubkey: email.sender_pubkey.clone(),
                recipient_pubkey: recipient_pubkey.or(email.recipient_pubkey.clone()),
                raw_headers: Some(email.raw_headers.clone()),
                is_draft: false,
                is_read: false,
                updated_at: None,
                created_at: chrono::Utc::now(),
                signature_valid: email.signature_valid,
                transport_auth_verified: email.transport_auth_verified,
            };
            println!("[RUST] Inserting new sent email to DB: message_id={}, from={}, to={}, subject_len={}, body_len={}", 
                db_email.message_id, db_email.from_address, db_email.to_address, 
                db_email.subject.len(), db_email.body.len());
            let email_id = match db.insert_email_direct(&db_email) {
                Ok(id) => {
                    println!("[RUST] Successfully inserted new sent email to DB with id: {}", id);
                    new_count += 1;
                    id
                }
                Err(e) => {
                    println!("[RUST] ERROR: Failed to insert email to DB: {}", e);
                    return Err(anyhow::anyhow!("Failed to insert email {} to DB: {}", email.message_id, e));
                }
            };
            
            // Extract and save attachments from the email body
            // Parse the email body to extract attachments (they're in encrypted form)
            println!("[RUST] sync_sent_emails_to_db: Email {} has {} attachments in RawNostrEmail", email.message_id, email.attachments.len());
            if !email.attachments.is_empty() {
                println!("[RUST] Saving {} attachments for email {} (id: {})", email.attachments.len(), email.message_id, email_id);
                for mut attachment in email.attachments.iter().cloned() {
                    attachment.email_id = email_id;
                    println!("[RUST] Saving attachment: filename={}, size={}, encrypted={}, email_id={}", 
                        attachment.filename, attachment.size, attachment.is_encrypted, attachment.email_id);
                    match db.save_attachment(&attachment) {
                        Ok(att_id) => {
                            println!("[RUST] Successfully saved attachment {} (id: {}) for email {}", attachment.filename, att_id, email_id);
                        }
                        Err(e) => {
                            println!("[RUST] ERROR: Failed to save attachment {}: {}", attachment.filename, e);
                            // Don't fail the whole sync if attachment save fails
                        }
                    }
                }
            } else {
                println!("[RUST] sync_sent_emails_to_db: Email {} has no attachments in RawNostrEmail, trying to extract from body", email.message_id);
                // Try to extract attachments by parsing the raw RFC822 email body
                // The email.body might just be the text part, so we need to re-fetch the full email
                // For now, try parsing the body - if it's multipart, we can extract attachments
                // TODO: Store raw RFC822 body in RawNostrEmail for proper attachment extraction
                if let Ok(parsed_email) = mailparse::parse_mail(email.body.as_bytes()) {
                    let extracted_attachments = extract_attachments_from_parsed_email(&parsed_email, &email.body);
                    if !extracted_attachments.is_empty() {
                        println!("[RUST] Extracted {} attachments from email body for email {}", extracted_attachments.len(), email_id);
                        for mut attachment in extracted_attachments {
                            attachment.email_id = email_id;
                            match db.save_attachment(&attachment) {
                                Ok(att_id) => {
                                    println!("[RUST] Saved extracted attachment {} (id: {}) for email {}", attachment.filename, att_id, email_id);
                                }
                                Err(e) => {
                                    println!("[RUST] ERROR: Failed to save extracted attachment {}: {}", attachment.filename, e);
                                }
                            }
                        }
                    }
                } else {
                    // Body is not parseable as multipart - might need to re-fetch from IMAP
                    // For now, log and continue
                    println!("[RUST] Could not parse email body to extract attachments for email {}", email_id);
                }
            }
        }
    }
    println!("[RUST] sync_sent_emails_to_db: Completed sync, {} new emails saved", new_count);
    Ok(new_count)
} 

pub struct RawNostrEmail {
    pub message_id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub sender_pubkey: Option<String>,
    pub recipient_pubkey: Option<String>,
    pub raw_headers: String,
    pub attachments: Vec<crate::database::Attachment>, // Attachments extracted from email (in encrypted form)
    pub signature_valid: Option<bool>,
    pub transport_auth_verified: Option<bool>,
}

/// Fetch Nostr emails from a specific IMAP folder
async fn fetch_nostr_emails_from_folder(config: &EmailConfig, latest: Option<chrono::DateTime<chrono::Utc>>, sync_cutoff_days: Option<i64>, folder_name: &str) -> anyhow::Result<Vec<RawNostrEmail>> {
    use chrono::Utc;
    use mailparse::parse_mail;
    use crate::email::extract_nostr_pubkey_from_headers;

    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    let is_gmail = host.contains("gmail.com");

    if !use_tls {
        return Err(anyhow::anyhow!("TLS is required for IMAP connections"));
    }

    let client = create_imap_tls_client!(host, &addr)?;
    let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
    
    // Try to select the folder - if it doesn't exist, return empty vec
    if session.select(folder_name).is_err() {
        println!("[RUST] fetch_nostr_emails_from_folder: Folder '{}' does not exist, skipping", folder_name);
        return Ok(vec![]);
    }
    
    println!("[RUST] fetch_nostr_emails_from_folder: Fetching from folder '{}'", folder_name);
    
    if is_gmail {
        // For Gmail, use optimized search with date filtering
        let cutoff = if let Some(latest_date) = latest {
            latest_date
        } else {
            let cutoff_days = sync_cutoff_days.unwrap_or(365);
            if cutoff_days <= 0 {
                Utc::now() - chrono::Duration::days(365 * 100)
            } else {
                Utc::now() - chrono::Duration::days(cutoff_days)
            }
        };
        
        let date_filter = if latest.is_some() {
            let unix_timestamp = cutoff.timestamp() - 3600; // 1 hour buffer
            format!(" AND after:{}", unix_timestamp)
        } else {
            String::new()
        };
        
        let search_terms = vec![
            format!("X-GM-RAW \"BEGIN NOSTR NIP-{}\"", date_filter),
            format!("X-GM-RAW \"END NOSTR NIP-{}\"", date_filter),
        ];
        
        let mut all_emails = Vec::new();
        for search_term in search_terms {
            let matching_messages = session.search(&search_term)?;
            if matching_messages.is_empty() {
                continue;
            }
            let messages = session.fetch(matching_messages.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
            
            for message in messages.iter() {
                if let Some(body) = message.body() {
                    if let Ok(email) = parse_mail(body) {
                        let raw_headers = email.headers.iter()
                            .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
                            .collect::<Vec<_>>()
                            .join("\n");
                        
                        // Only process emails with X-Nostr-Pubkey header
                        if !raw_headers.contains("X-Nostr-Pubkey:") {
                            continue;
                        }
                        
                        let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                        let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                        let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                        let subject = decode_header_value(&subject_raw);
                        let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                        let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                        let body_text = if let Some(body_part) = email.subparts.first() {
                            if let Ok(body_content) = body_part.get_body() {
                                body_content
                            } else {
                                email.get_body().unwrap_or_else(|_| "No body content".to_string())
                            }
                        } else {
                            email.get_body().unwrap_or_else(|_| "No body content".to_string())
                        };
                        
                        let extracted_attachments = extract_attachments_from_parsed_email(&email, &body_text);
                        let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                        
                        // Debug: Log body content for signature verification
                        println!("[RUST] fetch_nostr_emails_from_folder: Body text length: {}, first 100 chars: {}, last 100 chars: {}", 
                            body_text.len(), 
                            body_text.chars().take(100).collect::<String>(),
                            body_text.chars().rev().take(100).collect::<String>().chars().rev().collect::<String>());
                        println!("[RUST] fetch_nostr_emails_from_folder: Body contains \\n: {}, contains \\r\\n: {}", 
                            body_text.contains('\n'), body_text.contains("\r\n"));
                        
                        let signature_valid = if let Some(pubkey) = &sender_pubkey {
                            if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                                println!("[RUST] fetch_nostr_emails_from_folder: Found signature, verifying...");
                                Some(verify_email_signature(pubkey, &sig, &body_text))
                            } else {
                                println!("[RUST] fetch_nostr_emails_from_folder: No signature found in headers");
                                None
                            }
                        } else {
                            println!("[RUST] fetch_nostr_emails_from_folder: No sender pubkey found");
                            None
                        };
                        
                        let message_id = extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| Uuid::new_v4().to_string());
                        
                        // Verify transport authentication
                        let transport_auth = verify_transport_authentication(Some(body), Some(&email))
                            .unwrap_or_else(|e| TransportAuthVerdict {
                                transport_verified: false,
                                method: TransportAuthMethod::None,
                                reason: format!("Error verifying transport auth: {}", e),
                            });
                        
                        // Skip emails that fail transport authentication
                        if !transport_auth.transport_verified {
                            println!("[RUST] fetch_nostr_emails_from_folder: Email {} failed transport authentication: {}", message_id, transport_auth.reason);
                            continue;
                        }
                        
                        all_emails.push(RawNostrEmail {
                            message_id,
                            from,
                            to,
                            subject,
                            body: body_text,
                            date,
                            sender_pubkey: sender_pubkey.clone(),
                            recipient_pubkey: None,
                            raw_headers,
                            attachments: extracted_attachments,
                            signature_valid,
                            transport_auth_verified: Some(transport_auth.transport_verified),
                        });
                    }
                }
            }
        }
        Ok(all_emails)
    } else {
        // For non-Gmail, use standard IMAP search
        let since_date = match latest {
            Some(dt) => dt.format("%d-%b-%Y").to_string(),
            None => {
                let cutoff_days = sync_cutoff_days.unwrap_or(365);
                if cutoff_days <= 0 {
                    "01-Jan-1970".to_string()
                } else {
                    (Utc::now() - chrono::Duration::days(cutoff_days)).format("%d-%b-%Y").to_string()
                }
            }
        };
        
        let search_criteria = format!("ALL SINCE {}", since_date);
        let matching_messages = session.search(&search_criteria)?;
        let message_numbers: Vec<u32> = matching_messages.iter().cloned().collect();
        if message_numbers.is_empty() {
            return Ok(vec![]);
        }
        
        let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
        let mut emails = Vec::new();
        
        for message in messages.iter() {
            if let Some(body) = message.body() {
                if let Ok(email) = parse_mail(body) {
                    let raw_headers = email.headers.iter()
                        .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    
                    // Only process emails with X-Nostr-Pubkey header
                    if !raw_headers.contains("X-Nostr-Pubkey:") {
                        continue;
                    }
                    
                    let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                    let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                    let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                    let subject = decode_header_value(&subject_raw);
                    let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                    let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    let body_text = if let Some(body_part) = email.subparts.first() {
                        if let Ok(body_content) = body_part.get_body() {
                            body_content
                        } else {
                            email.get_body().unwrap_or_else(|_| "No body content".to_string())
                        }
                    } else {
                        email.get_body().unwrap_or_else(|_| "No body content".to_string())
                    };
                    
                    let extracted_attachments = extract_attachments_from_parsed_email(&email, &body_text);
                    let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                    
                    let signature_valid = if let Some(pubkey) = &sender_pubkey {
                        if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                            Some(verify_email_signature(pubkey, &sig, &body_text))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    
                    let message_id = extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| Uuid::new_v4().to_string());
                    
                    // Verify transport authentication
                    let transport_auth = verify_transport_authentication(Some(body), Some(&email))
                        .unwrap_or_else(|e| TransportAuthVerdict {
                            transport_verified: false,
                            method: TransportAuthMethod::None,
                            reason: format!("Error verifying transport auth: {}", e),
                        });
                    
                    // Skip emails that fail transport authentication
                    if !transport_auth.transport_verified {
                        println!("[RUST] fetch_nostr_emails_from_folder (non-Gmail): Email {} failed transport authentication: {}", message_id, transport_auth.reason);
                        continue;
                    }
                    
                    emails.push(RawNostrEmail {
                        message_id,
                        from,
                        to,
                        subject,
                        body: body_text,
                        date,
                        sender_pubkey: sender_pubkey.clone(),
                        recipient_pubkey: None,
                        raw_headers,
                        attachments: extracted_attachments,
                        signature_valid,
                        transport_auth_verified: Some(transport_auth.transport_verified),
                    });
                }
            }
        }
        Ok(emails)
    }
}

async fn fetch_nostr_emails_smart_raw(config: &EmailConfig, latest: Option<chrono::DateTime<chrono::Utc>>, sync_cutoff_days: Option<i64>) -> anyhow::Result<Vec<RawNostrEmail>> {
    use chrono::Utc;
    use mailparse::parse_mail;
    use crate::email::extract_nostr_pubkey_from_headers;

    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    let is_gmail = host.contains("gmail.com");

    let emails = if use_tls {
        let client = create_imap_tls_client!(host, &addr)?;
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            // fetch_nostr_emails_from_gmail_optimized returns emails and attachments, convert to Vec<RawNostrEmail>
            let (email_msgs, attachments_map) = fetch_nostr_emails_from_gmail_optimized(&mut session, config, latest, sync_cutoff_days)?;
            
            // Create a HashMap for quick lookup of attachments by message_id
            let attachments_by_msg_id: std::collections::HashMap<String, Vec<crate::database::Attachment>> = attachments_map.into_iter().collect();
            
            email_msgs.into_iter().map(|em| {
                let message_id = em.message_id.unwrap_or_else(|| em.id.clone());
                let attachments = attachments_by_msg_id.get(&message_id).cloned().unwrap_or_default();
                println!("[RUST] fetch_nostr_emails_smart_raw: Email {} has {} attachments", message_id, attachments.len());
                RawNostrEmail {
                    message_id,
                    from: em.from,
                    to: em.to,
                    subject: em.subject,
                    body: em.body,
                    date: em.date,
                    sender_pubkey: em.sender_pubkey.clone(),
                    recipient_pubkey: None, // Inbox emails don't have recipient_pubkey
                    raw_headers: em.raw_headers,
                    attachments,
                    signature_valid: em.signature_valid,
                    transport_auth_verified: em.transport_auth_verified,
                }
            }).collect()
        } else {
            let since_date = match latest {
                Some(dt) => dt.format("%d-%b-%Y").to_string(),
                None => "01-Jan-1970".to_string(),
            };
            let search_criteria = if latest.is_some() {
                format!("ALL SINCE {}", since_date)
            } else {
                "ALL".to_string()
            };
            let matching_messages = session.search(&search_criteria)?;
            let message_numbers: Vec<u32> = matching_messages.iter().cloned().collect();
            if message_numbers.is_empty() {
                return Ok(vec![]);
            }
            let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
            let mut emails = Vec::new();
            for message in messages.iter() {
                if let Some(body) = message.body() {
                    if let Ok(email) = parse_mail(body) {
                        let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                        let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                        let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                let subject = decode_header_value(&subject_raw);
                        let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                        let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                        let body_text = if let Some(body_part) = email.subparts.first() {
                            if let Ok(body_content) = body_part.get_body() {
                                body_content
                            } else {
                                email.get_body().unwrap_or_else(|_| "No body content".to_string())
                            }
                        } else {
                            email.get_body().unwrap_or_else(|_| "No body content".to_string())
                        };
                        let raw_headers = email.headers.iter()
                            .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
                            .collect::<Vec<_>>()
                            .join("\n");
                        
                        // Extract attachments from the parsed email (in encrypted form)
                        let extracted_attachments = extract_attachments_from_parsed_email(&email, &body_text);
                        println!("[RUST] fetch_sent_emails_smart_raw: Extracted {} attachments from email", extracted_attachments.len());
                        
                        let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                        
                        // Verify signature if present (verify on raw body)
                        let signature_valid = if let Some(pubkey) = &sender_pubkey {
                            if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                                Some(verify_email_signature(pubkey, &sig, &body_text))
                            } else {
                                None // No signature present
                            }
                        } else {
                            None // No pubkey, can't verify
                        };
                        
                        let email_message = RawNostrEmail {
                            message_id: extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| Uuid::new_v4().to_string()),
                            from,
                            to,
                            subject,
                            body: body_text,
                            date,
                            sender_pubkey: sender_pubkey.clone(),
                            recipient_pubkey: None, // Will be populated during sync if contact exists
                            raw_headers,
                            attachments: extracted_attachments,
                            signature_valid,
                            transport_auth_verified: None, // Not verified in this path
                        };
                        emails.push(email_message);
                    }
                }
            }
            emails
        }
    } else {
        return Err(anyhow::anyhow!("TLS is required for IMAP connections"));
    };

    Ok(emails)
}

async fn fetch_sent_emails_smart_raw(config: &EmailConfig, latest: Option<chrono::DateTime<chrono::Utc>>) -> anyhow::Result<Vec<RawNostrEmail>> {
    use chrono::Utc;
    use mailparse::parse_mail;
    use crate::email::extract_nostr_pubkey_from_headers;

    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    let is_gmail = host.contains("gmail.com");

    return if use_tls {
        let client = create_imap_tls_client!(host, &addr)?;
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        
        // Select the sent folder instead of INBOX
        let sent_folder = if is_gmail {
            "[Gmail]/Sent Mail" // Gmail's sent folder
        } else {
            "Sent" // Generic sent folder
        };
        
        println!("[RUST] fetch_sent_emails_smart_raw: Selecting sent folder: {}", sent_folder);
        
        // Try to select the sent folder, fallback to common variations
        let folder_selected = session.select(sent_folder).is_ok() || 
                             session.select("Sent Mail").is_ok() || 
                             session.select("Sent Items").is_ok() ||
                             session.select("Sent").is_ok();
        
        if !folder_selected {
            println!("[RUST] fetch_sent_emails_smart_raw: Could not select any sent folder, trying INBOX");
            session.select("INBOX")?;
        }
        
        let emails_result: anyhow::Result<Vec<RawNostrEmail>> = if is_gmail {
            // Pass latest directly to the optimized function
            // fetch_sent_emails_from_gmail_optimized now returns emails and attachments
            let (email_msgs, attachments_map) = fetch_sent_emails_from_gmail_optimized(&mut session, config, latest)?;
            
            // Create a HashMap for quick lookup of attachments by message_id
            let attachments_by_msg_id: std::collections::HashMap<String, Vec<crate::database::Attachment>> = attachments_map.into_iter().collect();
            
            // Convert EmailMessage to RawNostrEmail with attachments
            let mut raw_emails = Vec::new();
            for em in email_msgs {
                if let Some(msg_id) = &em.message_id {
                    let attachments = attachments_by_msg_id.get(msg_id).cloned().unwrap_or_default();
                    println!("[RUST] fetch_sent_emails_smart_raw: Email {} has {} attachments from attachments_by_msg_id", msg_id, attachments.len());
                    if !attachments.is_empty() {
                        for att in &attachments {
                            println!("[RUST] fetch_sent_emails_smart_raw: Attachment: filename={}, size={}, encrypted={}", 
                                att.filename, att.size, att.is_encrypted);
                        }
                    }
                    // For sent emails, extract sender_pubkey from headers (it's included when we send)
                    // and try to find recipient_pubkey from contacts
                    let sender_pubkey = extract_nostr_pubkey_from_headers(&em.raw_headers);
                    let recipient_pubkey = None; // Will be populated during sync if contact exists
                    raw_emails.push(RawNostrEmail {
                        message_id: msg_id.clone(),
                        from: em.from,
                        to: em.to,
                        subject: em.subject,
                        body: em.body,
                        date: em.date,
                        sender_pubkey: sender_pubkey,
                        recipient_pubkey: recipient_pubkey,
                        raw_headers: em.raw_headers,
                        attachments,
                        signature_valid: em.signature_valid,
                        transport_auth_verified: em.transport_auth_verified, // For sent emails, this will be None
                    });
                }
            }
            Ok(raw_emails)
        } else {
            let since_date = match latest {
                Some(dt) => dt.format("%d-%b-%Y").to_string(),
                None => "01-Jan-1970".to_string(),
            };
            let search_criteria = if latest.is_some() {
                format!("ALL SINCE {}", since_date)
            } else {
                "ALL".to_string()
            };
            let matching_messages = session.search(&search_criteria)?;
            let message_numbers: Vec<u32> = matching_messages.iter().cloned().collect();
            if message_numbers.is_empty() {
                Ok(vec![])
            } else {
                let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
                let mut emails = Vec::new();
                for message in messages.iter() {
                    if let Some(body) = message.body() {
                        if let Ok(email) = parse_mail(body) {
                            let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                            let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                            let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
                let subject = decode_header_value(&subject_raw);
                            let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
                            let date = chrono::DateTime::parse_from_rfc2822(&date_str)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now());
                            let body_text = if let Some(body_part) = email.subparts.first() {
                                if let Ok(body_content) = body_part.get_body() {
                                    body_content
                                } else {
                                    email.get_body().unwrap_or_else(|_| "No body content".to_string())
                                }
                            } else {
                                email.get_body().unwrap_or_else(|_| "No body content".to_string())
                            };
                            let raw_headers = email.headers.iter()
                                .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
                                .collect::<Vec<_>>()
                                .join("\n");
                            let sender_pubkey = extract_nostr_pubkey_from_headers(&raw_headers);
                            
                            // Verify signature if present (verify on raw body)
                            let signature_valid = if let Some(pubkey) = &sender_pubkey {
                                if let Some(sig) = extract_nostr_sig_from_headers(&raw_headers) {
                                    Some(verify_email_signature(pubkey, &sig, &body_text))
                                } else {
                                    None // No signature present
                                }
                            } else {
                                None // No pubkey, can't verify
                            };
                            
                            let email_message = RawNostrEmail {
                                message_id: extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| format!("msg_{}", chrono::Utc::now().timestamp())),
                                from,
                                to,
                                subject,
                                body: body_text,
                                date,
                                sender_pubkey: sender_pubkey.clone(),
                                recipient_pubkey: None, // Will be populated during sync if contact exists
                                raw_headers,
                                attachments: vec![], // Will be extracted during sync
                                signature_valid,
                                transport_auth_verified: None, // Not verified in this path
                            };
                            emails.push(email_message);
                        }
                    }
                }
                Ok(emails)
            }
        };
        
        // Always logout the session, even if there was an error
        let _ = session.logout();
        println!("[RUST] fetch_sent_emails_smart_raw: Session closed");
        
        emails_result
    } else {
        Err(anyhow::anyhow!("TLS is required for IMAP connections"))
    };
} 