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

pub async fn send_email(
    config: &EmailConfig,
    to_address: &str,
    subject: &str,
    body: &str,
) -> Result<String> {
    println!("[RUST] send_email: Starting email send process");
    println!("[RUST] send_email: SMTP Host: {}, Port: {}", config.smtp_host, config.smtp_port);
    println!("[RUST] send_email: From: {}, To: {}", config.email_address, to_address);
    println!("[RUST] send_email: Use TLS: {}", config.use_tls);
    
    let email = Message::builder()
        .from(config.email_address.parse()?)
        .to(to_address.parse()?)
        .subject(subject)
        .body(body.to_string())?;

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

pub async fn fetch_emails(config: &EmailConfig, limit: usize, search_query: Option<String>) -> Result<Vec<EmailMessage>> {
    println!("[RUST] fetch_emails: Starting to fetch emails with limit: {} and search: {:?}", limit, search_query);
    
    let host = &config.imap_host;
    let port = config.imap_port;
    let username = &config.email_address;
    let password = &config.password;
    let use_tls = config.use_tls;

    let addr = format!("{}:{}", host, port);
    println!("[RUST] fetch_emails: Connecting to IMAP server: {}", addr);

    let emails = if use_tls {
        let tls = TlsConnector::builder().build()?;
        let tcp_stream = TcpStream::connect(&addr)?;
        let tls_stream = tls.connect(host, tcp_stream)?;
        let client = imap::Client::new(tls_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        
        fetch_emails_from_session(&mut session, config, limit, search_query)?
    } else {
        let tcp_stream = TcpStream::connect(&addr)?;
        let client = imap::Client::new(tcp_stream);
        let mut session = client.login(username, password).map_err(|e| anyhow::anyhow!(e.0))?;
        
        fetch_emails_from_session(&mut session, config, limit, search_query)?
    };

    println!("[RUST] fetch_emails: Successfully fetched {} emails", emails.len());
    
    // Sort emails by date (newest first)
    let mut sorted_emails = emails;
    sorted_emails.sort_by(|a, b| b.date.cmp(&a.date));
    
    Ok(sorted_emails)
}

fn fetch_emails_from_session(session: &mut imap::Session<impl std::io::Read + std::io::Write>, config: &EmailConfig, limit: usize, search_query: Option<String>) -> Result<Vec<EmailMessage>> {
    println!("[RUST] fetch_emails_from_session: Successfully connected to IMAP server");

    // Select the INBOX
    session.select("INBOX")?;
    println!("[RUST] fetch_emails_from_session: Selected INBOX");

    // Calculate date for SINCE filter (7 days ago)
    let seven_days_ago = chrono::Utc::now() - chrono::Duration::days(7);
    let since_date = seven_days_ago.format("%d-%b-%Y").to_string();
    println!("[RUST] fetch_emails_from_session: Using SINCE filter for date: {}", since_date);

    // Build search criteria with SINCE filter
    let search_criteria = if let Some(query) = &search_query {
        println!("[RUST] fetch_emails_from_session: Searching for: {} with SINCE filter", query);
        // Use IMAP search to find emails from last 7 days containing the query
        format!("SINCE \"{}\" FROM \"{}\"", since_date, query)
    } else {
        println!("[RUST] fetch_emails_from_session: Fetching all emails from last 7 days");
        format!("SINCE \"{}\"", since_date)
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

                let email_message = EmailMessage {
                    id: email_id.to_string(),
                    from,
                    to,
                    subject,
                    body: body_text,
                    date,
                    is_read: true, // We'll assume all fetched emails are read for now
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