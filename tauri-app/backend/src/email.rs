use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::client::SmtpConnection;
use lettre::transport::smtp::Error as SmtpError;
use anyhow::Result;
use chrono::Utc;
use crate::types::{EmailConfig, EmailMessage};
use imap::Session;
use native_tls::TlsConnector;
use std::net::TcpStream;
use tokio::task;
use tokio::time::{timeout, Duration};

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

pub async fn fetch_emails(config: &EmailConfig, _limit: usize) -> Result<Vec<EmailMessage>> {
    // For now, we'll return a placeholder since IMAP is complex
    // In a full implementation, you'd want to add a proper IMAP client
    Ok(vec![
        EmailMessage {
            id: "1".to_string(),
            from: "example@example.com".to_string(),
            to: config.email_address.clone(),
            subject: "Welcome to Nostr Mail".to_string(),
            body: "This is a placeholder email. IMAP functionality will be implemented in a future version.".to_string(),
            date: Utc::now(),
            is_read: false,
        }
    ])
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