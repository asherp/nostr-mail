use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use lettre::message::{MultiPart, SinglePart, Attachment};
use anyhow::Result;
use mailparse::{MailHeaderMap, parse_mail};
use lettre::message::header::{Header, HeaderName, HeaderValue, ContentType};
use std::error::Error;
use crate::crypto;
use crate::database::{Database, Email as DbEmail};
use crate::types::{EmailConfig, EmailAttachment};
use tokio::task;
use tokio::time::timeout;
use std::time::Duration;
use crate::types::{TransportAuthVerdict, TransportAuthMethod};

// Verbose [RUST] logs in the decrypt hot path are silent by default — set the
// NOSTR_MAIL_DEBUG environment variable to any value to re-enable them for
// diagnostics. [RUST-PERF] profiling lines are gated behind the same variable
// (via debug_log!), so they're off in normal use and only printed when
// NOSTR_MAIL_DEBUG is set.
fn debug_log_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NOSTR_MAIL_DEBUG").is_ok())
}

macro_rules! debug_log {
    ($($arg:tt)*) => {
        if debug_log_enabled() {
            println!($($arg)*);
        }
    };
}

// Soft cap on cache size. When a cache grows beyond this, ~25% of entries are
// dropped (HashMap iteration order is unspecified, so this is effectively a
// random eviction). Realistic inboxes have a few thousand unique armor bodies
// at most; this guard exists for the pathological "100k+ encrypted messages"
// case.
const CACHE_MAX: usize = 10_000;

fn maybe_evict<K: Clone + Eq + std::hash::Hash, V>(
    map: &mut std::collections::HashMap<K, V>,
) {
    if map.len() > CACHE_MAX {
        let drop_count = map.len() / 4;
        let keys: Vec<K> = map.keys().take(drop_count).cloned().collect();
        for k in keys {
            map.remove(&k);
        }
    }
}
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose};

// IMAP connections are established and reused through `crate::imap_pool`, which
// owns TLS setup, socket timeouts, and warm-connection pooling for the active
// account. See that module for the per-transport details.
use crate::imap_pool::{self, ImapTarget};

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

#[derive(Debug, Clone)]
struct XNostrRecipient(String);

impl Header for XNostrRecipient {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str("X-Nostr-Recipient")
    }
    fn parse(s: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        Ok(XNostrRecipient(s.to_string()))
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
    html_body: Option<&str>,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    include_pubkey_header: bool,
    include_sig_header: bool,
    recipient_pubkey: Option<&str>,
    include_recipient_header: bool,
) -> Result<String> {
    debug_log!("[RUST] construct_email_headers: Constructing email headers");
    debug_log!("[RUST] construct_email_headers: From: {}, To: {}", config.email_address, to_address);
    
    let mut builder = Message::builder()
        .from(config.email_address.parse()?)
        .reply_to(config.email_address.parse()?)
        .to(to_address.parse()?)
        .subject(subject);

    // Add custom message ID if provided
    if let Some(msg_id) = message_id {
        debug_log!("[RUST] construct_email_headers: Setting message ID: {}", msg_id);
        // Try using the builder's message_id method
        builder = builder.message_id(Some(msg_id.to_string()));
        
        // Also try manually adding the header as a fallback
        // Note: This might not work with lettre's builder pattern, but worth trying
        // builder = builder.header(("Message-ID", msg_id));
    } else {
        debug_log!("[RUST] construct_email_headers: No message ID provided");
    }

    // Add In-Reply-To header if provided (for email threading)
    if let Some(reply_id) = in_reply_to {
        debug_log!("[RUST] construct_email_headers: Setting In-Reply-To: {}", reply_id);
        builder = builder.in_reply_to(reply_id.to_string());
    }

    // Add References header if provided (for email threading)
    if let Some(refs) = references {
        debug_log!("[RUST] construct_email_headers: Setting References: {}", refs);
        builder = builder.references(refs.to_string());
    }

    // Add the sender's public key to the headers (not the receiver's)
    // This allows the receiver to derive the shared secret using their private key.
    // Both X-Nostr-Pubkey and X-Nostr-Sig are gated on per-user toggles in
    // Advanced settings (default-on). X-Nostr-Sig also requires X-Nostr-Pubkey
    // since the recipient needs the pubkey to verify the signature.
    if let Some(private_key) = &config.private_key {
        if include_pubkey_header || include_sig_header {
            match crypto::get_public_key_from_private(private_key) {
                Ok(sender_pubkey) => {
                    if include_pubkey_header {
                        debug_log!("[RUST] construct_email_headers: Adding sender pubkey to headers: {}", sender_pubkey);
                        builder = builder.header(XNostrPubkey(sender_pubkey));
                    } else {
                        debug_log!("[RUST] construct_email_headers: Skipping X-Nostr-Pubkey (disabled by user)");
                    }

                    if include_sig_header && include_pubkey_header {
                        // Sign the binary ciphertext extracted from the body
                        let binary = extract_ciphertext_binary(body);
                        match crypto::sign_data_bytes(private_key, &binary) {
                            Ok(signature) => {
                                debug_log!("[RUST] construct_email_headers: Signing email body (binary, {} bytes), signature length: {}", binary.len(), signature.len());
                                builder = builder.header(XNostrSig(signature));
                            }
                            Err(e) => {
                                debug_log!("[RUST] construct_email_headers: Failed to sign email body: {}", e);
                            }
                        }
                    } else if include_sig_header && !include_pubkey_header {
                        debug_log!("[RUST] construct_email_headers: Skipping X-Nostr-Sig because X-Nostr-Pubkey is disabled");
                    } else {
                        debug_log!("[RUST] construct_email_headers: Skipping X-Nostr-Sig (disabled by user)");
                    }
                }
                Err(e) => {
                    debug_log!("[RUST] construct_email_headers: Failed to get public key from private key: {}", e);
                }
            }
        }
    }

    // X-Nostr-Recipient lets the receiver (and the sender's own Sent-folder
    // reader) anchor decryption to a specific pubkey without depending on a
    // Nostr relay or DM cross-reference. Independent of pubkey/sig toggles.
    if include_recipient_header {
        if let Some(rp) = recipient_pubkey {
            if !rp.is_empty() {
                debug_log!("[RUST] construct_email_headers: Adding recipient pubkey to headers: {}", rp);
                builder = builder.header(XNostrRecipient(rp.to_string()));
            }
        }
    }

    // Build the text (and optional HTML) body part
    let text_part = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string());

    let body_part: Option<MultiPart> = if let Some(html) = html_body {
        debug_log!("[RUST] construct_email_headers: Building multipart/alternative with HTML body");
        let html_part = SinglePart::builder()
            .header(ContentType::TEXT_HTML)
            .body(html.to_string());
        Some(MultiPart::alternative()
            .singlepart(text_part)
            .singlepart(html_part))
    } else {
        None
    };

    // Build email with or without attachments (for header construction)
    let email = if let Some(attachments) = attachments {
        if attachments.is_empty() {
            if let Some(alt) = body_part {
                builder.multipart(alt)?
            } else {
                builder.body(body.to_string())?
            }
        } else {
            debug_log!("[RUST] construct_email_headers: Building multipart email with {} attachments", attachments.len());

            // Create multipart/mixed; nest alternative or plain text inside
            let mut multipart = if let Some(alt) = body_part {
                MultiPart::mixed().multipart(alt)
            } else {
                MultiPart::mixed().singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(body.to_string()))
            };

            // Add each attachment (for header construction, we don't need the actual data)
            for attachment in attachments {
                debug_log!("[RUST] construct_email_headers: Adding attachment header: {}", attachment.filename);

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
        if let Some(alt) = body_part {
            builder.multipart(alt)?
        } else {
            builder.body(body.to_string())?
        }
    };
    
    // Convert the email to a string to get the raw headers
    let email_bytes = email.formatted();
    let email_string = String::from_utf8(email_bytes)?;
    
    debug_log!("[RUST] construct_email_headers: Full email string length: {}", email_string.len());
    
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
    debug_log!("[RUST] construct_email_headers: Final headers:");
    println!("{}", final_headers);
    
    // Check if Message-ID is present in the headers
    if final_headers.to_lowercase().contains("message-id:") {
        debug_log!("[RUST] construct_email_headers: Message-ID found in headers");
    } else {
        debug_log!("[RUST] construct_email_headers: Message-ID NOT found in headers");
        // If Message-ID is not present, manually add it
        if let Some(msg_id) = message_id {
            debug_log!("[RUST] construct_email_headers: Manually adding Message-ID: {}", msg_id);
            let headers_with_message_id = format!("Message-ID: {}\n{}", msg_id, final_headers);
            debug_log!("[RUST] construct_email_headers: Headers with manually added Message-ID:");
            println!("{}", headers_with_message_id);
            return Ok(headers_with_message_id);
        }
    }
    
    Ok(final_headers)
}

#[allow(clippy::too_many_arguments)]
pub async fn send_email(
    config: &EmailConfig,
    to_address: &str,
    subject: &str,
    body: &str,
    _nostr_npub: Option<&str>,
    message_id: Option<&str>,
    attachments: Option<&Vec<EmailAttachment>>,
    html_body: Option<&str>,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    include_pubkey_header: bool,
    include_sig_header: bool,
    recipient_pubkey: Option<&str>,
    include_recipient_header: bool,
) -> Result<String> {
    debug_log!("[RUST] send_email: Starting email send process");
    debug_log!("[RUST] send_email: SMTP Host: {}, Port: {}", config.smtp_host, config.smtp_port);
    debug_log!("[RUST] send_email: From: {}, To: {}", config.email_address, to_address);
    debug_log!("[RUST] send_email: Use TLS: {}", config.use_tls);
    
    let mut builder = Message::builder()
        .from(config.email_address.parse()?)
        .reply_to(config.email_address.parse()?)
        .to(to_address.parse()?)
        .subject(subject);

    // Add custom message ID if provided
    if let Some(msg_id) = message_id {
        // Pass the message ID as Option<String> to the builder
        builder = builder.message_id(Some(msg_id.to_string()));
    }

    // Add In-Reply-To header if provided (for email threading)
    if let Some(reply_id) = in_reply_to {
        debug_log!("[RUST] send_email: Setting In-Reply-To: {}", reply_id);
        builder = builder.in_reply_to(reply_id.to_string());
    }

    // Add References header if provided (for email threading)
    if let Some(refs) = references {
        debug_log!("[RUST] send_email: Setting References: {}", refs);
        builder = builder.references(refs.to_string());
    }

    // Add the sender's public key to the headers (not the receiver's).
    // X-Nostr-Pubkey and X-Nostr-Sig are gated on per-user toggles in Advanced
    // settings (default-on). X-Nostr-Sig requires X-Nostr-Pubkey since the
    // recipient needs the pubkey to verify the signature.
    if let Some(private_key) = &config.private_key {
        if include_pubkey_header || include_sig_header {
            match crypto::get_public_key_from_private(private_key) {
                Ok(sender_pubkey) => {
                    if include_pubkey_header {
                        debug_log!("[RUST] send_email: Adding sender pubkey to headers: {}", sender_pubkey);
                        builder = builder.header(XNostrPubkey(sender_pubkey));
                    } else {
                        debug_log!("[RUST] send_email: Skipping X-Nostr-Pubkey (disabled by user)");
                    }

                    if include_sig_header && include_pubkey_header {
                        debug_log!("[RUST] send_email: body passed to extract_ciphertext_binary ({} chars):\n{}", body.len(), &body[..body.len().min(500)]);
                        let binary = extract_ciphertext_binary(body);
                        let binary_hash = {
                            use sha2::{Sha256, Digest};
                            let mut h = Sha256::new();
                            h.update(&binary);
                            hex::encode(&h.finalize()[..8])
                        };
                        debug_log!("[RUST] send_email: extracted binary {} bytes, sha256_prefix: {}", binary.len(), binary_hash);
                        match crypto::sign_data_bytes(private_key, &binary) {
                            Ok(signature) => {
                                debug_log!("[RUST] send_email: Signing email body (binary, {} bytes), signature length: {}", binary.len(), signature.len());
                                builder = builder.header(XNostrSig(signature));
                            }
                            Err(e) => {
                                debug_log!("[RUST] send_email: Failed to sign email body: {}", e);
                            }
                        }
                    } else if include_sig_header && !include_pubkey_header {
                        debug_log!("[RUST] send_email: Skipping X-Nostr-Sig because X-Nostr-Pubkey is disabled");
                    } else {
                        debug_log!("[RUST] send_email: Skipping X-Nostr-Sig (disabled by user)");
                    }
                }
                Err(e) => {
                    debug_log!("[RUST] send_email: Failed to get public key from private key: {}", e);
                }
            }
        }
    }

    // X-Nostr-Recipient lets the receiver (and the sender's own Sent-folder
    // reader) anchor decryption to a specific pubkey without depending on a
    // Nostr relay or DM cross-reference. Independent of pubkey/sig toggles.
    if include_recipient_header {
        if let Some(rp) = recipient_pubkey {
            if !rp.is_empty() {
                debug_log!("[RUST] send_email: Adding recipient pubkey to headers: {}", rp);
                builder = builder.header(XNostrRecipient(rp.to_string()));
            }
        }
    }

    // Build the text (and optional HTML) body part
    let text_part = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string());

    let body_part: Option<MultiPart> = if let Some(html) = html_body {
        debug_log!("[RUST] send_email: Building multipart/alternative with HTML body");
        let html_part = SinglePart::builder()
            .header(ContentType::TEXT_HTML)
            .body(html.to_string());
        Some(MultiPart::alternative()
            .singlepart(text_part)
            .singlepart(html_part))
    } else {
        None
    };

    // Build email with or without attachments
    let email = if let Some(attachments) = attachments {
        if attachments.is_empty() {
            if let Some(alt) = body_part {
                builder.multipart(alt)?
            } else {
                builder.body(body.to_string())?
            }
        } else {
            debug_log!("[RUST] send_email: Building multipart email with {} attachments", attachments.len());

            // Create multipart/mixed; nest alternative or plain text inside
            let mut multipart = if let Some(alt) = body_part {
                MultiPart::mixed().multipart(alt)
            } else {
                MultiPart::mixed().singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(body.to_string()))
            };

            // Add each attachment
            for attachment in attachments {
                debug_log!("[RUST] send_email: Adding attachment: {} ({})", attachment.filename, attachment.size);

                // Decode base64 data
                let attachment_data = match general_purpose::STANDARD.decode(&attachment.data) {
                    Ok(data) => data,
                    Err(e) => {
                        debug_log!("[RUST] send_email: Failed to decode base64 attachment data for {}: {}", attachment.filename, e);
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
        if let Some(alt) = body_part {
            builder.multipart(alt)?
        } else {
            builder.body(body.to_string())?
        }
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
    } else {
        // Explicitly disable TLS/STARTTLS for mock servers
        mailer_builder = mailer_builder.tls(lettre::transport::smtp::client::Tls::None);
    }

    let mailer = mailer_builder.build();

    debug_log!("[RUST] send_email: Mailer built, attempting to send...");
    
    // Run the blocking SMTP send operation in a separate thread with a 60-second timeout
    let mailer_clone = mailer.clone();
    let email_clone = email.clone();
    
    let send_future = task::spawn_blocking(move || {
        debug_log!("[RUST] send_email: Executing SMTP send in blocking thread");
        mailer_clone.send(&email_clone)
    });
    
    match timeout(Duration::from_secs(60), send_future).await {
        Ok(join_res) => match join_res {
            Ok(send_res) => match send_res {
                Ok(_) => {
                    debug_log!("[RUST] send_email: Email sent successfully");
                    Ok(format!("Email sent successfully to {}", to_address))
                }
                Err(e) => {
                    debug_log!("[RUST] send_email: Failed to send email: {}", e);
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
                debug_log!("[RUST] send_email: Task join error: {}", e);
                Err(anyhow::anyhow!("Task join error: {}", e))
            }
        },
        Err(_) => {
            debug_log!("[RUST] send_email: SMTP send operation timed out after 60 seconds");
            Err(anyhow::anyhow!("SMTP send operation timed out after 60 seconds. Check your internet connection and SMTP settings."))
        }
    }
}

/// Delete a sent email from the IMAP server by moving it to Trash
/// For Gmail, moves to [Gmail]/Trash
/// For other providers, tries common trash folder names
pub async fn delete_sent_email_from_server(config: &EmailConfig, message_id: &str) -> Result<()> {
    let host = config.imap_host.clone();
    let port = config.imap_port;
    let username = config.email_address.clone();
    let password = config.password.clone();
    let use_tls = config.use_tls;
    let message_id = message_id.to_string();

    debug_log!("[RUST] delete_sent_email_from_server: Attempting to delete email with Message-ID: {}", message_id);

    // Run all blocking IMAP I/O on a dedicated thread to avoid blocking the Tokio runtime
    tokio::task::spawn_blocking(move || {
        let target = ImapTarget { host, port, username, password, use_tls };
        let is_gmail = target.host.contains("gmail.com");
        imap_pool::with_session(&target, |session| {
            delete_sent_email_from_session_sync(session, is_gmail, &message_id)
        })
    }).await.map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
}

/// Extract the text/plain body from a parsed email, recursing into multipart parts.
/// Returns None if no text/plain part is found.
fn extract_text_body(email: &mailparse::ParsedMail) -> Option<String> {
    // If this part itself is text/plain, return it
    if email.ctype.mimetype == "text/plain" && email.subparts.is_empty() {
        return email.get_body().ok();
    }
    for subpart in email.subparts.iter() {
        let ctype = &subpart.ctype;
        if ctype.mimetype == "text/plain" {
            return subpart.get_body().ok();
        }
        // Recurse into nested multipart
        if ctype.mimetype.starts_with("multipart/") {
            if let Some(text) = extract_text_body(subpart) {
                return Some(text);
            }
        }
    }
    None
}

/// Extract the text/html body from a parsed email (multipart/alternative).
/// Returns None if no HTML part is found.
fn extract_html_body(email: &mailparse::ParsedMail) -> Option<String> {
    debug_log!("[RUST] extract_html_body: top-level mimetype={}, subparts={}", email.ctype.mimetype, email.subparts.len());
    for (i, subpart) in email.subparts.iter().enumerate() {
        let ctype = &subpart.ctype;
        debug_log!("[RUST] extract_html_body: subpart[{}] mimetype={}", i, ctype.mimetype);
        if ctype.mimetype == "text/html" {
            let body = subpart.get_body().ok();
            debug_log!("[RUST] extract_html_body: found text/html, body length={}", body.as_ref().map(|b| b.len()).unwrap_or(0));
            return body;
        }
        // Recurse into nested multipart
        if ctype.mimetype.starts_with("multipart/") {
            if let Some(html) = extract_html_body(subpart) {
                return Some(html);
            }
        }
    }
    debug_log!("[RUST] extract_html_body: no text/html found");
    None
}

/// Helper function to delete email from IMAP session (works with both TLS and non-TLS)
fn delete_sent_email_from_session_sync(
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
    
    debug_log!("[RUST] delete_sent_email_from_session: Selecting sent folder: {}", sent_folder);
    
    // Try to select the sent folder, fallback to common variations
    let folder_selected = session.select(sent_folder).is_ok() || 
                         session.select("Sent Mail").is_ok() || 
                         session.select("Sent Items").is_ok() ||
                         session.select("Sent").is_ok();
    
    if !folder_selected {
        debug_log!("[RUST] delete_sent_email_from_session: Could not select sent folder, aborting server deletion");
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
        debug_log!("[RUST] delete_sent_email_from_session: Searching for email with query: {}", search_query);
        match session.search(search_query) {
            Ok(results) => {
                let result_count = results.len();
                if !results.is_empty() {
                    matching_messages.extend(results);
                    debug_log!("[RUST] delete_sent_email_from_session: Found {} matching message(s) with query: {}", result_count, search_query);
                    break; // Found results, no need to try other formats
                }
            }
            Err(e) => {
                debug_log!("[RUST] delete_sent_email_from_session: Search query failed: {} - {}", search_query, e);
            }
        }
    }
    
    if matching_messages.is_empty() {
        debug_log!("[RUST] delete_sent_email_from_session: No email found with Message-ID (tried: {}, {}, {})", full_msg_id, normalized_msg_id, message_id.trim());
        return Err(anyhow::anyhow!("Email not found on server"));
    }
    
    debug_log!("[RUST] delete_sent_email_from_session: Found {} matching message(s)", matching_messages.len());
    
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
    
    debug_log!("[RUST] delete_sent_email_from_session: Moving message {} to trash folder: {}", message_seq, trash_folder);
    
    // Use MOVE command (mv method) to move the message to trash
    // This is supported by Gmail and other modern IMAP servers
    let message_seq_str = format!("{}", message_seq);
    match session.mv(&message_seq_str, trash_folder) {
        Ok(_) => {
            debug_log!("[RUST] delete_sent_email_from_session: Successfully moved email to trash using MOVE command");
            return Ok(());
        }
        Err(e) => {
            debug_log!("[RUST] delete_sent_email_from_session: MOVE command failed: {}, trying COPY + DELETE", e);
        }
    }
    
    // Fallback: Use COPY + STORE + EXPUNGE if MOVE is not supported
    // First, try to copy to trash
    let copy_result = session.copy(&message_seq_str, trash_folder);
    match copy_result {
        Ok(_) => {
            debug_log!("[RUST] delete_sent_email_from_session: Successfully copied email to trash");
            // Mark original as deleted
            session.store(&message_seq_str, "+FLAGS (\\Deleted)")?;
            // Expunge to actually delete
            session.expunge()?;
            debug_log!("[RUST] delete_sent_email_from_session: Successfully deleted email from sent folder");
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
                debug_log!("[RUST] delete_sent_email_from_session: Trying alternative trash folder: {}", alt_trash);
                if session.copy(&message_seq_str, alt_trash).is_ok() {
                    session.store(&message_seq_str, "+FLAGS (\\Deleted)")?;
                    session.expunge()?;
                    debug_log!("[RUST] delete_sent_email_from_session: Successfully moved email to {} using COPY", alt_trash);
                    return Ok(());
                }
            }
            
            Err(anyhow::anyhow!("Failed to move email to trash: {}", e))
        }
    }
}

/// Delete an inbox email from the IMAP server (move to trash).
pub async fn delete_inbox_email_from_server(config: &EmailConfig, message_id: &str) -> Result<()> {
    let host = config.imap_host.clone();
    let port = config.imap_port;
    let username = config.email_address.clone();
    let password = config.password.clone();
    let use_tls = config.use_tls;
    let message_id = message_id.to_string();

    debug_log!("[RUST] delete_inbox_email_from_server: Attempting to delete email with Message-ID: {}", message_id);

    tokio::task::spawn_blocking(move || {
        let target = ImapTarget { host, port, username, password, use_tls };
        let is_gmail = target.host.contains("gmail.com");
        imap_pool::with_session(&target, |session| {
            delete_email_from_folder_sync(session, is_gmail, &message_id, &["INBOX", "nostr-mail"])
        })
    }).await.map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
}

/// Delete an email from a specific IMAP folder by searching multiple folders in order.
fn delete_email_from_folder_sync(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    is_gmail: bool,
    message_id: &str,
    source_folders: &[&str],
) -> Result<()> {
    // Try each source folder until we find and delete the email
    let mut folder_selected = false;
    for folder in source_folders {
        debug_log!("[RUST] delete_email_from_folder_sync: Trying folder: {}", folder);
        if session.select(folder).is_ok() {
            folder_selected = true;

            // Search for the email by Message-ID
            let normalized_msg_id = message_id.trim().trim_start_matches('<').trim_end_matches('>');
            let full_msg_id = if normalized_msg_id.contains('@') {
                format!("<{}>", normalized_msg_id)
            } else {
                format!("<{}@nostr-mail>", normalized_msg_id)
            };

            let search_queries = vec![
                format!("HEADER Message-ID \"{}\"", full_msg_id),
                format!("HEADER Message-ID \"{}\"", normalized_msg_id),
                format!("HEADER Message-ID \"{}\"", message_id.trim()),
            ];

            let mut matching_messages = std::collections::HashSet::new();
            for search_query in &search_queries {
                if let Ok(results) = session.search(search_query) {
                    if !results.is_empty() {
                        matching_messages.extend(results);
                        debug_log!("[RUST] delete_email_from_folder_sync: Found {} match(es) in {} with query: {}", matching_messages.len(), folder, search_query);
                        break;
                    }
                }
            }

            if !matching_messages.is_empty() {
                let message_seq = *matching_messages.iter().next().unwrap();
                return move_to_trash(session, is_gmail, message_seq);
            }
        }
    }

    if !folder_selected {
        return Err(anyhow::anyhow!("Could not select any source folder"));
    }
    Err(anyhow::anyhow!("Email not found on server"))
}

/// Move a message (by sequence number) to the trash folder.
fn move_to_trash(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    is_gmail: bool,
    message_seq: u32,
) -> Result<()> {
    let trash_folder = if is_gmail { "[Gmail]/Trash" } else { "Trash" };
    let seq_str = format!("{}", message_seq);

    debug_log!("[RUST] move_to_trash: Moving message {} to {}", message_seq, trash_folder);

    // Try MOVE first
    if session.mv(&seq_str, trash_folder).is_ok() {
        debug_log!("[RUST] move_to_trash: Successfully moved via MOVE command");
        return Ok(());
    }

    // Fallback: COPY + DELETE + EXPUNGE
    let trash_folders = if is_gmail {
        vec!["[Gmail]/Trash"]
    } else {
        vec!["Trash", "Deleted", "Deleted Items", "Junk"]
    };

    for folder in trash_folders {
        if session.copy(&seq_str, folder).is_ok() {
            session.store(&seq_str, "+FLAGS (\\Deleted)")?;
            session.expunge()?;
            debug_log!("[RUST] move_to_trash: Successfully moved to {} via COPY", folder);
            return Ok(());
        }
    }

    Err(anyhow::anyhow!("Failed to move email to trash"))
}

/// Move an inbox email (identified by Message-ID) to an arbitrary IMAP folder.
///
/// Mirrors `delete_inbox_email_from_server`, but instead of moving the matched
/// message to trash it moves it into `target_folder`, creating that folder if it
/// does not already exist. The message may live anywhere (it could have been
/// moved before), so the search spans the user's real folders rather than a fixed
/// `INBOX`/`nostr-mail` pair; if it's already in `target_folder` the move is a
/// no-op success.
pub async fn move_inbox_email_to_folder(
    config: &EmailConfig,
    message_id: &str,
    target_folder: &str,
) -> Result<()> {
    let host = config.imap_host.clone();
    let port = config.imap_port;
    let username = config.email_address.clone();
    let password = config.password.clone();
    let use_tls = config.use_tls;
    let message_id = message_id.to_string();
    let target_folder = target_folder.to_string();

    debug_log!("[RUST] move_inbox_email_to_folder: Moving Message-ID {} to folder {}", message_id, target_folder);

    tokio::task::spawn_blocking(move || {
        let target = ImapTarget { host, port, username, password, use_tls };
        imap_pool::with_session(&target, |session| {
            let source_folders = searchable_source_folders(session, &target_folder);
            move_email_to_folder_sync(session, &message_id, &target_folder, &source_folders)
        })
    }).await.map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
}

/// List the folders worth searching for a message we're about to move, ordered so
/// the cheap/likely ones come first: the target itself (to detect an already-there
/// no-op), then INBOX and nostr-mail, then everything else. Gmail's "All Mail" is
/// excluded — it contains every message (labels, not folders), so it would always
/// match and make the move ambiguous. Falls back to INBOX/nostr-mail if LIST fails.
fn searchable_source_folders(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    target_folder: &str,
) -> Vec<String> {
    let mut folders: Vec<String> = match session.list(Some(""), Some("*")) {
        Ok(mailboxes) => mailboxes
            .iter()
            .map(|mb| mb.name().to_string())
            .filter(|n| !n.to_lowercase().contains("all mail"))
            .collect(),
        Err(_) => vec!["INBOX".to_string(), "nostr-mail".to_string()],
    };

    let rank = |f: &str| -> u8 {
        let fl = f.to_lowercase();
        if fl == target_folder.to_lowercase() {
            0
        } else if fl == "inbox" {
            1
        } else if fl == "nostr-mail" {
            2
        } else {
            3
        }
    };
    // Stable sort preserves the server's order within each rank.
    folders.sort_by_key(|f| rank(f));
    folders
}

/// Find an email by Message-ID across `source_folders` and move it to
/// `target_folder`. Finding it already in `target_folder` is a no-op success.
fn move_email_to_folder_sync(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    message_id: &str,
    target_folder: &str,
    source_folders: &[String],
) -> Result<()> {
    let mut folder_selected = false;
    for folder in source_folders {
        debug_log!("[RUST] move_email_to_folder_sync: Trying folder: {}", folder);
        if session.select(folder).is_ok() {
            folder_selected = true;

            let normalized_msg_id = message_id.trim().trim_start_matches('<').trim_end_matches('>');
            let full_msg_id = if normalized_msg_id.contains('@') {
                format!("<{}>", normalized_msg_id)
            } else {
                format!("<{}@nostr-mail>", normalized_msg_id)
            };

            let search_queries = vec![
                format!("HEADER Message-ID \"{}\"", full_msg_id),
                format!("HEADER Message-ID \"{}\"", normalized_msg_id),
                format!("HEADER Message-ID \"{}\"", message_id.trim()),
            ];

            let mut matching_messages = std::collections::HashSet::new();
            for search_query in &search_queries {
                if let Ok(results) = session.search(search_query) {
                    if !results.is_empty() {
                        matching_messages.extend(results);
                        debug_log!("[RUST] move_email_to_folder_sync: Found {} match(es) in {}", matching_messages.len(), folder);
                        break;
                    }
                }
            }

            if !matching_messages.is_empty() {
                // Already in the destination — nothing to move.
                if folder.eq_ignore_ascii_case(target_folder) {
                    debug_log!("[RUST] move_email_to_folder_sync: Already in target {}, no-op", target_folder);
                    return Ok(());
                }
                let message_seq = *matching_messages.iter().next().unwrap();
                return move_message_to_folder(session, message_seq, target_folder);
            }
        }
    }

    if !folder_selected {
        return Err(anyhow::anyhow!("Could not select any source folder"));
    }
    Err(anyhow::anyhow!("Email not found on server"))
}

/// Move a message (by sequence number) into `target_folder`, creating the folder
/// if necessary. Uses the IMAP MOVE command, falling back to COPY + DELETE +
/// EXPUNGE on servers that don't support MOVE.
fn move_message_to_folder(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    message_seq: u32,
    target_folder: &str,
) -> Result<()> {
    let seq_str = format!("{}", message_seq);

    debug_log!("[RUST] move_message_to_folder: Moving message {} to {}", message_seq, target_folder);

    // When the user deliberately files a message INTO a spam folder, mark it
    // \Seen first. Spam rescue only pulls UNSEEN mail out of spam, so a read
    // message sitting in spam is the user's "leave it here" signal — and because
    // it's a server flag it's the same answer on every device. Our fetches use
    // BODY.PEEK[] (reading a body never sets \Seen), and the read-state sync
    // (`mark_inbox_email_seen_on_server`) only sets \Seen in non-spam inbox
    // folders — so within spam folders this move path is the only thing that
    // ever sets \Seen, and that flag must not be auto-cleared on spam mail.
    // IMAP COPY/MOVE preserve flags, so setting it before the move is enough.
    if is_spam_folder_name(target_folder) {
        let _ = session.store(&seq_str, "+FLAGS (\\Seen)");
    }

    // Try MOVE first.
    if session.mv(&seq_str, target_folder).is_ok() {
        debug_log!("[RUST] move_message_to_folder: Successfully moved via MOVE command");
        return Ok(());
    }

    // The target folder may not exist yet — create it and retry MOVE.
    if session.create(target_folder).is_ok() {
        debug_log!("[RUST] move_message_to_folder: Created folder {}", target_folder);
        if session.mv(&seq_str, target_folder).is_ok() {
            return Ok(());
        }
    }

    // Fallback for servers without MOVE support: COPY + flag deleted + EXPUNGE.
    if session.copy(&seq_str, target_folder).is_ok() {
        session.store(&seq_str, "+FLAGS (\\Deleted)")?;
        session.expunge()?;
        debug_log!("[RUST] move_message_to_folder: Successfully moved to {} via COPY", target_folder);
        return Ok(());
    }

    Err(anyhow::anyhow!("Failed to move email to folder {}", target_folder))
}

/// Find which of `candidate_folders` currently contains the message identified by
/// `message_id`, returning the first match in the given order (or None if it
/// isn't found in any). Inbox emails carry no folder field, so the server is the
/// only source of truth for the move picker's "(current)" label once a message
/// has been moved. Callers should order cheap/likely folders first; the search
/// stops at the first hit.
pub async fn find_message_folder(
    config: &EmailConfig,
    message_id: &str,
    candidate_folders: Vec<String>,
) -> Result<Option<String>> {
    let host = config.imap_host.clone();
    let port = config.imap_port;
    let username = config.email_address.clone();
    let password = config.password.clone();
    let use_tls = config.use_tls;
    let message_id = message_id.to_string();

    tokio::task::spawn_blocking(move || {
        let target = ImapTarget { host, port, username, password, use_tls };
        imap_pool::with_session(&target, |session| {
            find_message_folder_sync(session, &message_id, &candidate_folders)
        })
    }).await.map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
}

fn find_message_folder_sync(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    message_id: &str,
    candidate_folders: &[String],
) -> Result<Option<String>> {
    let normalized_msg_id = message_id.trim().trim_start_matches('<').trim_end_matches('>');
    let full_msg_id = if normalized_msg_id.contains('@') {
        format!("<{}>", normalized_msg_id)
    } else {
        format!("<{}@nostr-mail>", normalized_msg_id)
    };
    let search_queries = [
        format!("HEADER Message-ID \"{}\"", full_msg_id),
        format!("HEADER Message-ID \"{}\"", normalized_msg_id),
        format!("HEADER Message-ID \"{}\"", message_id.trim()),
    ];

    for folder in candidate_folders {
        if session.select(folder).is_err() {
            continue;
        }
        for search_query in &search_queries {
            if let Ok(results) = session.search(search_query) {
                if !results.is_empty() {
                    return Ok(Some(folder.clone()));
                }
            }
        }
    }
    Ok(None)
}

/// Resolve the logged-in user's configured inbox source folders, with spam/junk
/// folders filtered out. Mirrors how the sync chooses folders: the `inbox_folder`
/// setting (one folder per line) when set, otherwise the provider-aware
/// `default_inbox_folders`. Spam/junk folders are excluded because the read-state
/// sync must not touch the `\Seen` flag inside spam folders, where that flag is
/// reserved as the spam-rescue "user filed this here" signal.
pub fn configured_inbox_folders_excluding_spam(
    inbox_folder_setting: Option<&str>,
    imap_host: &str,
) -> Vec<String> {
    let configured: Vec<String> = inbox_folder_setting
        .map(|s| {
            s.split('\n')
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let folders = if configured.is_empty() {
        default_inbox_folders(imap_host)
    } else {
        configured
    };
    folders.into_iter().filter(|f| !is_spam_folder_name(f)).collect()
}

/// Mark an inbox email (identified by Message-ID) as `\Seen` on the IMAP server,
/// so read state set in the app propagates to other clients/devices.
///
/// `source_folders` is the user's configured inbox folders with spam/junk
/// excluded (see `configured_inbox_folders_excluding_spam`). Restricting the
/// search to non-spam folders is deliberate: inside spam/junk folders the
/// `\Seen` flag is reserved as the "user deliberately filed this here" signal
/// that spam rescue keys off of (see `move_message_to_folder`), so the read path
/// must never set it there.
pub async fn mark_inbox_email_seen_on_server(
    config: &EmailConfig,
    message_id: &str,
    source_folders: &[String],
) -> Result<()> {
    let host = config.imap_host.clone();
    let port = config.imap_port;
    let username = config.email_address.clone();
    let password = config.password.clone();
    let use_tls = config.use_tls;
    let message_id = message_id.to_string();
    let source_folders: Vec<String> = source_folders.to_vec();

    debug_log!("[RUST] mark_inbox_email_seen_on_server: Marking Message-ID {} as \\Seen in {:?}", message_id, source_folders);

    tokio::task::spawn_blocking(move || {
        let target = ImapTarget { host, port, username, password, use_tls };
        imap_pool::with_session(&target, |session| {
            set_seen_in_folder_sync(session, &message_id, &source_folders)
        })
    }).await.map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
}

/// Find an email by Message-ID across `source_folders` and set its `\Seen` flag.
/// Stops at the first folder where the message is found. Mirrors the Message-ID
/// search used by `delete_email_from_folder_sync` / `move_email_to_folder_sync`.
fn set_seen_in_folder_sync(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    message_id: &str,
    source_folders: &[String],
) -> Result<()> {
    let mut folder_selected = false;
    for folder in source_folders {
        debug_log!("[RUST] set_seen_in_folder_sync: Trying folder: {}", folder);
        if session.select(folder).is_ok() {
            folder_selected = true;

            let normalized_msg_id = message_id.trim().trim_start_matches('<').trim_end_matches('>');
            let full_msg_id = if normalized_msg_id.contains('@') {
                format!("<{}>", normalized_msg_id)
            } else {
                format!("<{}@nostr-mail>", normalized_msg_id)
            };

            let search_queries = vec![
                format!("HEADER Message-ID \"{}\"", full_msg_id),
                format!("HEADER Message-ID \"{}\"", normalized_msg_id),
                format!("HEADER Message-ID \"{}\"", message_id.trim()),
            ];

            let mut matching_messages = std::collections::HashSet::new();
            for search_query in &search_queries {
                if let Ok(results) = session.search(search_query) {
                    if !results.is_empty() {
                        matching_messages.extend(results);
                        break;
                    }
                }
            }

            if !matching_messages.is_empty() {
                let seq_list = matching_messages
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                session.store(&seq_list, "+FLAGS (\\Seen)")?;
                debug_log!("[RUST] set_seen_in_folder_sync: marked {} message(s) \\Seen in {}", matching_messages.len(), folder);
                return Ok(());
            }
        }
    }

    if !folder_selected {
        return Err(anyhow::anyhow!("Could not select any source folder"));
    }
    Err(anyhow::anyhow!("Email not found on server"))
}

/// List available IMAP folders/mailboxes
pub async fn list_imap_folders(config: &EmailConfig) -> Result<Vec<String>> {
    let target = ImapTarget::from_config(config);
    debug_log!("[RUST] list_imap_folders: Connecting to IMAP server: {}:{}", config.imap_host, config.imap_port);

    let folder_names = imap_pool::with_session(&target, |session| {
        let mailboxes = session.list(Some(""), Some("*"))?;
        Ok(mailboxes.iter().map(|mb| mb.name().to_string()).collect::<Vec<String>>())
    })?;

    debug_log!("[RUST] list_imap_folders: Found {} folders", folder_names.len());
    Ok(folder_names)
}

/// Test IMAP connection with the given config. Returns Ok(()) if successful, Err otherwise.
pub async fn test_imap_connection(config: &EmailConfig) -> Result<()> {
    let target = ImapTarget::from_config(config);
    debug_log!("[RUST] Testing IMAP connection to: {}:{} (TLS: {})", config.imap_host, config.imap_port, config.use_tls);

    // checkout connects (or validates a warm connection), proving credentials
    // and reachability; with_session returns it to the pool so the test also
    // warms the pool for the next real operation.
    imap_pool::with_session(&target, |session| {
        session.noop().map_err(|e| anyhow::anyhow!("{}", e))
    })?;

    debug_log!("[RUST] IMAP connection test successful");
    Ok(())
}

/// Test SMTP connection with the given config. Returns Ok(()) if successful, Err otherwise.
pub async fn test_smtp_connection(config: &EmailConfig) -> Result<()> {
    debug_log!("[RUST] test_smtp_connection: Starting SMTP connection test");
    debug_log!("[RUST] test_smtp_connection: SMTP Host: {}, Port: {}", config.smtp_host, config.smtp_port);
    debug_log!("[RUST] test_smtp_connection: Email: {}, Use TLS: {}", config.email_address, config.use_tls);
    
    let creds = Credentials::new(config.email_address.clone(), config.password.clone());

    // Build the mailer with proper TLS configuration
    let mut mailer_builder = SmtpTransport::relay(&config.smtp_host)?
        .port(config.smtp_port)
        .credentials(creds);

    // Configure TLS based on the use_tls setting
    if config.use_tls {
        let tls_params = lettre::transport::smtp::client::TlsParameters::new(config.smtp_host.clone())?;
        mailer_builder = mailer_builder.tls(lettre::transport::smtp::client::Tls::Required(tls_params));
    } else {
        // Explicitly disable TLS/STARTTLS for mock servers
        mailer_builder = mailer_builder.tls(lettre::transport::smtp::client::Tls::None);
    }

    let mailer = mailer_builder.build();

    debug_log!("[RUST] test_smtp_connection: Mailer built, testing connection...");
    
    // Test the connection with a timeout
    let mailer_clone = mailer.clone();
    let test_future = task::spawn_blocking(move || {
        debug_log!("[RUST] test_smtp_connection: Executing connection test in blocking thread");
        mailer_clone.test_connection()
    });
    
    match timeout(Duration::from_secs(30), test_future).await {
        Ok(join_res) => match join_res {
            Ok(test_res) => match test_res {
                Ok(_) => {
                    debug_log!("[RUST] test_smtp_connection: SMTP connection test successful");
                    Ok(())
                }
                Err(e) => {
                    debug_log!("[RUST] test_smtp_connection: SMTP connection test failed: {}", e);
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
                debug_log!("[RUST] test_smtp_connection: Task join error: {}", e);
                Err(anyhow::anyhow!("SMTP connection join error: {}", e))
            }
        },
        Err(_) => {
            debug_log!("[RUST] test_smtp_connection: SMTP connection test timed out after 30 seconds");
            Err(anyhow::anyhow!("SMTP connection test timed out after 30 seconds. Check your internet connection and SMTP settings."))
        }
    }
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
    
    // Recursively extract attachments from all subparts
    extract_attachments_recursive(email, &mut attachments, is_manifest_encrypted, 0);
    
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
    
    // Check Content-Type of this part
    let content_type = part.headers.get_first_value("Content-Type").unwrap_or_default();
    let content_disposition = part.headers.get_first_value("Content-Disposition").unwrap_or_default();

    // Check if this part itself is an attachment
    let is_attachment = content_disposition.to_lowercase().contains("attachment") ||
                       content_disposition.to_lowercase().contains("filename");

    let is_multipart = content_type.to_lowercase().starts_with("multipart/");
    let is_text = content_type.to_lowercase().starts_with("text/");

    // If this is a multipart container, recurse into subparts
    if is_multipart {
        for (_idx, subpart) in part.subparts.iter().enumerate() {
            extract_attachments_recursive(subpart, attachments, is_manifest_encrypted, depth + 1);
        }
    } else if is_attachment || (!is_text && !content_type.is_empty()) {
        // This part is an attachment (has Content-Disposition: attachment or is non-text)

        // Extract filename from Content-Disposition or Content-Type
        let filename = extract_filename_from_headers(&content_disposition, &content_type)
            .unwrap_or_else(|| format!("attachment_{}.dat", attachments.len()));

        // Get attachment data
        if let Ok(attachment_data) = part.get_body_raw() {
            
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
            debug_log!("[RUST] Extracted attachment: {} ({} bytes)", filename, attachment_data.len());
        }
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

/// Extract sender pubkey from headers, falling back to the outermost armor
/// signature block's pubkey when the `X-Nostr-Pubkey` header is absent.
/// The armor pubkey is only used if its inline signature verifies successfully,
/// and is converted from hex to npub (bech32) to match the header format.
pub fn extract_sender_pubkey_with_armor_fallback(raw_headers: &str, body_text: &str) -> Option<String> {
    if let Some(pk) = extract_nostr_pubkey_from_headers(raw_headers) {
        return Some(pk);
    }
    // No header — try the outermost armor signature block
    if let Some(parsed) = parse_armor_components(body_text) {
        if let (Some(sig_hex), Some(pubkey_hex)) = (&parsed.signature_hex, &parsed.sig_pubkey_hex) {
            // Verify the signature before trusting this pubkey
            let binary = extract_ciphertext_binary(body_text);
            if let Ok(true) = crate::crypto::verify_signature_bytes(pubkey_hex, sig_hex, &binary) {
                // Convert hex pubkey to npub bech32
                if let Ok(pk) = nostr_sdk::prelude::PublicKey::from_hex(pubkey_hex) {
                    // to_bech32 is infallible for PublicKey
                    let npub = nostr_sdk::prelude::ToBech32::to_bech32(&pk).expect("bech32 encode");
                    debug_log!("[RUST] extract_sender_pubkey_with_armor_fallback: using verified armor pubkey {} ({})", &npub[..std::cmp::min(npub.len(), 20)], &pubkey_hex[..std::cmp::min(pubkey_hex.len(), 16)]);
                    return Some(npub);
                }
            }
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

/// Extract the X-Nostr-Recipient header value if present. This is an
/// unauthenticated assertion by the sender that the body was encrypted to this
/// recipient pubkey — callers must still verify (decryption will fail if the
/// claim is false). Lets clients anchor decryption without a Nostr relay.
pub fn extract_nostr_recipient_from_headers(raw_headers: &str) -> Option<String> {
    for line in raw_headers.lines() {
        if line.to_lowercase().starts_with("x-nostr-recipient:") {
            return Some(line.split_once(':').unwrap_or(("", "")).1.trim().to_string());
        }
    }
    None
}

/// Extract the body content from an ASCII-armored email.
/// Finds the content between the BEGIN ENCRYPTED line and the next armor boundary
/// (END, SIGNATURE, or SEAL marker). Returns None if no armor is found.
/// Find the offset of `marker` in `text` where it appears on a non-quoted line.
/// A non-quoted line is one where the "-----" prefix starts at the beginning of the line,
/// not after a ">" quote prefix. This skips markers inside nested quoted replies.
#[allow(dead_code)]
fn find_unquoted_marker(text: &str, marker: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find(marker) {
        let abs_pos = search_from + pos;
        // Walk back from the marker to find the start of this line
        let line_start = text[..abs_pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prefix = text[line_start..abs_pos].trim_start();
        // Non-quoted: line prefix should be only dashes (e.g. "-----"), not "> -----"
        if !prefix.starts_with('>') {
            return Some(abs_pos);
        }
        search_from = abs_pos + marker.len();
    }
    None
}

#[allow(dead_code)]
fn extract_armor_body_content(body: &str) -> Option<&str> {
    // Find the BEGIN line (encrypted or signed plaintext)
    let begin_idx = body.find("BEGIN NOSTR NIP-")
        .or_else(|| body.find("BEGIN NOSTR SIGNED"))?;
    // Find the end of that line (first newline after BEGIN)
    let line_end = body[begin_idx..].find('\n').map(|i| begin_idx + i + 1)?;

    // Content ends at the next non-quoted armor marker line.
    // Skip quoted markers like "> ----- BEGIN NOSTR SIGNATURE -----" in nested replies;
    // only match markers where "-----" starts at the beginning of a line (no ">" prefix).
    let content_region = &body[line_end..];
    let content_end = find_unquoted_marker(content_region, "BEGIN NOSTR SIGNATURE")
        .or_else(|| find_unquoted_marker(content_region, "BEGIN NOSTR SEAL"))
        .or_else(|| find_unquoted_marker(content_region, "END NOSTR"))
        .unwrap_or(content_region.len());

    // Walk back past the dash prefix on the marker line (e.g. "----- BEGIN...")
    let end_abs = line_end + content_end;
    let trimmed_end = body[line_end..end_abs].trim_end().len() + line_end;
    // Strip trailing dashes from the content (the "-----" prefix of the next marker line)
    let content = body[line_end..trimmed_end].trim_end_matches('-').trim();

    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

/// Decode a fixed-width payload produced by `encode_base_n("bitpack_fixed")`
/// — the same codec that `glossia_encode_raw_base_n` (lib.rs) uses for
/// signature and pubkey blocks. Mirrors that encoder exactly: no header
/// word, byte length known up front.
///
/// Tries each (language, wordlist) candidate ranked by `detect_dialect`
/// and returns the first decode that produces exactly `expected_bytes`.
/// This is the correct decoder for SIGNATURE/SEAL block content. The
/// `decode_from_language` path used by `try_glossia_decode_to_bytes`
/// assumes a leading bitpack header word and is wrong for raw fixed
/// payloads (it round-trips by luck for high-entropy inputs).
/// The only (language, wordlist) pairs nostr-mail ever emits as `bitpack_fixed`
/// raw payloads — Latin and English-BIP39 (see `glossia_roundtrip_to_bytes`'s
/// encoding map and the per-field `glossia_encoding_*` settings). `detect_dialect`
/// will otherwise also propose large *cover-word* vocabularies (e.g.
/// `english/lemmas`, `english/ngram` — 2^17 words each) because the Latin payload
/// words incidentally overlap them, but those are never valid payload encodings
/// here. Probing them cost a ~50ms cold `WordlistTree` build each (the "190ms
/// decode_sig_and_pubkey" outlier) for a tree no nostr-mail payload is encoded
/// against. We feed this allowlist to `glossia::detect_dialect_with`, which drops
/// non-allowed wordlists *before* its binary searches (glossia #14).
const PAYLOAD_WORDLISTS: &[(&str, &str)] = &[("latin", "default"), ("english", "bip39")];

fn try_decode_raw_base_n_fixed(text: &str, expected_bytes: usize) -> Option<Vec<u8>> {
    let words: Vec<String> = text.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return None;
    }

    let mut filter = glossia::DialectFilter::new();
    for (language, wordlist) in PAYLOAD_WORDLISTS {
        filter = filter.allow_wordlist(*language, *wordlist);
    }
    let candidates = glossia::detect_dialect_with(&words, &filter);
    if candidates.is_empty() {
        return None;
    }

    for cand in candidates {
        // Shared, process-wide-cached tree (glossia #12) — built at most once per
        // dialect across glossia's encode pipeline and our decode path. Its
        // internal index is already lowercased, so `contains()` IS the membership
        // check; no separate payload set is needed.
        let payload_tree = match glossia::cached_payload_tree(&cand.language, &cand.wordlist) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Normalize every input token the same way (lowercase, strip leading/trailing
        // non-alphanumerics, drop empties), then split into kept-vs-dropped against
        // the payload wordlist. We need the unfiltered set to enforce strictness below.
        let all_tokens: Vec<String> = text
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
        let extracted: Vec<String> = all_tokens
            .iter()
            .filter(|w| payload_tree.contains(w.as_str()))
            .cloned()
            .collect();

        if extracted.is_empty() {
            continue;
        }

        // Strictness: this function is meant for pure bitpack_fixed-encoded blobs
        // (all words from the same wordlist, no markers, no mixed content). If the
        // payload-word filter dropped any tokens, the input isn't pure — e.g. it's
        // a SIGNATURE block whose last line is an npub. Bail so decode_sig_and_pubkey
        // falls through to Phase 2 (separate-line sig + pubkey) instead of padding
        // the partial bit-stream up to expected_bytes with zeros and yielding
        // garbage sig/pubkey bytes that crash verification with "malformed public
        // key". See `manifest_attachment_default_jsformat_inline_sig_verifies` test.
        if extracted.len() != all_tokens.len() {
            continue;
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            glossia::codec::decode_base_n_fixed(&extracted, &payload_tree, "bitpack_fixed", expected_bytes)
        }));

        if let Ok(Ok(bytes)) = result {
            if bytes.len() == expected_bytes {
                return Some(bytes);
            }
        }
    }
    None
}

/// Try to decode glossia-encoded text (BIP39/Latin words) back to binary bytes.
/// Returns None if the text doesn't appear to be glossia-encoded or decode fails.
/// Uses catch_unwind because glossia's codec can panic on malformed input
/// (e.g. partial wordlist matches with bad padding).
fn try_glossia_decode_to_bytes(text: &str) -> Option<Vec<u8>> {
    let cached = glossia_detect_and_decode_cached(text)?;
    // decode_from_language returns hex when bytes aren't valid UTF-8 (i.e. binary ciphertext)
    if let Some(bytes) = glossia::hex_decode(&cached.decoded) {
        Some(bytes)
    } else {
        Some(cached.decoded.clone().into_bytes())
    }
}

/// Glossia round-trip: encode plaintext bytes into the given language, then decode back
/// to get canonical bytes. This ensures signature verification survives transport
/// (word-wrap, quote prefixes, etc.) because the signature is on the decoded binary.
/// Returns None if glossia encode/decode fails (caller should fall back to raw UTF-8 bytes).
pub fn glossia_roundtrip_to_bytes(text: &str, encoding: &str) -> Option<Vec<u8>> {
    let hex_input = glossia::hex_encode(text.as_bytes());
    // Map frontend encoding names to glossia parameters
    let encoding_lower = encoding.to_lowercase();
    let (language, wordlist) = match encoding_lower.as_str() {
        "latin" => ("latin", "default"),
        "english" | "english - bip39" => ("english", "bip39"),
        _ => (encoding_lower.as_str(), "default"),
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        glossia::encode_into_language(
            &hex_input, language, wordlist, "body",
            None, 42, false, None, None, None, None,
        )
    }));
    let encoded = match result {
        Ok(Ok((encoded_text, _, _, _))) => encoded_text,
        _ => {
            debug_log!("[RUST] glossia_roundtrip_to_bytes: encode failed for encoding={}", encoding);
            return None;
        }
    };
    // Decode back to bytes
    try_glossia_decode_to_bytes(&encoded)
}

/// Decode a single section of armor body content (non-quoted lines only) to bytes.
/// Tries glossia decode, then base64 decode. Returns None if all fail.
pub fn decode_armor_section(content: &str) -> Option<Vec<u8>> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    // Try glossia decode first (handles BIP39/Latin encoded content)
    if let Some(bytes) = try_glossia_decode_to_bytes(content) {
        return Some(bytes);
    }
    // Strip all whitespace from the base64 content
    let b64_clean: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    if !b64_clean.is_empty() {
        // Check for NIP-04 format: base64?iv=base64
        if let Some((payload_b64, iv_b64)) = b64_clean.split_once("?iv=") {
            if let (Ok(payload), Ok(iv)) = (
                general_purpose::STANDARD.decode(payload_b64),
                general_purpose::STANDARD.decode(iv_b64),
            ) {
                let mut combined = payload;
                combined.extend_from_slice(&iv);
                return Some(combined);
            }
        }
        // NIP-44: pure base64
        if let Ok(decoded) = general_purpose::STANDARD.decode(&b64_clean) {
            return Some(decoded);
        }
    }
    None
}

/// Parse armor structure with depth counting, separating outermost body from nested armor.
/// Returns (body_text, nested_armor) where nested_armor has one level of "> " prefix stripped.
/// Handles both non-quoted nested armor (reply chains) and > quoted nested armor.
fn parse_armor_depth(body: &str) -> Option<(String, Option<String>)> {
    let body = body.replace("\r\n", "\n");
    let lines: Vec<&str> = body.lines().collect();

    let contains_begin_body = |l: &str| {
        l.contains("BEGIN NOSTR NIP-") || l.contains("BEGIN NOSTR SIGNED")
    };
    let contains_sig_seal = |l: &str| {
        l.contains("BEGIN NOSTR SIGNATURE") || l.contains("BEGIN NOSTR SEAL")
    };
    let contains_end = |l: &str| l.contains("END NOSTR");

    let mut depth: i32 = 0;
    let mut in_body = false;
    let mut in_nested = false;
    let mut body_lines: Vec<&str> = Vec::new();
    let mut nested_lines: Vec<&str> = Vec::new();

    for line in &lines {
        if !in_body && !in_nested {
            if contains_begin_body(line) {
                depth = 1;
                in_body = true;
            }
            continue;
        }

        if in_body {
            if contains_begin_body(line) {
                depth += 1;
                in_nested = true;
                in_body = false;
                nested_lines.push(line);
                continue;
            }
            if depth == 1 && (contains_sig_seal(line) || contains_end(line)) {
                break;
            }
            body_lines.push(line);
            continue;
        }

        if in_nested {
            nested_lines.push(line);
            if contains_begin_body(line) {
                depth += 1;
            }
            if contains_end(line) {
                depth -= 1;
                if depth == 1 {
                    in_nested = false;
                    in_body = true;
                }
            }
        }
    }

    let body_text = body_lines.join("\n").trim().to_string();
    if body_text.is_empty() {
        return None;
    }

    let nested = if nested_lines.is_empty() {
        None
    } else {
        let stripped: Vec<&str> = nested_lines.iter().map(|l| {
            if l.starts_with("> ") { &l[2..] }
            else if *l == ">" { "" }
            else { *l }
        }).collect();
        Some(stripped.join("\n").trim().to_string())
    };

    Some((body_text, nested))
}

/// Decode a combined 96-byte signature+pubkey block.
/// Tries glossia decode first (for encoded content), then raw hex fallback.
/// Returns (sig_hex_128_chars, pubkey_hex_64_chars) or None.
/// Try to decode text as a 32-byte pubkey.
/// Accepts: glossia (32 bytes), npub bech32, hex (64 chars).
fn try_decode_as_pubkey(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() { return None; }

    // Raw bitpack_fixed decode (matches glossia_encode_raw_base_n on the encode side)
    if let Some(bytes) = try_decode_raw_base_n_fixed(trimmed, 32) {
        return Some(hex::encode(bytes));
    }

    // Legacy: decode_from_language path (body-dialect with header word).
    // Kept as a fallback for older messages encoded via the body pipeline.
    if let Some(bytes) = try_glossia_decode_to_bytes(trimmed) {
        if bytes.len() == 32 {
            return Some(hex::encode(bytes));
        }
    }

    let stripped: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();

    // npub bech32
    if stripped.starts_with("npub1") {
        if let Ok(pk) = nostr_sdk::prelude::PublicKey::parse(&stripped) {
            return Some(pk.to_hex());
        }
    }

    // Raw hex (64 chars = 32 bytes)
    if stripped.len() == 64 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(stripped);
    }

    None
}

/// Try to decode text as a 64-byte Schnorr signature.
/// Accepts: glossia (64 bytes), hex (128 chars).
fn try_decode_as_signature(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() { return None; }

    // Raw bitpack_fixed decode (matches glossia_encode_raw_base_n on the encode side)
    if let Some(bytes) = try_decode_raw_base_n_fixed(trimmed, 64) {
        return Some(hex::encode(bytes));
    }

    // Legacy: decode_from_language path (body-dialect with header word).
    if let Some(bytes) = try_glossia_decode_to_bytes(trimmed) {
        if bytes.len() == 64 {
            return Some(hex::encode(bytes));
        }
    }

    // Raw hex (128 chars = 64 bytes)
    let stripped: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if stripped.len() == 128 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(stripped);
    }

    None
}

/// Decode the content of a SIGNATURE block into (sig_hex, pubkey_hex).
///
/// The wire format priority follows the Cap'n Proto schema
/// (`schema/nostr_mail.capnp` lines 188-197):
///
/// ```text
/// CANONICAL (current emit format — JS encodeSigPubkey with default settings):
///   <signature: glossia-encoded or hex — 64 bytes>
///   <pubkey:    glossia-encoded, hex, or npub (bech32) — 32 bytes>
///
/// LEGACY (must-also-accept, never re-emitted by this codebase):
///   <combined 96-byte glossia or hex of sig||pubkey on a single line>
/// ```
///
/// We try the canonical two-line format FIRST. Doing the legacy
/// combined-96 attempts first is what caused the
/// "inline signature invalid" / "malformed public key" bug fixed in
/// `0a8b3e2`: try_decode_raw_base_n_fixed would silently consume the
/// canonical format (dropping the npub line as a non-payload token)
/// and zero-pad the partial bit-stream up to 96 bytes, producing a
/// garbage sig+pubkey pair instead of letting Phase 2 fire. With this
/// ordering Phase 2 always wins for canonical inputs, and the legacy
/// phases are a strict fallback for older messages.
///
/// Returns `Some((sig_hex, pubkey_hex))` for sig (128-char hex / 64 bytes)
/// and pubkey (64-char hex / 32 bytes), or `None` if no format matched.
fn decode_sig_and_pubkey(content: &str) -> Option<(String, String)> {
    // ── Canonical: two-line sig + pubkey ────────────────────────────────
    // Last non-empty line is the pubkey (glossia, hex, or npub); preceding
    // lines together form the signature payload (glossia or hex).
    let lines: Vec<&str> = content.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.len() >= 2 {
        let last = lines[lines.len() - 1];
        if let Some(pk) = try_decode_as_pubkey(last) {
            let sig_text = lines[..lines.len() - 1].join("\n");
            if let Some(sig) = try_decode_as_signature(&sig_text) {
                return Some((sig, pk));
            }
        }
    }

    // ── Legacy: combined 96-byte payload ────────────────────────────────
    // Strict bitpack_fixed: try_decode_raw_base_n_fixed rejects mixed-token
    // inputs (since 0a8b3e2), so this only fires for genuine combined
    // payloads now — keeping it as a safety net for old archived mail.
    if let Some(bytes) = try_decode_raw_base_n_fixed(content, 96) {
        let sig_hex = hex::encode(&bytes[..64]);
        let pubkey_hex = hex::encode(&bytes[64..]);
        return Some((sig_hex, pubkey_hex));
    }
    // Legacy body-dialect glossia (decode_from_language) variant.
    if let Some(bytes) = try_glossia_decode_to_bytes(content) {
        if bytes.len() == 96 {
            let sig_hex = hex::encode(&bytes[..64]);
            let pubkey_hex = hex::encode(&bytes[64..]);
            return Some((sig_hex, pubkey_hex));
        }
    }
    // Legacy raw 192-char hex (whitespace stripped).
    let stripped: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    if stripped.len() == 192 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some((stripped[..128].to_string(), stripped[128..].to_string()));
    }

    None
}

/// Parse a complete ASCII-armored nostr-mail message into a structured result.
/// Extracts body, signature, seal, profile names, prefix text, and nested quoted armor.
///
/// Internally builds a capnp ArmorMessage for schema validation and type identification:
/// - Body union variant (encrypted/signed/plain) determined from BEGIN tag
/// - NipVersion enum (nip04/nip44) set from the tag
/// - SignatureBlock populated with decoded 64-byte sig + 32-byte pubkey
/// - SealBlock populated with decoded 32-byte pubkey
/// - Body.quoted recursively populated for nested reply chains
///
/// Returns a serde-friendly ParsedArmorMessage for Tauri JSON IPC.
/// All serde fields are derived from reading the capnp message — nothing bypasses the schema.
/// Process-wide cache for parse_armor_components results. Keyed by hash of the
/// line-ending-normalized armor text — both decrypt and verify Tauri commands
/// call into this function with the same body, and on first parse we walk the
/// returned tree to pre-populate sub-tree entries (one per nesting level), so
/// verify_all_signatures' recursive parse calls hit cache too. Across thread
/// reopens the entire parse becomes a HashMap lookup.
static PARSE_ARMOR_CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, crate::types::ParsedArmorMessage>>>
    = std::sync::OnceLock::new();

fn hash_armor_text(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Recursively insert each nested level of the parsed tree into the cache.
/// Each level's cache key is hash(level.quoted_armor_text), matching what a
/// top-level parse_armor_components(quoted_armor_text) would compute.
fn populate_parse_subtree_cache(
    node: &crate::types::ParsedArmorMessage,
    cache: &mut std::collections::HashMap<u64, crate::types::ParsedArmorMessage>,
) {
    if let (Some(quoted_text), Some(quoted_box)) = (node.quoted_armor_text.as_ref(), node.quoted.as_ref()) {
        let key = hash_armor_text(quoted_text);
        cache.entry(key).or_insert_with(|| (**quoted_box).clone());
        populate_parse_subtree_cache(quoted_box, cache);
    }
}

pub fn parse_armor_components(armor_text: &str) -> Option<crate::types::ParsedArmorMessage> {
    use crate::nostr_mail_capnp;

    // Normalize line endings up front so the cache key doesn't fragment between
    // callers (verify_all_signatures passes raw \r\n text from the DB, decrypt_email_body_pipeline
    // passes its own pre-normalized copy). The downstream populate_armor_from_text
    // also normalizes, but that's a cheap no-op on already-normalized input.
    let normalized: std::borrow::Cow<str> = if armor_text.contains("\r\n") {
        std::borrow::Cow::Owned(armor_text.replace("\r\n", "\n"))
    } else {
        std::borrow::Cow::Borrowed(armor_text)
    };
    let cache_key = hash_armor_text(&normalized);

    let cache_map = PARSE_ARMOR_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(cached) = cache_map.lock().unwrap().get(&cache_key) {
        debug_log!("[RUST-PERF] parse_armor cache HIT key={:x} (in={}b, has_quoted={})",
            cache_key, armor_text.len(), cached.quoted.is_some());
        return Some(cached.clone());
    }

    let preview: String = armor_text.chars().take(120).collect();
    debug_log!("[RUST] parse_armor_components: input length={} preview={:?}", armor_text.len(), preview);

    let perf = std::time::Instant::now();

    // Build the capnp ArmorMessage — this is the parsing target
    let perf_populate = std::time::Instant::now();
    let mut capnp_msg = ::capnp::message::Builder::new_default();
    let (prefix_text, raw_quoted_text) = {
        let armor_builder = capnp_msg.init_root::<nostr_mail_capnp::armor_message::Builder>();
        populate_armor_from_text(armor_builder, &normalized)?
    };
    let populate_ms = perf_populate.elapsed().as_millis();

    // Read the capnp message → serde struct (all fields derived from capnp)
    let perf_serde = std::time::Instant::now();
    let reader = capnp_msg.get_root_as_reader::<nostr_mail_capnp::armor_message::Reader>().ok()?;
    let mut result = armor_message_to_serde(reader);
    let serde_ms = perf_serde.elapsed().as_millis();
    result.prefix_text = prefix_text;
    // Override quoted_armor_text with the verbatim text from the input (the capnp
    // encoded_content field only stores the inner body text, losing delimiters,
    // signatures, and deeper nesting levels).
    if raw_quoted_text.is_some() {
        result.quoted_armor_text = raw_quoted_text;
    }

    debug_log!("[RUST] parse_armor_components: success body_type={} nip={:?} has_sig={} has_seal={} has_quoted={}",
        result.body_type, result.encryption_nip, result.signature_hex.is_some(),
        result.seal_pubkey_hex.is_some(), result.quoted.is_some());
    debug_log!("[RUST-PERF] parse_armor cache MISS key={:x} compute={}ms (in={}b, has_quoted={})",
        cache_key, perf.elapsed().as_millis(), armor_text.len(), result.quoted.is_some());

    // Insert outer + each nested level under its own hash key so
    // verify_all_signatures' recursive parse calls hit cache as well.
    let perf_subtree = std::time::Instant::now();
    {
        let mut guard = cache_map.lock().unwrap();
        maybe_evict(&mut guard);
        guard.insert(cache_key, result.clone());
        populate_parse_subtree_cache(&result, &mut guard);
    }
    let subtree_ms = perf_subtree.elapsed().as_millis();

    // Sub-phase breakdown of the parse_armor cache-miss path. Lets us localize the
    // ~500ms baseline: populate = text state machine + capnp build, serde =
    // capnp→struct conversion (incl. NIP-04 glossia decode, deferred for NIP-44),
    // subtree_cache = recursive clone of nested levels into the parse cache.
    debug_log!("[RUST-PERF] parse_armor breakdown key={:x} total={}ms = populate={}ms + serde={}ms + subtree_cache={}ms (in={}b, normalized={}b, body={}b, nip={:?}, has_quoted={})",
        cache_key, perf.elapsed().as_millis(), populate_ms, serde_ms, subtree_ms,
        armor_text.len(), normalized.len(), result.body_text.len(),
        result.encryption_nip, result.quoted.is_some());

    Some(result)
}

/// Populate a capnp ArmorMessage builder from armor text.
/// Writes directly into capnp builders — the capnp message IS the parsing result.
/// Returns (prefix_text, quoted_armor_text) or None if no armor found.
/// `quoted_armor_text` is the raw text of nested quoted levels (with delimiters and signatures),
/// preserved verbatim from the input for round-tripping.
fn populate_armor_from_text(
    mut armor_builder: crate::nostr_mail_capnp::armor_message::Builder<'_>,
    armor_text: &str,
) -> Option<(Option<String>, Option<String>)> {
    use crate::nostr_mail_capnp;

    let normalized = armor_text.replace("\r\n", "\n");

    // Extract prefix text before first armor delimiter (accept both "----- BEGIN" and "-----BEGIN")
    let armor_start = normalized.find("----- BEGIN NOSTR ")
        .or_else(|| normalized.find("-----BEGIN NOSTR "))
        .or_else(|| normalized.find("--- BEGIN NOSTR "));
    let prefix_text = match armor_start {
        Some(idx) if idx > 0 => {
            let p = normalized[..idx].trim();
            if p.is_empty() { None } else { Some(p.to_string()) }
        }
        _ => None,
    };
    let armor_start = match armor_start {
        Some(idx) => idx,
        None => {
            debug_log!("[RUST] populate_armor_from_text: no armor delimiter found");
            return None;
        }
    };

    // ── Phase A: Line extraction (state machine) ──
    let lines: Vec<&str> = normalized[armor_start..].lines().collect();

    let is_begin_body = |l: &str| -> bool {
        let t = l.trim().trim_matches('-').trim();
        t.starts_with("BEGIN NOSTR NIP-") || t.starts_with("BEGIN NOSTR SIGNED")
    };
    let is_begin_sig = |l: &str| -> bool {
        let t = l.trim().trim_matches('-').trim();
        t == "BEGIN NOSTR SIGNATURE"
    };
    let is_begin_seal = |l: &str| -> bool {
        let t = l.trim().trim_matches('-').trim();
        t == "BEGIN NOSTR SEAL"
    };
    let is_end = |l: &str| -> bool {
        let t = l.trim().trim_matches('-').trim();
        t.starts_with("END NOSTR")
    };

    let mut depth: i32 = 0;
    let mut state = "before";
    let mut begin_tag = String::new();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut quoted_armor_lines: Vec<&str> = Vec::new();
    let mut sig_lines: Vec<&str> = Vec::new();
    let mut seal_lines: Vec<&str> = Vec::new();

    for line in &lines {
        match state {
            "before" => {
                if is_begin_body(line) {
                    depth = 1;
                    state = "body";
                    begin_tag = line.trim().to_string();
                }
            }
            "body" => {
                if is_begin_body(line) {
                    depth += 1;
                    state = "quoted";
                    quoted_armor_lines.push(line);
                } else if is_begin_sig(line) && depth == 1 {
                    state = "sig";
                } else if is_begin_seal(line) && depth == 1 {
                    state = "seal";
                } else if is_end(line) && depth == 1 {
                    state = "done";
                } else {
                    body_lines.push(line);
                }
            }
            "quoted" => {
                quoted_armor_lines.push(line);
                if is_begin_body(line) { depth += 1; }
                else if is_end(line) {
                    depth -= 1;
                    if depth == 1 { state = "body"; }
                }
            }
            "sig" => {
                if is_end(line) { state = "done"; }
                else if is_begin_seal(line) { state = "seal"; }
                else { sig_lines.push(line); }
            }
            "seal" => {
                if is_end(line) { state = "done"; }
                else { seal_lines.push(line); }
            }
            _ => {}
        }
    }

    if state == "before" {
        debug_log!("[RUST] populate_armor_from_text: state machine never left 'before'");
        return None;
    }

    let body_text = body_lines.join("\n").trim().to_string();

    // ── Phase B: Write directly into capnp builders ──

    // Body: set union variant + decoded bytes + encoded content
    let mut body_builder = armor_builder.reborrow().init_body();
    body_builder.set_encoded_content(&body_text);

    let is_encrypted = begin_tag.contains("ENCRYPTED");
    let is_signed_body = begin_tag.contains("SIGNED");

    if is_encrypted {
        let mut enc = body_builder.reborrow().init_encrypted();
        let nip_version = if begin_tag.contains("NIP-04") {
            nostr_mail_capnp::NipVersion::Nip04
        } else {
            nostr_mail_capnp::NipVersion::Nip44
        };
        enc.set_nip(nip_version);
        // Only NIP-04 actually consumes the pre-decoded ciphertext bytes (used as
        // the MAC payload during signature verification, see decrypt_armor_tree).
        // NIP-44 re-runs glossia decode in decrypt_single_block on the encoded
        // body_text and never reads enc.ciphertext, so doing the decode here
        // wastes a full glossia detect+decode round trip per nesting level — the
        // single biggest cost in parse_armor_components according to profiling.
        if matches!(nip_version, nostr_mail_capnp::NipVersion::Nip04) {
            if let Some(bytes) = decode_armor_section(&body_text) {
                enc.set_ciphertext(&bytes);
            }
        }
    } else if is_signed_body {
        let mut sgn = body_builder.reborrow().init_signed();
        if let Some(bytes) = decode_armor_section(&body_text) {
            sgn.set_plaintext(&bytes);
        }
    } else {
        let mut pln = body_builder.reborrow().init_plain();
        pln.set_text(&body_text);
    }

    // Quoted: recursively populate nested ArmorMessage
    let stripped_quoted: Vec<&str> = quoted_armor_lines.iter().map(|l| {
        if l.starts_with("> ") { &l[2..] }
        else if *l == ">" { "" }
        else { *l }
    }).collect();
    let raw_quoted_text = if !stripped_quoted.is_empty() {
        let quoted_text = stripped_quoted.join("\n").trim().to_string();
        if !quoted_text.is_empty() {
            let quoted_builder = body_builder.init_quoted();
            // Recursive: populate the nested ArmorMessage
            populate_armor_from_text(quoted_builder, &quoted_text);
            Some(quoted_text)
        } else {
            None
        }
    } else {
        None
    };

    // Signature block: decode sig+pubkey, write to capnp
    if !sig_lines.is_empty() {
        let mut sig_builder = armor_builder.reborrow().init_signature();

        if let Some(name_line) = sig_lines.iter().find(|l| l.trim().starts_with('@')) {
            sig_builder.set_profile_name(name_line.trim().trim_start_matches('@'));
        }

        let content_lines: Vec<&str> = sig_lines.iter()
            .filter(|l| !l.trim().starts_with('@'))
            .copied()
            .collect();
        let all_content = content_lines.join("\n").trim().to_string();
        if !all_content.is_empty() {
            sig_builder.set_encoded_sig_pubkey(&all_content);
            // Time the signature-block decode in isolation. Suspected to dominate
            // the parse "populate" phase: decode_sig_and_pubkey can build a fresh
            // 32k-word WordlistTree per attempt (try_decode_as_pubkey /
            // try_decode_as_signature / try_decode_raw_base_n_fixed) with no cache.
            let perf_sig_decode = std::time::Instant::now();
            let decoded_sig = decode_sig_and_pubkey(&all_content);
            debug_log!("[RUST-PERF] populate: decode_sig_and_pubkey={}ms (sig_content={}b, lines={})",
                perf_sig_decode.elapsed().as_millis(), all_content.len(), content_lines.len());
            if let Some((sig_hex, pubkey_hex)) = decoded_sig {
                if let Ok(sig_bytes) = hex::decode(&sig_hex) {
                    sig_builder.set_signature(&sig_bytes);
                }
                if let Ok(pk_bytes) = hex::decode(&pubkey_hex) {
                    sig_builder.set_pubkey(&pk_bytes);
                }
            }
        }
    }

    // Seal block: decode pubkey, write to capnp
    if !seal_lines.is_empty() {
        let mut seal_builder = armor_builder.reborrow().init_seal();

        if let Some(name_line) = seal_lines.iter().find(|l| l.trim().starts_with('@')) {
            seal_builder.set_display_name(name_line.trim().trim_start_matches('@'));
        }

        let content_lines: Vec<&str> = seal_lines.iter()
            .filter(|l| !l.trim().starts_with('@'))
            .copied()
            .collect();
        let seal_content = content_lines.join("\n").trim().to_string();
        if !seal_content.is_empty() {
            if let Some(pk_hex) = try_decode_as_pubkey(&seal_content) {
                if let Ok(bytes) = hex::decode(&pk_hex) {
                    seal_builder.set_pubkey(&bytes);
                }
            }
        }
    }

    Some((prefix_text, raw_quoted_text))
}

/// Read a capnp ArmorMessage and produce a serde-friendly ParsedArmorMessage.
/// All fields are derived from the capnp reader — nothing bypasses the schema.
fn armor_message_to_serde(reader: crate::nostr_mail_capnp::armor_message::Reader) -> crate::types::ParsedArmorMessage {
    use crate::nostr_mail_capnp;

    // Read body
    let (body_text, body_type, encryption_nip, body_bytes_b64) = if reader.has_body() {
        if let Ok(body) = reader.get_body() {
            let encoded = if body.has_encoded_content() {
                body.reborrow().get_encoded_content().map(|s| s.to_string().unwrap_or_default()).unwrap_or_default()
            } else {
                String::new()
            };

            match body.which() {
                Ok(nostr_mail_capnp::body::Encrypted(enc)) => {
                    let nip = match enc.get_nip() {
                        Ok(nostr_mail_capnp::NipVersion::Nip04) => "nip04",
                        _ => "nip44",
                    };
                    let bytes_b64 = if enc.has_ciphertext() {
                        enc.get_ciphertext().ok().map(|d| general_purpose::STANDARD.encode(d))
                    } else {
                        None
                    };
                    (encoded, "encrypted".to_string(), Some(nip.to_string()), bytes_b64)
                }
                Ok(nostr_mail_capnp::body::Signed(sgn)) => {
                    let bytes_b64 = if sgn.has_plaintext() {
                        sgn.get_plaintext().ok().map(|d| general_purpose::STANDARD.encode(d))
                    } else {
                        None
                    };
                    (encoded, "signed".to_string(), None, bytes_b64)
                }
                Ok(nostr_mail_capnp::body::Plain(pln)) => {
                    let text = pln.get_text().map(|t| t.to_string().unwrap_or_default()).unwrap_or_default();
                    (text, "plain".to_string(), None, None)
                }
                _ => (encoded, "unknown".to_string(), None, None),
            }
        } else {
            (String::new(), "unknown".to_string(), None, None)
        }
    } else {
        (String::new(), "unknown".to_string(), None, None)
    };

    // Read signature block
    let (signature_hex, sig_pubkey_hex, profile_name, raw_sig_pubkey) = if reader.has_signature() {
        if let Ok(sig) = reader.get_signature() {
            let sig_hex = if sig.has_signature() {
                sig.get_signature().ok().map(|d| hex::encode(d))
            } else {
                None
            };
            let pk_hex = if sig.has_pubkey() {
                sig.get_pubkey().ok().map(|d| hex::encode(d))
            } else {
                None
            };
            let name = if sig.has_profile_name() {
                sig.get_profile_name().ok().and_then(|s| s.to_str().ok()).map(|s| s.to_string())
            } else {
                None
            };
            let raw = if sig.has_encoded_sig_pubkey() {
                sig.get_encoded_sig_pubkey().ok().and_then(|s| s.to_str().ok()).map(|s| s.to_string())
            } else {
                None
            };
            (sig_hex, pk_hex, name, raw)
        } else {
            (None, None, None, None)
        }
    } else {
        (None, None, None, None)
    };

    // Read seal block
    let (seal_pubkey_hex, seal_display_name) = if reader.has_seal() {
        if let Ok(seal) = reader.get_seal() {
            let pk_hex = if seal.has_pubkey() {
                seal.get_pubkey().ok().map(|d| hex::encode(d))
            } else {
                None
            };
            let name = if seal.has_display_name() {
                seal.get_display_name().ok().and_then(|s| s.to_str().ok()).map(|s| s.to_string())
            } else {
                None
            };
            (pk_hex, name)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let display_name = seal_display_name.or_else(|| profile_name.clone());

    // Read quoted (recursive)
    // Note: quoted_armor_text is set to None here — parse_armor_components overrides
    // it with the verbatim text from the input, since capnp only stores structured
    // fields (encoded_content), not the full armor with delimiters and signatures.
    let (quoted, quoted_armor_text) = if reader.has_body() {
        if let Ok(body) = reader.get_body() {
            if body.has_quoted() {
                if let Ok(quoted_reader) = body.get_quoted() {
                    let inner = armor_message_to_serde(quoted_reader);
                    (Some(Box::new(inner)), None)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    crate::types::ParsedArmorMessage {
        body_text,
        body_type,
        encryption_nip,
        signature_hex,
        sig_pubkey_hex,
        seal_pubkey_hex,
        profile_name,
        display_name,
        raw_sig_pubkey,
        prefix_text: None, // Set by caller (prefix_text is outside the capnp message)
        quoted,
        quoted_armor_text,
        body_bytes_b64,
    }
}

// ── Shared glossia/NIP postprocess helpers ──────────────────────────

/// Post-process glossia decode output: hex→base64 conversion and NIP-04 unpacking.
/// Mirrors JS: _isHex → _hexToBase64 → _autoUnpack pipeline.
/// Moved from lib.rs decode_glossia_postprocess so both Tauri commands and email decrypt can use it.
pub fn glossia_postprocess(decoded: &str, algorithm: &str) -> Result<String, String> {
    use base64::Engine;

    let is_hex = !decoded.is_empty()
        && decoded.len() % 2 == 0
        && decoded.chars().all(|c| c.is_ascii_hexdigit());

    if is_hex {
        let bytes: Vec<u8> = (0..decoded.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&decoded[i..i + 2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|e| format!("Hex decode failed: {}", e))?;

        let b64 = base64::engine::general_purpose::STANDARD;
        // NIP-04 binary unpack: [len_hi, len_lo, payload..., iv(16 bytes)] → base64?iv=base64
        if algorithm == "nip04" && bytes.len() >= 2 {
            let payload_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
            if 2 + payload_len <= bytes.len() {
                let payload_b64 = b64.encode(&bytes[2..2 + payload_len]);
                let iv_b64 = b64.encode(&bytes[2 + payload_len..]);
                return Ok(format!("{}?iv={}", payload_b64, iv_b64));
            }
        }
        Ok(b64.encode(&bytes))
    } else {
        // Already a valid string (e.g. base64 for NIP-44) — return as-is
        Ok(decoded.to_string())
    }
}

/// Check if content looks like base64 or base64?iv=base64 (already ciphertext, not glossia).
/// Mirrors JS: /^[A-Za-z0-9+/=?]+$/.test(stripped)
fn is_base64_content(content: &str) -> bool {
    let stripped: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    !stripped.is_empty() && stripped.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c == '?')
}

// is_likely_encrypted_content is defined as pub fn later in this file (line ~3918)

/// Glossia-decode body content to NIP-decrypt-ready ciphertext string.
/// Uses `detect_dialect_best` to pick the (language, wordlist) pair that the
/// prose was encoded with, then decodes once with that pair. Mirrors the
/// subject path (`glossia_decode_subject`) and `try_glossia_decode_to_bytes`.
/// The `nip_hint` is "nip04" or "nip44" from the armor BEGIN tag.
/// Result of glossia detect + decode for a piece of armored body text.
/// Cached process-wide by hash of the input so repeated decodes of the
/// same text (across nesting levels, sender_extract, sig verification,
/// thread reopen, etc.) skip both `detect_dialect_best` and
/// `decode_from_language`.
#[derive(Clone, Debug)]
struct GlossiaDecodeCached {
    language: String,
    wordlist: String,
    /// Output of `glossia::decode_from_language` — either UTF-8 text or
    /// hex of the underlying bytes when the bytes weren't valid UTF-8.
    decoded: String,
    hit_rate: f64,
}

static GLOSSIA_DECODE_CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, GlossiaDecodeCached>>>
    = std::sync::OnceLock::new();

fn hash_glossia_input(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Cached `detect_dialect_best` + `decode_from_language`. Pure function of the
/// input text — the same prose always produces the same dialect + decoded
/// output, so a process-wide cache is safe. Returns None for plaintext or
/// non-glossia input (low hit rate / detection failure).
fn glossia_detect_and_decode_cached(text: &str) -> Option<GlossiaDecodeCached> {
    let key = hash_glossia_input(text);
    let cache = GLOSSIA_DECODE_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));

    if let Some(entry) = cache.lock().unwrap().get(&key) {
        debug_log!("[RUST-PERF] glossia cache HIT key={:x} (in={}b, dialect={}/{}, out={}b)",
            key, text.len(), entry.language, entry.wordlist, entry.decoded.len());
        return Some(entry.clone());
    }

    let perf = std::time::Instant::now();
    let words: Vec<String> = text.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return None;
    }
    let detect_words = words.clone();
    let best = match std::panic::catch_unwind(move || glossia::detect_dialect_best(&detect_words)) {
        Ok(Some(b)) => b,
        _ => return None,
    };
    if best.hit_rate < 0.3 {
        return None;
    }
    let language = best.language.clone();
    let wordlist = best.wordlist.clone();
    let text_owned = text.to_string();
    let decoded = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        glossia::decode_from_language(&text_owned, &language, &wordlist, false)
    })) {
        Ok(Ok(d)) => d,
        _ => return None,
    };
    let entry = GlossiaDecodeCached {
        language,
        wordlist,
        decoded,
        hit_rate: best.hit_rate,
    };
    debug_log!("[RUST-PERF] glossia cache MISS key={:x} compute={}ms (in={}b, words={}, dialect={}/{}, out={}b, hit_rate={:.3})",
        key, perf.elapsed().as_millis(), text.len(), words.len(),
        entry.language, entry.wordlist, entry.decoded.len(), entry.hit_rate);
    {
        let mut guard = cache.lock().unwrap();
        maybe_evict(&mut guard);
        guard.insert(key, entry.clone());
    }
    Some(entry)
}

fn glossia_decode_to_ciphertext(encoded_content: &str, nip_hint: &str) -> Result<String, String> {
    // If already base64 or base64?iv=base64, return as-is
    if is_base64_content(encoded_content) {
        let stripped: String = encoded_content.chars().filter(|c| !c.is_whitespace()).collect();
        return Ok(stripped);
    }

    let cached = glossia_detect_and_decode_cached(encoded_content)
        .ok_or_else(|| "Glossia decode failed: no dialect detected or hit_rate too low".to_string())?;
    debug_log!("[RUST] glossia_decode_to_ciphertext: detected dialect={:?} wordlist={:?} hit_rate={}",
        cached.language, cached.wordlist, cached.hit_rate);
    debug_log!("[RUST] glossia_decode_to_ciphertext: decoded len={}", cached.decoded.len());

    glossia_postprocess(&cached.decoded, nip_hint)
}

/// Glossia-decode subject (payload_only mode, hit_rate >= 0.8).
/// Mirrors JS: decodeGlossiaSubject — uses "decode from <dialect> raw" mode.
/// Detect the NIP version a glossia-encoded email used, from the armor tag on
/// the body. Falls back to "nip44" when no tag is present.
///
/// The NIP type drives `glossia_postprocess`: NIP-04 reconstructs the canonical
/// `base64?iv=base64` shape from the packed bytes, while NIP-44 is plain base64.
/// The companion DM stores raw NIP ciphertext in those same shapes, so picking
/// the right hint here is what makes `subject_hash` match `content_hash`.
fn detect_nip_from_body(body: &str) -> &'static str {
    if body.contains("BEGIN NOSTR NIP-04") {
        "nip04"
    } else if body.contains("BEGIN NOSTR NIP-44") {
        "nip44"
    } else {
        // Unknown — default to nip44 (plain base64 passthrough).
        "nip44"
    }
}

/// Compute the subject_hash for an IMAP-synced encrypted email by attempting
/// glossia-decoding the subject to the NIP ciphertext and hashing those bytes.
///
/// `body` is the email body; its armor tag tells us which NIP variant to use
/// when reconstructing the canonical ciphertext shape from glossia output.
/// Without that hint the postprocess would always pass `"nip44"` and, for
/// NIP-04 subjects, produce a base64-only string instead of `base64?iv=base64`
/// — structurally unable to match a companion DM's `content_hash`.
///
/// Returns None when:
/// - the subject is empty,
/// - glossia detection / decoding fails (e.g. plaintext subject), or
/// - the decoded result doesn't look like NIP ciphertext.
pub fn compute_subject_ciphertext_hash(subject: &str, body: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    // Two acceptable shapes for the hash input:
    //   1. Subject IS already raw ciphertext (e.g., a third-party client that
    //      didn't glossia-encode) — hash it directly.
    //   2. Subject is glossia prose — decode to ciphertext, then hash.
    let ciphertext = if is_likely_encrypted_content(subject) {
        subject.to_string()
    } else {
        // Try the body's NIP first (most likely to match), then the other.
        // Preferring `?iv=`-shaped outputs when both succeed keeps the result
        // canonical for NIP-04 (which the DM side always stores with `?iv=`).
        let primary = detect_nip_from_body(body);
        let fallback = if primary == "nip04" { "nip44" } else { "nip04" };

        let try_one = |hint: &str| -> Option<String> {
            let decoded = glossia_decode_subject(subject, hint)?;
            if !is_likely_encrypted_content(&decoded) {
                return None;
            }
            Some(decoded)
        };

        let primary_decoded = try_one(primary);
        let fallback_decoded = try_one(fallback);

        match (primary_decoded, fallback_decoded) {
            // Prefer the one carrying `?iv=` (canonical NIP-04 shape); the DM
            // side hashes the same canonical form.
            (Some(p), Some(f)) => {
                if p.contains("?iv=") { p }
                else if f.contains("?iv=") { f }
                else { p }
            }
            (Some(p), None) => p,
            (None, Some(f)) => f,
            (None, None) => return None,
        }
    };
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(ciphertext.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

static GLOSSIA_SUBJECT_CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, Option<String>>>>
    = std::sync::OnceLock::new();

fn hash_subject(subject: &str, nip_hint: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    subject.hash(&mut h);
    nip_hint.hash(&mut h);
    h.finish()
}

fn glossia_decode_subject(subject: &str, nip_hint: &str) -> Option<String> {
    // Subject decode tries multiple wordlists and applies nip-specific
    // postprocess, so we memoize the final Option<String> result keyed by
    // (subject, nip_hint). Re-opens of the same email skip the entire
    // detect + decode + postprocess flow.
    let key = hash_subject(subject, nip_hint);
    let cache = GLOSSIA_SUBJECT_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(cached) = cache.lock().unwrap().get(&key) {
        debug_log!("[RUST-PERF] glossia subject cache HIT key={:x} (in={}b, nip={})",
            key, subject.len(), nip_hint);
        return cached.clone();
    }

    // Compute the result via an IIFE so every (early-) return path flows into
    // the cache insertion below without scattering insert calls everywhere.
    let result: Option<String> = (|| -> Option<String> {
        debug_log!("[RUST] glossia_decode_subject: len={} nip_hint={} preview={:?}", subject.len(), nip_hint, &subject[..subject.len().min(80)]);
        if subject.is_empty() || is_likely_encrypted_content(subject) {
            debug_log!("[RUST] glossia_decode_subject: empty or already encrypted, returning None");
            return None;
        }

        // Detect dialect with hit_rate filtering
        let words: Vec<String> = subject.split_whitespace().map(|w| w.to_lowercase()).collect();
        debug_log!("[RUST] glossia_decode_subject: word_count={} words={:?}", words.len(), &words[..words.len().min(6)]);
        if words.is_empty() {
            return None;
        }
        let detect_result = std::panic::catch_unwind(move || {
            glossia::detect_dialect_best(&words)
        });
        let dialect = match detect_result {
            Ok(Some(best)) => {
                debug_log!("[RUST] glossia_decode_subject: detected dialect={:?} hit_rate={}", best.language, best.hit_rate);
                if best.hit_rate >= 0.8 {
                    best.language.clone()
                } else {
                    debug_log!("[RUST] glossia_decode_subject: hit_rate too low (<0.8), returning None");
                    return None;
                }
            }
            Ok(None) => {
                debug_log!("[RUST] glossia_decode_subject: no dialect detected, returning None");
                return None;
            }
            Err(e) => {
                debug_log!("[RUST] glossia_decode_subject: detect_dialect_best panicked: {:?}", e.downcast_ref::<String>());
                return None;
            }
        };

        // Try decoding with both "default" and "raw" wordlists, pick longest
        let wordlists = ["default", "raw"];
        let mut best_decoded: Option<String> = None;
        for wl in &wordlists {
            let text = subject.to_string();
            let lang = dialect.clone();
            let wl_str = wl.to_string();
            let decode_result = std::panic::catch_unwind(move || {
                glossia::decode_from_language(&text, &lang, &wl_str, false)
            });
            match decode_result {
                Ok(Ok(decoded)) => {
                    debug_log!("[RUST] glossia_decode_subject: wl={} decoded len={} preview={:?}", wl, decoded.len(), &decoded[..decoded.len().min(40)]);
                    match &best_decoded {
                        Some(prev) if prev.len() >= decoded.len() => {}
                        _ => { best_decoded = Some(decoded); }
                    }
                }
                Ok(Err(e)) => {
                    debug_log!("[RUST] glossia_decode_subject: wl={} decode error: {:?}", wl, e);
                }
                Err(e) => {
                    debug_log!("[RUST] glossia_decode_subject: wl={} decode panicked: {:?}", wl, e.downcast_ref::<String>());
                }
            }
        }

        match best_decoded {
            Some(decoded) => {
                let result = glossia_postprocess(&decoded, nip_hint);
                debug_log!("[RUST] glossia_decode_subject: postprocess result={:?}", result.as_ref().map(|s| &s[..s.len().min(40)]));
                result.ok()
            }
            _ => {
                debug_log!("[RUST] glossia_decode_subject: no successful decode from any wordlist");
                None
            }
        }
    })();

    {
        let mut guard = cache.lock().unwrap();
        maybe_evict(&mut guard);
        guard.insert(key, result.clone());
    }
    result
}

// ── Decrypt pipeline ─────────────────────────────────────────────────

/// JSON manifest structs for legacy email format (serde deserialization).
#[derive(Debug, serde::Deserialize)]
struct JsonManifest {
    body: Option<JsonEncryptedBlob>,
    attachments: Option<Vec<JsonAttachment>>,
}

#[derive(Debug, serde::Deserialize)]
struct JsonEncryptedBlob {
    ciphertext: String,
    #[allow(dead_code)]
    cipher_sha256: Option<String>,
    #[allow(dead_code)]
    cipher_size: Option<u64>,
    key_wrap: String,
}

#[derive(Debug, serde::Deserialize)]
struct JsonAttachment {
    id: String,
    orig_filename: String,
    orig_mime: String,
    cipher_sha256: Option<String>,
    cipher_size: Option<u64>,
    key_wrap: String,
}

/// Determine which pubkey to use for NIP decryption at a given armor level.
/// Mirrors JS: _decryptFromArmorParts pubkey selection logic.
/// Returns hex pubkey string.
fn determine_decrypt_pubkey(
    sig_pubkey_hex: Option<&str>,
    seal_pubkey_hex: Option<&str>,
    user_pubkey_hex: &str,
    fallback_pubkey: &str,
) -> Result<String, String> {
    // Normalize a candidate pubkey (npub or hex) to hex, and refuse to return the
    // user's own pubkey — that would produce ECDH-with-self and silently fail to
    // decrypt. Hitting this branch means we routed to the wrong decrypt direction
    // (cross-account row, self-email, or a classifier bug upstream).
    let resolve = |candidate: &str| -> Result<String, String> {
        let hex = if candidate.starts_with("npub1") {
            let pk = nostr_sdk::prelude::PublicKey::parse(candidate)
                .map_err(|e| format!("Invalid fallback npub: {:?}", e))?;
            pk.to_hex()
        } else {
            candidate.to_string()
        };
        if hex == user_pubkey_hex {
            return Err("decrypt pubkey resolved to user's own key — refusing self-DH (likely cross-account row or mis-routed sent email)".to_string());
        }
        Ok(hex)
    };

    // Get pubkey from seal or signature block, or fall back to provided pubkey
    let other_pubkey = match seal_pubkey_hex.or(sig_pubkey_hex) {
        Some(pk) => pk,
        None => {
            // No seal/sig in armor — use fallback pubkey directly
            if fallback_pubkey.is_empty() {
                return Err("No pubkey in seal/signature block and no fallback provided".to_string());
            }
            return resolve(fallback_pubkey);
        }
    };

    // If seal/sig pubkey is the user's own, use fallback (the other party)
    if other_pubkey == user_pubkey_hex {
        if fallback_pubkey.is_empty() {
            return Err("Cannot determine recipient pubkey (seal pubkey matches user)".to_string());
        }
        resolve(fallback_pubkey)
    } else {
        resolve(other_pubkey)
    }
}

/// Decrypt a single armor block level.
/// Mirrors JS: _decryptFromArmorParts + manifest detection.
fn decrypt_single_block(
    body_text: &str,
    body_type: &str,
    encryption_nip: Option<&str>,
    sig_pubkey_hex: Option<&str>,
    seal_pubkey_hex: Option<&str>,
    profile_name: Option<&str>,
    private_key: &str,
    user_pubkey_hex: &str,
    fallback_pubkey: &str,
) -> (crate::types::DecryptedBlock, Option<JsonManifest>) {
    use base64::Engine;

    debug_log!("[RUST] decrypt_single_block: type={} nip={:?} sig_pk={:?} seal_pk={:?} fallback={:?} body_preview={:?}",
        body_type, encryption_nip, sig_pubkey_hex.map(|s| &s[..s.len().min(16)]),
        seal_pubkey_hex.map(|s| &s[..s.len().min(16)]),
        &fallback_pubkey[..fallback_pubkey.len().min(16)],
        &body_text[..body_text.len().min(60)]);

    let mut block = crate::types::DecryptedBlock {
        decrypted_text: None,
        error: None,
        was_encrypted: body_type == "encrypted",
        profile_name: profile_name.map(|s| s.to_string()),
        body_type: body_type.to_string(),
    };

    if body_type != "encrypted" {
        // Signed/plain blocks aren't encrypted — try glossia decode to recover plaintext,
        // then fall back to raw body text if it doesn't look glossia-encoded.
        let decoded = try_glossia_decode_to_bytes(body_text)
            .and_then(|bytes| String::from_utf8(bytes).ok());
        if let Some(ref text) = decoded {
            debug_log!("[RUST] decrypt_single_block: glossia decoded signed/plain body, len={} preview={:?}",
                text.len(), &text[..text.len().min(60)]);
        }
        block.decrypted_text = Some(decoded.unwrap_or_else(|| body_text.to_string()));
        return (block, None);
    }

    let nip = encryption_nip.unwrap_or("nip44");

    // Step 1: Glossia-decode body text → ciphertext string
    let perf_glossia = std::time::Instant::now();
    let ciphertext = match glossia_decode_to_ciphertext(body_text, nip) {
        Ok(ct) => {
            debug_log!("[RUST] decrypt_single_block: glossia decoded, ciphertext len={}", ct.len());
            ct
        }
        Err(e) => {
            debug_log!("[RUST] decrypt_single_block: glossia decode FAILED: {}", e);
            block.error = Some(format!("Glossia decode failed: {}", e));
            return (block, None);
        }
    };
    let glossia_ms = perf_glossia.elapsed().as_millis();
    let ciphertext_len = ciphertext.len();

    // Step 2: Determine which pubkey to decrypt with
    let decrypt_pubkey_hex = match determine_decrypt_pubkey(sig_pubkey_hex, seal_pubkey_hex, user_pubkey_hex, fallback_pubkey) {
        Ok(pk) => {
            debug_log!("[RUST] decrypt_single_block: decrypt pubkey={}", &pk[..pk.len().min(16)]);
            pk
        }
        Err(e) => {
            debug_log!("[RUST] decrypt_single_block: determine_decrypt_pubkey FAILED: {}", e);
            block.error = Some(e);
            return (block, None);
        }
    };

    // Step 3: NIP decrypt
    let decrypt_npub = match nostr_sdk::prelude::PublicKey::parse(&decrypt_pubkey_hex) {
        Ok(pk) => {
            use nostr_sdk::prelude::ToBech32;
            pk.to_bech32().unwrap_or_default()
        }
        Err(e) => {
            block.error = Some(format!("Invalid decrypt pubkey: {:?}", e));
            return (block, None);
        }
    };

    let perf_nip = std::time::Instant::now();
    let decrypted = match crate::nostr::decrypt_dm_content(private_key, &decrypt_npub, &ciphertext) {
        Ok(d) => {
            debug_log!("[RUST] decrypt_single_block: NIP decrypt SUCCESS, len={} preview={:?}", d.len(), &d[..d.len().min(40)]);
            d
        }
        Err(e) => {
            debug_log!("[RUST] decrypt_single_block: NIP decrypt FAILED: {}", e);
            block.error = Some(format!("NIP decrypt failed: {}", e));
            return (block, None);
        }
    };
    let nip_decrypt_ms = perf_nip.elapsed().as_millis();
    debug_log!("[RUST-PERF] decrypt_single_block: nip={} glossia={}ms (ct={}b) nip_decrypt={}ms (out={}b)",
        nip, glossia_ms, ciphertext_len, nip_decrypt_ms, decrypted.len());

    // Step 4: Detect manifest vs legacy
    let trimmed = decrypted.trim();
    if trimmed.starts_with('{') {
        // Try JSON manifest parse
        if let Ok(manifest) = serde_json::from_str::<JsonManifest>(trimmed) {
            if let Some(ref body_blob) = manifest.body {
                // AES decrypt the manifest body
                let key_bytes = match base64::engine::general_purpose::STANDARD.decode(&body_blob.key_wrap) {
                    Ok(k) => k,
                    Err(e) => {
                        block.error = Some(format!("Manifest key_wrap base64 decode failed: {}", e));
                        return (block, Some(manifest));
                    }
                };
                let ct_bytes = match base64::engine::general_purpose::STANDARD.decode(&body_blob.ciphertext) {
                    Ok(c) => c,
                    Err(e) => {
                        block.error = Some(format!("Manifest ciphertext base64 decode failed: {}", e));
                        return (block, Some(manifest));
                    }
                };

                match crate::crypto::aes_gcm_decrypt_raw(&key_bytes, &ct_bytes) {
                    Ok(plaintext_bytes) => {
                        // The plaintext is base64-encoded UTF-8 body (matching JS: atob(aesResult))
                        match String::from_utf8(plaintext_bytes) {
                            Ok(b64_body) => {
                                // Decode the base64 to get the actual text
                                match base64::engine::general_purpose::STANDARD.decode(b64_body.trim()) {
                                    Ok(body_bytes) => {
                                        match String::from_utf8(body_bytes) {
                                            Ok(text) => block.decrypted_text = Some(text),
                                            Err(_) => block.decrypted_text = Some(b64_body),
                                        }
                                    }
                                    Err(_) => {
                                        // Not base64 — use as-is (the plaintext IS the body)
                                        block.decrypted_text = Some(b64_body);
                                    }
                                }
                            }
                            Err(_) => {
                                block.error = Some("Manifest body AES plaintext is not UTF-8".to_string());
                            }
                        }
                    }
                    Err(e) => {
                        block.error = Some(format!("Manifest AES decrypt failed: {}", e));
                    }
                }
                return (block, Some(manifest));
            }
        }
    }

    // Legacy format: decrypted content is the body text directly
    block.decrypted_text = Some(decrypted);
    (block, None)
}

/// Walk the capnp ArmorMessage tree recursively, decrypting each encrypted block.
/// Returns results innermost-first (matching JS decryptAllEncryptedBlocks convention).
fn decrypt_armor_tree(
    parsed: &crate::types::ParsedArmorMessage,
    private_key: &str,
    user_pubkey_hex: &str,
    fallback_pubkey: &str,
    raw_headers: Option<&str>,
    // When false (user disabled "Require Signatures"), NIP-04 signature
    // verification failures no longer block decryption — the body is decrypted
    // anyway and the UI still surfaces the invalid/missing-signature indicator
    // via the separate verify path. When true (default), an unverified NIP-04
    // message is rejected before decryption (the signature is NIP-04's only MAC).
    require_signature: bool,
    depth: usize,
) -> (Vec<crate::types::DecryptedBlock>, Option<JsonManifest>) {
    let perf_level = std::time::Instant::now();
    let mut results = Vec::new();
    let mut outer_manifest = None;

    // Recurse into quoted first (innermost-first ordering)
    // Propagate current level's sig/seal pubkey as fallback for inner blocks,
    // so nested blocks can identify the other party when their own sig matches the user.
    let mut inner_ms = 0u128;
    if let Some(ref quoted) = parsed.quoted {
        let inner_fallback = if fallback_pubkey.is_empty() {
            parsed.sig_pubkey_hex.as_deref()
                .or(parsed.seal_pubkey_hex.as_deref())
                .unwrap_or("")
        } else {
            fallback_pubkey
        };
        // raw_headers (X-Nostr-Sig) signs the outermost canonical body bytes, which
        // includes nested quoted bytes by concatenation. Inner subtrees only see a
        // slice of that signed data, so the header sig would never verify against
        // them — pass None so inner levels rely solely on their inline signatures.
        let perf_inner = std::time::Instant::now();
        let (inner_results, _) = decrypt_armor_tree(quoted, private_key, user_pubkey_hex, inner_fallback, None, require_signature, depth + 1);
        inner_ms = perf_inner.elapsed().as_millis();
        results.extend(inner_results);
    }

    // NIP-04 mandatory signature verification (spec section 4.1).
    // NIP-04 (AES-256-CBC) lacks authenticated encryption; the Schnorr signature
    // serves as the MAC. Verification MUST happen before decryption to prevent
    // padding oracle attacks.
    let perf_sig = std::time::Instant::now();
    let mut sig_verify_ran = false;
    let mut sig_verify_bytes_len: usize = 0;
    // Only enforce (and even compute) NIP-04 signature verification when the user
    // requires signatures. With "Require Signatures" off, the user has opted into
    // reading unauthenticated mail, so we skip the gate entirely and let decryption
    // proceed; the missing/invalid signature still shows in the UI via the separate
    // verify_all_signatures path.
    if require_signature && parsed.encryption_nip.as_deref() == Some("nip04") {
        sig_verify_ran = true;
        // Compute the canonical signed bytes once: this level's raw decoded body
        // concatenated with all nested quoted body bytes. Both the inline
        // SIGNATURE block and the X-Nostr-Sig header sign these same bytes.
        let verify_bytes = if let Some(ref b64) = parsed.body_bytes_b64 {
            general_purpose::STANDARD.decode(b64).unwrap_or_else(|_| parsed.body_text.as_bytes().to_vec())
        } else {
            extract_ciphertext_binary(&parsed.body_text)
        };
        let mut all_bytes = verify_bytes;

        fn collect_quoted_bytes(
            quoted: &Option<Box<crate::types::ParsedArmorMessage>>,
            buf: &mut Vec<u8>,
        ) {
            if let Some(ref q) = quoted {
                if let Some(ref b64) = q.body_bytes_b64 {
                    if let Ok(bytes) = general_purpose::STANDARD.decode(b64) {
                        buf.extend_from_slice(&bytes);
                    }
                }
                collect_quoted_bytes(&q.quoted, buf);
            }
        }
        collect_quoted_bytes(&parsed.quoted, &mut all_bytes);
        sig_verify_bytes_len = all_bytes.len();

        // Primary trust path: inline SIGNATURE block inside the armor.
        let inline_verified = match (&parsed.signature_hex, &parsed.sig_pubkey_hex) {
            (Some(sig_hex), Some(pubkey_hex)) => {
                let ok = matches!(
                    crate::crypto::verify_signature_bytes(pubkey_hex, sig_hex, &all_bytes),
                    Ok(true)
                );
                if ok {
                    debug_log!("[RUST] NIP-04 inline signature verified ({} bytes)", all_bytes.len());
                } else {
                    debug_log!("[RUST] NIP-04 inline signature INVALID");
                }
                Some(ok)
            }
            _ => None,
        };

        // Fallback trust path: X-Nostr-Sig + X-Nostr-Pubkey transport headers.
        // Only consulted at the outermost armor level (raw_headers is None for
        // recursive calls into nested quoted blocks), because the header sig
        // signs the full canonical body, not inner subtrees.
        let header_verified = if inline_verified != Some(true) {
            raw_headers.and_then(|rh| {
                let pk = extract_nostr_pubkey_from_headers(rh)?;
                let sig = extract_nostr_sig_from_headers(rh)?;
                let ok = matches!(
                    crate::crypto::verify_signature_bytes(&pk, &sig, &all_bytes),
                    Ok(true)
                );
                if ok {
                    debug_log!("[RUST] NIP-04 X-Nostr-Sig header verified ({} bytes)", all_bytes.len());
                } else {
                    debug_log!("[RUST] NIP-04 X-Nostr-Sig header INVALID");
                }
                Some(ok)
            })
        } else {
            None
        };

        let verified = inline_verified == Some(true) || header_verified == Some(true);
        if !verified {
            let (msg, log) = match (inline_verified, header_verified) {
                (Some(false), Some(false)) => (
                    "NIP-04 signature verification failed (both inline SIGNATURE block and \
                     X-Nostr-Sig header). The message was rejected without decrypting to \
                     prevent potential ciphertext manipulation.".to_string(),
                    "NIP-04 both inline + header sigs INVALID — rejecting"
                ),
                (Some(false), None) => (
                    "NIP-04 signature verification failed. The message was rejected without \
                     decrypting to prevent potential ciphertext manipulation. The message may \
                     have been tampered with in transit.".to_string(),
                    "NIP-04 inline signature INVALID, no header sig — rejecting"
                ),
                (None, Some(false)) => (
                    "NIP-04 X-Nostr-Sig header verification failed. No inline SIGNATURE block \
                     was present, and the transport-header signature did not verify. The \
                     message was rejected without decrypting.".to_string(),
                    "NIP-04 header sig INVALID, no inline sig — rejecting"
                ),
                _ => (
                    "This NIP-04 encrypted message has no signature (neither an inline \
                     SIGNATURE block nor an X-Nostr-Sig header). NIP-04 requires a signature \
                     for authentication because it lacks built-in message integrity (MAC). \
                     To opt into decrypting unsigned messages anyway, disable \"Require \
                     Signatures\" in Settings → Advanced.".to_string(),
                    "NIP-04 message has no signature (inline or header) — rejecting"
                ),
            };
            debug_log!("[RUST] {}", log);
            results.push(crate::types::DecryptedBlock {
                decrypted_text: None,
                error: Some(msg),
                was_encrypted: true,
                profile_name: parsed.profile_name.clone().or(parsed.display_name.clone()),
                body_type: "encrypted".to_string(),
            });
            let sig_ms_reject = perf_sig.elapsed().as_millis();
            debug_log!("[RUST-PERF] decrypt_armor_tree depth={} body_len={}b nip={:?} inner={}ms sig_verify={}ms (verify_bytes={}b) decrypt_block=SKIPPED(sig fail) total={}ms",
                depth, parsed.body_text.len(), parsed.encryption_nip.as_deref(),
                inner_ms, sig_ms_reject, sig_verify_bytes_len,
                perf_level.elapsed().as_millis());
            return (results, outer_manifest);
        }
    }
    let sig_verify_ms = if sig_verify_ran { perf_sig.elapsed().as_millis() } else { 0 };

    // Decrypt this level
    let perf_block = std::time::Instant::now();
    let (block, manifest) = decrypt_single_block(
        &parsed.body_text,
        &parsed.body_type,
        parsed.encryption_nip.as_deref(),
        parsed.sig_pubkey_hex.as_deref(),
        parsed.seal_pubkey_hex.as_deref(),
        parsed.profile_name.as_deref().or(parsed.display_name.as_deref()),
        private_key,
        user_pubkey_hex,
        fallback_pubkey,
    );
    let block_ms = perf_block.elapsed().as_millis();
    if manifest.is_some() {
        outer_manifest = manifest;
    }
    results.push(block);

    debug_log!("[RUST-PERF] decrypt_armor_tree depth={} body_len={}b nip={:?} inner={}ms sig_verify={}ms (verify_bytes={}b) decrypt_block={}ms total={}ms",
        depth, parsed.body_text.len(), parsed.encryption_nip.as_deref(),
        inner_ms, sig_verify_ms, sig_verify_bytes_len, block_ms,
        perf_level.elapsed().as_millis());

    (results, outer_manifest)
}

/// Top-level decrypt pipeline: parse armor → decrypt tree → decrypt subject → assemble result.
/// Mirrors the full JS decryptManifestMessage + decryptAllEncryptedBlocks pipeline.
pub fn decrypt_email_body_pipeline(
    private_key: &str,
    armor_text: &str,
    subject: &str,
    sender_pubkey: Option<&str>,
    recipient_pubkey: Option<&str>,
    raw_headers: Option<&str>,
    // When false (user disabled "Require Signatures"), skip the NIP-04 signature
    // gate so unauthenticated mail still decrypts. When true (default), an
    // unverified NIP-04 message is rejected before decryption.
    require_signature: bool,
    // When true, decrypt only the most recent (outermost) message and skip the
    // quoted thread history. DM conversation rendering uses this: the inline body
    // shows just the latest message (the full thread is reachable via the
    // envelope icon), so decrypting nested quoted levels is wasted work the UI
    // discards. No-op for NIP-04, whose signature is computed over the outer body
    // PLUS all nested quoted bytes — see the gate where `parsed.quoted` is cleared.
    shallow: bool,
) -> Result<crate::types::DecryptEmailResult, String> {
    let perf_total = std::time::Instant::now();
    debug_log!("[RUST] decrypt_email_body: armor_len={} subject_len={}", armor_text.len(), subject.len());

    // Normalize line endings (spec section 8 step 2)
    let normalized = armor_text.replace("\r\n", "\n");

    // Parse armor into capnp ArmorMessage → serde struct
    let perf_parse = std::time::Instant::now();
    let mut parsed = match parse_armor_components(&normalized) {
        Some(p) => p,
        None => {
            debug_log!("[RUST] decrypt_email_body: no armor found");
            return Ok(crate::types::DecryptEmailResult {
                subject: subject.to_string(),
                body: armor_text.to_string(),
                is_manifest: false,
                attachments: Vec::new(),
                block_results: Vec::new(),
                success: false,
                error: Some("No armor block found in email body".to_string()),
                subject_ciphertext: None,
                sender_pubkey: None,
            });
        }
    };
    let parse_ms = perf_parse.elapsed().as_millis();

    // Shallow mode: discard the quoted subtree so decrypt_armor_tree won't
    // recurse into thread history the caller won't display. Skipped for NIP-04,
    // where the signature is verified over the outer body PLUS all nested quoted
    // bytes (see decrypt_armor_tree's collect_quoted_bytes) — dropping them there
    // would fail verification and block decryption.
    if shallow && parsed.encryption_nip.as_deref() != Some("nip04") {
        parsed.quoted = None;
    }

    // Derive user's pubkey hex from private key
    let perf_derive = std::time::Instant::now();
    let user_pubkey_hex = {
        let sk = nostr_sdk::prelude::SecretKey::parse(private_key)
            .map_err(|e| format!("Invalid private key: {:?}", e))?;
        let keys = nostr_sdk::prelude::Keys::new(sk);
        keys.public_key().to_hex()
    };
    let derive_pk_ms = perf_derive.elapsed().as_millis();

    // Determine fallback pubkey (the other party's pubkey for decryption).
    // Prefer whichever provided pubkey is NOT the user's own: when you view your
    // OWN sent mail from the inbox, sender_pubkey is your key, and using it would
    // trip the self-DH guard in determine_decrypt_pubkey. The real counterparty is
    // then the recipient. Picking the non-self candidate makes decryption work
    // regardless of which folder (inbox/sent) the caller routed through.
    let normalize_hex = |p: &str| -> String {
        if p.starts_with("npub1") {
            nostr_sdk::prelude::PublicKey::parse(p)
                .map(|k| k.to_hex())
                .unwrap_or_else(|_| p.to_string())
        } else {
            p.to_string()
        }
    };
    let candidates: [Option<&str>; 2] = [sender_pubkey, recipient_pubkey];
    let fallback = candidates
        .iter()
        .flatten()
        .copied()
        .find(|c| normalize_hex(c) != user_pubkey_hex)
        .or_else(|| candidates.iter().flatten().copied().next())
        .unwrap_or("");

    // Walk the armor tree, decrypt each level
    let perf_tree = std::time::Instant::now();
    let (block_results, manifest) = decrypt_armor_tree(&parsed, private_key, &user_pubkey_hex, fallback, raw_headers, require_signature, 0);
    let tree_ms = perf_tree.elapsed().as_millis();

    // Extract outermost decrypted body (last element in innermost-first array)
    let outer_block = block_results.last();
    let mut body = outer_block
        .and_then(|b| b.decrypted_text.clone())
        .unwrap_or_else(|| {
            outer_block
                .and_then(|b| b.error.clone())
                .unwrap_or_else(|| armor_text.to_string())
        });
    // Prepend plaintext prefix that appeared before the first armor delimiter
    if let Some(ref prefix) = parsed.prefix_text {
        if !prefix.is_empty() {
            body = format!("{}\n\n{}", prefix, body);
        }
    }
    let success = outer_block.map(|b| b.decrypted_text.is_some()).unwrap_or(false);
    let error = if success { None } else { outer_block.and_then(|b| b.error.clone()) };

    // Extract attachment metadata from manifest
    let (is_manifest, attachments) = if let Some(ref m) = manifest {
        let atts = m.attachments.as_ref().map(|att_list| {
            att_list.iter().map(|a| crate::types::ManifestAttachmentInfo {
                id: a.id.clone(),
                orig_filename: a.orig_filename.clone(),
                orig_mime: a.orig_mime.clone(),
                key_wrap_b64: a.key_wrap.clone(),
                cipher_sha256_hex: a.cipher_sha256.clone(),
                cipher_size: a.cipher_size.unwrap_or(0),
            }).collect()
        }).unwrap_or_default();
        (true, atts)
    } else {
        (false, Vec::new())
    };

    // Decrypt subject — use armor's embedded pubkey as fallback when sender_pubkey wasn't provided
    let perf_subject = std::time::Instant::now();
    let (decrypted_subject, subject_ciphertext) = if parsed.body_type == "encrypted" {
        let nip_hint = parsed.encryption_nip.as_deref().unwrap_or("nip44");
        let subject_fallback = if fallback.is_empty() {
            // The armor signature/seal block contains the sender's pubkey
            parsed.sig_pubkey_hex.as_deref()
                .or(parsed.seal_pubkey_hex.as_deref())
                .unwrap_or("")
        } else {
            fallback
        };
        decrypt_subject(subject, private_key, &user_pubkey_hex, subject_fallback, nip_hint)
    } else {
        (subject.to_string(), None)
    };
    let subject_ms = perf_subject.elapsed().as_millis();

    // Extract sender pubkey from outermost armor signature (for avatar fallback)
    let perf_sender = std::time::Instant::now();
    let armor_sender_pubkey = if sender_pubkey.is_none() {
        // No header-provided pubkey — try to derive from verified armor signature
        parsed.sig_pubkey_hex.as_deref().and_then(|pk_hex| {
            parsed.signature_hex.as_deref().and_then(|sig_hex| {
                let binary = extract_ciphertext_binary(armor_text);
                match crate::crypto::verify_signature_bytes(pk_hex, sig_hex, &binary) {
                    Ok(true) => {
                        nostr_sdk::prelude::PublicKey::from_hex(pk_hex).ok()
                            .and_then(|pk| nostr_sdk::prelude::ToBech32::to_bech32(&pk).ok())
                    }
                    _ => None,
                }
            })
        })
    } else {
        None
    };
    let sender_extract_ms = perf_sender.elapsed().as_millis();

    debug_log!("[RUST] decrypt_email_body: success={} is_manifest={} blocks={} attachments={} armor_sender_pubkey={:?}",
        success, is_manifest, block_results.len(), attachments.len(),
        armor_sender_pubkey.as_deref().map(|s: &str| &s[..std::cmp::min(s.len(), 20)]));
    debug_log!("[RUST-PERF] decrypt_email_body_pipeline: total={}ms parse={}ms derive_pk={}ms tree={}ms subject={}ms sender_extract={}ms (armor_len={}b, levels={})",
        perf_total.elapsed().as_millis(), parse_ms, derive_pk_ms, tree_ms, subject_ms, sender_extract_ms,
        armor_text.len(), block_results.len());

    Ok(crate::types::DecryptEmailResult {
        subject: decrypted_subject,
        body,
        is_manifest,
        attachments,
        block_results,
        success,
        error,
        subject_ciphertext,
        sender_pubkey: armor_sender_pubkey,
    })
}

/// Decrypt the email subject.
/// Mirrors JS: decodeGlossiaSubject + NIP decrypt.
/// Returns (decrypted_subject, subject_ciphertext).
/// subject_ciphertext is the intermediate value after glossia decode / before NIP decrypt,
/// needed by the frontend for DM↔email subject_hash matching.
fn decrypt_subject(
    subject: &str,
    private_key: &str,
    _user_pubkey_hex: &str,
    fallback_pubkey: &str,
    nip_hint: &str,
) -> (String, Option<String>) {
    debug_log!("[RUST] decrypt_subject: len={} nip_hint={} preview={:?}", subject.len(), nip_hint, &subject[..subject.len().min(80)]);
    if subject.is_empty() {
        debug_log!("[RUST] decrypt_subject: empty subject, returning as-is");
        return (subject.to_string(), None);
    }

    // Try to get ciphertext from subject
    let is_encrypted = is_likely_encrypted_content(subject);
    debug_log!("[RUST] decrypt_subject: is_likely_encrypted={}", is_encrypted);
    let ciphertext = if is_encrypted {
        debug_log!("[RUST] decrypt_subject: using subject directly as ciphertext");
        subject.to_string()
    } else if let Some(decoded) = glossia_decode_subject(subject, nip_hint) {
        debug_log!("[RUST] decrypt_subject: glossia decoded to ciphertext len={} preview={:?}", decoded.len(), &decoded[..decoded.len().min(60)]);
        decoded
    } else {
        debug_log!("[RUST] decrypt_subject: glossia decode failed, returning subject as-is");
        return (subject.to_string(), None);
    };

    let subject_ciphertext = Some(ciphertext.clone());

    // Determine which pubkey to use — use fallback (other party's pubkey)
    let decrypt_pubkey = if fallback_pubkey.is_empty() {
        debug_log!("[RUST] decrypt_subject: no fallback pubkey, returning subject as-is");
        return (subject.to_string(), subject_ciphertext);
    } else if fallback_pubkey.starts_with("npub1") {
        fallback_pubkey.to_string()
    } else {
        use nostr_sdk::prelude::ToBech32;
        match nostr_sdk::prelude::PublicKey::parse(fallback_pubkey) {
            Ok(pk) => pk.to_bech32().unwrap_or_else(|_| fallback_pubkey.to_string()),
            Err(_) => {
                debug_log!("[RUST] decrypt_subject: failed to parse fallback pubkey {:?}", fallback_pubkey);
                return (subject.to_string(), subject_ciphertext);
            }
        }
    };

    debug_log!("[RUST] decrypt_subject: attempting NIP decrypt with pubkey prefix={:?}", &decrypt_pubkey[..decrypt_pubkey.len().min(20)]);
    match crate::nostr::decrypt_dm_content(private_key, &decrypt_pubkey, &ciphertext) {
        Ok(decrypted) => {
            debug_log!("[RUST] decrypt_subject: success! decrypted={:?}", &decrypted[..decrypted.len().min(60)]);
            (decrypted, subject_ciphertext)
        }
        Err(e) => {
            debug_log!("[RUST] decrypt_subject: NIP decrypt failed: {:?}", e);
            (subject.to_string(), subject_ciphertext)
        }
    }
}

/// Decrypt a manifest attachment (separate from body for large payloads).
pub fn decrypt_attachment_pipeline(
    attachment_data_b64: &str,
    key_wrap_b64: &str,
    cipher_sha256_hex: Option<&str>,
    orig_filename: &str,
    orig_mime: &str,
) -> Result<crate::types::DecryptedAttachment, String> {
    use base64::Engine;
    use sha2::{Sha256, Digest};

    debug_log!("[RUST] decrypt_attachment: data_len={} filename={:?}", attachment_data_b64.len(), orig_filename);

    let b64 = base64::engine::general_purpose::STANDARD;

    // Decode attachment data
    let encrypted_data = b64.decode(attachment_data_b64)
        .map_err(|e| format!("Attachment data base64 decode failed: {}", e))?;

    // Decode AES key
    let key_bytes = b64.decode(key_wrap_b64)
        .map_err(|e| format!("key_wrap base64 decode failed: {}", e))?;

    // Verify SHA-256 if provided (warn on mismatch but continue, matching JS behavior)
    if let Some(expected_hash) = cipher_sha256_hex {
        let mut hasher = Sha256::new();
        hasher.update(&encrypted_data);
        let actual_hash = hex::encode(hasher.finalize());
        if actual_hash != expected_hash {
            debug_log!("[RUST] decrypt_attachment_pipeline: hash mismatch (expected {}, got {}) — continuing anyway", expected_hash, actual_hash);
        }
    }

    // AES-256-GCM decrypt with padding removal
    let decrypted = crate::crypto::aes_gcm_decrypt_padded(&key_bytes, &encrypted_data)
        .map_err(|e| format!("Attachment AES decrypt failed: {}", e))?;

    let size = decrypted.len();
    let data_b64 = b64.encode(&decrypted);

    Ok(crate::types::DecryptedAttachment {
        id: String::new(), // Caller sets this
        filename: orig_filename.to_string(),
        content_type: orig_mime.to_string(),
        data_b64,
        size,
    })
}

/// Extract binary ciphertext from the email body for signing/verification.
/// For ASCII-armored bodies: extracts glossia-encoded or base64 payload and decodes to bytes.
/// For nested reply chains, uses depth-counting to separate each level and concatenates
/// decoded bytes from all levels (matching the JS signing behavior).
/// For non-armored bodies: returns the UTF-8 bytes of the body text.
pub fn extract_ciphertext_binary(body: &str) -> Vec<u8> {
    // Use depth-counting parser to properly handle nested reply armor
    if let Some((body_text, nested_armor)) = parse_armor_depth(body) {
        if let Some(mut bytes) = decode_armor_section(&body_text) {
            if let Some(ref nested) = nested_armor {
                let nested_bytes = extract_ciphertext_binary(nested);
                debug_log!("[RUST] extract_ciphertext_binary: concatenating {} outer + {} nested bytes",
                    bytes.len(), nested_bytes.len());
                bytes.extend_from_slice(&nested_bytes);
            }
            return bytes;
        }
    }

    // Non-armored body: return UTF-8 bytes
    debug_log!("[RUST] extract_ciphertext_binary: plain text, {} bytes", body.len());
    body.as_bytes().to_vec()
}

/// Verify email signature using binary ciphertext extraction.
/// Extracts the binary payload from ASCII armor (or uses raw text bytes),
/// then verifies the schnorr signature against SHA-256(binary).
pub fn verify_email_signature(sender_pubkey: &str, signature: &str, body: &str) -> bool {
    let binary = extract_ciphertext_binary(body);
    match crypto::verify_signature_bytes(sender_pubkey, signature, &binary) {
        Ok(valid) => {
            debug_log!("[RUST] verify_email_signature: {} ({} bytes)", if valid { "valid" } else { "INVALID" }, binary.len());
            valid
        },
        Err(e) => {
            debug_log!("[RUST] verify_email_signature: error: {}", e);
            false
        }
    }
}

/// Verify email signature using the in-body SIGNATURE block (primary trust path).
/// Returns `Some(true/false)` if an inline signature was found, `None` if no inline sig exists.
pub fn verify_email_signature_inline(body: &str) -> Option<bool> {
    let parsed = parse_armor_components(body)?;
    let sig_hex = parsed.signature_hex.as_ref()?;
    let pubkey_hex = parsed.sig_pubkey_hex.as_ref()?;
    let binary = extract_ciphertext_binary(body);
    match crypto::verify_signature_bytes(pubkey_hex, sig_hex, &binary) {
        Ok(valid) => {
            debug_log!("[RUST] verify_email_signature_inline: {} ({} bytes, pubkey={}...)",
                if valid { "valid" } else { "INVALID" }, binary.len(), &pubkey_hex[..8.min(pubkey_hex.len())]);
            Some(valid)
        }
        Err(e) => {
            debug_log!("[RUST] verify_email_signature_inline: error: {}", e);
            Some(false)
        }
    }
}

/// Verify email signature trying both in-body (primary) and header (secondary) trust paths.
/// Returns (signature_valid, signature_source) where source is "body", "header", "both", or None.
pub fn verify_email_signature_full(body: &str, raw_headers: &str) -> (Option<bool>, Option<String>) {
    let body_result = verify_email_signature_inline(body);

    let header_result = {
        let pubkey = extract_nostr_pubkey_from_headers(raw_headers);
        let sig = extract_nostr_sig_from_headers(raw_headers);
        match (pubkey, sig) {
            (Some(pk), Some(s)) => Some(verify_email_signature(&pk, &s, body)),
            _ => None,
        }
    };

    debug_log!("[RUST] verify_email_signature_full: body={:?}, header={:?}", body_result, header_result);

    match (body_result, header_result) {
        (Some(true), Some(true)) => (Some(true), Some("both".to_string())),
        (Some(true), _)          => (Some(true), Some("body".to_string())),
        (_, Some(true))          => (Some(true), Some("header".to_string())),
        (Some(false), _) | (_, Some(false)) => (Some(false), None),
        (None, None)             => (None, None),
    }
}

/// Recursively verify ALL signatures in an armor body, including nested quoted blocks.
/// Returns a Vec of verification results ordered innermost-first
/// (matching the JS verifyAllSignatures convention for DOM h4 matching).
pub fn verify_all_signatures_inline(body: &str) -> Vec<crate::types::SignatureVerificationResult> {
    verify_all_signatures_recursive(body, 0)
}

fn verify_all_signatures_recursive(body: &str, depth: usize) -> Vec<crate::types::SignatureVerificationResult> {
    let mut results = Vec::new();

    debug_log!("[RUST] verify_all_sigs_recursive: depth={}, body_len={}, preview={:?}",
        depth, body.len(), &body[..80.min(body.len())]);

    // Parse armor at this level to get sig/pubkey and body type
    let parsed = match parse_armor_components(body) {
        Some(p) => p,
        None => {
            debug_log!("[RUST] verify_all_sigs_recursive: depth={}, parse_armor_components returned None", depth);
            return results;
        }
    };

    debug_log!("[RUST] verify_all_sigs_recursive: depth={}, parsed: body_type={}, has_sig={}, has_pubkey={}, has_quoted={}",
        depth, parsed.body_type,
        parsed.signature_hex.is_some(), parsed.sig_pubkey_hex.is_some(),
        parsed.quoted.is_some());

    // Use parse_armor_depth to get the nested armor text for recursion
    let depth_result = parse_armor_depth(body);
    match &depth_result {
        Some((_body_text, Some(ref nested_armor))) => {
            debug_log!("[RUST] verify_all_sigs_recursive: depth={}, found nested armor ({} bytes), recursing",
                depth, nested_armor.len());
            let inner_results = verify_all_signatures_recursive(nested_armor, depth + 1);
            debug_log!("[RUST] verify_all_sigs_recursive: depth={}, inner recursion returned {} results",
                depth, inner_results.len());
            results.extend(inner_results);
        }
        Some((_body_text, None)) => {
            debug_log!("[RUST] verify_all_sigs_recursive: depth={}, no nested armor (leaf node)", depth);
        }
        None => {
            debug_log!("[RUST] verify_all_sigs_recursive: depth={}, parse_armor_depth returned None", depth);
        }
    }

    // Verify this level's signature if present
    let sig_hex = parsed.signature_hex.as_ref();
    let pubkey_hex = parsed.sig_pubkey_hex.as_ref();

    if let (Some(sig), Some(pk)) = (sig_hex, pubkey_hex) {
        // extract_ciphertext_binary already concatenates this level's bytes + all nested bytes
        let binary = extract_ciphertext_binary(body);
        let is_valid = match crate::crypto::verify_signature_bytes(pk, sig, &binary) {
            Ok(valid) => {
                debug_log!("[RUST] verify_all_signatures: depth={}, {} ({} bytes, pubkey={}...)",
                    depth, if valid { "valid" } else { "INVALID" }, binary.len(),
                    &pk[..8.min(pk.len())]);
                valid
            }
            Err(e) => {
                debug_log!("[RUST] verify_all_signatures: depth={}, error: {}", depth, e);
                false
            }
        };

        results.push(crate::types::SignatureVerificationResult {
            signature_hex: Some(sig.clone()),
            pubkey_hex: Some(pk.clone()),
            is_valid,
            depth,
            body_type: parsed.body_type.clone(),
            profile_name: parsed.profile_name.clone(),
        });
    } else {
        debug_log!("[RUST] verify_all_sigs_recursive: depth={}, no sig/pubkey at this level (sig={}, pk={})",
            depth, sig_hex.is_some(), pubkey_hex.is_some());
    }

    debug_log!("[RUST] verify_all_sigs_recursive: depth={}, returning {} total results", depth, results.len());
    results
}

/// Extract message ID from email headers
pub fn extract_message_id_from_headers(raw_headers: &str) -> Option<String> {
    // Try multiple patterns to find Message-ID
    for line in raw_headers.lines() {
        let line_trimmed = line.trim();
        let line_lower = line_trimmed.to_lowercase();
        
        // Check for Message-ID header (case-insensitive)
        if line_lower.starts_with("message-id:") {
            let msg_id = line_trimmed
                .split_once(':')
                .unwrap_or(("", ""))
                .1
                .trim()
                .to_string();
            
            // Remove angle brackets if present
            let msg_id_clean = msg_id
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim()
                .to_string();
            
            if !msg_id_clean.is_empty() {
                debug_log!("[RUST] extract_message_id_from_headers: Found Message-ID: {} (cleaned: {})", msg_id, msg_id_clean);
                return Some(msg_id_clean);
            }
        }
    }
    
    // Also try using mailparse if available (for structured parsing)
    // This handles continuation lines and other edge cases
    if let Ok(parsed) = parse_mail(raw_headers.as_bytes()) {
        if let Some(msg_id_header) = parsed.headers.get_first_header("message-id") {
            let msg_id = msg_id_header.get_value().trim().to_string();
            let msg_id_clean = msg_id
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim()
                .to_string();
            if !msg_id_clean.is_empty() {
                debug_log!("[RUST] extract_message_id_from_headers: Found Message-ID via mailparse: {} (cleaned: {})", msg_id, msg_id_clean);
                return Some(msg_id_clean);
            }
        }
    }
    
    debug_log!("[RUST] extract_message_id_from_headers: No Message-ID header found in headers ({} chars). First 200 chars: {}", 
        raw_headers.len(), raw_headers.chars().take(200).collect::<String>());
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
            debug_log!("[RUST] No private key available for decryption");
            return Ok((subject.to_string(), body.to_string()));
        }
    };
    
    // Extract the sender's public key from headers
    // For inbox emails: shared secret = user's private key × sender's public key
    let sender_pubkey = match extract_nostr_pubkey_from_headers(raw_headers) {
        Some(pubkey) => pubkey,
        None => {
            debug_log!("[RUST] No X-Nostr-Pubkey header found");
            return Ok((subject.to_string(), body.to_string()));
        }
    };
    
    debug_log!("[RUST] Attempting to decrypt inbox email using sender_pubkey (shared secret: user_privkey × sender_pubkey): {}", sender_pubkey);
    
    // Try to decrypt subject - encrypted subjects are typically just the raw encrypted content
    // without ASCII armor, and are usually base64 encoded
    let decrypted_subject = if is_likely_encrypted_content(subject) {
        match crypto::decrypt_message(private_key, &sender_pubkey, subject) {
            Ok(decrypted) => {
                debug_log!("[RUST] Successfully decrypted subject");
                decrypted
            }
            Err(e) => {
                debug_log!("[RUST] Failed to decrypt subject: {}", e);
                subject.to_string()
            }
        }
    } else {
        subject.to_string()
    };
    
    // Try to decrypt body - check for both NIP-04 and NIP-44 ASCII armor
    let decrypted_body = if body.contains("BEGIN NOSTR NIP-04 ENCRYPTED") || body.contains("BEGIN NOSTR NIP-44 ENCRYPTED") {
        // Remove the ASCII armor if present (handle both legacy MESSAGE and new BODY formats)
        let clean_body = body
            .replace("-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
            .replace("-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
            .replace("-----BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE-----", "")
            .replace("-----END NOSTR NIP-44 ENCRYPTED MESSAGE-----", "")
            .replace("-----BEGIN NOSTR NIP-04 ENCRYPTED BODY-----", "")
            .replace("-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----", "")
            .replace("-----END NOSTR MESSAGE-----", "")
            .trim()
            .to_string();
        
        // Detect format for logging
        let detected_format = crypto::detect_encryption_format(&clean_body);
        debug_log!("[RUST] Detected encryption format: {} for email body", detected_format);
        
        match crypto::decrypt_message(private_key, &sender_pubkey, &clean_body) {
            Ok(decrypted) => {
                debug_log!("[RUST] Successfully decrypted body using format: {}", detected_format);
                decrypted
            }
            Err(e) => {
                debug_log!("[RUST] Failed to decrypt body (detected format: {}): {}", detected_format, e);
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
    
    // Check if it looks like base64 encoded content (typical for encrypted data).
    // Base64 contains A-Z, a-z, 0-9, +, /, and = for padding. NIP-04 ciphertext is
    // shaped `base64?iv=base64`, so the `?` separator is also allowed — without it,
    // canonical NIP-04 strings would fail this check and downstream hashing paths
    // (compute_subject_ciphertext_hash) would refuse to commit a valid hash.
    let base64_chars = content.chars().all(|c| {
        c.is_ascii_alphabetic() || c.is_ascii_digit()
            || c == '+' || c == '/' || c == '=' || c == '?'
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
    use crate::types::EmailConfig;

    fn make_config() -> EmailConfig {
        EmailConfig {
            email_address: "sender@example.com".to_string(),
            password: "pass".to_string(),
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            use_tls: true,
            private_key: None,
        }
    }

    #[test]
    fn test_next_back_batch_first_iter_takes_newest() {
        // 100 candidates, want 5 matches, generous budget → fetch the newest 5
        // (the tail of the ascending list).
        assert_eq!(next_back_batch(0, 100, 0, 5, 50), Some((95, 100)));
    }

    #[test]
    fn test_next_back_batch_respects_remaining_target() {
        // Already have 2 of 5 matches after scanning 5 → take 3 more from the tail.
        assert_eq!(next_back_batch(5, 100, 2, 5, 50), Some((92, 95)));
    }

    #[test]
    fn test_next_back_batch_stops_at_target() {
        assert_eq!(next_back_batch(10, 100, 5, 5, 50), None);
    }

    #[test]
    fn test_next_back_batch_stops_at_budget() {
        // Hit the scan budget before finding enough matches.
        assert_eq!(next_back_batch(50, 10_000, 0, 100, 50), None);
    }

    #[test]
    fn test_next_back_batch_stops_when_exhausted() {
        assert_eq!(next_back_batch(3, 3, 0, 5, 50), None);
    }

    #[test]
    fn test_next_back_batch_clamps_to_remaining_candidates() {
        // Only 2 candidates total, target wants 5 → take just the 2.
        assert_eq!(next_back_batch(0, 2, 0, 5, 50), Some((0, 2)));
    }

    #[test]
    fn test_next_back_batch_clamps_to_remaining_budget() {
        // 3 budget slots left, target wants 10 → take 3 from the tail.
        assert_eq!(next_back_batch(47, 1000, 0, 10, 50), Some((950, 953)));
    }

    #[test]
    fn test_resolve_folder_count_explicit_override_wins() {
        let json = r#"{"nostr-mail": 800, "INBOX": 25}"#;
        assert_eq!(resolve_folder_count("nostr-mail", Some(json), 50), 800);
        assert_eq!(resolve_folder_count("INBOX", Some(json), 50), 25);
    }

    #[test]
    fn test_resolve_folder_count_dense_default_for_nostr_mail() {
        // No override map → nostr-mail gets the deep dense default, not the global.
        assert_eq!(resolve_folder_count("nostr-mail", None, 50), DEFAULT_DENSE_COUNT);
        // Case-insensitive on the well-known dense folder name.
        assert_eq!(resolve_folder_count("Nostr-Mail", None, 50), DEFAULT_DENSE_COUNT);
    }

    #[test]
    fn test_resolve_folder_count_global_default_for_other_folders() {
        assert_eq!(resolve_folder_count("INBOX", None, 50), 50);
        assert_eq!(resolve_folder_count("Archive", None, 77), 77);
    }

    #[test]
    fn test_resolve_folder_count_malformed_json_falls_back() {
        // Garbage / wrong-typed JSON is ignored, not fatal.
        assert_eq!(resolve_folder_count("INBOX", Some("not json"), 50), 50);
        assert_eq!(resolve_folder_count("nostr-mail", Some("{\"nostr-mail\": -3}"), 50), DEFAULT_DENSE_COUNT);
    }

    #[test]
    fn test_resolve_folder_count_override_for_unlisted_folder() {
        // A map that doesn't mention this folder → fall through to the defaults.
        let json = r#"{"Archive": 10}"#;
        assert_eq!(resolve_folder_count("INBOX", Some(json), 50), 50);
        assert_eq!(resolve_folder_count("nostr-mail", Some(json), 50), DEFAULT_DENSE_COUNT);
    }

    #[test]
    fn test_email_config_creation() {
        let config = make_config();
        assert_eq!(config.email_address, "sender@example.com");
        assert_eq!(config.smtp_port, 587);
    }

    // =====================
    // decode_header_value
    // =====================

    #[test]
    fn test_decode_header_value_plain_ascii() {
        assert_eq!(decode_header_value("Hello World"), "Hello World");
    }

    #[test]
    fn test_decode_header_value_right_single_quote() {
        let input = "It\u{00E2}\u{0080}\u{0099}s a test";
        let result = decode_header_value(input);
        assert_eq!(result, "It's a test");
    }

    #[test]
    fn test_decode_header_value_left_right_double_quotes() {
        let input = "\u{00E2}\u{0080}\u{009C}quoted\u{00E2}\u{0080}\u{009D}";
        let result = decode_header_value(input);
        assert_eq!(result, "\"quoted\"");
    }

    #[test]
    fn test_decode_header_value_em_dash() {
        let input = "word\u{00E2}\u{0080}\u{0094}word";
        let result = decode_header_value(input);
        assert!(result.contains("\u{2014}")); // em dash
    }

    #[test]
    fn test_decode_header_value_en_dash() {
        let input = "word\u{00E2}\u{0080}\u{0093}word";
        let result = decode_header_value(input);
        assert!(result.contains("\u{2013}")); // en dash
    }

    #[test]
    fn test_decode_header_value_replacement_char() {
        let input = "It\u{FFFD}s fine";
        let result = decode_header_value(input);
        assert_eq!(result, "It's fine");
    }

    #[test]
    fn test_decode_header_value_contraction_doesnt() {
        assert_eq!(decode_header_value("doesn\u{00E2}"), "doesn't");
    }

    #[test]
    fn test_decode_header_value_contraction_wont() {
        assert_eq!(decode_header_value("won\u{00E2}"), "won't");
    }

    #[test]
    fn test_decode_header_value_contraction_cant() {
        assert_eq!(decode_header_value("can\u{00E2}"), "can't");
    }

    #[test]
    fn test_decode_header_value_contraction_isnt() {
        assert_eq!(decode_header_value("isn\u{00E2}"), "isn't");
    }

    #[test]
    fn test_decode_header_value_contraction_shouldnt() {
        assert_eq!(decode_header_value("shouldn\u{00E2}"), "shouldn't");
    }

    #[test]
    fn test_decode_header_value_pattern_a_space_t() {
        // When the input is "word\u{00E2} t", the contraction replacement for
        // "can\u{00E2}" fires first, turning it into "can't t do it".
        // The final "\u{00E2} t" -> "'t" replacement only matches standalone patterns.
        // Test the standalone pattern where no contraction prefix matches:
        let input = "he said\u{00E2} t do it";
        let result = decode_header_value(input);
        assert!(result.contains("'t"));
    }

    #[test]
    fn test_decode_header_value_empty_string() {
        assert_eq!(decode_header_value(""), "");
    }

    #[test]
    fn test_decode_header_value_no_encoding_issues() {
        let input = "Normal subject line without any issues";
        assert_eq!(decode_header_value(input), input);
    }

    // =====================
    // XNostrPubkey / XNostrSig header types
    // =====================

    #[test]
    fn test_x_nostr_pubkey_header_name() {
        let name = XNostrPubkey::name();
        assert_eq!(name.to_string(), "X-Nostr-Pubkey");
    }

    #[test]
    fn test_x_nostr_pubkey_parse() {
        let header = XNostrPubkey::parse("abc123hex").unwrap();
        assert_eq!(header.0, "abc123hex");
    }

    #[test]
    fn test_x_nostr_pubkey_display() {
        let header = XNostrPubkey("mypubkey".to_string());
        let display = header.display();
        // Verify it creates a valid HeaderValue (use Debug formatting)
        let formatted = format!("{:?}", display);
        assert!(formatted.contains("mypubkey"));
    }

    #[test]
    fn test_x_nostr_sig_header_name() {
        let name = XNostrSig::name();
        assert_eq!(name.to_string(), "X-Nostr-Sig");
    }

    #[test]
    fn test_x_nostr_sig_parse() {
        let header = XNostrSig::parse("sig_data_hex").unwrap();
        assert_eq!(header.0, "sig_data_hex");
    }

    #[test]
    fn test_x_nostr_sig_display() {
        let header = XNostrSig("mysig".to_string());
        let display = header.display();
        // Verify it creates a valid HeaderValue (use Debug formatting)
        let formatted = format!("{:?}", display);
        assert!(formatted.contains("mysig"));
    }

    // =====================
    // construct_email_headers
    // =====================

    #[test]
    fn test_construct_email_headers_basic() {
        let config = make_config();

        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "Test Subject",
            "Hello body",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            true,
            None,
            true,
        );

        assert!(result.is_ok());
        let headers = result.unwrap();
        assert!(headers.contains("From:"));
        assert!(headers.contains("To:"));
        assert!(headers.contains("Subject: Test Subject"));
    }

    #[test]
    fn test_construct_email_headers_with_message_id() {
        let config = make_config();

        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "Subject",
            "Body",
            None,
            Some("<custom-id@example.com>"),
            None,
            None,
            None,
            None,
            true,
            true,
            None,
            true,
        );

        assert!(result.is_ok());
        let headers = result.unwrap();
        let headers_lower = headers.to_lowercase();
        assert!(headers_lower.contains("message-id"));
    }

    #[test]
    fn test_construct_email_headers_with_private_key() {
        let keypair = crate::crypto::generate_keypair().unwrap();

        let config = EmailConfig {
            private_key: Some(keypair.private_key.clone()),
            ..make_config()
        };

        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "Test",
            "Body content",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            true,
            None,
            true,
        );

        assert!(result.is_ok());
        let headers = result.unwrap();
        assert!(headers.contains("X-Nostr-Pubkey:"), "Missing X-Nostr-Pubkey header");
        assert!(headers.contains("X-Nostr-Sig:"), "Missing X-Nostr-Sig header");
    }

    #[test]
    fn test_construct_email_headers_with_empty_attachments() {
        let config = make_config();

        let empty_attachments: Vec<crate::types::EmailAttachment> = vec![];
        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "Subject",
            "Body",
            None,
            None,
            Some(&empty_attachments),
            None,
            None,
            None,
            true,
            true,
            None,
            true,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_construct_email_headers_with_attachments() {
        let config = make_config();

        let attachments = vec![crate::types::EmailAttachment {
            filename: "test.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            data: "dGVzdA==".to_string(),
            size: 4,
            is_encrypted: false,
            encryption_method: None,
            algorithm: None,
            original_filename: None,
            original_type: None,
            original_size: None,
        }];

        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "With Attachment",
            "Body",
            None,
            None,
            Some(&attachments),
            None,
            None,
            None,
            true,
            true,
            None,
            true,
        );

        assert!(result.is_ok());
        let headers = result.unwrap();
        assert!(headers.contains("Content-Type:"));
    }

    #[test]
    fn test_construct_email_headers_with_recipient() {
        let config = make_config();
        let recipient_npub = "npub1v3j6xqw0000000000000000000000000000000000000000000000000000";

        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "Subject",
            "Body",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            true,
            Some(recipient_npub),
            true,
        );

        assert!(result.is_ok());
        let headers = result.unwrap();
        assert!(headers.contains("X-Nostr-Recipient:"), "Missing X-Nostr-Recipient header");
        assert!(headers.contains(recipient_npub), "Header missing recipient pubkey value");
    }

    #[test]
    fn test_construct_email_headers_recipient_disabled() {
        let config = make_config();
        let recipient_npub = "npub1v3j6xqw0000000000000000000000000000000000000000000000000000";

        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "Subject",
            "Body",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            true,
            Some(recipient_npub),
            false,
        );

        assert!(result.is_ok());
        let headers = result.unwrap();
        assert!(!headers.contains("X-Nostr-Recipient:"), "Header should be absent when disabled");
    }

    #[test]
    fn test_construct_email_headers_recipient_none() {
        let config = make_config();

        let result = construct_email_headers(
            &config,
            "recipient@example.com",
            "Subject",
            "Body",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            true,
            None,
            true,
        );

        assert!(result.is_ok());
        let headers = result.unwrap();
        assert!(!headers.contains("X-Nostr-Recipient:"), "Header should be absent when pubkey is None");
    }

    // =====================
    // detect_encryption_format (from crypto, used by email)
    // =====================

    #[test]
    fn test_detect_encryption_format_empty() {
        assert_eq!(crypto::detect_encryption_format(""), "unknown");
    }

    #[test]
    fn test_detect_encryption_format_nip04() {
        let content = "SGVsbG8gV29ybGQ=?iv=dGVzdGl2";
        assert_eq!(crypto::detect_encryption_format(content), "nip04");
    }

    #[test]
    fn test_detect_encryption_format_nip04_with_armor() {
        let content = "-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----\nSGVsbG8gV29ybGQ=?iv=dGVzdGl2\n-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----";
        assert_eq!(crypto::detect_encryption_format(content), "nip04");
    }

    #[test]
    fn test_detect_encryption_format_unknown_plain_text() {
        assert_eq!(crypto::detect_encryption_format("Hello, world!"), "unknown");
    }

    // =====================
    // extract_ciphertext_binary with glossia
    // =====================

    #[test]
    fn test_extract_ciphertext_binary_glossia_in_armor() {
        // Encode known bytes through glossia, wrap in armor, verify round-trip
        let original_bytes: Vec<u8> = (0..32).collect(); // 32 bytes of test data
        let hex_input = glossia::hex_encode(&original_bytes);

        // Encode through glossia pipeline (english/bip39/body dialect)
        let (encoded, _used, _payload_words, _mode) = glossia::encode_into_language(
            &hex_input, "english", "bip39", "body",
            None, 42, false, None, None, None, None,
        ).expect("glossia encode should succeed");

        // Wrap in armor block (matching frontend format: ----- BEGIN ... -----)
        let armored = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n{}\n----- END NOSTR NIP-44 ENCRYPTED BODY -----",
            encoded
        );

        let result = extract_ciphertext_binary(&armored);
        assert_eq!(result, original_bytes,
            "glossia-encoded armored body should decode back to original bytes");
    }

    #[test]
    fn test_extract_ciphertext_binary_plain_text_fallback() {
        // Non-armored body returns UTF-8 bytes
        let body = "Hello, this is a plain text email body";
        let result = extract_ciphertext_binary(body);
        assert_eq!(result, body.as_bytes().to_vec());
    }

    #[test]
    fn test_try_glossia_decode_rejects_base64() {
        // Base64 strings should not match glossia wordlists (hit_rate < 0.3)
        let b64 = "SGVsbG8gV29ybGQhIFRoaXMgaXMgYSB0ZXN0IG1lc3NhZ2U=";
        assert!(try_glossia_decode_to_bytes(b64).is_none(),
            "base64 should not be detected as glossia");
    }


    #[test]
    fn test_user_seal_block_extracts_pubkey_via_pipeline() {
        // Confirms that SEAL-only armor still produces a `seal_pubkey_hex` in the
        // serde output that decrypt_single_block consumes. (User suspected the
        // SEAL-only case wasn't reaching the decrypt path with a pubkey.)
        let armor = "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
Is ceu perdives herbitum.\n\
----- BEGIN NOSTR SEAL -----\n\
aca alutiae myrsineum compositio speculor lanthanum catecizo luteipes\n\
bestialis insolo october pascor detego angustus reduco deprecor recito\n\
nitela\n\
----- END NOSTR MESSAGE -----";
        let parsed = super::parse_armor_components(armor).expect("must parse");
        assert_eq!(parsed.body_type, "encrypted");
        assert_eq!(parsed.encryption_nip.as_deref(), Some("nip44"));
        assert!(parsed.sig_pubkey_hex.is_none(), "no SIGNATURE block expected");
        assert_eq!(
            parsed.seal_pubkey_hex.as_deref(),
            Some("9d1dff92e1dc2dc36277347bb4424c0984d74477de48ed5483dec12be680b5da"),
            "SEAL pubkey must decode to the sender's identity npub"
        );
    }

    #[test]
    fn test_nip44_v2_roundtrip_via_nostr_sdk() {
        // Sanity check: encrypt "hey" between two fresh keypairs and decrypt it back.
        // Confirms the underlying nostr_sdk nip44 impl is internally consistent —
        // any failure here would explain why the user-reported email fails too.
        use nostr_sdk::prelude::{Keys, nip44};
        let sender = Keys::generate();
        let recipient = Keys::generate();
        let ct = nip44::encrypt(
            sender.secret_key(),
            &recipient.public_key(),
            "hey",
            nip44::Version::default(),
        ).expect("encrypt");
        // Verify version byte (0x02) is the first byte after base64 decoding.
        use base64::Engine;
        let raw = base64::engine::general_purpose::STANDARD.decode(&ct).expect("valid base64");
        assert_eq!(raw[0], 0x02, "expected NIP-44 v2 version byte");
        assert_eq!(raw.len(), 99, "expected 99 bytes for 'hey' plaintext");
        // Symmetric decrypt: receiver_priv + sender_pub
        let pt = nip44::decrypt(
            recipient.secret_key(),
            &sender.public_key(),
            &ct,
        ).expect("decrypt");
        assert_eq!(pt, "hey");
    }

    #[test]
    fn test_extract_armor_body_content_with_combined_signature() {
        // New format: combined SIGNATURE block with sig + pubkey, no separate SEAL
        let body = "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            some glossia words here\n\
            ----- BEGIN NOSTR SIGNATURE -----\n\
            @alice\n\
            signature words pubkey words\n\
            ----- END NOSTR MESSAGE -----";
        let content = extract_armor_body_content(body).unwrap();
        assert_eq!(content, "some glossia words here",
            "should extract only body content, not signature block");
    }

    #[test]
    fn test_extract_armor_body_content_with_legacy_signature_seal() {
        // Legacy format: separate SIGNATURE and SEAL blocks
        let body = "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            some glossia words here\n\
            ----- BEGIN NOSTR SIGNATURE -----\n\
            @alice\n\
            signature words\n\
            ----- BEGIN NOSTR SEAL -----\n\
            @bob\n\
            pubkey words\n\
            ----- END NOSTR MESSAGE -----";
        let content = extract_armor_body_content(body).unwrap();
        assert_eq!(content, "some glossia words here",
            "should extract only body content, not sig/seal blocks (legacy format)");
    }

    #[test]
    fn test_extract_armor_body_content_base64() {
        let b64 = general_purpose::STANDARD.encode(&[1u8, 2, 3, 4, 5]);
        let body = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n{}\n----- END NOSTR MESSAGE -----",
            b64
        );
        let content = extract_armor_body_content(&body).unwrap();
        assert_eq!(content, b64, "should cleanly extract base64 content");
    }

    #[test]
    fn test_extract_ciphertext_binary_base64_armor() {
        // Base64 armor should now work correctly with the new parser
        let original_bytes = vec![1u8, 2, 3, 4, 5];
        let b64 = general_purpose::STANDARD.encode(&original_bytes);
        let body = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n{}\n----- END NOSTR MESSAGE -----",
            b64
        );
        let result = extract_ciphertext_binary(&body);
        assert_eq!(result, original_bytes);
    }

    #[test]
    fn test_extract_ciphertext_binary_glossia_with_combined_signature() {
        // Full signed email: glossia body should decode correctly with combined signature block
        let original_bytes: Vec<u8> = (0..32).collect();
        let hex_input = glossia::hex_encode(&original_bytes);
        let (encoded, _, _, _) = glossia::encode_into_language(
            &hex_input, "english", "bip39", "body",
            None, 42, false, None, None, None, None,
        ).expect("glossia encode should succeed");

        let body = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            {}\n\
            ----- BEGIN NOSTR SIGNATURE -----\n\
            @alice\n\
            some signature words some pubkey words\n\
            ----- END NOSTR MESSAGE -----",
            encoded
        );

        let result = extract_ciphertext_binary(&body);
        assert_eq!(result, original_bytes,
            "glossia body should decode correctly with combined signature block");
    }

    // =============================================
    // parse_armor_components tests
    // =============================================

    #[test]
    fn test_parse_armor_components_encrypted_nip44() {
        let body = "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            SGVsbG8gV29ybGQ=\n\
            ----- END NOSTR MESSAGE -----";
        let result = parse_armor_components(body).expect("should parse");
        assert_eq!(result.body_type, "encrypted");
        assert_eq!(result.encryption_nip.as_deref(), Some("nip44"));
        assert_eq!(result.body_text, "SGVsbG8gV29ybGQ=");
        assert!(result.signature_hex.is_none());
        assert!(result.seal_pubkey_hex.is_none());
        assert!(result.prefix_text.is_none());
        assert!(result.quoted.is_none());
    }

    #[test]
    fn test_parse_armor_components_encrypted_nip04() {
        let body = "----- BEGIN NOSTR NIP-04 ENCRYPTED BODY -----\n\
            SGVsbG8=?iv=dGVzdA==\n\
            ----- END NOSTR MESSAGE -----";
        let result = parse_armor_components(body).expect("should parse");
        assert_eq!(result.body_type, "encrypted");
        assert_eq!(result.encryption_nip.as_deref(), Some("nip04"));
    }

    #[test]
    fn test_parse_armor_components_signed_body() {
        let body = "----- BEGIN NOSTR SIGNED BODY -----\n\
            some glossia encoded text here\n\
            ----- END NOSTR MESSAGE -----";
        let result = parse_armor_components(body).expect("should parse");
        assert_eq!(result.body_type, "signed");
        assert!(result.encryption_nip.is_none());
        assert_eq!(result.body_text, "some glossia encoded text here");
    }

    #[test]
    fn test_parse_armor_components_with_hex_signature() {
        // 64-byte sig (128 hex) + 32-byte pubkey (64 hex) = 192 hex total
        let sig_hex = "aa".repeat(64);   // 128 hex chars
        let pub_hex = "bb".repeat(32);   // 64 hex chars
        let combined = format!("{}{}", sig_hex, pub_hex);

        let body = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            SGVsbG8gV29ybGQ=\n\
            ----- BEGIN NOSTR SIGNATURE -----\n\
            @Alice\n\
            {}\n\
            ----- END NOSTR MESSAGE -----",
            combined
        );

        let result = parse_armor_components(&body).expect("should parse");
        assert_eq!(result.body_type, "encrypted");
        assert_eq!(result.signature_hex.as_deref(), Some(sig_hex.as_str()));
        assert_eq!(result.sig_pubkey_hex.as_deref(), Some(pub_hex.as_str()));
        assert_eq!(result.profile_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_parse_armor_components_with_seal() {
        let pub_hex = "cc".repeat(32); // 64 hex chars
        let body = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            SGVsbG8=\n\
            ----- BEGIN NOSTR SEAL -----\n\
            @Bob\n\
            {}\n\
            ----- END NOSTR MESSAGE -----",
            pub_hex
        );

        let result = parse_armor_components(&body).expect("should parse");
        assert_eq!(result.seal_pubkey_hex.as_deref(), Some(pub_hex.as_str()));
        assert_eq!(result.display_name.as_deref(), Some("Bob"));
        assert!(result.signature_hex.is_none());
    }

    #[test]
    fn test_parse_armor_components_legacy_separate_seal() {
        // Legacy format: SIGNATURE block followed by separate SEAL block
        let sig_hex = "aa".repeat(64);
        let sig_pub_hex = "bb".repeat(32);
        let seal_pub_hex = "cc".repeat(32);
        let combined_sig = format!("{}{}", sig_hex, sig_pub_hex);

        let body = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            SGVsbG8=\n\
            ----- BEGIN NOSTR SIGNATURE -----\n\
            @Alice\n\
            {}\n\
            ----- BEGIN NOSTR SEAL -----\n\
            @Bob\n\
            {}\n\
            ----- END NOSTR MESSAGE -----",
            combined_sig, seal_pub_hex
        );

        let result = parse_armor_components(&body).expect("should parse");
        assert_eq!(result.signature_hex.as_deref(), Some(sig_hex.as_str()));
        assert_eq!(result.sig_pubkey_hex.as_deref(), Some(sig_pub_hex.as_str()));
        assert_eq!(result.profile_name.as_deref(), Some("Alice"));
        assert_eq!(result.seal_pubkey_hex.as_deref(), Some(seal_pub_hex.as_str()));
        assert_eq!(result.display_name.as_deref(), Some("Bob"));
    }

    #[test]
    fn test_parse_armor_components_prefix_text() {
        let body = "Hello, this is plaintext.\n\n\
            ----- BEGIN NOSTR SIGNED BODY -----\n\
            glossia content\n\
            ----- END NOSTR MESSAGE -----";
        let result = parse_armor_components(body).expect("should parse");
        assert_eq!(result.prefix_text.as_deref(), Some("Hello, this is plaintext."));
        assert_eq!(result.body_type, "signed");
    }

    #[test]
    fn test_parse_armor_components_no_armor() {
        let body = "Just a plain email with no armor blocks.";
        assert!(parse_armor_components(body).is_none());
    }

    #[test]
    fn test_parse_armor_components_nested_reply() {
        let body = "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            reply_ciphertext_here\n\
            ----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            original_ciphertext_here\n\
            ----- BEGIN NOSTR SIGNATURE -----\n\
            @OriginalAuthor\n\
            aabbccdd\n\
            ----- END NOSTR MESSAGE -----\n\
            ----- BEGIN NOSTR SIGNATURE -----\n\
            @ReplyAuthor\n\
            eeff0011\n\
            ----- END NOSTR MESSAGE -----";

        let result = parse_armor_components(body).expect("should parse outer");
        assert_eq!(result.body_text, "reply_ciphertext_here");
        assert_eq!(result.profile_name.as_deref(), Some("ReplyAuthor"));

        // Check nested quoted message
        assert!(result.quoted.is_some());
        assert!(result.quoted_armor_text.is_some());
        let inner = result.quoted.as_ref().unwrap();
        assert_eq!(inner.body_text, "original_ciphertext_here");
        assert_eq!(inner.profile_name.as_deref(), Some("OriginalAuthor"));
        assert_eq!(inner.body_type, "encrypted");
    }

    #[test]
    fn test_parse_armor_components_body_bytes_base64() {
        // After the NIP-44 eager-decode-skip optimization, parse_armor_components
        // no longer populates body_bytes_b64 for NIP-44 — those pre-decoded bytes
        // were only ever consumed by NIP-04 signature verification, and re-decoding
        // them at parse time was the single biggest cost in the decrypt pipeline
        // (see commit c9095f4). NIP-04 still populates the field; NIP-44 leaves it
        // None and decrypt_single_block / extract_ciphertext_binary do the decode
        // lazily.
        let nip04 = "----- BEGIN NOSTR NIP-04 ENCRYPTED BODY -----\n\
            SGVsbG8gV29ybGQ=\n\
            ----- END NOSTR MESSAGE -----";
        let nip04_parsed = parse_armor_components(nip04).expect("should parse nip04");
        assert!(
            nip04_parsed.body_bytes_b64.is_some(),
            "NIP-04 must keep eager-decoded body_bytes_b64 (used as MAC input by sig verify)"
        );
        let decoded = general_purpose::STANDARD
            .decode(nip04_parsed.body_bytes_b64.unwrap())
            .unwrap();
        assert_eq!(std::str::from_utf8(&decoded).unwrap(), "Hello World");

        let nip44 = "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
            SGVsbG8gV29ybGQ=\n\
            ----- END NOSTR MESSAGE -----";
        let nip44_parsed = parse_armor_components(nip44).expect("should parse nip44");
        assert!(
            nip44_parsed.body_bytes_b64.is_none(),
            "NIP-44 must skip the eager glossia decode at parse time — \
             body_bytes_b64 stays None and decrypt_single_block re-decodes the \
             body_text lazily"
        );
    }

    #[test]
    fn test_decode_sig_and_pubkey_combined_hex() {
        // Phase 1: combined 192-char hex (backward compat)
        let sig = "aa".repeat(64);
        let pubkey = "bb".repeat(32);
        let combined = format!("{}{}", sig, pubkey);
        let (s, p) = decode_sig_and_pubkey(&combined).expect("should split");
        assert_eq!(s, sig);
        assert_eq!(p, pubkey);
    }

    #[test]
    fn test_decode_sig_and_pubkey_too_short() {
        assert!(decode_sig_and_pubkey("aabb").is_none());
    }

    #[test]
    fn test_decode_sig_and_pubkey_last_line_npub() {
        // Phase 3: last-line heuristic with npub
        let sig = "aa".repeat(64);
        let npub = "npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r";
        let content = format!("{}\n{}", sig, npub);
        let (s, p) = decode_sig_and_pubkey(&content).expect("should split");
        assert_eq!(s, sig);
        assert_eq!(p.len(), 64);
    }

    #[test]
    fn test_try_decode_as_pubkey_npub() {
        let npub = "npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r";
        let hex = try_decode_as_pubkey(npub).expect("should decode");
        assert_eq!(hex.len(), 64);
    }

    #[test]
    fn test_try_decode_as_pubkey_hex() {
        let pk = "bb".repeat(32);
        let hex = try_decode_as_pubkey(&pk).expect("should decode");
        assert_eq!(hex, pk);
    }

    #[test]
    fn test_try_decode_as_signature_hex() {
        let sig = "aa".repeat(64);
        let hex = try_decode_as_signature(&sig).expect("should decode");
        assert_eq!(hex, sig);
    }

    #[test]
    fn test_parse_armor_components_body_bytes_match_extract_ciphertext() {
        // Critical contract for NIP-04 sig verification: the eager-decoded
        // body_bytes_b64 must equal what extract_ciphertext_binary recovers
        // lazily — otherwise the inline-sig MAC input on the parse side
        // disagrees with what verify_email_signature_inline recomputes.
        //
        // For NIP-44 the eager decode is skipped (see
        // test_parse_armor_components_body_bytes_base64), so this contract
        // only applies to NIP-04.
        let body = "----- BEGIN NOSTR NIP-04 ENCRYPTED BODY -----\n\
            SGVsbG8gV29ybGQ=\n\
            ----- END NOSTR MESSAGE -----";
        let parsed = parse_armor_components(body).expect("should parse");
        let parsed_bytes = general_purpose::STANDARD.decode(parsed.body_bytes_b64.unwrap()).unwrap();
        let extract_bytes = extract_ciphertext_binary(body);
        assert_eq!(parsed_bytes, extract_bytes,
            "parse_armor_components body bytes must match extract_ciphertext_binary output");
    }



    // ── Decrypt pipeline tests ──────────────────────────────────

    #[test]
    fn test_is_base64_content() {
        assert!(is_base64_content("SGVsbG8gV29ybGQ="));
        assert!(is_base64_content("abc123+/=="));
        assert!(is_base64_content("abc?iv=def")); // NIP-04 format
        // Note: is_base64_content strips whitespace first (matching JS behavior),
        // so it only rejects content with non-base64 chars like punctuation
        assert!(!is_base64_content("Access are acoustic to crawl.")); // period
        assert!(!is_base64_content("Hello, world!")); // comma and exclamation
        assert!(!is_base64_content("")); // empty
    }

    #[test]
    fn test_is_likely_encrypted_content() {
        assert!(is_likely_encrypted_content("SGVsbG8gV29ybGQgdGhpcyBpcyBhIHRlc3Q="));
        assert!(is_likely_encrypted_content("abc123def456ghi789jkl0mn+/=="));
        assert!(!is_likely_encrypted_content("Re: Meeting tomorrow"));
        assert!(!is_likely_encrypted_content("short"));
        assert!(!is_likely_encrypted_content("user@example.com sent a message"));
    }

    #[test]
    fn test_glossia_postprocess_hex_nip44() {
        // Hex input for NIP-44 → base64 output
        let hex = "48656c6c6f"; // "Hello" in hex
        let result = glossia_postprocess(hex, "nip44").unwrap();
        assert_eq!(result, "SGVsbG8="); // base64 of "Hello"
    }

    #[test]
    fn test_glossia_postprocess_non_hex() {
        // Non-hex input → returned as-is
        let b64 = "SGVsbG8gV29ybGQ=";
        let result = glossia_postprocess(b64, "nip44").unwrap();
        assert_eq!(result, b64);
    }

    #[test]
    fn test_glossia_postprocess_hex_nip04_unpack() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        // Build NIP-04 packed format: [len_hi, len_lo, payload..., iv(16 bytes)]
        let payload = b"encrypted_data!!"; // 16 bytes
        let iv = b"0123456789abcdef"; // 16 bytes
        let payload_len = payload.len() as u16;
        let mut packed = Vec::new();
        packed.extend_from_slice(&payload_len.to_be_bytes());
        packed.extend_from_slice(payload);
        packed.extend_from_slice(iv);

        let hex: String = packed.iter().map(|b| format!("{:02x}", b)).collect();
        let result = glossia_postprocess(&hex, "nip04").unwrap();

        // Should be base64(payload)?iv=base64(iv)
        let expected = format!("{}?iv={}", b64.encode(payload), b64.encode(iv));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_glossia_decode_to_ciphertext_base64_passthrough() {
        let b64 = "SGVsbG8gV29ybGQ=";
        let result = glossia_decode_to_ciphertext(b64, "nip44").unwrap();
        assert_eq!(result, b64);
    }

    #[test]
    fn test_glossia_decode_to_ciphertext_nip04_passthrough() {
        let nip04 = "SGVsbG8=?iv=V29ybGQ=";
        let result = glossia_decode_to_ciphertext(nip04, "nip04").unwrap();
        assert_eq!(result, nip04);
    }

    #[test]
    fn test_determine_decrypt_pubkey_uses_seal() {
        let seal = "aa".repeat(32); // 64 hex chars
        let user = "bb".repeat(32);
        let result = determine_decrypt_pubkey(None, Some(&seal), &user, "").unwrap();
        assert_eq!(result, seal);
    }

    #[test]
    fn test_determine_decrypt_pubkey_self_send_uses_fallback() {
        let same = "aa".repeat(32);
        let fallback = "cc".repeat(32);
        let result = determine_decrypt_pubkey(None, Some(&same), &same, &fallback).unwrap();
        assert_eq!(result, fallback);
    }

    #[test]
    fn test_determine_decrypt_pubkey_prefers_seal_over_sig() {
        let seal = "aa".repeat(32);
        let sig = "bb".repeat(32);
        let user = "cc".repeat(32);
        let result = determine_decrypt_pubkey(Some(&sig), Some(&seal), &user, "").unwrap();
        assert_eq!(result, seal);
    }

    #[test]
    fn test_decrypt_single_block_non_encrypted() {
        let (block, manifest) = decrypt_single_block(
            "Hello world",
            "plain",
            None, None, None, None,
            "nsec1fake", "aabb", "",
        );
        assert!(!block.was_encrypted);
        assert_eq!(block.decrypted_text.as_deref(), Some("Hello world"));
        assert!(manifest.is_none());
    }

    #[test]
    fn test_json_manifest_parse() {
        let json = r#"{"body":{"ciphertext":"dGVzdA==","cipher_sha256":"abc123","key_wrap":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="},"attachments":[{"id":"a1","orig_filename":"test.pdf","orig_mime":"application/pdf","cipher_sha256":"def456","cipher_size":65536,"key_wrap":"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB="}]}"#;
        let manifest: JsonManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.body.is_some());
        assert_eq!(manifest.attachments.as_ref().unwrap().len(), 1);
        assert_eq!(manifest.attachments.as_ref().unwrap()[0].id, "a1");
        assert_eq!(manifest.attachments.as_ref().unwrap()[0].orig_filename, "test.pdf");
    }
}


/// Apply the standard inbox filters (transport_auth, require_signature) and
/// persist a batch of fetched messages, returning the count of newly-inserted
/// rows. Shared between forward sync and fetch-older. Updates existing rows
/// in place (preserving read state and attachments); new rows get fresh
/// attachment extraction.
///
/// With `require_signature` true, unsigned / invalid-signature mail is dropped
/// (not stored) — the user has opted into a signed-only inbox. This is *not*
/// permanent loss: turning the setting off makes the next sync re-fetch and
/// persist what was skipped. Forward sync picks up new unsigned mail
/// automatically, fetch-older recovers it below the floor as the user scrolls,
/// and `gap_fill_in_folder` re-scans the already-synced range (see its
/// `recover_dropped` handling) so previously-dropped messages reappear on the
/// next Refresh.
fn persist_inbox_raw_emails(
    raw_emails: Vec<RawNostrEmail>,
    db: &Database,
    require_signature: bool,
) -> anyhow::Result<usize> {
    let mut emails = raw_emails;
    // Filter transport-unauthenticated.
    emails.retain(|email| !matches!(email.transport_auth_verified, Some(false)));
    // Enforce signature requirement: drop nostr mail that claims a sender but
    // isn't validly signed. Recoverable — see the doc comment above.
    if require_signature {
        emails.retain(|email| {
            if email.sender_pubkey.is_some() {
                matches!(email.signature_valid, Some(true))
            } else {
                true
            }
        });
    }

    let mut new_count = 0;
    for email in emails {
        let existing_email = db
            .get_email(&email.message_id)
            .map_err(|e| anyhow::anyhow!("Failed to check email {} in DB: {}", email.message_id, e))?;

        if let Some(existing_email) = existing_email {
            let updated_email = DbEmail {
                id: existing_email.id,
                message_id: existing_email.message_id.clone(),
                from_address: email.from.clone(),
                to_address: email.to.clone(),
                subject: email.subject.clone(),
                body: email.body.clone(),
                body_plain: None,
                body_html: email.html_body.clone(),
                received_at: email.date,
                is_nostr_encrypted: true,
                sender_pubkey: email.sender_pubkey.clone(),
                recipient_pubkey: email.recipient_pubkey.clone(),
                raw_headers: Some(email.raw_headers.clone()),
                is_draft: false,
                is_read: existing_email.is_read,
                updated_at: Some(chrono::Utc::now()),
                created_at: existing_email.created_at,
                signature_valid: email.signature_valid,
                signature_source: email.signature_source.clone(),
                transport_auth_verified: email.transport_auth_verified,
                subject_hash: compute_subject_ciphertext_hash(&email.subject, &email.body),
                in_reply_to: None,
                references: None,
                thread_id: None,
            };
            db.save_email(&updated_email)?;
        } else {
            let db_email = DbEmail {
                id: None,
                message_id: email.message_id.clone(),
                from_address: email.from.clone(),
                to_address: email.to.clone(),
                subject: email.subject.clone(),
                body: email.body.clone(),
                body_plain: None,
                body_html: email.html_body.clone(),
                received_at: email.date,
                is_nostr_encrypted: true,
                sender_pubkey: email.sender_pubkey.clone(),
                recipient_pubkey: email.recipient_pubkey.clone(),
                raw_headers: Some(email.raw_headers.clone()),
                is_draft: false,
                // Seed read state from the server `\Seen` flag captured at fetch
                // time, so mail already read on another client/device imports as
                // read. Unknown (`None`) → unread. Existing rows above keep their
                // local read state; this only seeds the initial insert.
                is_read: email.seen.unwrap_or(false),
                updated_at: None,
                created_at: chrono::Utc::now(),
                signature_valid: email.signature_valid,
                signature_source: email.signature_source.clone(),
                transport_auth_verified: email.transport_auth_verified,
                subject_hash: compute_subject_ciphertext_hash(&email.subject, &email.body),
                in_reply_to: None,
                references: None,
                thread_id: None,
            };
            let email_id = db.save_email(&db_email)?;
            persist_attachments_for_email(db, &email, email_id);
            new_count += 1;
        }
    }
    Ok(new_count)
}

/// Save attachments parsed from a RawNostrEmail. Falls back to re-parsing the
/// RFC822 body when the RawNostrEmail came in with no attachments attached
/// (e.g. older parse paths). Errors per-attachment are logged but never abort.
fn persist_attachments_for_email(db: &Database, email: &RawNostrEmail, email_id: i64) {
    if !email.attachments.is_empty() {
        for mut attachment in email.attachments.iter().cloned() {
            attachment.email_id = email_id;
            if let Err(e) = db.save_attachment(&attachment) {
                debug_log!("[RUST] ERROR: Failed to save attachment {}: {}", attachment.filename, e);
            }
        }
        return;
    }
    if let Ok(parsed_email) = mailparse::parse_mail(email.body.as_bytes()) {
        let extracted_attachments = extract_attachments_from_parsed_email(&parsed_email, &email.body);
        for mut attachment in extracted_attachments {
            attachment.email_id = email_id;
            if let Err(e) = db.save_attachment(&attachment) {
                debug_log!("[RUST] ERROR: Failed to save extracted attachment {}: {}", attachment.filename, e);
            }
        }
    }
}

/// Provider-aware default folder list for inbox sync. Returned when the user
/// hasn't picked any folders explicitly. Folders that don't exist on the
/// server are tolerated by `uid_sync_folder` (logged and skipped), so this can
/// include best-guess names like `Archive` without breaking anything.
///
/// Spam/junk folders are not in this static list. How spam is handled depends
/// on the spam-rescue setting (decided at sync time): with rescue ON, spam is
/// never scanned and `rescue_nostr_emails_from_spam` moves misfiled nostr mail
/// into the rescue target; with rescue OFF, `extend_with_spam_folders` appends
/// discovered spam folders so that mail still surfaces in the inbox.
pub fn default_inbox_folders(imap_host: &str) -> Vec<String> {
    let h = imap_host.to_lowercase();
    if h.contains("gmail.com") || h.contains("googlemail.com") {
        // Gmail: INBOX covers the Primary tab. No Archive — Gmail uses
        // [Gmail]/All Mail for that, which would re-scan everything.
        vec![
            "INBOX".to_string(),
            "nostr-mail".to_string(),
        ]
    } else {
        // Generic IMAP: Archive is added because Outlook/Fastmail/etc users
        // heavily route mail there. Missing on Yahoo etc; the sync loop logs
        // the miss and continues.
        vec![
            "INBOX".to_string(),
            "nostr-mail".to_string(),
            "Archive".to_string(),
        ]
    }
}

pub async fn sync_nostr_emails_to_db(config: &EmailConfig, folders_arg: Option<&[String]>, active_pubkey: &str, db: &Database) -> anyhow::Result<usize> {
    sync_nostr_emails_to_db_inner(config, folders_arg, active_pubkey, db, /* include_gap_fill = */ false).await
}

/// Same as `sync_nostr_emails_to_db` but also runs `gap_fill_in_folder` per
/// folder in the same IMAP session. Powers the Refresh button when the user
/// wants a thorough check; the auto-sync path keeps the cheap forward-only
/// behaviour.
pub async fn refresh_inbox_emails_to_db(config: &EmailConfig, folders_arg: Option<&[String]>, active_pubkey: &str, db: &Database) -> anyhow::Result<usize> {
    sync_nostr_emails_to_db_inner(config, folders_arg, active_pubkey, db, /* include_gap_fill = */ true).await
}

async fn sync_nostr_emails_to_db_inner(config: &EmailConfig, folders_arg: Option<&[String]>, active_pubkey: &str, db: &Database, include_gap_fill: bool) -> anyhow::Result<usize> {
    let account_key = config.email_address.trim().to_lowercase();
    let max_scan = lookup_max_scan(db, active_pubkey);
    let require_signature = lookup_require_signature(db, active_pubkey);
    let spam_rescue = lookup_spam_rescue(db, active_pubkey);
    let auto_move_nostr = lookup_auto_move_nostr(db, active_pubkey);
    let rescue_target = lookup_spam_rescue_target(db, active_pubkey);
    debug_log!("[RUST] sync: spam_rescue={}, auto_move_nostr={}, rescue_target='{}'", spam_rescue, auto_move_nostr, rescue_target);

    // Folders to scan. Default (empty/None) = provider-aware list from
    // `default_inbox_folders`. Multiple folders may be supplied to scan in a
    // single pass; dedupe to avoid re-scanning the same folder twice if a
    // caller passed duplicates.
    let folders: Vec<String> = match folders_arg {
        Some(list) => {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut out: Vec<String> = Vec::new();
            for f in list {
                let trimmed = f.trim();
                if trimmed.is_empty() { continue; }
                if seen.insert(trimmed.to_string()) {
                    out.push(trimmed.to_string());
                }
            }
            if out.is_empty() {
                default_inbox_folders(&config.imap_host)
            } else {
                out
            }
        }
        None => default_inbox_folders(&config.imap_host),
    };

    // Strip any spam/junk/bulk name from the base list (a default never has
    // one; a stale persisted selection might). Whether spam gets scanned is
    // decided below, by the spam_rescue flag — not by the saved selection:
    //   - rescue ON  → spam is NOT scanned. Scanning it would let an in-app
    //     read mark the message \Seen, which rescue treats as "the user filed
    //     this here, leave it" — permanently suppressing the rescue. Rescue
    //     instead moves eligible mail into the rescue target (added below).
    //   - rescue OFF → discovered spam folders ARE appended to the scan set
    //     (inside the IMAP session) so nostr mail a provider misfiled still
    //     surfaces in the inbox. Nothing moves it, so it stays in spam and is
    //     eventually auto-purged by the provider — which some users prefer.
    let mut folders: Vec<String> = folders
        .into_iter()
        .filter(|f| !is_spam_folder_name(f))
        .collect();

    // With spam rescue or inbox auto-filing on, ensure the destination folder is
    // in the scan set so moved messages land in the local DB during this same pass.
    if (spam_rescue || auto_move_nostr) && !folders.iter().any(|f| f.eq_ignore_ascii_case(&rescue_target)) {
        folders.push(rescue_target.clone());
    }

    println!(
        "[RUST] sync_nostr_emails_to_db: account={}, folders={:?}, max_scan={}",
        account_key, folders, max_scan
    );

    // Per-folder watermark updates, applied AFTER all per-message DB saves succeed.
    // Tuple: (folder, uid_validity, last_seen_uid, bootstrap_min_uid).
    // `bootstrap_min_uid` is Some only on the run that created or replaced the
    // row — it seeds folder_sync_state.min_seen_uid so backward pagination has
    // a defined floor. Incremental runs leave min_seen_uid alone.
    let mut pending_state: Vec<(String, u32, u32, Option<u32>)> = Vec::new();
    // Per-folder gap-fill examined windows (folder, uid_validity, floor,
    // last_seen), committed after persist so a future pass skips them.
    let mut pending_gap_examined: Vec<(String, u32, u32, u32)> = Vec::new();
    let mut raw_nostr_emails: Vec<RawNostrEmail> = Vec::new();

    let target = ImapTarget::from_config(config);
    let mut session = imap_pool::checkout(&target)?;
    if spam_rescue {
        let moved = rescue_nostr_emails_from_spam(&mut session, &rescue_target, /* unseen_only = */ true);
        if moved > 0 {
            println!("[RUST] sync_nostr_emails_to_db: spam rescue moved {} message(s) to '{}'", moved, rescue_target);
        }
    } else {
        extend_with_spam_folders(&mut session, &mut folders);
    }

    // Auto-file: move nostr mail out of the regular inbox folders into the
    // destination folder before scanning, so it consolidates there and the scan
    // below imports it from a single place (no duplicate — dedup is by
    // Message-ID). Spam folders are excluded: when rescue is off the user wants
    // spam visible in the inbox, and when on, spam is handled above.
    if auto_move_nostr {
        let sources: Vec<String> = folders
            .iter()
            .filter(|f| !f.eq_ignore_ascii_case(&rescue_target) && !is_spam_folder_name(f))
            .cloned()
            .collect();
        let moved = auto_file_nostr_from_inbox(&mut session, &sources, &rescue_target);
        if moved > 0 {
            println!("[RUST] sync_nostr_emails_to_db: auto-filed {} nostr message(s) to '{}'", moved, rescue_target);
        }
    }

    println!("[RUST] sync_nostr_emails_to_db: folders to scan: {:?}", folders);
    for f in &folders {
        let target_count = lookup_folder_count(db, active_pubkey, f);
        match uid_sync_folder(&mut session, config, db, &account_key, f, target_count, max_scan,
                              parse_nostr_email_from_imap_body) {
            Ok(r) => {
                if r.max_uid > 0 || !r.had_existing_state {
                    let bootstrap_min = if !r.had_existing_state && r.min_uid > 0 {
                        Some(r.min_uid)
                    } else {
                        None
                    };
                    pending_state.push((f.clone(), r.uid_validity, r.max_uid, bootstrap_min));
                }
                raw_nostr_emails.extend(r.emails);
            }
            Err(e) => debug_log!("[RUST] sync_nostr_emails_to_db: folder '{}' failed: {}", f, e),
        }
        if include_gap_fill {
            // recover_dropped = !require_signature: when the user has lifted the
            // signed-only filter, gap-fill re-scans the full synced range to
            // bring back mail that was dropped while the filter was on.
            match gap_fill_in_folder(&mut session, config, db, &account_key, f,
                                      !require_signature, parse_nostr_email_from_imap_body) {
                Ok(r) => {
                    raw_nostr_emails.extend(r.emails);
                    if let Some((uidv, lo, hi)) = r.examined {
                        pending_gap_examined.push((f.clone(), uidv, lo, hi));
                    }
                }
                Err(e) => debug_log!("[RUST] sync_nostr_emails_to_db: gap_fill folder '{}' failed: {}", f, e),
            }
        }
    }
    imap_pool::checkin(&target, session);

    let new_count = persist_inbox_raw_emails(raw_nostr_emails, db, require_signature)?;

    // Commit per-folder UID watermarks now that the DB writes have all succeeded.
    // A partial-batch failure earlier returned Err and we never reach here, so
    // the watermark only advances when every fetched message was persisted.
    for (folder_name, uid_validity, max_uid, bootstrap_min) in pending_state {
        if let Err(e) = db.set_folder_sync_state(&account_key, &folder_name, uid_validity, max_uid) {
            println!(
                "[RUST] sync_nostr_emails_to_db: failed to persist folder_sync_state for '{}': {}",
                folder_name, e
            );
            continue;
        }
        println!(
            "[RUST] sync_nostr_emails_to_db: persisted folder_sync_state for '{}' (uid_validity={}, last_seen_uid={})",
            folder_name, uid_validity, max_uid
        );
        if let Some(min) = bootstrap_min {
            if let Err(e) = db.set_folder_min_seen_uid(&account_key, &folder_name, min) {
                println!(
                    "[RUST] sync_nostr_emails_to_db: failed to seed min_seen_uid for '{}': {}",
                    folder_name, e
                );
            } else {
                println!(
                    "[RUST] sync_nostr_emails_to_db: seeded min_seen_uid={} for '{}'",
                    min, folder_name
                );
            }
        }
    }

    // Commit gap-fill examined windows after the persist succeeded, so the next
    // refresh skips this range instead of re-fetching all the (non-nostr) mail
    // in it. Done post-persist for the same reason as the watermarks above: a
    // persist failure must leave the range unexamined so it's rescanned.
    for (folder_name, uid_validity, lo, hi) in pending_gap_examined {
        if let Err(e) = db.set_folder_gap_examined(&account_key, &folder_name, uid_validity, lo, hi) {
            debug_log!("[RUST] sync_nostr_emails_to_db: failed to record gap_examined for '{}': {}", folder_name, e);
        }
    }

    debug_log!("[RUST] sync_nostr_emails_to_db: Completed sync, {} new emails saved", new_count);
    Ok(new_count)
}

/// Summary of one `fetch_older_*_emails_to_db` call. `new_count` is the rows
/// newly inserted into the DB across all scanned folders. `hit_bottom` is
/// true only when every scanned folder walked all the way to UID 1 — i.e.
/// there's nothing older on the server to find. When false, the per-call
/// scan budget was exhausted before reaching bottom and another call may
/// yield more.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FetchOlderSummary {
    pub new_count: usize,
    pub hit_bottom: bool,
}

/// Backward UID pagination for the inbox. Walks UIDs back per folder past
/// the stored `min_seen_uid` floor, persists matches, and lowers the floor.
/// Returns counts plus a `hit_bottom` aggregate over all folders.
pub async fn fetch_older_inbox_emails_to_db(
    config: &EmailConfig,
    folder: Option<&str>,
    page_size: usize,
    active_pubkey: &str,
    db: &Database,
) -> anyhow::Result<FetchOlderSummary> {
    let account_key = config.email_address.trim().to_lowercase();
    let sync_cutoff_days = lookup_sync_cutoff_days(db, active_pubkey);
    let require_signature = lookup_require_signature(db, active_pubkey);

    let folders: Vec<String> = match folder {
        Some(f) if !f.is_empty() => vec![f.to_string()],
        _ => default_inbox_folders(&config.imap_host),
    };

    println!(
        "[RUST] fetch_older_inbox_emails_to_db: account={}, folders={:?}, page_size={}",
        account_key, folders, page_size
    );

    let mut pending_floors: Vec<(String, u32)> = Vec::new();
    let mut raw_emails: Vec<RawNostrEmail> = Vec::new();
    // Folder is "exhausted" iff fetch_older_in_folder reported hit_bottom
    // for it. Overall hit_bottom = all scanned folders exhausted, AND at
    // least one folder was scanned successfully. Folder errors are treated
    // as "not exhausted" — the caller may want to retry.
    let mut any_folder_scanned = false;
    let mut all_folders_exhausted = true;

    let target = ImapTarget::from_config(config);
    let mut session = imap_pool::checkout(&target)?;
    for f in &folders {
        match fetch_older_in_folder(&mut session, config, db, &account_key, f,
                                     page_size, sync_cutoff_days,
                                     parse_nostr_email_from_imap_body) {
            Ok(r) => {
                any_folder_scanned = true;
                if !r.hit_bottom { all_folders_exhausted = false; }
                if let Some(new_floor) = r.new_floor_uid {
                    pending_floors.push((f.clone(), new_floor));
                }
                raw_emails.extend(r.emails);
            }
            Err(e) => {
                debug_log!("[RUST] fetch_older_inbox_emails_to_db: folder '{}' failed: {}", f, e);
                all_folders_exhausted = false;
            }
        }
    }
    imap_pool::checkin(&target, session);

    let new_count = persist_inbox_raw_emails(raw_emails, db, require_signature)?;

    for (folder_name, new_floor) in pending_floors {
        if let Err(e) = db.set_folder_min_seen_uid(&account_key, &folder_name, new_floor) {
            println!(
                "[RUST] fetch_older_inbox_emails_to_db: failed to lower min_seen_uid for '{}': {}",
                folder_name, e
            );
        } else {
            println!(
                "[RUST] fetch_older_inbox_emails_to_db: lowered min_seen_uid to {} for '{}'",
                new_floor, folder_name
            );
        }
    }

    let hit_bottom = any_folder_scanned && all_folders_exhausted;
    println!(
        "[RUST] fetch_older_inbox_emails_to_db: completed, {} new emails saved, hit_bottom={}",
        new_count, hit_bottom
    );
    Ok(FetchOlderSummary { new_count, hit_bottom })
}

/// Backward UID pagination for the sent folder. See `fetch_older_inbox_emails_to_db`.
pub async fn fetch_older_sent_emails_to_db(
    config: &EmailConfig,
    page_size: usize,
    active_pubkey: &str,
    db: &Database,
) -> anyhow::Result<FetchOlderSummary> {
    let account_key = config.email_address.trim().to_lowercase();
    let sync_cutoff_days = lookup_sync_cutoff_days(db, active_pubkey);

    let is_gmail = config.imap_host.contains("gmail.com");

    let mut pending_floors: Vec<(String, u32)> = Vec::new();
    let mut raw_emails: Vec<RawNostrEmail> = Vec::new();
    let mut any_folder_scanned = false;
    let mut all_folders_exhausted = true;

    let target = ImapTarget::from_config(config);
    let mut session = imap_pool::checkout(&target)?;
    let sent_folder = discover_sent_mailbox(&mut session)?
        .unwrap_or_else(|| if is_gmail { "[Gmail]/Sent Mail".to_string() } else { "Sent".to_string() });
    for f in [sent_folder.as_str(), "nostr-mail"] {
        match fetch_older_in_folder(&mut session, config, db, &account_key, f,
                                     page_size, sync_cutoff_days,
                                     parse_nostr_sent_email_from_imap_body) {
            Ok(r) => {
                any_folder_scanned = true;
                if !r.hit_bottom { all_folders_exhausted = false; }
                if let Some(new_floor) = r.new_floor_uid {
                    pending_floors.push((f.to_string(), new_floor));
                }
                raw_emails.extend(r.emails);
            }
            Err(e) => {
                debug_log!("[RUST] fetch_older_sent_emails_to_db: folder '{}' failed: {}", f, e);
                all_folders_exhausted = false;
            }
        }
    }
    imap_pool::checkin(&target, session);

    // Sent emails skip the require_signature filter — you authored them, so
    // they're always kept regardless of the inbox signature policy.
    let new_count = persist_inbox_raw_emails(raw_emails, db, /* require_signature */ false)?;

    for (folder_name, new_floor) in pending_floors {
        if let Err(e) = db.set_folder_min_seen_uid(&account_key, &folder_name, new_floor) {
            println!(
                "[RUST] fetch_older_sent_emails_to_db: failed to lower min_seen_uid for '{}': {}",
                folder_name, e
            );
        } else {
            println!(
                "[RUST] fetch_older_sent_emails_to_db: lowered min_seen_uid to {} for '{}'",
                new_floor, folder_name
            );
        }
    }

    let hit_bottom = any_folder_scanned && all_folders_exhausted;
    println!(
        "[RUST] fetch_older_sent_emails_to_db: completed, {} new emails saved, hit_bottom={}",
        new_count, hit_bottom
    );
    Ok(FetchOlderSummary { new_count, hit_bottom })
}

pub async fn sync_sent_emails_to_db(config: &EmailConfig, active_pubkey: &str, db: &Database) -> anyhow::Result<usize> {
    sync_sent_emails_to_db_inner(config, active_pubkey, db, /* include_gap_fill = */ false).await
}

/// Refresh-button variant of `sync_sent_emails_to_db`: also runs a gap-fill
/// pass per folder. See `gap_fill_in_folder` for shape.
pub async fn refresh_sent_emails_to_db(config: &EmailConfig, active_pubkey: &str, db: &Database) -> anyhow::Result<usize> {
    sync_sent_emails_to_db_inner(config, active_pubkey, db, /* include_gap_fill = */ true).await
}

async fn sync_sent_emails_to_db_inner(config: &EmailConfig, active_pubkey: &str, db: &Database, include_gap_fill: bool) -> anyhow::Result<usize> {
    debug_log!("[RUST] sync_sent_emails_to_db: Starting sync for email: {}", config.email_address);
    let account_key = config.email_address.trim().to_lowercase();
    let max_scan = lookup_max_scan(db, active_pubkey);

    // See sync_nostr_emails_to_db for the meaning of the 4-tuple.
    let mut pending_state: Vec<(String, u32, u32, Option<u32>)> = Vec::new();
    // Per-folder gap-fill examined windows (folder, uid_validity, floor, last_seen).
    let mut pending_gap_examined: Vec<(String, u32, u32, u32)> = Vec::new();
    let mut raw_sent_emails: Vec<RawNostrEmail> = Vec::new();

    let is_gmail = config.imap_host.contains("gmail.com");

    let target = ImapTarget::from_config(config);
    let mut session = imap_pool::checkout(&target)?;
    let sent_folder = discover_sent_mailbox(&mut session)?
        .unwrap_or_else(|| if is_gmail { "[Gmail]/Sent Mail".to_string() } else { "Sent".to_string() });
    for f in [sent_folder.as_str(), "nostr-mail"] {
        let target_count = lookup_folder_count(db, active_pubkey, f);
        match uid_sync_folder(&mut session, config, db, &account_key, f, target_count, max_scan,
                              parse_nostr_sent_email_from_imap_body) {
            Ok(r) => {
                if r.max_uid > 0 || !r.had_existing_state {
                    let bootstrap_min = if !r.had_existing_state && r.min_uid > 0 {
                        Some(r.min_uid)
                    } else {
                        None
                    };
                    pending_state.push((f.to_string(), r.uid_validity, r.max_uid, bootstrap_min));
                }
                raw_sent_emails.extend(r.emails);
            }
            Err(e) => debug_log!("[RUST] sync_sent_emails_to_db: folder '{}' failed: {}", f, e),
        }
        if include_gap_fill {
            // Sent mail is never dropped, so it never needs recovery scanning.
            match gap_fill_in_folder(&mut session, config, db, &account_key, f,
                                      /* recover_dropped */ false, parse_nostr_sent_email_from_imap_body) {
                Ok(r) => {
                    raw_sent_emails.extend(r.emails);
                    if let Some((uidv, lo, hi)) = r.examined {
                        pending_gap_examined.push((f.to_string(), uidv, lo, hi));
                    }
                }
                Err(e) => debug_log!("[RUST] sync_sent_emails_to_db: gap_fill folder '{}' failed: {}", f, e),
            }
        }
    }
    imap_pool::checkin(&target, session);

    debug_log!("[RUST] sync_sent_emails_to_db: Fetched {} emails from IMAP", raw_sent_emails.len());

    let mut new_count = 0;
    debug_log!("[RUST] sync_sent_emails_to_db: Processing {} emails for saving", raw_sent_emails.len());
    for (idx, email) in raw_sent_emails.iter().enumerate() {
        debug_log!("[RUST] sync_sent_emails_to_db: Processing email {} of {}: message_id={}, from={}, date={}", 
            idx + 1, raw_sent_emails.len(), email.message_id, email.from, email.date);
        // Skip emails that failed transport authentication
        if let Some(false) = email.transport_auth_verified {
            debug_log!("[RUST] sync_nostr_emails_to_db: Skipping email {} - transport authentication failed", email.message_id);
            continue;
        }
        
        // Check if already in DB by message_id (only check, don't save yet)
        debug_log!("[RUST] sync_sent_emails_to_db: Checking for existing email with message_id: {}", email.message_id);
        let existing_email = match db.get_email(&email.message_id) {
            Ok(Some(existing)) => {
                debug_log!("[RUST] sync_sent_emails_to_db: Found existing email in DB: id={:?}, message_id={}", existing.id, existing.message_id);
                Some(existing)
            },
            Ok(None) => {
                debug_log!("[RUST] sync_sent_emails_to_db: No existing email found in DB for message_id: {}", email.message_id);
                None
            },
            Err(e) => {
                debug_log!("[RUST] ERROR: Failed to check if email exists: {}", e);
                return Err(anyhow::anyhow!("Failed to check email {} in DB: {}", email.message_id, e));
            }
        };
        
        if let Some(existing_email) = existing_email {
                // Email already exists - update it with IMAP data (but preserve attachments)
                // Only update fields that might have changed from IMAP, don't overwrite attachment data
                debug_log!("[RUST] Email with message_id {} already exists (id: {:?}), updating with IMAP data (preserving attachments)", 
                    email.message_id, existing_email.id);
                let updated_email = DbEmail {
                    id: existing_email.id,
                    message_id: existing_email.message_id.clone(),
                    from_address: email.from.clone(),
                    to_address: email.to.clone(),
                    subject: email.subject.clone(), // Update with IMAP subject (might be more recent)
                    body: email.body.clone(),       // Update with IMAP body (might be more recent)
                    body_plain: existing_email.body_plain.clone(), // Preserve decrypted body if exists
                    body_html: existing_email.body_html.clone().or_else(|| email.html_body.clone()), // Preserve HTML if exists, otherwise use IMAP HTML
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
                    signature_source: email.signature_source.clone(),
                    transport_auth_verified: email.transport_auth_verified,
                    subject_hash: compute_subject_ciphertext_hash(&email.subject, &email.body),
                    in_reply_to: None,
                    references: None,
                    thread_id: None,
                };
                match db.save_email(&updated_email) {
                    Ok(id) => debug_log!("[RUST] Updated existing email with IMAP data, id: {}", id),
                    Err(e) => {
                        debug_log!("[RUST] ERROR: Failed to update email {}: {}", email.message_id, e);
                        return Err(anyhow::anyhow!("Failed to update email {}: {}", email.message_id, e));
                    }
                }
        } else {
            // New email - save raw email to DB directly without checking again
            debug_log!("[RUST] Email is new, inserting directly to DB (skipping redundant get_email check)");
            let db_email = DbEmail {
                id: None,
                message_id: email.message_id.clone(),
                from_address: email.from.clone(),
                to_address: email.to.clone(),
                subject: email.subject.clone(), // still encrypted
                body: email.body.clone(),       // still encrypted
                body_plain: None,
                body_html: email.html_body.clone(),
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
                signature_source: email.signature_source.clone(),
                transport_auth_verified: email.transport_auth_verified,
                subject_hash: compute_subject_ciphertext_hash(&email.subject, &email.body),
                in_reply_to: None,
                references: None,
                thread_id: None,
            };
            debug_log!("[RUST] Inserting new sent email to DB: message_id={}, from={}, to={}, subject_len={}, body_len={}",
                db_email.message_id, db_email.from_address, db_email.to_address, 
                db_email.subject.len(), db_email.body.len());
            let email_id = match db.insert_email_direct(&db_email) {
                Ok(id) => {
                    debug_log!("[RUST] Successfully inserted new sent email to DB with id: {}", id);
                    new_count += 1;
                    id
                }
                Err(e) => {
                    debug_log!("[RUST] ERROR: Failed to insert email to DB: {}", e);
                    return Err(anyhow::anyhow!("Failed to insert email {} to DB: {}", email.message_id, e));
                }
            };
            
            // Extract and save attachments from the email body
            // Parse the email body to extract attachments (they're in encrypted form)
            debug_log!("[RUST] sync_sent_emails_to_db: Email {} has {} attachments in RawNostrEmail", email.message_id, email.attachments.len());
            if !email.attachments.is_empty() {
                debug_log!("[RUST] Saving {} attachments for email {} (id: {})", email.attachments.len(), email.message_id, email_id);
                for mut attachment in email.attachments.iter().cloned() {
                    attachment.email_id = email_id;
                    debug_log!("[RUST] Saving attachment: filename={}, size={}, encrypted={}, email_id={}", 
                        attachment.filename, attachment.size, attachment.is_encrypted, attachment.email_id);
                    match db.save_attachment(&attachment) {
                        Ok(att_id) => {
                            debug_log!("[RUST] Successfully saved attachment {} (id: {}) for email {}", attachment.filename, att_id, email_id);
                        }
                        Err(e) => {
                            debug_log!("[RUST] ERROR: Failed to save attachment {}: {}", attachment.filename, e);
                            // Don't fail the whole sync if attachment save fails
                        }
                    }
                }
            } else {
                debug_log!("[RUST] sync_sent_emails_to_db: Email {} has no attachments in RawNostrEmail, trying to extract from body", email.message_id);
                // Try to extract attachments by parsing the raw RFC822 email body
                // The email.body might just be the text part, so we need to re-fetch the full email
                // For now, try parsing the body - if it's multipart, we can extract attachments
                // TODO: Store raw RFC822 body in RawNostrEmail for proper attachment extraction
                if let Ok(parsed_email) = mailparse::parse_mail(email.body.as_bytes()) {
                    let extracted_attachments = extract_attachments_from_parsed_email(&parsed_email, &email.body);
                    if !extracted_attachments.is_empty() {
                        debug_log!("[RUST] Extracted {} attachments from email body for email {}", extracted_attachments.len(), email_id);
                        for mut attachment in extracted_attachments {
                            attachment.email_id = email_id;
                            match db.save_attachment(&attachment) {
                                Ok(att_id) => {
                                    debug_log!("[RUST] Saved extracted attachment {} (id: {}) for email {}", attachment.filename, att_id, email_id);
                                }
                                Err(e) => {
                                    debug_log!("[RUST] ERROR: Failed to save extracted attachment {}: {}", attachment.filename, e);
                                }
                            }
                        }
                    }
                } else {
                    // Body is not parseable as multipart - might need to re-fetch from IMAP
                    // For now, log and continue
                    debug_log!("[RUST] Could not parse email body to extract attachments for email {}", email_id);
                }
            }
        }
    }
    // Commit per-folder UID watermarks after all per-message DB writes succeeded.
    for (folder_name, uid_validity, max_uid, bootstrap_min) in pending_state {
        if let Err(e) = db.set_folder_sync_state(&account_key, &folder_name, uid_validity, max_uid) {
            println!(
                "[RUST] sync_sent_emails_to_db: failed to persist folder_sync_state for '{}': {}",
                folder_name, e
            );
            continue;
        }
        println!(
            "[RUST] sync_sent_emails_to_db: persisted folder_sync_state for '{}' (uid_validity={}, last_seen_uid={})",
            folder_name, uid_validity, max_uid
        );
        if let Some(min) = bootstrap_min {
            if let Err(e) = db.set_folder_min_seen_uid(&account_key, &folder_name, min) {
                println!(
                    "[RUST] sync_sent_emails_to_db: failed to seed min_seen_uid for '{}': {}",
                    folder_name, e
                );
            } else {
                println!(
                    "[RUST] sync_sent_emails_to_db: seeded min_seen_uid={} for '{}'",
                    min, folder_name
                );
            }
        }
    }

    // Commit gap-fill examined windows post-persist (see sync_nostr_emails_to_db).
    for (folder_name, uid_validity, lo, hi) in pending_gap_examined {
        if let Err(e) = db.set_folder_gap_examined(&account_key, &folder_name, uid_validity, lo, hi) {
            debug_log!("[RUST] sync_sent_emails_to_db: failed to record gap_examined for '{}': {}", folder_name, e);
        }
    }

    debug_log!("[RUST] sync_sent_emails_to_db: Completed sync, {} new emails saved", new_count);
    Ok(new_count)
}

pub struct RawNostrEmail {
    pub message_id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub html_body: Option<String>,
    pub date: chrono::DateTime<chrono::Utc>,
    pub sender_pubkey: Option<String>,
    pub recipient_pubkey: Option<String>,
    pub raw_headers: String,
    pub attachments: Vec<crate::database::Attachment>, // Attachments extracted from email (in encrypted form)
    pub signature_valid: Option<bool>,
    pub signature_source: Option<String>,
    pub transport_auth_verified: Option<bool>,
    /// Server `\Seen` flag at fetch time, when the IMAP fetch requested FLAGS.
    /// `None` means "not known from this fetch" (parse paths don't see flags);
    /// callers treat `None` as unread for new inserts. Set by the inbox fetch
    /// loops so read state set on another client/device imports on first sync.
    pub seen: Option<bool>,
}

/// Parse an IMAP RFC822 message body and return Some(RawNostrEmail) if it is a
/// nostr email (X-Nostr-Pubkey header OR inline armor markers) that passes
/// transport authentication. The signed-body armor format carries the pubkey
/// inline, so a header-only filter would drop valid emails.
fn parse_nostr_email_from_imap_body(raw_body: &[u8], config: &EmailConfig) -> Option<RawNostrEmail> {
    parse_nostr_email_from_imap_body_inner(raw_body, config, /* verify_transport = */ true)
}

/// Same as `parse_nostr_email_from_imap_body` but skips transport authentication.
/// Used for the user's own sent mail, where DKIM/SPF semantics don't apply (we
/// know we sent it; the SMTP server doesn't re-sign on save to "Sent").
fn parse_nostr_sent_email_from_imap_body(raw_body: &[u8], config: &EmailConfig) -> Option<RawNostrEmail> {
    parse_nostr_email_from_imap_body_inner(raw_body, config, /* verify_transport = */ false)
}

fn parse_nostr_email_from_imap_body_inner(
    raw_body: &[u8],
    config: &EmailConfig,
    verify_transport: bool,
) -> Option<RawNostrEmail> {
    use chrono::Utc;
    use crate::email::extract_sender_pubkey_with_armor_fallback;

    let email = parse_mail(raw_body).ok()?;
    let raw_headers = email
        .headers
        .iter()
        .map(|h| format!("{}: {}", h.get_key(), h.get_value()))
        .collect::<Vec<_>>()
        .join("\n");

    let body_text = extract_text_body(&email)
        .unwrap_or_else(|| email.get_body().unwrap_or_else(|_| "No body content".to_string()));

    let has_nostr_header = raw_headers.contains("X-Nostr-Pubkey:");
    let has_armor = body_text.contains("BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE")
        || body_text.contains("BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE")
        || body_text.contains("BEGIN NOSTR NIP-04 ENCRYPTED BODY")
        || body_text.contains("BEGIN NOSTR NIP-44 ENCRYPTED BODY");
    if !has_nostr_header && !has_armor {
        return None;
    }

    let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
    let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
    let subject_raw = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
    let subject = decode_header_value(&subject_raw);
    let date_str = email.headers.get_first_value("Date").unwrap_or_else(|| Utc::now().to_rfc2822());
    let date = chrono::DateTime::parse_from_rfc2822(&date_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let extracted_attachments = extract_attachments_from_parsed_email(&email, &body_text);
    let sender_pubkey = extract_sender_pubkey_with_armor_fallback(&raw_headers, &body_text);
    let recipient_pubkey = extract_nostr_recipient_from_headers(&raw_headers);
    let (signature_valid, signature_source) = verify_email_signature_full(&body_text, &raw_headers);
    let message_id = extract_message_id_from_headers(&raw_headers).unwrap_or_else(|| Uuid::new_v4().to_string());

    let transport_auth_verified = if verify_transport {
        let verdict = verify_transport_authentication(Some(raw_body), Some(&email))
            .unwrap_or_else(|e| TransportAuthVerdict {
                transport_verified: false,
                method: TransportAuthMethod::None,
                reason: format!("Error verifying transport auth: {}", e),
            });
        if !verdict.transport_verified {
            debug_log!("[RUST] parse_nostr_email_from_imap_body: Email {} failed transport authentication: {}", message_id, verdict.reason);
            return None;
        }
        Some(true)
    } else {
        None
    };

    Some(RawNostrEmail {
        message_id,
        from,
        to,
        subject,
        body: body_text,
        html_body: extract_html_body(&email),
        date,
        sender_pubkey,
        recipient_pubkey,
        raw_headers,
        attachments: extracted_attachments,
        signature_valid,
        signature_source,
        transport_auth_verified,
        // Parsing only sees the message body, not IMAP flags. The fetch loop
        // overrides this from `msg.flags()` when it requested FLAGS.
        seen: None,
    })
}

/// Result of a UID-based per-folder sync. The caller persists `emails` to the
/// DB and only after success calls `db.set_folder_sync_state(...)` with the
/// returned `(uid_validity, max_uid)`.
struct UidSyncResult {
    emails: Vec<RawNostrEmail>,
    uid_validity: u32,
    max_uid: u32,
    // Lowest UID actually returned by the IMAP server for this sync. 0 if no
    // UIDs matched (caller should ignore in that case). On bootstrap this is
    // the floor of "messages we've definitely fetched"; on incremental sync
    // it's just the lowest new UID in the delta (caller should NOT use it to
    // lower min_seen_uid past the existing bootstrap floor).
    min_uid: u32,
    had_existing_state: bool,
}

/// Legacy-only: build a date-windowed `SINCE <date> UID 1:*` SEARCH (filters by
/// INTERNALDATE, independent of Gmail's full-text index). Forward-sync bootstrap
/// and gap-fill are now count-based and no longer call this. It survives solely
/// as `fetch_older`'s floor-backfill for installs that have a `folder_sync_state`
/// row predating the `min_seen_uid` column — a cheap way (one SEARCH, no body
/// fetches) to seed an approximate floor so backward paging can start. New
/// installs always record a floor at bootstrap, so they never reach this.
fn build_bootstrap_query(sync_cutoff_days: i64) -> String {
    if sync_cutoff_days <= 0 {
        "UID 1:*".to_string()
    } else {
        let since = (chrono::Utc::now() - chrono::Duration::days(sync_cutoff_days))
            .format("%d-%b-%Y")
            .to_string();
        format!("SINCE {} UID 1:*", since)
    }
}

/// UID-based sync of one IMAP folder.
///
/// Workflow:
/// 1. SELECT folder, read mailbox UIDVALIDITY.
/// 2. Compare to stored `folder_sync_state`.
/// 3. On match (incremental): `UID SEARCH UID <last_seen+1>:*`, fetch every new
///    UID. New mail is always taken in full — the count only bounds history.
///    On mismatch / no row (bootstrap): `UID SEARCH 1:*`, then walk newest→oldest
///    via `walk_back_collecting` until `target_count` nostr matches accumulate or
///    `max_scan` raw messages have been examined. The folder watermark
///    (`max_uid`) is the true newest UID even when we body-fetch far fewer.
/// 4. Run `parse_fn` on each fetched body; collect.
///
/// The caller is responsible for persisting parsed emails AND updating
/// `folder_sync_state` after a successful save — that way a partial-failure
/// run can retry from the same watermark.
fn uid_sync_folder<S: std::io::Read + std::io::Write>(
    session: &mut imap::Session<S>,
    config: &EmailConfig,
    db: &Database,
    account_key: &str,
    folder_name: &str,
    target_count: usize,
    max_scan: usize,
    parse_fn: fn(&[u8], &EmailConfig) -> Option<RawNostrEmail>,
) -> anyhow::Result<UidSyncResult> {
    let mb = session.select(folder_name)?;
    let uid_validity = mb.uid_validity
        .ok_or_else(|| anyhow::anyhow!("server did not advertise UIDVALIDITY for folder '{}'", folder_name))?;

    let stored = db.get_folder_sync_state(account_key, folder_name)?;
    let had_existing_state = stored.is_some();
    let incremental = matches!(&stored, Some(s) if s.uid_validity == uid_validity);

    // ---- Incremental path: take ALL new mail above the watermark ----
    if incremental {
        let next = stored.as_ref().unwrap().last_seen_uid.saturating_add(1);
        let query = format!("UID {}:*", next);
        debug_log!("[RUST] uid_sync_folder: '{}' UID SEARCH {}", folder_name, query);
        let uid_set = session.uid_search(&query)?;
        if uid_set.is_empty() {
            return Ok(UidSyncResult {
                emails: vec![], uid_validity, max_uid: 0, min_uid: 0, had_existing_state,
            });
        }
        let mut uids: Vec<u32> = uid_set.into_iter().collect();
        uids.sort_unstable();
        let min_uid = uids.first().copied().unwrap_or(0);
        debug_log!("[RUST] uid_sync_folder: '{}' matched {} new UIDs (min {}, max {})",
            folder_name, uids.len(), min_uid, uids.last().copied().unwrap_or(0));

        // Chunk into ~500-UID batches to stay well under Gmail's ~8KB IMAP line limit.
        const FETCH_BATCH: usize = 500;
        let mut emails: Vec<RawNostrEmail> = Vec::new();
        let mut max_uid: u32 = 0;
        for chunk in uids.chunks(FETCH_BATCH) {
            let uid_list = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
            // BODY.PEEK[] (not RFC822) so reading a message body never *sets* \Seen.
            // FLAGS lets us read the server's \Seen and seed local read state.
            let messages = session.uid_fetch(&uid_list, "(UID FLAGS BODY.PEEK[])")?;
            for msg in messages.iter() {
                if let Some(uid) = msg.uid {
                    if uid > max_uid { max_uid = uid; }
                }
                if let Some(body) = msg.body() {
                    if let Some(mut parsed) = parse_fn(body, config) {
                        parsed.seen = Some(msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen)));
                        emails.push(parsed);
                    }
                }
            }
        }
        return Ok(UidSyncResult { emails, uid_validity, max_uid, min_uid, had_existing_state });
    }

    // ---- Bootstrap path: count-based backward walk from the newest UID ----
    // Triggered when there's no stored state, or UIDVALIDITY changed (UIDs were
    // reassigned, so prior watermarks are meaningless and we re-bootstrap).
    if let Some(s) = &stored {
        println!("[RUST] uid_sync_folder: UIDVALIDITY changed for '{}' ({} -> {}), re-bootstrapping",
            folder_name, s.uid_validity, uid_validity);
    } else {
        println!("[RUST] uid_sync_folder: no stored state for '{}', bootstrapping", folder_name);
    }

    let uid_set = session.uid_search("UID 1:*")?;
    if uid_set.is_empty() {
        return Ok(UidSyncResult {
            emails: vec![], uid_validity, max_uid: 0, min_uid: 0, had_existing_state,
        });
    }
    let mut uids: Vec<u32> = uid_set.into_iter().collect();
    uids.sort_unstable();
    // The true newest UID becomes the watermark even though we body-fetch only
    // the count window below it — forward sync then resumes from here.
    let watermark = uids.last().copied().unwrap_or(0);
    let target = target_count.max(1);
    println!("[RUST] uid_sync_folder: '{}' bootstrap — {} UIDs in folder, target {} match(es), max_scan {}",
        folder_name, uids.len(), target, max_scan);

    // Seed `lowest_scanned` at the watermark so a no-op walk never claims to
    // have scanned below it; the walk lowers it to the oldest UID it touches,
    // which becomes the bootstrap floor (min_seen_uid).
    let walk = walk_back_collecting(session, config, &uids, watermark, target, max_scan, parse_fn)?;

    Ok(UidSyncResult {
        emails: walk.emails,
        uid_validity,
        max_uid: watermark,
        min_uid: walk.lowest_scanned,
        had_existing_state,
    })
}

/// Result of a backward UID fetch ("load older from server"). `new_floor_uid`
/// is the value to lower `folder_sync_state.min_seen_uid` to once the caller
/// has persisted all parsed emails. None means "no further old messages on the
/// server" — caller can record that we hit the bottom (we set the floor to 1).
struct FetchOlderResult {
    emails: Vec<RawNostrEmail>,
    new_floor_uid: Option<u32>,
    // True iff we walked through every candidate UID strictly below the
    // current floor without finding more (or there were none). Distinguishes
    // "real bottom of the folder" from "we exhausted this call's scan budget
    // and there might still be more older nostr mail further down".
    hit_bottom: bool,
}

/// Result of `walk_back_collecting`: the nostr emails found, the lowest UID we
/// actually touched (the caller turns this into the new floor), how many UIDs
/// were scanned, and whether the candidate list was fully exhausted.
struct CountWalk {
    emails: Vec<RawNostrEmail>,
    lowest_scanned: u32,
    scanned: usize,
    hit_bottom: bool,
}

/// Pure per-iteration decision for the backward count-walk: given how many
/// UIDs we've scanned so far (`scanned`), the total candidate count (`total`),
/// the matches accumulated (`matches_so_far`), the match target and the scan
/// budget, return the `[start, end)` slice of the ascending candidate list to
/// fetch next — newest UIDs are at the tail, so we walk from the end inward.
/// Returns `None` when the walk should stop (target met, budget spent, or list
/// exhausted). Factored out of `walk_back_collecting` so the boundary logic is
/// unit-testable without a live IMAP session.
fn next_back_batch(
    scanned: usize,
    total: usize,
    matches_so_far: usize,
    target_count: usize,
    max_scan: usize,
) -> Option<(usize, usize)> {
    if scanned >= total || matches_so_far >= target_count || scanned >= max_scan {
        return None;
    }
    let remaining_target = target_count.saturating_sub(matches_so_far);
    let remaining_budget = max_scan.saturating_sub(scanned);
    let remaining_candidates = total - scanned;
    let take = remaining_target.max(1).min(remaining_budget).min(remaining_candidates);
    let end = total - scanned;
    let start = end - take;
    Some((start, end))
}

/// Walk a pre-sorted (ascending) UID list newest→oldest, fetching bodies in
/// batches and running `parse_fn`, until either `target_count` nostr matches
/// accumulate, `max_scan` UIDs have been examined, or the list is exhausted.
///
/// `floor_init` seeds `lowest_scanned` so a no-op call never raises the floor.
/// Shared by `fetch_older_in_folder` (and, later, the count-based bootstrap):
/// the only thing that differs between callers is which candidate UIDs they
/// feed in. BODY.PEEK[] keeps reads from setting \Seen; FLAGS lets us seed
/// local read state for newly-imported rows.
fn walk_back_collecting<S: std::io::Read + std::io::Write>(
    session: &mut imap::Session<S>,
    config: &EmailConfig,
    uids_sorted: &[u32],
    floor_init: u32,
    target_count: usize,
    max_scan: usize,
    parse_fn: fn(&[u8], &EmailConfig) -> Option<RawNostrEmail>,
) -> anyhow::Result<CountWalk> {
    let mut emails: Vec<RawNostrEmail> = Vec::new();
    let mut scanned: usize = 0;
    let mut lowest_scanned: u32 = floor_init;

    // Cap each UID FETCH line at ~500 UIDs to stay under Gmail's ~8KB IMAP line
    // limit. `take` can be large on bootstrap (a deep count target), so split
    // the slice even though a single batch is logically one walk step.
    const FETCH_BATCH: usize = 500;
    while let Some((start, end)) =
        next_back_batch(scanned, uids_sorted.len(), emails.len(), target_count, max_scan)
    {
        for chunk in uids_sorted[start..end].chunks(FETCH_BATCH) {
            let uid_list = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
            let msgs = session.uid_fetch(&uid_list, "(UID FLAGS BODY.PEEK[])")?;
            for msg in msgs.iter() {
                if let Some(uid) = msg.uid {
                    if uid < lowest_scanned { lowest_scanned = uid; }
                }
                if let Some(body) = msg.body() {
                    if let Some(mut parsed) = parse_fn(body, config) {
                        parsed.seen = Some(msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen)));
                        emails.push(parsed);
                    }
                }
            }
        }
        scanned += end - start;
    }

    let hit_bottom = scanned >= uids_sorted.len();
    Ok(CountWalk { emails, lowest_scanned, scanned, hit_bottom })
}

/// Pull older messages from one folder, walking UIDs backward.
///
/// Requires a prior forward sync (folder_sync_state row must exist). The floor
/// (`min_seen_uid`) is normally recorded at bootstrap. Only legacy installs
/// whose row predates the `min_seen_uid` column hit the backfill branch, which
/// seeds an approximate floor via the date-windowed SEARCH (`build_bootstrap_query`).
///
/// Walks newest-to-oldest from `min_seen_uid - 1`, fetching full bodies and
/// running `parse_fn`. Stops when either:
///   * `page_size` nostr matches have accumulated, OR
///   * `max(page_size, MIN_SCAN_PER_CALL)` UIDs have been scanned, OR
///   * the candidate set is exhausted (`hit_bottom = true`).
///
/// Scanning more than `page_size` matters when the folder is mostly non-nostr
/// mail — common for Sent. Without it, one batch of non-matches looks like
/// "bottom hit" and the UI gives up prematurely.
fn fetch_older_in_folder<S: std::io::Read + std::io::Write>(
    session: &mut imap::Session<S>,
    config: &EmailConfig,
    db: &Database,
    account_key: &str,
    folder_name: &str,
    page_size: usize,
    sync_cutoff_days: i64,
    parse_fn: fn(&[u8], &EmailConfig) -> Option<RawNostrEmail>,
) -> anyhow::Result<FetchOlderResult> {
    let mb = session.select(folder_name)?;
    let uid_validity = mb.uid_validity
        .ok_or_else(|| anyhow::anyhow!("server did not advertise UIDVALIDITY for folder '{}'", folder_name))?;

    let stored = match db.get_folder_sync_state(account_key, folder_name)? {
        Some(s) => s,
        None => {
            debug_log!("[RUST] fetch_older_in_folder: '{}' has no sync state, run forward sync first", folder_name);
            return Ok(FetchOlderResult { emails: vec![], new_floor_uid: None, hit_bottom: false });
        }
    };

    if stored.uid_validity != uid_validity {
        debug_log!("[RUST] fetch_older_in_folder: '{}' UIDVALIDITY changed ({} -> {}), aborting backward fetch",
            folder_name, stored.uid_validity, uid_validity);
        return Ok(FetchOlderResult { emails: vec![], new_floor_uid: None, hit_bottom: false });
    }

    let floor_uid = match stored.min_seen_uid {
        Some(v) => v,
        None => {
            let bootstrap_query = build_bootstrap_query(sync_cutoff_days);
            debug_log!("[RUST] fetch_older_in_folder: '{}' has no min_seen_uid, backfilling via '{}'",
                folder_name, bootstrap_query);
            let set = session.uid_search(&bootstrap_query)?;
            if set.is_empty() {
                return Ok(FetchOlderResult { emails: vec![], new_floor_uid: None, hit_bottom: false });
            }
            let backfilled_min = set.into_iter().min().unwrap_or(0);
            db.set_folder_min_seen_uid(account_key, folder_name, backfilled_min)?;
            backfilled_min
        }
    };

    if floor_uid <= 1 {
        return Ok(FetchOlderResult { emails: vec![], new_floor_uid: None, hit_bottom: true });
    }

    let query = format!("UID 1:{}", floor_uid - 1);
    debug_log!("[RUST] fetch_older_in_folder: '{}' UID SEARCH {}", folder_name, query);
    let uid_set = session.uid_search(&query)?;
    if uid_set.is_empty() {
        // Server has nothing below the floor — mark bottom so subsequent
        // calls short-circuit (floor moves to 1, hit_bottom signals UI).
        return Ok(FetchOlderResult { emails: vec![], new_floor_uid: Some(1), hit_bottom: true });
    }

    let mut uids: Vec<u32> = uid_set.into_iter().collect();
    uids.sort_unstable();

    // Per-call scan budget. `page_size` of 5 with a folder dominated by
    // non-nostr mail would otherwise terminate at the first batch of misses.
    // Keep this generous enough to walk through the common pattern of a few
    // non-nostr in between nostr matches, but small enough that one scroll
    // trigger doesn't pull megabytes of unrelated bodies. 50 hits a
    // reasonable middle.
    const MIN_SCAN_PER_CALL: usize = 50;
    let target_count = page_size.max(1);
    let max_scan = target_count.max(MIN_SCAN_PER_CALL);

    // Walk newest-to-oldest, fetching bodies in batches, stopping when we have
    // enough matches or hit the per-call cap. `floor_uid` seeds the lowest-UID
    // tracker so a no-op call never raises the floor.
    let walk = walk_back_collecting(session, config, &uids, floor_uid, target_count, max_scan, parse_fn)?;

    // When we've truly hit bottom, set floor to 1 so the next call
    // short-circuits. Otherwise lower it to the lowest UID we touched.
    let returned_floor = if walk.hit_bottom { 1 } else { walk.lowest_scanned };

    debug_log!(
        "[RUST] fetch_older_in_folder: '{}' scanned {}/{} UIDs, found {} nostr match(es), hit_bottom={}, new_floor={}",
        folder_name, walk.scanned, uids.len(), walk.emails.len(), walk.hit_bottom, returned_floor
    );

    Ok(FetchOlderResult {
        emails: walk.emails,
        new_floor_uid: Some(returned_floor),
        hit_bottom: walk.hit_bottom,
    })
}

/// Gap-fill scan over one folder. Looks for UIDs the server claims should be
/// in our "scanned range" (between `min_seen_uid` and `last_seen_uid`) but
/// whose Message-IDs aren't in the local DB — the classic Gmail-index-lag
/// scenario where a recent message wasn't returned by the bootstrap SEARCH
/// but is now visible to subsequent searches.
///
/// Caller is responsible for SELECTing the folder via prior `uid_sync_folder`;
/// we re-SELECT here defensively to refresh UIDVALIDITY.
///
/// Cost shape: 1 cheap UID SEARCH, 1 batched ENVELOPE fetch (server-side
/// parse, no body bytes), then a body fetch only for the gaps we actually
/// find. On a healthy DB with no gaps, the body fetch is skipped entirely.
/// Outcome of one `gap_fill_in_folder` pass.
struct GapFillResult {
    /// Newly-found nostr emails to persist.
    emails: Vec<RawNostrEmail>,
    /// The `[floor, last_seen]` window now considered fully gap-examined under
    /// the current `uid_validity`. The caller persists it (via
    /// `set_folder_gap_examined`) *after* a successful save, so a future pass
    /// can skip this range. `None` when gap-fill bailed before establishing a
    /// window (no state / UIDVALIDITY mismatch / empty search).
    examined: Option<(u32, u32, u32)>, // (uid_validity, floor, last_seen)
}

impl GapFillResult {
    fn empty() -> Self {
        GapFillResult { emails: Vec::new(), examined: None }
    }
}

fn gap_fill_in_folder<S: std::io::Read + std::io::Write>(
    session: &mut imap::Session<S>,
    config: &EmailConfig,
    db: &Database,
    account_key: &str,
    folder_name: &str,
    recover_dropped: bool,
    parse_fn: fn(&[u8], &EmailConfig) -> Option<RawNostrEmail>,
) -> anyhow::Result<GapFillResult> {
    let mb = session.select(folder_name)?;
    let uid_validity = mb.uid_validity
        .ok_or_else(|| anyhow::anyhow!("server did not advertise UIDVALIDITY for folder '{}'", folder_name))?;

    let stored = match db.get_folder_sync_state(account_key, folder_name)? {
        Some(s) if s.uid_validity == uid_validity => s,
        // No state, or UIDVALIDITY change — bail. Forward sync will set up
        // fresh watermarks; gap-fill only makes sense once we have a stable
        // [min, last] range to scan inside of.
        _ => return Ok(GapFillResult::empty()),
    };

    // Resolve the floor (lower bound of the synced range). A legacy install may
    // have a watermark but no recorded floor; we can't honestly define
    // [floor, last_seen] without one (guessing the folder minimum would claim
    // everything down to UID 1 is synced), so skip gap-fill until the floor is
    // established by a re-bootstrap or by fetch_older on scroll.
    let floor = match stored.min_seen_uid {
        Some(v) => v,
        None => {
            debug_log!("[RUST] gap_fill: '{}' has no min_seen_uid yet, skipping until a floor is established", folder_name);
            return Ok(GapFillResult::empty());
        }
    };
    if floor == 0 || stored.last_seen_uid < floor {
        return Ok(GapFillResult::empty());
    }

    // The window we'll report as gap-examined to the caller (committed only
    // after a successful persist). Covers the full scanned range even when no
    // gaps are found, so a future pass can skip it.
    let examined = Some((uid_validity, floor, stored.last_seen_uid));

    // Candidates: UIDs inside our synced range [floor, last_seen] to (re)check.
    // Below `floor` is fetch_older territory; above `last_seen_uid` is forward
    // sync territory. Both have their own paths and shouldn't be re-touched here.
    // We always scan the full range by UID (no date window) — the count-based
    // bootstrap keeps [floor, last_seen] bounded, and the ENVELOPE pass below is
    // cheap (no bodies until a genuine gap is found).
    //
    // Steady state (recover_dropped = false): skip the already-examined
    // sub-range [gap_examined_min, gap_examined_max] — those UIDs' nostr-ness
    // was already determined (bodies are immutable), so re-checking them is
    // wasted work. This is what stops every refresh from re-examining the whole
    // window.
    //
    // Recovery (recover_dropped = true): the user just lifted the signed-only
    // filter, so mail we previously dropped must come back. Those drops can sit
    // anywhere in [floor, last_seen], so we ignore the examined marker and
    // re-check the full range. Once the drops are re-persisted, the ENVELOPE
    // re-scan finds nothing new while the filter stays off.
    let already_examined = |uid: u32| match (stored.gap_examined_min_uid, stored.gap_examined_max_uid) {
        (Some(lo), Some(hi)) => uid >= lo && uid <= hi,
        _ => false,
    };
    let candidates: Vec<u32> = session
        .uid_search(&format!("UID {}:{}", floor, stored.last_seen_uid))?
        .into_iter()
        .filter(|uid| {
            *uid >= floor
                && *uid <= stored.last_seen_uid
                && (recover_dropped || !already_examined(*uid))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(GapFillResult { emails: Vec::new(), examined });
    }

    // Resolve Message-IDs via ENVELOPE (server-side parse, no body fetch) and
    // look each up in the local DB. UIDs whose Message-ID is missing — or
    // whose server-side ENVELOPE has no Message-ID at all — are flagged as
    // gaps and pulled in the next step.
    const FETCH_BATCH: usize = 500;
    let mut missing: Vec<u32> = Vec::new();
    for chunk in candidates.chunks(FETCH_BATCH) {
        let uid_list = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        let msgs = session.uid_fetch(&uid_list, "(UID ENVELOPE)")?;
        for msg in msgs.iter() {
            let uid = match msg.uid { Some(u) => u, None => continue };
            let mid = match msg.envelope().and_then(|e| e.message_id.clone()) {
                Some(bytes) => std::str::from_utf8(&bytes).unwrap_or("").to_string(),
                None => { missing.push(uid); continue; }
            };
            let mid = mid.trim().trim_start_matches('<').trim_end_matches('>').trim().to_string();
            if mid.is_empty() {
                missing.push(uid);
                continue;
            }
            match db.get_email(&mid) {
                Ok(Some(_)) => continue,
                Ok(None) => missing.push(uid),
                Err(e) => debug_log!("[RUST] gap_fill: get_email({}) failed: {}", mid, e),
            }
        }
    }

    if missing.is_empty() {
        return Ok(GapFillResult { emails: Vec::new(), examined });
    }
    println!(
        "[RUST] gap_fill_in_folder: '{}' found {} missing UID(s) within [{}, {}]",
        folder_name, missing.len(), floor, stored.last_seen_uid
    );

    let mut emails: Vec<RawNostrEmail> = Vec::new();
    for chunk in missing.chunks(FETCH_BATCH) {
        let uid_list = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        // BODY.PEEK[] so gap-fill backfill doesn't *set* \Seen; FLAGS so we can
        // read it and seed local read state for newly-imported rows.
        let msgs = session.uid_fetch(&uid_list, "(UID FLAGS BODY.PEEK[])")?;
        for msg in msgs.iter() {
            if let Some(body) = msg.body() {
                if let Some(mut parsed) = parse_fn(body, config) {
                    parsed.seen = Some(msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen)));
                    emails.push(parsed);
                }
            }
        }
    }
    Ok(GapFillResult { emails, examined })
}

/// Legacy-only: read `sync_cutoff_days` for the active pubkey, defaulting to 30.
/// The settings UI no longer writes this key (replaced by per-folder counts);
/// it now feeds only `fetch_older`'s legacy floor-backfill (see
/// `build_bootstrap_query`), where the 30-day default is a fine approximation.
/// Read directly from the active pubkey's settings — no email-based reverse
/// lookup. Sharing an email address across multiple identities used to make
/// this return the first matching pubkey's value, which was rarely the active
/// one (see log "Found pubkeys for email_address setting 'X': [npub_a, npub_b]").
fn lookup_sync_cutoff_days(db: &Database, pubkey: &str) -> i64 {
    if let Ok(Some(value)) = db.get_setting(pubkey, "sync_cutoff_days") {
        if let Ok(parsed) = value.parse::<i64>() {
            return parsed;
        }
    }
    30
}

/// Default number of nostr matches to pull when bootstrapping a folder, when
/// the user hasn't set `sync_initial_count`.
const DEFAULT_INITIAL_COUNT: usize = 50;
/// Default bootstrap depth for the dedicated dense `nostr-mail` folder, where
/// (nearly) every message is nostr mail so a deep count is cheap and accurate.
/// Overridable per-folder via `sync_folder_counts`.
const DEFAULT_DENSE_COUNT: usize = 500;
/// Default cap on raw messages examined per folder bootstrap. Bounds the cost
/// on sparse mixed folders (e.g. a busy INBOX) where the target match count
/// would otherwise force an unbounded backward walk.
const DEFAULT_MAX_SCAN: usize = 2000;

/// Pure resolution of a folder's bootstrap count. An explicit override in the
/// user's `sync_folder_counts` map wins; otherwise the dedicated dense
/// `nostr-mail` folder defaults deep while every other folder uses the global
/// `sync_initial_count`. Factored out for unit testing without a `Database`.
fn resolve_folder_count(folder: &str, folder_counts_json: Option<&str>, initial_count: usize) -> usize {
    if let Some(json) = folder_counts_json {
        if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, usize>>(json) {
            if let Some(n) = map.get(folder) {
                return *n;
            }
        }
    }
    if folder.eq_ignore_ascii_case("nostr-mail") {
        return DEFAULT_DENSE_COUNT;
    }
    initial_count
}

/// Global per-folder bootstrap target (nostr matches), defaulting to
/// `DEFAULT_INITIAL_COUNT`. Per-pubkey preference, like the cutoff above.
fn lookup_initial_count(db: &Database, pubkey: &str) -> usize {
    db.get_setting(pubkey, "sync_initial_count")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_INITIAL_COUNT)
}

/// Cap on raw messages examined per folder bootstrap, defaulting to
/// `DEFAULT_MAX_SCAN`.
fn lookup_max_scan(db: &Database, pubkey: &str) -> usize {
    db.get_setting(pubkey, "sync_max_scan")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_SCAN)
}

/// Per-folder bootstrap count for the active pubkey: per-folder override →
/// dense-folder default → global `sync_initial_count`.
fn lookup_folder_count(db: &Database, pubkey: &str, folder: &str) -> usize {
    let json = db.get_setting(pubkey, "sync_folder_counts").ok().flatten();
    resolve_folder_count(folder, json.as_deref(), lookup_initial_count(db, pubkey))
}

/// Read `require_signature` for the active pubkey, defaulting to true.
pub(crate) fn lookup_require_signature(db: &Database, pubkey: &str) -> bool {
    if let Ok(Some(value)) = db.get_setting(pubkey, "require_signature") {
        return value == "true";
    }
    true
}

/// Read `spam_rescue` for the active pubkey, defaulting to true (on by default).
/// Only an explicit "false" disables it; absent/blank settings stay on.
pub(crate) fn lookup_spam_rescue(db: &Database, pubkey: &str) -> bool {
    if let Ok(Some(value)) = db.get_setting(pubkey, "spam_rescue") {
        return value != "false";
    }
    true
}

/// Read `auto_move_nostr` for the active pubkey, defaulting to true. When on,
/// each sync moves nostr mail found in the regular inbox folders into the
/// rescue/nostr-mail folder, consolidating all nostr mail in one place.
pub(crate) fn lookup_auto_move_nostr(db: &Database, pubkey: &str) -> bool {
    if let Ok(Some(value)) = db.get_setting(pubkey, "auto_move_nostr") {
        return value != "false";
    }
    true
}

/// Read the folder spam-rescued nostr mail should be moved into, defaulting to
/// `nostr-mail`. Blank/whitespace settings fall back to the default.
pub(crate) fn lookup_spam_rescue_target(db: &Database, pubkey: &str) -> String {
    if let Ok(Some(value)) = db.get_setting(pubkey, "spam_rescue_target") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "nostr-mail".to_string()
}

/// True if a mailbox name looks like a provider spam/junk/bulk folder
/// (case-insensitive). Catches `[Gmail]/Spam`, Outlook's `Junk Email`,
/// Fastmail's `Junk`, Yahoo's `Bulk Mail`, etc.
fn is_spam_folder_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("spam") || lower.contains("junk") || lower.contains("bulk")
}

fn list_spam_folders(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
) -> Vec<String> {
    let mailboxes = match session.list(Some(""), Some("*")) {
        Ok(m) => m,
        Err(e) => {
            debug_log!("[RUST] list_spam_folders: LIST failed: {}", e);
            return Vec::new();
        }
    };
    mailboxes
        .iter()
        .map(|mb| mb.name().to_string())
        .filter(|name| is_spam_folder_name(name))
        .collect()
}

/// Append discovered spam/junk/bulk server folders to `folders` (deduped).
/// Called only when spam rescue is OFF, so nostr mail a provider misfiled into
/// spam still surfaces in the inbox view. (With rescue ON we never scan spam —
/// rescue moves eligible mail into the rescue target instead, and scanning spam
/// would let an in-app read mark it \Seen and suppress its rescue.) LIST
/// failures are non-fatal: the sync proceeds with the unexpanded folder set.
fn extend_with_spam_folders(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    folders: &mut Vec<String>,
) {
    for name in list_spam_folders(session) {
        if folders.iter().any(|f| f.eq_ignore_ascii_case(&name)) { continue; }
        debug_log!("[RUST] extend_with_spam_folders: adding {}", name);
        folders.push(name);
    }
}

/// Decide whether a message sitting in a spam folder should be rescued.
///
/// A message qualifies when BOTH hold:
/// 1. It carries a nostr marker — an `X-Nostr-Pubkey` or `X-Nostr-Sig` header,
///    or an inline `BEGIN NOSTR ...` armor block (encrypted body, signed body,
///    signature, or seal).
/// 2. It passes transport authentication (SPF/DKIM/alignment).
///
/// Transport auth is required because the rest of the inbox enforces it: a
/// message that fails it would be moved out of spam yet still get filtered from
/// the inbox, stranding it where the user can't see it. Mail that fails SPF/DKIM
/// is therefore deliberately left in spam — only authenticated nostr mail that
/// the provider misfiled gets rescued.
fn should_rescue_message(raw_body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(raw_body);
    let has_nostr_marker = text.contains("X-Nostr-Pubkey:")
        || text.contains("X-Nostr-Sig:")
        || text.contains("BEGIN NOSTR NIP-")
        || text.contains("BEGIN NOSTR SIGNED")
        || text.contains("BEGIN NOSTR SIGNATURE")
        || text.contains("BEGIN NOSTR SEAL");
    if !has_nostr_marker {
        debug_log!("[RUST] rescue: should_rescue_message=false (no nostr marker)");
        return false;
    }
    match verify_transport_authentication(Some(raw_body), None) {
        Ok(verdict) => {
            if !verdict.transport_verified {
                debug_log!("[RUST] rescue: should_rescue_message=false (has marker, transport auth NOT verified)");
            }
            verdict.transport_verified
        }
        Err(e) => {
            debug_log!("[RUST] rescue: should_rescue_message=false (transport auth check errored: {})", e);
            false
        }
    }
}

/// Move a UID set into `target_folder`, creating the folder if needed. Uses the
/// IMAP UID MOVE command, falling back to UID COPY + flag-deleted + EXPUNGE on
/// servers without MOVE. UID-based so source sequence renumbering during the
/// operation can't misaddress messages.
fn move_uids_to_folder(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    uid_set: &str,
    target_folder: &str,
) -> Result<()> {
    if session.uid_mv(uid_set, target_folder).is_ok() {
        return Ok(());
    }
    // Target may not exist yet — create and retry.
    if session.create(target_folder).is_ok() && session.uid_mv(uid_set, target_folder).is_ok() {
        return Ok(());
    }
    if session.uid_copy(uid_set, target_folder).is_ok() {
        session.uid_store(uid_set, "+FLAGS (\\Deleted)")?;
        session.expunge()?;
        return Ok(());
    }
    Err(anyhow::anyhow!("Failed to move UIDs {} to folder {}", uid_set, target_folder))
}

/// Scan every spam/junk/bulk folder and move nostr messages out of them into
/// `target_folder`, so providers' misclassified encrypted mail stays reachable.
/// Returns the number of messages moved. Per-folder failures are logged and
/// skipped — rescue is best-effort and never aborts the surrounding sync.
///
/// When `unseen_only` is true (the normal per-sync rescue) only UNSEEN mail is
/// considered: there is no rescue-once ledger — to keep a message in spam, the
/// user (via the app's move-to-spam) marks it \Seen, which the server
/// replicates to every device, so the UNSEEN guard alone encodes intent
/// identically on all clients. When false (the one-time catch-up run when a
/// user first enables rescue) the \Seen guard is dropped so already-read nostr
/// mail sitting in spam — which the normal run would skip — is swept out too.
fn rescue_nostr_emails_from_spam(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    target_folder: &str,
    unseen_only: bool,
) -> usize {
    let spam_folders = list_spam_folders(session);
    debug_log!("[RUST] rescue: spam folders found = {:?}, target = '{}', unseen_only = {}", spam_folders, target_folder, unseen_only);
    if spam_folders.is_empty() {
        return 0;
    }
    // Make sure the destination exists before we start moving into it.
    let _ = session.create(target_folder);

    let mut moved_total = 0usize;
    for folder in spam_folders {
        moved_total += move_nostr_from_folder(session, &folder, target_folder, unseen_only);
    }
    moved_total
}

/// Scan one folder for nostr mail and move it into `target_folder`. Returns the
/// number of messages moved. No-op if `folder` is the target itself or can't be
/// selected. Shared by spam rescue and inbox auto-filing.
///
/// `unseen_only` gates the candidate search on UNSEEN — spam rescue sets it so a
/// message the user read and deliberately filed into spam (the app marks such
/// mail \Seen, replicated server-side to every device) is left alone. Inbox
/// auto-filing clears it: there's no "leave it here" intent for ordinary inbox
/// nostr mail, and once moved a message is gone from the source so it can't be
/// re-moved.
fn move_nostr_from_folder(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    folder: &str,
    target_folder: &str,
    unseen_only: bool,
) -> usize {
    if folder.eq_ignore_ascii_case(target_folder) {
        return 0;
    }
    if session.select(folder).is_err() {
        return 0;
    }

    // Cheap server-side narrowing to candidate UIDs: messages that are
    // header-marked (X-Nostr-Pubkey / X-Nostr-Sig) or carry a nostr armor block
    // (encrypted, signed, signature, or seal). BODY search support varies by
    // server, so we re-confirm each candidate by fetching it and checking the
    // transport auth gate.
    let guard = if unseen_only { "UNSEEN " } else { "" };
    let mut candidates: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for q in &[
        "HEADER X-Nostr-Pubkey \"\"",
        "HEADER X-Nostr-Sig \"\"",
        "BODY \"BEGIN NOSTR NIP-\"",
        "BODY \"BEGIN NOSTR SIGNED\"",
        "BODY \"BEGIN NOSTR SIGNATURE\"",
        "BODY \"BEGIN NOSTR SEAL\"",
    ] {
        let query = format!("{}{}", guard, q);
        if let Ok(found) = session.uid_search(&query) {
            candidates.extend(found);
        }
    }
    debug_log!("[RUST] move_nostr_from_folder: folder '{}' nostr candidate UIDs = {}", folder, candidates.len());
    if candidates.is_empty() {
        return 0;
    }

    let mut to_move: Vec<u32> = Vec::new();
    let uids: Vec<u32> = candidates.into_iter().collect();
    for chunk in uids.chunks(500) {
        let uid_list = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        // BODY.PEEK[] (not RFC822) so confirming a candidate doesn't set the
        // \Seen flag — moved messages keep their read state. The response key is
        // still BODY[], so msg.body() works unchanged.
        let messages = match session.uid_fetch(&uid_list, "(UID BODY.PEEK[])") {
            Ok(m) => m,
            Err(e) => {
                debug_log!("[RUST] move_nostr_from_folder: fetch in '{}' failed: {}", folder, e);
                continue;
            }
        };
        for msg in messages.iter() {
            if let (Some(uid), Some(body)) = (msg.uid, msg.body()) {
                if should_rescue_message(body) {
                    to_move.push(uid);
                } else {
                    debug_log!("[RUST] move_nostr_from_folder: uid {} in '{}' not eligible (missing nostr marker or transport auth failed)", uid, folder);
                }
            }
        }
    }

    if to_move.is_empty() {
        return 0;
    }
    let uid_set = to_move.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    match move_uids_to_folder(session, &uid_set, target_folder) {
        Ok(_) => {
            debug_log!("[RUST] move_nostr_from_folder: moved {} message(s) from '{}' to '{}'",
                to_move.len(), folder, target_folder);
            to_move.len()
        }
        Err(e) => {
            debug_log!("[RUST] move_nostr_from_folder: move from '{}' failed: {}", folder, e);
            0
        }
    }
}

/// Move nostr mail out of the regular inbox folders into `target_folder`, so it
/// consolidates in the dedicated nostr folder the user reads from. Mirrors spam
/// rescue but over the given inbox folders, never gating on \Seen. Spam folders
/// must be excluded by the caller — they're handled by spam rescue. Returns the
/// number of messages moved; per-folder failures are logged and skipped.
fn auto_file_nostr_from_inbox(
    session: &mut imap::Session<impl std::io::Read + std::io::Write>,
    source_folders: &[String],
    target_folder: &str,
) -> usize {
    // Make sure the destination exists before we start moving into it.
    let _ = session.create(target_folder);
    let mut moved_total = 0usize;
    for folder in source_folders {
        moved_total += move_nostr_from_folder(session, folder, target_folder, /* unseen_only */ false);
    }
    moved_total
}

/// One-time "catch-up" rescue invoked when the user first switches spam rescue
/// ON. Opens an IMAP session and runs `rescue_nostr_emails_from_spam` with the
/// \Seen guard dropped, so already-read nostr mail sitting in spam (which the
/// per-sync rescue intentionally skips) is moved into `target_folder` too.
/// Returns the number of messages moved. The caller is expected to sync
/// afterwards so the moved messages land in the local DB.
pub async fn rescue_spam_now(config: &EmailConfig, target_folder: &str) -> anyhow::Result<usize> {
    let target = ImapTarget::from_config(config);
    let moved = imap_pool::with_session(&target, |session| {
        Ok(rescue_nostr_emails_from_spam(session, target_folder, /* unseen_only = */ false))
    })?;
    Ok(moved)
}

fn discover_sent_mailbox(session: &mut imap::Session<impl std::io::Read + std::io::Write>) -> anyhow::Result<Option<String>> {
    // Use LIST to get all mailboxes
    let mailboxes = session.list(Some(""), Some("*"))?;
    
    // Check for Gmail-specific sent folder first (most specific match)
    for mailbox in mailboxes.iter() {
        let mailbox_name = mailbox.name().to_lowercase();
        if mailbox_name == "[gmail]/sent mail" {
            debug_log!("[RUST] discover_sent_mailbox: Found sent mailbox: {}", mailbox.name());
            return Ok(Some(mailbox.name().to_string()));
        }
    }

    // Common sent folder names (case-insensitive), ordered by specificity
    let sent_patterns = vec![
        "sent mail",
        "sent items",
        "sent",
    ];

    for mailbox in mailboxes.iter() {
        let mailbox_name = mailbox.name().to_lowercase();

        // Check if this mailbox matches any sent pattern
        for pattern in &sent_patterns {
            if mailbox_name.contains(pattern) {
                debug_log!("[RUST] discover_sent_mailbox: Found sent mailbox: {}", mailbox.name());
                return Ok(Some(mailbox.name().to_string()));
            }
        }
    }
    
    debug_log!("[RUST] discover_sent_mailbox: No sent mailbox found");
    Ok(None)
}

#[cfg(test)]
mod decode_perf_bench {
    //! Opt-level isolation harness for the glossia signature-block decode path.
    //!
    //! Times `decode_sig_and_pubkey` — the function profiled as the dominant cost
    //! in `parse_armor` (the `[RUST-PERF] populate: decode_sig_and_pubkey` line).
    //! Run under a speed profile to tell whether residual decode cost is
    //! algorithmic or just opt-level=0 codegen overhead:
    //!
    //! ```sh
    //! CARGO_PROFILE_RELEASE_OPT_LEVEL=3 CARGO_PROFILE_RELEASE_LTO=false \
    //!   cargo test --release --lib decode_sig_and_pubkey_timing -- --ignored --nocapture
    //! ```
    //!
    //! `--lib` (no backend bins) sidesteps the glossia cdylib+rlib output-filename
    //! collision (cargo #6313) that breaks `cargo test --release` on the bin graph.
    //! `#[ignore]` keeps it out of normal test sweeps.

    /// Encode raw bytes into bare Latin payload words via the same bitpack_fixed
    /// codec the app's `glossia_encode_raw_base_n` (lib.rs) uses on the encode side.
    fn encode_latin(bytes: &[u8]) -> String {
        let wordlist = glossia::generator::data::default_wordlist("latin");
        let payload_words = glossia::generator::data::load_payload_words_for_wordlist("latin", wordlist)
            .expect("load latin payload wordlist");
        let tree = glossia::WordlistTree::new(payload_words);
        let words = glossia::codec::encode_base_n(bytes, &tree, "bitpack_fixed")
            .expect("encode_base_n");
        words.join(" ")
    }

    fn percentile(sorted_us: &[u128], p: f64) -> u128 {
        if sorted_us.is_empty() { return 0; }
        let idx = ((sorted_us.len() as f64 - 1.0) * p).round() as usize;
        sorted_us[idx]
    }

    #[test]
    #[ignore]
    fn decode_sig_and_pubkey_timing() {
        // Deterministic 64-byte signature + 32-byte pubkey (no RNG needed).
        let sig_bytes: Vec<u8> = (0..64u16).map(|i| (i.wrapping_mul(37) ^ 0xA5) as u8).collect();
        let pubkey_bytes: Vec<u8> = (0..32u16).map(|i| (i.wrapping_mul(53) ^ 0x3C) as u8).collect();

        // Canonical two-line SIGNATURE block: glossia sig line(s) then pubkey line.
        let content = format!("{}\n{}", encode_latin(&sig_bytes), encode_latin(&pubkey_bytes));
        let sig_hex = hex::encode(&sig_bytes);
        let pubkey_hex = hex::encode(&pubkey_bytes);

        // Cold call — first decode for this dialect builds the cached WordlistTree
        // + payload HashSet (the ~497ms cold build in the debug-profile profiling).
        let t0 = std::time::Instant::now();
        let cold = super::decode_sig_and_pubkey(&content);
        let cold_us = t0.elapsed().as_micros();
        assert_eq!(cold, Some((sig_hex.clone(), pubkey_hex.clone())),
            "decode must round-trip the encoded sig+pubkey (else we're timing an early bail, not real work)");

        // Warm calls — steady state with the cache populated.
        const N: usize = 100;
        let mut warm_us: Vec<u128> = Vec::with_capacity(N);
        for _ in 0..N {
            let t = std::time::Instant::now();
            let r = super::decode_sig_and_pubkey(&content);
            warm_us.push(t.elapsed().as_micros());
            assert!(r.is_some());
        }
        warm_us.sort_unstable();
        let sum: u128 = warm_us.iter().sum();

        println!("[BENCH] decode_sig_and_pubkey  cold={}us ({:.2}ms)", cold_us, cold_us as f64 / 1000.0);
        println!("[BENCH] decode_sig_and_pubkey  warm n={} min={}us p50={}us p90={}us p99={}us mean={:.1}us ({:.3}ms)",
            N, warm_us[0], percentile(&warm_us, 0.50), percentile(&warm_us, 0.90),
            percentile(&warm_us, 0.99), sum as f64 / N as f64, (sum as f64 / N as f64) / 1000.0);
        println!("[BENCH] sig block: {} chars, {} words", content.len(), content.split_whitespace().count());
    }
}
