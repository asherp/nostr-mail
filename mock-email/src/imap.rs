use crate::store::EmailStore;
use crate::types::Email;
use anyhow::Result;
use log::{debug, error, info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// IMAP server for fetching emails
pub struct ImapServer {
    store: Arc<EmailStore>,
    addr: SocketAddr,
}

impl ImapServer {
    pub fn new(addr: SocketAddr, store: Arc<EmailStore>) -> Self {
        Self { store, addr }
    }

    /// Start the IMAP server and listen for connections
    pub async fn start(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.addr).await?;
        info!("Mock IMAP server listening on imap://{}", self.addr);

        while let Ok((stream, addr)) = listener.accept().await {
            let store = self.store.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_imap_connection(stream, addr, store).await {
                    error!("Error handling IMAP connection from {}: {}", addr, e);
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

/// Handle an IMAP connection
async fn handle_imap_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    store: Arc<EmailStore>,
) -> Result<()> {
    info!("New IMAP connection from {}", addr);
    
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    // Send greeting
    writer.write_all(b"* OK mock-email IMAP server ready\r\n").await?;
    writer.flush().await?;

    let mut authenticated = false;
    let mut selected_mailbox: Option<String> = None;
    let _tag_counter = 0u64;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;
        
        if bytes_read == 0 {
            break; // Connection closed
        }

        let line = line.trim_end();
        debug!("[IMAP] {} -> {}", addr, line);

        // Parse IMAP command
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let tag = parts[0];
        let command = if parts.len() > 1 {
            parts[1].to_uppercase()
        } else {
            continue;
        };

        match command.as_str() {
            "CAPABILITY" => {
                info!("[IMAP] {} CAPABILITY", addr);
                writer.write_all(b"* CAPABILITY IMAP4rev1\r\n").await?;
                writer.write_all(format!("{} OK CAPABILITY completed\r\n", tag).as_bytes()).await?;
            }
            "NOOP" => {
                debug!("[IMAP] {} NOOP", addr);
                writer.write_all(format!("{} OK NOOP completed\r\n", tag).as_bytes()).await?;
            }
            "LOGOUT" => {
                info!("[IMAP] {} LOGOUT", addr);
                writer.write_all(b"* BYE mock-email logging out\r\n").await?;
                writer.write_all(format!("{} OK LOGOUT completed\r\n", tag).as_bytes()).await?;
                break;
            }
            "LOGIN" => {
                if parts.len() >= 4 {
                    let username = parts[2];
                    let _password = parts[3];
                    authenticated = true;
                    info!("[IMAP] {} LOGIN: {}", addr, username);
                    writer.write_all(format!("{} OK LOGIN completed\r\n", tag).as_bytes()).await?;
                } else {
                    writer.write_all(format!("{} BAD Invalid LOGIN command\r\n", tag).as_bytes()).await?;
                }
            }
            "AUTHENTICATE" => {
                // For simplicity, accept any authentication
                info!("[IMAP] {} AUTHENTICATE", addr);
                writer.write_all(b"+ OK\r\n").await?;
                authenticated = true;
                writer.write_all(format!("{} OK AUTHENTICATE completed\r\n", tag).as_bytes()).await?;
                info!("[IMAP] {} AUTHENTICATE completed", addr);
            }
            "SELECT" | "EXAMINE" => {
                if !authenticated {
                    writer.write_all(format!("{} NO Not authenticated\r\n", tag).as_bytes()).await?;
                    continue;
                }
                
                if parts.len() >= 3 {
                    let mailbox_name = parts[2].trim_matches('"');
                    // Get mailbox emails (case-insensitive lookup, except INBOX)
                    let emails = store.get_mailbox_emails(mailbox_name).await;
                    let exists = emails.len();
                    
                    // Find the actual mailbox name (for response)
                    let actual_mailbox_name = if mailbox_name.to_uppercase() == "INBOX" {
                        "INBOX".to_string()
                    } else {
                        // Find matching mailbox name (case-insensitive)
                        let mailboxes = store.list_mailboxes().await;
                        mailboxes.iter()
                            .find(|mb| mb.to_lowercase() == mailbox_name.to_lowercase())
                            .cloned()
                            .unwrap_or_else(|| mailbox_name.to_string())
                    };
                    
                    selected_mailbox = Some(actual_mailbox_name.clone());
                    
                    writer.write_all(format!("* {} EXISTS\r\n", exists).as_bytes()).await?;
                    writer.write_all(format!("* {} RECENT\r\n", exists).as_bytes()).await?;
                    writer.write_all(b"* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)\r\n").await?;
                    writer.write_all(b"* OK [PERMANENTFLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)]\r\n").await?;
                    writer.write_all(format!("{} OK [READ-WRITE] {} selected\r\n", tag, actual_mailbox_name).as_bytes()).await?;
                    
                    info!("[IMAP] {} SELECT: {} ({} emails)", addr, actual_mailbox_name, exists);
                } else {
                    writer.write_all(format!("{} BAD Invalid SELECT command\r\n", tag).as_bytes()).await?;
                }
            }
            "LIST" => {
                if !authenticated {
                    writer.write_all(format!("{} NO Not authenticated\r\n", tag).as_bytes()).await?;
                    continue;
                }
                
                info!("[IMAP] {} LIST", addr);
                let mailboxes = store.list_mailboxes().await;
                for mailbox in &mailboxes {
                    writer.write_all(format!("* LIST () \"/\" {}\r\n", mailbox).as_bytes()).await?;
                }
                writer.write_all(format!("{} OK LIST completed\r\n", tag).as_bytes()).await?;
                info!("[IMAP] {} LIST completed: {} mailboxes", addr, mailboxes.len());
            }
            "FETCH" => {
                if !authenticated || selected_mailbox.is_none() {
                    writer.write_all(format!("{} NO Not authenticated or no mailbox selected\r\n", tag).as_bytes()).await?;
                    continue;
                }
                
                if parts.len() < 3 {
                    writer.write_all(format!("{} BAD Invalid FETCH command\r\n", tag).as_bytes()).await?;
                    continue;
                }
                
                let mailbox = selected_mailbox.as_ref().unwrap();
                let emails = store.get_mailbox_emails(mailbox).await;
                
                // Parse sequence set (e.g., "1", "1:5", "*")
                let sequence = parts[2];
                let indices = parse_sequence_set(sequence, emails.len());
                
                // Parse data items (e.g., "BODY[]", "RFC822", "ENVELOPE")
                let data_items = if parts.len() > 3 {
                    parts[3..].join(" ")
                } else {
                    "BODY[]".to_string()
                };
                
                info!("[IMAP] {} FETCH: mailbox={}, sequence={}, data_items={}, matching_indices={:?}", 
                    addr, mailbox, sequence, data_items, indices);
                
                let mut fetched_count = 0;
                for idx in &indices {
                    if *idx > 0 && *idx <= emails.len() {
                        let email = &emails[emails.len() - idx]; // IMAP uses 1-based indexing, newest first
                        send_email_fetch(&mut writer, *idx, email, &data_items).await?;
                        fetched_count += 1;
                    }
                }
                
                writer.write_all(format!("{} OK FETCH completed\r\n", tag).as_bytes()).await?;
                info!("[IMAP] {} FETCH completed: fetched {} messages", addr, fetched_count);
            }
            "SEARCH" => {
                if !authenticated || selected_mailbox.is_none() {
                    writer.write_all(format!("{} NO Not authenticated or no mailbox selected\r\n", tag).as_bytes()).await?;
                    continue;
                }
                
                let mailbox = selected_mailbox.as_ref().unwrap();
                let emails = store.get_mailbox_emails(mailbox).await;
                
                // Extract search criteria for logging
                let search_criteria = if parts.len() > 2 {
                    parts[2..].join(" ")
                } else {
                    "ALL".to_string()
                };
                
                info!("[IMAP] {} SEARCH: mailbox={}, criteria={}", addr, mailbox, search_criteria);
                
                // Simple search - return all email sequence numbers
                let mut seq_nums = Vec::new();
                for (i, _) in emails.iter().enumerate() {
                    seq_nums.push((emails.len() - i).to_string());
                }
                
                writer.write_all(format!("* SEARCH {}\r\n", seq_nums.join(" ")).as_bytes()).await?;
                writer.write_all(format!("{} OK SEARCH completed\r\n", tag).as_bytes()).await?;
                info!("[IMAP] {} SEARCH completed: found {} messages", addr, seq_nums.len());
            }
            "STATUS" => {
                if !authenticated {
                    writer.write_all(format!("{} NO Not authenticated\r\n", tag).as_bytes()).await?;
                    continue;
                }
                
                if parts.len() >= 4 {
                    let mailbox_name = parts[2].trim_matches('"');
                    
                    // Parse the attribute list - it starts at parts[3] and may span multiple parts
                    // e.g., "(MESSAGES)" or "(MESSAGES RECENT UNSEEN)"
                    let attr_list_start = parts[3];
                    let mut requested_attrs = Vec::new();
                    
                    // Check if parts[3] starts with "(" - if so, parse the attribute list
                    if attr_list_start.starts_with('(') {
                        // Collect all parts until we find one ending with ")"
                        let mut attr_parts = vec![parts[3]];
                        for i in 4..parts.len() {
                            attr_parts.push(parts[i]);
                            if parts[i].ends_with(')') {
                                break;
                            }
                        }
                        
                        // Parse attributes from the collected parts
                        let attr_str = attr_parts.join(" ");
                        // Remove parentheses and split by whitespace
                        let attrs: Vec<&str> = attr_str
                            .trim_start_matches('(')
                            .trim_end_matches(')')
                            .split_whitespace()
                            .collect();
                        
                        for attr in attrs {
                            requested_attrs.push(attr.to_uppercase());
                        }
                    }
                    
                    // If no attributes specified, default to all
                    if requested_attrs.is_empty() {
                        requested_attrs = vec!["MESSAGES".to_string(), "RECENT".to_string(), "UNSEEN".to_string()];
                    }
                    
                    let emails = store.get_mailbox_emails(mailbox_name).await;
                    let exists = emails.len();
                    
                    // Find the actual mailbox name (for response)
                    let actual_mailbox_name = if mailbox_name.to_uppercase() == "INBOX" {
                        "INBOX".to_string()
                    } else {
                        let mailboxes = store.list_mailboxes().await;
                        mailboxes.iter()
                            .find(|mb| mb.to_lowercase() == mailbox_name.to_lowercase())
                            .cloned()
                            .unwrap_or_else(|| mailbox_name.to_string())
                    };
                    
                    // Build response with only requested attributes
                    let mut attr_responses = Vec::new();
                    for attr in &requested_attrs {
                        match attr.as_str() {
                            "MESSAGES" => attr_responses.push(format!("MESSAGES {}", exists)),
                            "RECENT" => attr_responses.push(format!("RECENT {}", exists)),
                            "UNSEEN" => attr_responses.push(format!("UNSEEN {}", exists)),
                            _ => {} // Ignore unknown attributes
                        }
                    }
                    
                    // Ensure we have at least one attribute (shouldn't happen, but safety check)
                    if attr_responses.is_empty() {
                        attr_responses.push(format!("MESSAGES {}", exists));
                    }
                    
                    let attr_list = attr_responses.join(" ");
                    writer.write_all(format!("* STATUS \"{}\" ({})\r\n", 
                        actual_mailbox_name, attr_list).as_bytes()).await?;
                    writer.write_all(format!("{} OK STATUS completed\r\n", tag).as_bytes()).await?;
                    
                    info!("[IMAP] {} STATUS: {} ({})", addr, actual_mailbox_name, attr_list);
                } else {
                    writer.write_all(format!("{} BAD Invalid STATUS command\r\n", tag).as_bytes()).await?;
                }
            }
            _ => {
                warn!("[IMAP] {} Unknown command: {}", addr, command);
                writer.write_all(format!("{} BAD Command not recognized\r\n", tag).as_bytes()).await?;
            }
        }
        
        writer.flush().await?;
    }

    info!("IMAP connection from {} closed", addr);
    Ok(())
}

/// Parse IMAP sequence set (e.g., "1", "1:5", "*", "1,2,3", "1:3,5")
fn parse_sequence_set(sequence: &str, total: usize) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut seen = std::collections::HashSet::new();
    
    // Split by comma to handle multiple sequences
    for part in sequence.split(',') {
        let part = part.trim();
        
        if part == "*" {
            if !seen.contains(&1) {
                indices.push(1);
                seen.insert(1);
            }
        } else if part.contains(':') {
            // Handle range (e.g., "1:5" or "1:*")
            let parts: Vec<&str> = part.split(':').collect();
            if parts.len() == 2 {
                let start: usize = parts[0].trim().parse().unwrap_or(1);
                let end = if parts[1].trim() == "*" {
                    total
                } else {
                    parts[1].trim().parse().unwrap_or(total)
                };
                for i in start..=end.min(total) {
                    if !seen.contains(&i) {
                        indices.push(i);
                        seen.insert(i);
                    }
                }
            }
        } else if let Ok(num) = part.parse::<usize>() {
            // Single number
            if num > 0 && num <= total && !seen.contains(&num) {
                indices.push(num);
                seen.insert(num);
            }
        }
    }
    
    indices
}

/// Send email data in FETCH response
async fn send_email_fetch(
    writer: &mut tokio::net::tcp::WriteHalf<'_>,
    seq_num: usize,
    email: &Email,
    data_items: &str,
) -> Result<()> {
    if data_items.contains("BODY[]") || data_items.contains("RFC822") {
        // Send full email
        let email_text = format_email_rfc822(email);
        let email_bytes = email_text.as_bytes();
        let email_size = email_bytes.len(); // Use byte length, not char length
        
        // Use the requested data item name (RFC822 or BODY[])
        let data_item_name = if data_items.contains("RFC822") {
            "RFC822"
        } else {
            "BODY[]"
        };
        
        // IMAP FETCH response format: * seq FETCH (data_item {size}\r\n<data>)\r\n
        // The literal data must be sent immediately after the size declaration
        let fetch_line = format!("* {} FETCH ({} {{{}}}\r\n", seq_num, data_item_name, email_size);
        info!("[IMAP] Sending FETCH response: {}", fetch_line.trim());
        info!("[IMAP] Email content preview (first 200 chars): {}", 
            email_text.chars().take(200).collect::<String>());
        info!("[IMAP] Email ends with: {:?}", 
            email_text.chars().rev().take(10).collect::<String>());
        
        // Send FETCH line, then literal data, then closing - all without flushing in between
        writer.write_all(fetch_line.as_bytes()).await?;
        writer.write_all(email_bytes).await?;
        writer.write_all(b")\r\n").await?;
        writer.flush().await?; // Only flush at the end
        info!("[IMAP] Sent complete FETCH response: {} bytes ({} header + {} body + 3 closing)", 
            fetch_line.len() + email_size + 3, fetch_line.len(), email_size);
    } else if data_items.contains("ENVELOPE") {
        // Send envelope
        let envelope = format_envelope(email);
        writer.write_all(format!("* {} FETCH (ENVELOPE {})\r\n", seq_num, envelope).as_bytes()).await?;
    } else if data_items.contains("BODYSTRUCTURE") {
        // Send body structure
        let bodystructure = format_bodystructure(email);
        writer.write_all(format!("* {} FETCH (BODYSTRUCTURE {})\r\n", seq_num, bodystructure).as_bytes()).await?;
    } else {
        // Default: send envelope
        let envelope = format_envelope(email);
        writer.write_all(format!("* {} FETCH (ENVELOPE {})\r\n", seq_num, envelope).as_bytes()).await?;
    }
    
    Ok(())
}

/// Format email as RFC822
fn format_email_rfc822(email: &Email) -> String {
    let mut rfc822 = String::new();
    
    // Headers
    rfc822.push_str(&format!("From: {}\r\n", email.from.to_string()));
    rfc822.push_str(&format!("To: {}\r\n", 
        email.to.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")));
    
    if !email.cc.is_empty() {
        rfc822.push_str(&format!("Cc: {}\r\n", 
            email.cc.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")));
    }
    
    rfc822.push_str(&format!("Subject: {}\r\n", email.subject));
    
    // Date header
    let date_str = chrono::DateTime::<chrono::Utc>::from_timestamp(email.created_at, 0)
        .map(|d| d.to_rfc2822())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc2822());
    rfc822.push_str(&format!("Date: {}\r\n", date_str));
    
    // Message-ID header (required by RFC)
    if !email.headers.contains_key("message-id") && !email.headers.contains_key("Message-ID") {
        rfc822.push_str(&format!("Message-ID: <{}@mock-email>\r\n", email.id));
    }
    
    // MIME-Version header (helpful for email clients)
    if !email.headers.contains_key("mime-version") && !email.headers.contains_key("MIME-Version") {
        rfc822.push_str("MIME-Version: 1.0\r\n");
    }
    
    // Content-Type header
    if !email.headers.contains_key("content-type") && !email.headers.contains_key("Content-Type") {
        if email.html_body.is_some() {
            rfc822.push_str("Content-Type: multipart/alternative; boundary=\"boundary\"\r\n");
        } else {
            rfc822.push_str("Content-Type: text/plain; charset=utf-8\r\n");
        }
    }
    
    // Add custom headers (preserve original case for headers we haven't added)
    info!("[IMAP] format_email_rfc822: Email has {} headers: {:?}", 
        email.headers.len(), email.headers.keys().collect::<Vec<_>>());
    for (key, value) in &email.headers {
        let key_lower = key.to_lowercase();
        // Skip headers we've already added
        if key_lower != "from" && key_lower != "to" && key_lower != "cc" 
            && key_lower != "subject" && key_lower != "date" && key_lower != "message-id"
            && key_lower != "mime-version" && key_lower != "content-type" {
            rfc822.push_str(&format!("{}: {}\r\n", key, value));
            info!("[IMAP] format_email_rfc822: Added header {}: {}", key, value);
        }
    }
    if email.headers.contains_key("X-Nostr-Pubkey") {
        info!("[IMAP] format_email_rfc822: X-Nostr-Pubkey header IS in email.headers");
    } else {
        info!("[IMAP] format_email_rfc822: X-Nostr-Pubkey header NOT in email.headers");
    }
    
    rfc822.push_str("\r\n");
    rfc822.push_str(&email.body);
    // Ensure body ends with CRLF (required for IMAP)
    if !rfc822.ends_with("\r\n") {
        if rfc822.ends_with('\n') {
            // Replace single LF with CRLF
            rfc822.pop();
            rfc822.push_str("\r\n");
        } else {
            rfc822.push_str("\r\n");
        }
    }
    
    rfc822
}

/// Format email envelope for IMAP
fn format_envelope(email: &Email) -> String {
    format!(
        "(\"{}\" \"{}\" (\"{}\" \"{}\") (\"{}\" \"{}\") ((\"{}\" \"{}\")) ((\"{}\" \"{}\")) NIL NIL \"{}\" \"{}\")",
        email.subject,
        email.subject,
        email.from.local,
        email.from.domain,
        email.to.first().map(|t| t.local.as_str()).unwrap_or(""),
        email.to.first().map(|t| t.domain.as_str()).unwrap_or(""),
        email.from.local,
        email.from.domain,
        email.to.first().map(|t| t.local.as_str()).unwrap_or(""),
        email.to.first().map(|t| t.domain.as_str()).unwrap_or(""),
        email.subject,
        chrono::DateTime::<chrono::Utc>::from_timestamp(email.created_at, 0)
            .map(|d| d.to_rfc2822())
            .unwrap_or_else(|| "".to_string())
    )
}

/// Format body structure for IMAP
fn format_bodystructure(email: &Email) -> String {
    if email.html_body.is_some() || !email.attachments.is_empty() {
        // Multipart message
        format!("(\"multipart\" \"mixed\" NIL NIL NIL NIL)")
    } else {
        // Simple text message
        format!("(\"text\" \"plain\" NIL NIL NIL \"7bit\" {} NIL NIL NIL)",
            email.body.len())
    }
}
