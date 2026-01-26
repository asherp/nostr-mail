use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use base64::Engine;

/// Configuration for the mock email server
#[derive(Debug, Clone, Parser)]
#[command(name = "mock-email")]
#[command(about = "Mock email server (SMTP/IMAP) for testing")]
pub struct Config {
    /// SMTP port to listen on (default: 2525)
    #[arg(long, default_value = "2525")]
    pub smtp_port: u16,

    /// IMAP port to listen on (default: 1143)
    #[arg(long, default_value = "1143")]
    pub imap_port: u16,

    /// Path to JSON file with emails to preload
    #[arg(long)]
    pub preload_emails: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Log file path (default: email.log)
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    /// Generate and preload fake emails
    #[arg(long)]
    pub generate_fake_emails: Option<usize>,

    /// Output file for generated emails (JSON format)
    #[arg(long)]
    pub output_emails: Option<PathBuf>,

    /// Seed for random number generator (for deterministic email generation)
    #[arg(long, default_value = "0")]
    pub seed: u64,
}

/// Email preload configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadConfig {
    pub emails: Vec<PreloadEmail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailboxes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadEmail {
    pub id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_body: Option<String>,
    pub headers: std::collections::HashMap<String, String>,
    pub created_at: i64,
    pub attachments: Vec<PreloadAttachment>,
    pub mailbox: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadAttachment {
    pub filename: String,
    pub content_type: String,
    pub content: String, // Base64 encoded
}

impl From<PreloadEmail> for (crate::types::Email, String) {
    fn from(email: PreloadEmail) -> Self {
        let from_addr = crate::types::EmailAddress::from_string(&email.from)
            .unwrap_or_else(|| crate::types::EmailAddress::new("unknown".to_string(), "unknown".to_string()));
        
        let to_addrs: Vec<crate::types::EmailAddress> = email.to
            .iter()
            .filter_map(|a| crate::types::EmailAddress::from_string(a))
            .collect();
        
        let cc_addrs: Vec<crate::types::EmailAddress> = email.cc
            .iter()
            .filter_map(|a| crate::types::EmailAddress::from_string(a))
            .collect();
        
        let bcc_addrs: Vec<crate::types::EmailAddress> = email.bcc
            .iter()
            .filter_map(|a| crate::types::EmailAddress::from_string(a))
            .collect();

        let attachments: Vec<crate::types::Attachment> = email.attachments
            .iter()
            .map(|a| {
                use base64::Engine;
                let content = base64::engine::general_purpose::STANDARD.decode(&a.content).unwrap_or_default();
                crate::types::Attachment {
                    filename: a.filename.clone(),
                    content_type: a.content_type.clone(),
                    content,
                }
            })
            .collect();

        let email_obj = crate::types::Email {
            id: email.id,
            from: from_addr,
            to: to_addrs,
            cc: cc_addrs,
            bcc: bcc_addrs,
            subject: email.subject,
            body: email.body,
            html_body: email.html_body,
            headers: email.headers,
            created_at: email.created_at,
            attachments,
        };

        (email_obj, email.mailbox)
    }
}

/// Load emails from a JSON file
pub fn load_preload_emails(path: PathBuf) -> anyhow::Result<Vec<(crate::types::Email, String)>> {
    let content = std::fs::read_to_string(path)?;
    let config: PreloadConfig = serde_json::from_str(&content)?;
    Ok(config.emails.into_iter().map(|e| e.into()).collect())
}

/// Write emails to a JSON file
pub fn write_emails_to_file(
    emails_with_mailboxes: &[(crate::types::Email, String)],
    path: PathBuf,
) -> anyhow::Result<()> {
    let preload_emails: Vec<PreloadEmail> = emails_with_mailboxes
        .iter()
        .map(|(email, mailbox)| {
            let attachments: Vec<PreloadAttachment> = email.attachments
                .iter()
                .map(|a| PreloadAttachment {
                    filename: a.filename.clone(),
                    content_type: a.content_type.clone(),
                    content: base64::engine::general_purpose::STANDARD.encode(&a.content),
                })
                .collect();

            PreloadEmail {
                id: email.id.clone(),
                from: email.from.to_string(),
                to: email.to.iter().map(|a| a.to_string()).collect(),
                cc: email.cc.iter().map(|a| a.to_string()).collect(),
                bcc: email.bcc.iter().map(|a| a.to_string()).collect(),
                subject: email.subject.clone(),
                body: email.body.clone(),
                html_body: email.html_body.clone(),
                headers: email.headers.clone(),
                created_at: email.created_at,
                attachments,
                mailbox: mailbox.clone(),
            }
        })
        .collect();

    let config = PreloadConfig {
        emails: preload_emails,
        mailboxes: None,
    };

    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let _config = Config {
            smtp_port: 2525,
            imap_port: 1143,
            preload_emails: None,
            log_level: "info".to_string(),
            log_file: None,
            generate_fake_emails: None,
            output_emails: None,
            seed: 0,
        };
    }
}
