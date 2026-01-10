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
use chrono::Utc;
use tokio::task;
use tokio::time::timeout;
use std::time::Duration;
use crate::types::EmailMessage;
use std::net::TcpStream;
use native_tls::TlsConnector;
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose};

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
    
    let result = timeout(Duration::from_secs(60), send_future).await;
    
    match result {
        Ok(Ok(Ok(_))) => {
            println!("[RUST] send_email: Email sent successfully");
            Ok(format!("Email sent successfully to {}", to_address))
        },
        Ok(Ok(Err(e))) => {
            println!("[RUST] send_email: Failed to send email: {}", e);
            // Provide more helpful error messages for common issues
            let error_msg = if e.to_string().to_lowercase().contains("authentication") {
                "Authentication failed. For Gmail, make sure you're using an App Password, not your regular password."
            } else if e.to_string().to_lowercase().contains("connection") || e.to_string().to_lowercase().contains("host") {
                "SMTP client error. Check your SMTP host and port settings."
            } else if e.is_transient() {
                "Temporary SMTP error. Please try again."
            } else if e.is_permanent() {
                "Permanent SMTP error. Check your email configuration."
            } else {
                &format!("SMTP error: {}", e)
            };
            Err(anyhow::anyhow!("Failed to send email: {}", error_msg))
        },
        Ok(Err(e)) => {
            println!("[RUST] send_email: Task join error: {}", e);
            Err(anyhow::anyhow!("Task join error: {}", e))
        },
        Err(_) => {
            println!("[RUST] send_email: SMTP send operation timed out after 60 seconds");
            Err(anyhow::anyhow!("SMTP send operation timed out after 60 seconds. Check your internet connection and SMTP settings."))
        },
    }
}

/// Delete a sent email from the IMAP server by moving it to Trash
/// For Gmail, moves to [Gmail]/Trash
/// For other providers, tries common trash folder names
pub async fn delete_sent_email_from_server(config: &EmailConfig, message_id: &str) -> Result<()> {
    use std::net::TcpStream;
    use native_tls::TlsConnector;
    
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
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
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
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
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

pub async fn fetch_emails(config: &EmailConfig, limit: usize, search_query: Option<String>, only_nostr: bool) -> Result<Vec<EmailMessage>> {
    println!("[RUST] fetch_emails: Starting to fetch emails with limit: {}, search: {:?}, only_nostr: {}", limit, search_query, only_nostr);
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
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if use_gmail_optimization {
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, chrono::Duration::hours(24))?
        } else {
            fetch_emails_from_session(&mut session, config, limit, search_query, only_nostr)?
        }
    } else {
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if use_gmail_optimization {
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, chrono::Duration::hours(24))?
        } else {
            fetch_emails_from_session(&mut session, config, limit, search_query, only_nostr)?
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

fn fetch_emails_from_session(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, limit: usize, search_query: Option<String>, _only_nostr: bool) -> Result<Vec<EmailMessage>> {
    println!("[RUST] fetch_emails_from_session: Successfully connected to IMAP server");
    session.select("INBOX")?;
    println!("[RUST] fetch_emails_from_session: Selected INBOX");
    let seven_days_ago = chrono::Utc::now() - chrono::Duration::days(7);
    let since_date = seven_days_ago.format("%d-%b-%Y").to_string();
    println!("[RUST] fetch_emails_from_session: Using SINCE filter for date: {}", since_date);
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

    for message in messages.iter() {
        email_id += 1;
        println!("[RUST] Processing message {}", email_id);
        
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
                    nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                    message_id: extract_message_id_from_headers(&raw_headers),
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

fn fetch_emails_from_session_last_24h(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig) -> Result<Vec<EmailMessage>> {
    use chrono::{Duration, Utc};
    session.select("INBOX")?;
    let since_date = (Utc::now() - Duration::hours(24)).format("%d-%b-%Y").to_string();
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
    let cutoff = Utc::now() - Duration::hours(24);
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
                // Only keep emails from the last 24 hours
                if date < cutoff {
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
                    nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                    message_id: extract_message_id_from_headers(&raw_headers),
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
    
    let result = timeout(Duration::from_secs(30), test_future).await;
    
    match result {
        Ok(Ok(Ok(_))) => {
            println!("[RUST] test_smtp_connection: SMTP connection test successful");
            Ok(())
        },
        Ok(Ok(Err(e))) => {
            println!("[RUST] test_smtp_connection: SMTP connection test failed: {}", e);
            // Provide more helpful error messages for common issues
            let error_msg = if e.to_string().to_lowercase().contains("authentication") {
                "Authentication failed. For Gmail, make sure you're using an App Password, not your regular password."
            } else if e.to_string().to_lowercase().contains("connection") || e.to_string().to_lowercase().contains("host") {
                "SMTP client error. Check your SMTP host and port settings."
            } else if e.is_transient() {
                "Temporary SMTP error. Please try again."
            } else if e.is_permanent() {
                "Permanent SMTP error. Check your email configuration."
            } else {
                &format!("SMTP connection error: {}", e)
            };
            Err(anyhow::anyhow!("SMTP connection failed: {}", error_msg))
        },
        Ok(Err(e)) => {
            println!("[RUST] test_smtp_connection: Task join error: {}", e);
            Err(anyhow::anyhow!("SMTP connection join error: {}", e))
        },
        Err(_) => {
            println!("[RUST] test_smtp_connection: SMTP connection test timed out after 30 seconds");
            Err(anyhow::anyhow!("SMTP connection test timed out after 30 seconds. Check your internet connection and SMTP settings."))
        },
    }
}

/// Fetch up to 100 emails from the last 24 hours that have the X-Nostr-Pubkey header
pub async fn fetch_nostr_emails_last_24h(config: &EmailConfig) -> Result<Vec<EmailMessage>> {
    use chrono::Duration;
    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;
    let addr = format!("{}:{}", host, port);
    println!("[RUST] fetch_nostr_emails_last_24h: Connecting to IMAP server: {}", addr);
    
    // Check if this is Gmail to use optimized search
    let is_gmail = host.contains("gmail.com");
    
    let emails = if use_tls {
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, Duration::hours(24))?
        } else {
            fetch_emails_from_session_last_24h(&mut session, config)?
        }
    } else {
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, Duration::hours(24))?
        } else {
            fetch_emails_from_session_last_24h(&mut session, config)?
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
        let fallback_emails = if use_tls {
            let tls = TlsConnector::builder().build()?;
            let tcp_stream = TcpStream::connect(&addr)?;
            let tls_stream = tls.connect(host, tcp_stream)?;
            let client = imap::Client::new(tls_stream);
            let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, Duration::days(7))?
        } else {
            let tcp_stream = TcpStream::connect(&addr)?;
            let client = imap::Client::new(tcp_stream);
            let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, Duration::days(7))?
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
fn fetch_nostr_emails_from_gmail_optimized(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, time_window: chrono::Duration) -> Result<Vec<EmailMessage>> {
    use chrono::Utc;
    
    let time_window_str = if time_window == chrono::Duration::hours(24) { "24h" } else { "7 days" };
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Using Gmail's X-GM-RAW search for last {}", time_window_str);
    session.select("INBOX")?;
    
    // Use more specific search terms to avoid false positives
    let search_terms = vec![
        "X-GM-RAW \"BEGIN NOSTR NIP-\"",
        "X-GM-RAW \"X-Nostr-Pubkey:\"",
        "X-GM-RAW \"END NOSTR NIP-\"",
    ];
    
    let mut all_message_numbers: HashSet<u32> = HashSet::new();
    
    // Search with each term and collect results
    for search_term in search_terms {
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
    
    let message_numbers: Vec<u32> = all_message_numbers.into_iter().collect();
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Total unique messages to fetch: {}", message_numbers.len());
    
    if message_numbers.is_empty() {
        println!("[RUST] fetch_nostr_emails_from_gmail_optimized: No messages found, returning empty result");
        return Ok(vec![]);
    }
    
    // Fetch all matching messages
    let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Successfully fetched {} message objects", messages.len());
    
    let mut emails = Vec::new();
    let mut email_id = 0;
    
    // Filter for emails within the specified time window
    let cutoff = Utc::now() - time_window;
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Filtering for emails after: {}", cutoff);
    
    for message in messages.iter() {
        email_id += 1;
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
                
                // Only keep emails within the time window
                if date < cutoff {
                    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} is too old ({} < {}), skipping", 
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
                let has_encrypted_content = body_text.contains("BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE");
                let has_end_marker = body_text.contains("END NOSTR NIP-04 ENCRYPTED MESSAGE");
                
                println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} - Has Nostr header: {}, Has encrypted content: {}, Has end marker: {}", 
                    email_id, has_nostr_header, has_encrypted_content, has_end_marker);
                
                // More lenient verification: accept if it has either the header or encrypted content
                let is_nostr_email = has_nostr_header || has_encrypted_content;
                
                if is_nostr_email {
                    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} is confirmed as Nostr email", email_id);
                    
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
                        nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                        message_id: extract_message_id_from_headers(&raw_headers),
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
    println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Successfully processed {} Nostr emails", emails.len());
    Ok(emails)
}

fn fetch_sent_emails_from_gmail_optimized(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, time_window: chrono::Duration) -> Result<Vec<EmailMessage>> {
    use chrono::Utc;
    
    let time_window_str = if time_window == chrono::Duration::hours(24) { "24h" } else { "7 days" };
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Using Gmail's X-GM-RAW search for sent emails in last {}", time_window_str);
    
    // Try to select the sent folder
    let sent_folder = "[Gmail]/Sent Mail";
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Selecting sent folder: {}", sent_folder);
    session.select(sent_folder)?;
    
    // Calculate cutoff date for filtering
    let cutoff = Utc::now() - time_window;
    let cutoff_date_str = cutoff.format("%Y/%m/%d").to_string();
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Filtering for emails after: {} ({})", cutoff, cutoff_date_str);
    
    // Use Gmail's date filtering in X-GM-RAW search to reduce server load
    // Gmail supports "after:YYYY/MM/DD" syntax in X-GM-RAW searches
    // Combine with AND to filter by both content and date
    // Format: X-GM-RAW "content search AND after:YYYY/MM/DD"
    let date_filter = if time_window < chrono::Duration::days(365 * 5) {
        // Only add date filter if we're not doing a full sync (5 years)
        format!(" AND after:{}", cutoff_date_str)
    } else {
        String::new()
    };
    
    // Use more specific search terms with date filtering to avoid fetching old emails
    // Gmail X-GM-RAW syntax: "search term AND after:date"
    // Note: Gmail searches body content by default, so "BEGIN NOSTR NIP-" should work
    let search_terms = vec![
        format!("X-GM-RAW \"BEGIN NOSTR NIP-{}\"", date_filter),
        format!("X-GM-RAW \"X-Nostr-Pubkey:{}\"", date_filter),
        format!("X-GM-RAW \"END NOSTR NIP-{}\"", date_filter),
    ];
    
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
    
    let message_numbers: Vec<u32> = all_message_numbers.into_iter().collect();
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Total unique sent messages to fetch: {}", message_numbers.len());
    
    if message_numbers.is_empty() {
        println!("[RUST] fetch_sent_emails_from_gmail_optimized: No sent messages found, returning empty result");
        return Ok(vec![]);
    }
    
    // Fetch all matching messages
    let messages = session.fetch(message_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","), "RFC822")?;
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Successfully fetched {} sent message objects", messages.len());
    
    let mut emails = Vec::new();
    let mut email_id = 0;
    
    // Filter for emails within the specified time window
    let cutoff = Utc::now() - time_window;
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Filtering for sent emails after: {}", cutoff);
    
    for message in messages.iter() {
        email_id += 1;
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
                
                // Only keep emails within the time window
                if date < cutoff {
                    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent email {} is too old ({} < {}), skipping", 
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
                let has_encrypted_content = body_text.contains("BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE");
                let has_end_marker = body_text.contains("END NOSTR NIP-04 ENCRYPTED MESSAGE");
                
                println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent email {} - Has Nostr header: {}, Has encrypted content: {}, Has end marker: {}", 
                    email_id, has_nostr_header, has_encrypted_content, has_end_marker);
                
                // More lenient verification: accept if it has either the header or encrypted content
                let is_nostr_email = has_nostr_header || has_encrypted_content;
                
                if is_nostr_email {
                    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Sent email {} is confirmed as Nostr email", email_id);
                    
                    // For sent emails, don't decrypt during sync - the subject is encrypted with recipient's pubkey
                    // which we don't have access to here. The frontend will decrypt it when displaying.
                    // Just save the encrypted content as-is.
                    let (_final_subject, _final_body) = (subject.clone(), body_text.clone());
                    
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
                        nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                        message_id: extract_message_id_from_headers(&raw_headers),
                    };
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
    println!("[RUST] fetch_sent_emails_from_gmail_optimized: Successfully processed {} sent Nostr emails", emails.len());
    Ok(emails)
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

/// Extract message ID from email headers
pub fn extract_message_id_from_headers(raw_headers: &str) -> Option<String> {
    for line in raw_headers.lines() {
        if line.to_lowercase().starts_with("message-id:") {
            return Some(line.split_once(':').unwrap_or(("", "")).1.trim().to_string());
        }
    }
    None
}

/// Decrypt email content if it's a Nostr encrypted email
fn decrypt_nostr_email_content(config: &EmailConfig, raw_headers: &str, subject: &str, body: &str) -> Result<(String, String)> {
    // Check if we have a private key to decrypt with
    let private_key = match &config.private_key {
        Some(key) => key,
        None => {
            println!("[RUST] No private key available for decryption");
            return Ok((subject.to_string(), body.to_string()));
        }
    };
    
    // Extract the sender's public key from headers
    let sender_pubkey = match extract_nostr_pubkey_from_headers(raw_headers) {
        Some(pubkey) => pubkey,
        None => {
            println!("[RUST] No X-Nostr-Pubkey header found");
            return Ok((subject.to_string(), body.to_string()));
        }
    };
    
    println!("[RUST] Attempting to decrypt email from pubkey: {}", sender_pubkey);
    
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
fn is_likely_encrypted_content(content: &str) -> bool {
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
    use chrono::{Utc, Duration};
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

    // 2. If None, fetch all Nostr emails from IMAP (no date filter)
    // 3. If Some(date), fetch Nostr emails from IMAP since that date
    let emails = if use_tls {
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            // For Gmail, use a time window: if we have a latest, use since then, else use a large window (e.g. 5 years)
            let now = Utc::now();
            let time_window = match latest {
                Some(dt) => now.signed_duration_since(dt),
                None => Duration::days(365 * 5), // 5 years
            };
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, time_window)?
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
                            let email_message = EmailMessage {
                                id: _email_id.to_string(),
                                from,
                                to,
                                subject,
                                body: body_text.clone(),
                                raw_body: body_text.clone(),
                                date,
                                is_read: true,
                                raw_headers: raw_headers.clone(),
                                nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                                message_id: extract_message_id_from_headers(&raw_headers),
                            };
                            emails.push(email_message);
                        }
                    }
                }
            }
            session.logout()?;
            emails
        }
    } else {
        // Not using TLS
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            let now = Utc::now();
            let time_window = match latest {
                Some(dt) => now.signed_duration_since(dt),
                None => Duration::days(365 * 5),
            };
            fetch_nostr_emails_from_gmail_optimized(&mut session, config, time_window)?
        } else {
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
                        if raw_headers.contains("X-Nostr-Pubkey:") {
                            let (_final_subject, _final_body) = match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                                Ok((dec_subject, dec_body)) => (dec_subject, dec_body),
                                Err(_) => (subject.clone(), body_text.clone()),
                            };
                            let email_message = EmailMessage {
                                id: _email_id.to_string(),
                                from,
                                to,
                                subject,
                                body: body_text.clone(),
                                raw_body: body_text.clone(),
                                date,
                                is_read: true,
                                raw_headers: raw_headers.clone(),
                                nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                                message_id: extract_message_id_from_headers(&raw_headers),
                            };
                            emails.push(email_message);
                        }
                    }
                }
            }
            session.logout()?;
            emails
        }
    };
    Ok(emails)
} 

pub async fn sync_nostr_emails_to_db(config: &EmailConfig, db: &Database) -> anyhow::Result<usize> {
    // Fetch latest nostr email date from DB
    let latest = db.get_latest_nostr_email_received_at()?;
    // Fetch new Nostr emails from IMAP (raw, not decrypted)
    let raw_nostr_emails = fetch_nostr_emails_smart_raw(config, latest).await?;

    let mut new_count = 0;
    for email in raw_nostr_emails {
        // Check if already in DB by message_id
        if db.get_email(&email.message_id)?.is_none() {
            // Save raw email to DB
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
                nostr_pubkey: email.nostr_pubkey.clone(),
                raw_headers: Some(email.raw_headers.clone()),
                is_draft: false,
                is_read: false,
                updated_at: None,
                created_at: chrono::Utc::now(),
            };
            println!("[RUST] Saving email to DB: {:?}", db_email);
            db.save_email(&db_email)?;
            println!("[RUST] Saved email to DB: {:?}", db_email);
            new_count += 1;
        }
    }
    Ok(new_count)
} 

pub async fn sync_sent_emails_to_db(config: &EmailConfig, db: &Database) -> anyhow::Result<usize> {
    println!("[RUST] sync_sent_emails_to_db: Starting sync for email: {}", config.email_address);
    // Fetch latest sent email date from DB using the user's email address
    let latest = db.get_latest_sent_email_received_at(Some(&config.email_address))?;
    println!("[RUST] sync_sent_emails_to_db: Latest sent email date: {:?}", latest);
    // Fetch new sent Nostr emails from IMAP (raw, not decrypted)
    println!("[RUST] sync_sent_emails_to_db: Fetching emails from IMAP...");
    let raw_sent_emails = fetch_sent_emails_smart_raw(config, latest).await?;
    println!("[RUST] sync_sent_emails_to_db: Fetched {} emails from IMAP", raw_sent_emails.len());

    let mut new_count = 0;
    println!("[RUST] sync_sent_emails_to_db: Processing {} emails for saving", raw_sent_emails.len());
    for (idx, email) in raw_sent_emails.iter().enumerate() {
        println!("[RUST] sync_sent_emails_to_db: Processing email {} of {}: message_id={}, from={}, date={}", 
            idx + 1, raw_sent_emails.len(), email.message_id, email.from, email.date);
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
                    nostr_pubkey: email.nostr_pubkey.clone(),
                    raw_headers: Some(email.raw_headers.clone()), // Update with IMAP headers
                    is_draft: false,
                    is_read: existing_email.is_read, // Preserve read status
                    updated_at: Some(chrono::Utc::now()),
                    created_at: existing_email.created_at, // Preserve original creation time
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
                nostr_pubkey: email.nostr_pubkey.clone(),
                raw_headers: Some(email.raw_headers.clone()),
                is_draft: false,
                is_read: false,
                updated_at: None,
                created_at: chrono::Utc::now(),
            };
            println!("[RUST] Inserting new sent email to DB: message_id={}, from={}, to={}, subject_len={}, body_len={}", 
                db_email.message_id, db_email.from_address, db_email.to_address, 
                db_email.subject.len(), db_email.body.len());
            match db.insert_email_direct(&db_email) {
                Ok(id) => {
                    println!("[RUST] Successfully inserted new sent email to DB with id: {}", id);
                    new_count += 1;
                }
                Err(e) => {
                    println!("[RUST] ERROR: Failed to insert email to DB: {}", e);
                    return Err(anyhow::anyhow!("Failed to insert email {} to DB: {}", email.message_id, e));
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
    pub nostr_pubkey: Option<String>,
    pub raw_headers: String,
}

async fn fetch_nostr_emails_smart_raw(config: &EmailConfig, latest: Option<chrono::DateTime<chrono::Utc>>) -> anyhow::Result<Vec<RawNostrEmail>> {
    use chrono::{Utc, Duration};
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
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        if is_gmail {
            let now = Utc::now();
            let time_window = match latest {
                Some(dt) => now.signed_duration_since(dt),
                None => Duration::days(365 * 5),
            };
            // fetch_nostr_emails_from_gmail_optimized returns Vec<EmailMessage>, convert to Vec<RawNostrEmail>
            let email_msgs = fetch_nostr_emails_from_gmail_optimized(&mut session, config, time_window)?;
            email_msgs.into_iter().map(|em| RawNostrEmail {
                message_id: em.message_id.unwrap_or_else(|| em.id.clone()),
                from: em.from,
                to: em.to,
                subject: em.subject,
                body: em.body,
                date: em.date,
                nostr_pubkey: extract_nostr_pubkey_from_headers(&em.raw_headers),
                raw_headers: em.raw_headers,
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
                        let email_message = RawNostrEmail {
                            message_id: extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| Uuid::new_v4().to_string()),
                            from,
                            to,
                            subject,
                            body: body_text,
                            date,
                            nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                            raw_headers,
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
    use chrono::{Utc, Duration};
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
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
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
            let now = Utc::now();
            let time_window = match latest {
                Some(dt) => now.signed_duration_since(dt),
                None => Duration::days(365 * 5),
            };
            // Use the same optimized function but for sent emails
            let email_msgs = fetch_sent_emails_from_gmail_optimized(&mut session, config, time_window)?;
            Ok(email_msgs.into_iter().map(|em| RawNostrEmail {
                message_id: em.message_id.unwrap_or_else(|| em.id.clone()),
                from: em.from,
                to: em.to,
                subject: em.subject,
                body: em.body,
                date: em.date,
                nostr_pubkey: extract_nostr_pubkey_from_headers(&em.raw_headers),
                raw_headers: em.raw_headers,
            }).collect())
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
                            let email_message = RawNostrEmail {
                                message_id: extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| format!("msg_{}", chrono::Utc::now().timestamp())),
                                from,
                                to,
                                subject,
                                body: body_text,
                                date,
                                nostr_pubkey: extract_nostr_pubkey_from_headers(&raw_headers),
                                raw_headers,
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