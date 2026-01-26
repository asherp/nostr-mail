use crate::store::EmailStore;
use crate::types::{Email, EmailAddress, Attachment};
use anyhow::Result;
use log::{debug, error, info, warn};
use mailparse::*;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// SMTP server for receiving emails
pub struct SmtpServer {
    store: Arc<EmailStore>,
    addr: SocketAddr,
}

impl SmtpServer {
    pub fn new(addr: SocketAddr, store: Arc<EmailStore>) -> Self {
        Self { store, addr }
    }

    /// Start the SMTP server and listen for connections
    pub async fn start(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.addr).await?;
        info!("Mock SMTP server listening on smtp://{}", self.addr);

        while let Ok((stream, addr)) = listener.accept().await {
            let store = self.store.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_smtp_connection(stream, addr, store).await {
                    error!("Error handling SMTP connection from {}: {}", addr, e);
                }
            });
        }

        Ok(())
    }

    /// Get the address the server is bound to
    #[allow(dead_code)]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Handle an SMTP connection
async fn handle_smtp_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    store: Arc<EmailStore>,
) -> Result<()> {
    info!("New SMTP connection from {}", addr);
    
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    // Send greeting
    writer.write_all(b"220 mock-email ESMTP server ready\r\n").await?;
    writer.flush().await?;

    let mut from: Option<EmailAddress> = None;
    let mut to: Vec<EmailAddress> = Vec::new();
    let mut data_mode = false;
    let mut email_data = Vec::new();
    let mut auth_mode: Option<String> = None; // Track which AUTH method is in progress

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;
        
        if bytes_read == 0 {
            break; // Connection closed
        }

        let line = line.trim_end();
        debug!("[SMTP] {} -> {}", addr, line);

        if data_mode {
            // We're receiving email data
            if line == "." {
                // End of data
                data_mode = false;
                debug!("[SMTP] Received email data ({} bytes)", email_data.len());
                
                // Parse and store the email
                match parse_and_store_email(&email_data, from.clone(), to.clone(), &store).await {
                    Ok(_) => {
                        writer.write_all(b"250 OK: Message accepted\r\n").await?;
                        info!("[SMTP] {} Email accepted: {} -> {:?}", addr, 
                            from.as_ref().map(|f| f.to_string()).unwrap_or_default(),
                            to.iter().map(|t| t.to_string()).collect::<Vec<_>>());
                    }
                    Err(e) => {
                        warn!("[SMTP] {} Failed to parse/store email: {}", addr, e);
                        writer.write_all(format!("550 Error: {}\r\n", e).as_bytes()).await?;
                    }
                }
                
                email_data.clear();
                from = None;
                to.clear();
                writer.flush().await?;
            } else {
                // Remove escaping if present
                let data_line = if line.starts_with("..") {
                    &line[1..]
                } else {
                    line
                };
                email_data.extend_from_slice(data_line.as_bytes());
                email_data.push(b'\n');
            }
        } else {
            // Handle SMTP commands
            let command = line.to_uppercase();
            
            if command.starts_with("EHLO") || command.starts_with("HELO") {
                // Advertise AUTH PLAIN and AUTH LOGIN support
                if command.starts_with("EHLO") {
                    writer.write_all(b"250-mock-email\r\n").await?;
                    writer.write_all(b"250-AUTH PLAIN LOGIN\r\n").await?;
                    writer.write_all(b"250-SIZE 10485760\r\n").await?;
                    writer.write_all(b"250-8BITMIME\r\n").await?;
                    writer.write_all(b"250 OK\r\n").await?;
                } else {
                    writer.write_all(b"250 mock-email\r\n").await?;
                }
            } else if command.starts_with("MAIL FROM:") {
                // Extract email from original line (before uppercasing) to preserve case
                let from_str = extract_email_from_command(&line, "MAIL FROM:");
                // Normalize to lowercase (email addresses are case-insensitive)
                let from_str_lower = from_str.to_lowercase();
                from = EmailAddress::from_string(&from_str_lower);
                if from.is_some() {
                    writer.write_all(b"250 OK\r\n").await?;
                    debug!("[SMTP] {} MAIL FROM: {}", addr, from_str_lower);
                } else {
                    writer.write_all(b"550 Invalid sender address\r\n").await?;
                }
            } else if command.starts_with("RCPT TO:") {
                // Extract email from original line (before uppercasing) to preserve case
                let to_str = extract_email_from_command(&line, "RCPT TO:");
                // Normalize to lowercase (email addresses are case-insensitive)
                let to_str_lower = to_str.to_lowercase();
                if let Some(addr) = EmailAddress::from_string(&to_str_lower) {
                    to.push(addr.clone());
                    writer.write_all(b"250 OK\r\n").await?;
                    debug!("[SMTP] {} RCPT TO: {}", addr.to_string(), to_str_lower);
                } else {
                    writer.write_all(b"550 Invalid recipient address\r\n").await?;
                }
            } else if command == "DATA" {
                data_mode = true;
                writer.write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n").await?;
            } else if command == "QUIT" {
                writer.write_all(b"221 Bye\r\n").await?;
                break;
            } else if command == "RSET" {
                from = None;
                to.clear();
                email_data.clear();
                data_mode = false;
                writer.write_all(b"250 OK\r\n").await?;
            } else if command.starts_with("NOOP") {
                writer.write_all(b"250 OK\r\n").await?;
            } else if command.starts_with("STARTTLS") {
                writer.write_all(b"502 TLS not supported\r\n").await?;
            } else if command.starts_with("AUTH") {
                // Accept any authentication for testing
                let parts: Vec<&str> = command.split_whitespace().collect();
                
                if parts.len() >= 2 {
                    let auth_method = parts[1].to_uppercase();
                    
                    if parts.len() >= 3 {
                        // Credentials provided inline (AUTH PLAIN <base64>)
                        auth_mode = None;
                        writer.write_all(b"235 Authentication successful\r\n").await?;
                        info!("[SMTP] {} AUTH {}: Accepted (mock server accepts any credentials)", addr, auth_method);
                    } else {
                        // Start authentication handshake
                        auth_mode = Some(auth_method.clone());
                        if auth_method == "LOGIN" {
                            writer.write_all(b"334 VXNlcm5hbWU6\r\n").await?; // "Username:" in base64
                        } else if auth_method == "PLAIN" {
                            writer.write_all(b"334 \r\n").await?; // Request base64 credentials
                        } else {
                            writer.write_all(b"334 \r\n").await?; // Generic response
                        }
                    }
                } else {
                    writer.write_all(b"501 Syntax error in parameters\r\n").await?;
                }
            } else if auth_mode.is_some() {
                // Handle continuation of AUTH LOGIN (username/password in separate lines)
                let auth_method = auth_mode.as_ref().unwrap().clone();
                if auth_method == "LOGIN" {
                    // First line is username, second line is password
                    // We need to track if we've seen username yet
                    // For simplicity, accept after receiving any non-empty line
                    auth_mode = None;
                    writer.write_all(b"235 Authentication successful\r\n").await?;
                    info!("[SMTP] {} AUTH LOGIN: Accepted (mock server accepts any credentials)", addr);
                } else {
                    // For PLAIN or other methods, accept the credentials
                    auth_mode = None;
                    writer.write_all(b"235 Authentication successful\r\n").await?;
                    info!("[SMTP] {} AUTH {}: Accepted (mock server accepts any credentials)", addr, auth_method);
                }
            } else {
                warn!("[SMTP] {} Unknown command: {}", addr, line);
                writer.write_all(b"500 Command not recognized\r\n").await?;
            }
            
            writer.flush().await?;
        }
    }

    info!("SMTP connection from {} closed", addr);
    Ok(())
}

/// Extract email address from SMTP command
fn extract_email_from_command(command: &str, prefix: &str) -> String {
    let after_prefix = command
        .strip_prefix(prefix)
        .unwrap_or("")
        .trim();
    
    // Handle cases like "MAIL FROM:<email@example.com> size=199"
    // Extract just the email address part (between < > or before any space)
    if let Some(start) = after_prefix.find('<') {
        if let Some(end) = after_prefix[start+1..].find('>') {
            return after_prefix[start+1..start+1+end].to_string();
        }
    }
    
    // If no angle brackets, take everything up to the first space
    if let Some(space_pos) = after_prefix.find(' ') {
        after_prefix[..space_pos].to_string()
    } else {
        after_prefix.to_string()
    }
}

/// Parse email data and store it
async fn parse_and_store_email(
    data: &[u8],
    from: Option<EmailAddress>,
    to: Vec<EmailAddress>,
    store: &EmailStore,
) -> Result<()> {
    // Parse the email using mailparse
    let parsed = parse_mail(data)?;
    
    // Extract ALL headers from the parsed email
    let headers = parsed.get_headers();
    let mut header_map = std::collections::HashMap::new();
    
    // Parse raw email data to extract all headers
    // Headers end with \r\n\r\n or \n\n (blank line)
    let email_str = String::from_utf8_lossy(data);
    info!("[SMTP] Raw email data (first 500 chars): {}", email_str.chars().take(500).collect::<String>());
    
    // Try to find header end - check for both \r\n\r\n and \n\n
    let header_end = email_str.find("\r\n\r\n")
        .or_else(|| email_str.find("\n\n"))
        .or_else(|| {
            // If no double newline, headers might end at first empty line after headers
            // Look for pattern: header line followed by empty line
            let lines: Vec<&str> = email_str.lines().collect();
            let mut header_end_pos = 0;
            let mut prev_was_header = false;
            for (idx, line) in lines.iter().enumerate() {
                if line.trim().is_empty() && prev_was_header {
                    // Found empty line after headers
                    header_end_pos = email_str.lines().take(idx).map(|l| l.len() + 1).sum();
                    break;
                }
                prev_was_header = !line.trim().is_empty() && line.contains(':');
            }
            if header_end_pos > 0 {
                Some(header_end_pos)
            } else {
                None
            }
        });
    
    if let Some(header_end) = header_end {
        let header_section = &email_str[..header_end];
        info!("[SMTP] Header section ({} chars): {}", header_section.len(), header_section);
        let mut current_key: Option<String> = None;
        for line in header_section.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            
            // Check if this line contains a colon (header field)
            if let Some(colon_pos) = trimmed.find(':') {
                // This is a header line (even if it starts with spaces)
                let key = trimmed[..colon_pos].trim().to_string();
                let value = trimmed[colon_pos + 1..].trim().to_string();
                info!("[SMTP] Parsed header: {} = {}", key, value);
                current_key = Some(key.clone());
                header_map.insert(key, value);
            } else if let Some(ref key) = current_key {
                // This is a continuation line (no colon, append to previous header)
                if let Some(value) = header_map.get_mut(key) {
                    *value = format!("{} {}", value, trimmed);
                }
            }
        }
    } else {
        warn!("[SMTP] Could not find header end marker in email data, trying to parse all lines as headers");
        // Fallback: try to parse all lines until we hit body content
        let mut current_key: Option<String> = None;
        for line in email_str.lines() {
            let trimmed = line.trim();
            // Stop if we hit what looks like body content (starts with -----BEGIN or similar)
            if trimmed.starts_with("-----BEGIN") || trimmed.starts_with("-----END") {
                break;
            }
            if trimmed.is_empty() && current_key.is_some() {
                // Empty line after headers, stop parsing headers
                break;
            }
            
            // Check if this line contains a colon (header field)
            if let Some(colon_pos) = trimmed.find(':') {
                // This is a header line
                let key = trimmed[..colon_pos].trim().to_string();
                let value = trimmed[colon_pos + 1..].trim().to_string();
                info!("[SMTP] Parsed header (fallback): {} = {}", key, value);
                current_key = Some(key.clone());
                header_map.insert(key, value);
            } else if let Some(ref key) = current_key {
                // This is a continuation line (no colon, append to previous header)
                if let Some(value) = header_map.get_mut(key) {
                    *value = format!("{} {}", value, trimmed);
                }
            }
        }
    }
    
    // Also get headers via mailparse API (for standard headers, ensures we have them)
    // This is a fallback in case raw parsing missed something
    if let Some(subject_header) = headers.get_first_header("subject") {
        header_map.insert("subject".to_string(), subject_header.get_value().to_string());
    }
    if let Some(from_header) = headers.get_first_header("from") {
        header_map.insert("from".to_string(), from_header.get_value().to_string());
    }
    if let Some(to_header) = headers.get_first_header("to") {
        header_map.insert("to".to_string(), to_header.get_value().to_string());
    }
    if let Some(cc_header) = headers.get_first_header("cc") {
        header_map.insert("cc".to_string(), cc_header.get_value().to_string());
    }
    if let Some(bcc_header) = headers.get_first_header("bcc") {
        header_map.insert("bcc".to_string(), bcc_header.get_value().to_string());
    }
    if let Some(msgid_header) = headers.get_first_header("message-id") {
        header_map.insert("message-id".to_string(), msgid_header.get_value().to_string());
    }

    // Get subject
    let subject = header_map
        .get("subject")
        .cloned()
        .unwrap_or_else(|| "".to_string());

    // Get from/to if not provided in SMTP commands
    let from_addr = from.unwrap_or_else(|| {
        header_map
            .get("from")
            .and_then(|f| extract_email_from_header(f))
            .and_then(|e| EmailAddress::from_string(e.as_str()))
            .unwrap_or_else(|| EmailAddress::new("unknown".to_string(), "unknown".to_string()))
    });

    let to_addrs = if to.is_empty() {
        header_map
            .get("to")
            .and_then(|t| extract_emails_from_header(t))
            .unwrap_or_default()
    } else {
        to
    };

    // Get CC and BCC
    let cc = header_map
        .get("cc")
        .and_then(|c| extract_emails_from_header(c))
        .unwrap_or_default();
    
    let bcc = header_map
        .get("bcc")
        .and_then(|b| extract_emails_from_header(b))
        .unwrap_or_default();

    // Extract body
    let body = parsed.get_body()?;
    
    // Check for HTML body
    let html_body = if parsed.subparts.len() > 0 {
        parsed.subparts.iter().find_map(|part| {
            if part.ctype.mimetype.starts_with("text/html") {
                part.get_body().ok()
            } else {
                None
            }
        })
    } else {
        None
    };

    // Extract attachments
    let mut attachments = Vec::new();
    for part in &parsed.subparts {
        if part.ctype.mimetype.starts_with("text/") {
            continue; // Skip text parts (already handled)
        }
        
        if let Ok(content) = part.get_body() {
            // Try to get filename from name parameter in content-type, or generate one
            let filename = part.ctype.params.get("name")
                .cloned()
                .unwrap_or_else(|| format!("attachment_{}.bin", attachments.len()));
            
            attachments.push(Attachment {
                filename,
                content_type: part.ctype.mimetype.clone(),
                content: content.into_bytes(),
            });
        }
    }

    // Log stored headers for debugging
    info!("[SMTP] Stored headers: {:?}", header_map.keys().collect::<Vec<_>>());
    if let Some(nostr_pubkey) = header_map.get("X-Nostr-Pubkey") {
        info!("[SMTP] Found X-Nostr-Pubkey header: {}", nostr_pubkey);
    } else {
        info!("[SMTP] X-Nostr-Pubkey header NOT found in stored headers");
    }
    
    // Create email
    let email = Email {
        id: uuid::Uuid::new_v4().to_string(),
        from: from_addr,
        to: to_addrs,
        cc,
        bcc,
        subject,
        body: body,
        html_body: html_body,
        headers: header_map,
        created_at: chrono::Utc::now().timestamp(),
        attachments,
    };

    // Store in INBOX for recipients, Sent for sender
    info!("[SMTP] Storing email: id={}, from={}, to={:?}", email.id, email.from.to_string(), email.to.iter().map(|a| a.to_string()).collect::<Vec<_>>());
    for recipient in &email.to {
        store.add_email(email.clone(), "INBOX").await;
        info!("[SMTP] Stored email {} in INBOX for recipient {}", email.id, recipient.to_string());
    }
    store.add_email(email.clone(), "Sent").await;
    info!("[SMTP] Stored email {} in Sent mailbox for sender {}", email.id, email.from.to_string());

    Ok(())
}

/// Extract email address from header value
fn extract_email_from_header(header_value: &str) -> Option<String> {
    // Simple extraction: look for <email@domain.com> or just email@domain.com
    if let Some(start) = header_value.find('<') {
        if let Some(end) = header_value[start..].find('>') {
            return Some(header_value[start + 1..start + end].to_string());
        }
    }
    
    // Try to find @ symbol
    if header_value.contains('@') {
        let parts: Vec<&str> = header_value.split('@').collect();
        if parts.len() >= 2 {
            let local = parts[0].trim_end_matches(|c: char| !c.is_alphanumeric());
            let domain = parts[1].split_whitespace().next().unwrap_or("").trim_end_matches(|c: char| !c.is_alphanumeric());
            if !local.is_empty() && !domain.is_empty() {
                return Some(format!("{}@{}", local, domain));
            }
        }
    }
    
    None
}

/// Extract multiple email addresses from header value
fn extract_emails_from_header(header_value: &str) -> Option<Vec<EmailAddress>> {
    let mut emails = Vec::new();
    
    // Split by comma and extract each email
    for part in header_value.split(',') {
            if let Some(email_str) = extract_email_from_header(part.trim()) {
                if let Some(addr) = EmailAddress::from_string(&email_str) {
                    emails.push(addr);
                }
            }
    }
    
    if emails.is_empty() {
        None
    } else {
        Some(emails)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_email_from_header() {
        assert_eq!(
            extract_email_from_header("John Doe <john@example.com>"),
            Some("john@example.com".to_string())
        );
        assert_eq!(
            extract_email_from_header("john@example.com"),
            Some("john@example.com".to_string())
        );
    }
}
