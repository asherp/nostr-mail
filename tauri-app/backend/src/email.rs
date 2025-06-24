use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use anyhow::Result;
use chrono::Utc;
use crate::types::{EmailConfig, EmailMessage};

pub async fn send_email(
    config: &EmailConfig,
    to_address: &str,
    subject: &str,
    body: &str,
) -> Result<String> {
    let email = Message::builder()
        .from(config.email_address.parse()?)
        .to(to_address.parse()?)
        .subject(subject)
        .body(body.to_string())?;

    let creds = Credentials::new(config.email_address.clone(), config.password.clone());

    let mailer = SmtpTransport::relay(&config.smtp_host)?
        .port(config.smtp_port)
        .credentials(creds)
        .build();

    match mailer.send(&email) {
        Ok(_) => Ok(format!("Email sent successfully to {}", to_address)),
        Err(e) => Err(anyhow::anyhow!("Failed to send email: {}", e)),
    }
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