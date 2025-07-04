use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use anyhow::Result;
use chrono::Utc;
use crate::types::{EmailConfig, EmailMessage};
use native_tls::TlsConnector;
use std::net::TcpStream;
use tokio::task;
use tokio::time::{timeout, Duration};
use mailparse::{MailHeaderMap, parse_mail};
use lettre::message::header::{Header, HeaderName, HeaderValue};
use std::error::Error;
use std::collections::HashSet;
use crate::crypto;

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

pub async fn send_email(
    config: &EmailConfig,
    to_address: &str,
    subject: &str,
    body: &str,
    nostr_npub: Option<&str>,
) -> Result<String> {
    println!("[RUST] send_email: Starting email send process");
    println!("[RUST] send_email: SMTP Host: {}, Port: {}", config.smtp_host, config.smtp_port);
    println!("[RUST] send_email: From: {}, To: {}", config.email_address, to_address);
    println!("[RUST] send_email: Use TLS: {}", config.use_tls);
    
    let mut builder = Message::builder()
        .from(config.email_address.parse()?)
        .to(to_address.parse()?)
        .subject(subject);
    if let Some(npub) = nostr_npub {
        builder = builder.header(XNostrPubkey(npub.to_string()));
    }
    let email = builder.body(body.to_string())?;

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
                
                let subject = email.headers
                    .get_first_value("Subject")
                    .unwrap_or_else(|| "No Subject".to_string());
                
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
                            (subject, body_text.clone())
                        }
                    }
                } else {
                    (subject, body_text.clone())
                };

                let email_message = EmailMessage {
                    id: email_id.to_string(),
                    from,
                    to,
                    subject: final_subject,
                    body: final_body,
                    raw_body: body_text.clone(),
                    date,
                    is_read: true, // We'll assume all fetched emails are read for now
                    raw_headers,
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
    use chrono::{Duration, Utc};
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
/// 1. Search for "BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE" - finds emails with encrypted content
/// 2. Search for "X-Nostr-Pubkey:" - finds emails with Nostr public key headers
/// 3. Combine results and remove duplicates
/// 4. Fetch only the matching emails (much more efficient than fetching all emails)
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
        "X-GM-RAW \"BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE\"",
        "X-GM-RAW \"X-Nostr-Pubkey:\"",
        "X-GM-RAW \"END NOSTR NIP-04 ENCRYPTED MESSAGE\"",
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
                let subject = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
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
                    let (final_subject, final_body) = match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                        Ok((dec_subject, dec_body)) => {
                            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} decryption completed", email_id);
                            (dec_subject, dec_body)
                        }
                        Err(e) => {
                            println!("[RUST] fetch_nostr_emails_from_gmail_optimized: Email {} decryption failed: {}, using original content", email_id, e);
                            (subject, body_text.clone())
                        }
                    };
                    
                    let email_message = EmailMessage {
                        id: email_id.to_string(),
                        from,
                        to,
                        subject: final_subject,
                        body: final_body,
                        raw_body: body_text.clone(),
                        date,
                        is_read: true,
                        raw_headers,
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
    let mut email_id = 0;
    let cutoff = Utc::now() - Duration::hours(24);
    for message in messages.iter() {
        email_id += 1;
        if let Some(body) = message.body() {
            if let Ok(email) = parse_mail(body) {
                let from = email.headers.get_first_value("From").unwrap_or_else(|| "Unknown".to_string());
                let to = email.headers.get_first_value("To").unwrap_or_else(|| config.email_address.clone());
                let subject = email.headers.get_first_value("Subject").unwrap_or_else(|| "No Subject".to_string());
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
                let (final_subject, final_body) = if raw_headers.contains("X-Nostr-Pubkey:") {
                    match decrypt_nostr_email_content(config, &raw_headers, &subject, &body_text) {
                        Ok((dec_subject, dec_body)) => {
                            println!("[RUST] fetch_emails_from_session_last_24h: Email {} decryption completed", email_id);
                            (dec_subject, dec_body)
                        }
                        Err(e) => {
                            println!("[RUST] fetch_emails_from_session_last_24h: Email {} decryption failed: {}, using original content", email_id, e);
                            (subject, body_text.clone())
                        }
                    }
                } else {
                    (subject, body_text.clone())
                };
                
                let email_message = EmailMessage {
                    id: email_id.to_string(),
                    from,
                    to,
                    subject: final_subject,
                    body: final_body,
                    raw_body: body_text.clone(),
                    date,
                    is_read: true,
                    raw_headers,
                };
                emails.push(email_message);
            }
        }
    }
    session.logout()?;
    Ok(emails)
}

/// Extract Nostr public key from email headers
fn extract_nostr_pubkey_from_headers(raw_headers: &str) -> Option<String> {
    for line in raw_headers.lines() {
        if line.starts_with("X-Nostr-Pubkey:") {
            let pubkey = line.split_once(':')
                .and_then(|(_, value)| Some(value.trim()))
                .filter(|s| !s.is_empty());
            return pubkey.map(|s| s.to_string());
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
                println!("[RUST] Successfully decrypted body");
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