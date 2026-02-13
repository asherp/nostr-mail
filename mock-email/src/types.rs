use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Email address structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EmailAddress {
    pub local: String,
    pub domain: String,
}

impl EmailAddress {
    pub fn new(local: String, domain: String) -> Self {
        Self { local, domain }
    }

    pub fn from_string(addr: &str) -> Option<Self> {
        let parts: Vec<&str> = addr.split('@').collect();
        if parts.len() == 2 {
            Some(Self {
                local: parts[0].to_string(),
                domain: parts[1].to_string(),
            })
        } else {
            None
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}@{}", self.local, self.domain)
    }
}

/// Email structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Email {
    pub id: String,
    pub from: EmailAddress,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub bcc: Vec<EmailAddress>,
    pub subject: String,
    pub body: String,
    pub html_body: Option<String>,
    pub headers: HashMap<String, String>,
    pub created_at: i64,
    pub attachments: Vec<Attachment>,
}

/// Email attachment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub content: Vec<u8>,
}

/// Filter for querying emails
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct EmailFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox: Option<String>,
}

impl EmailFilter {
    /// Check if an email matches this filter
    #[allow(dead_code)]
    pub fn matches(&self, email: &Email, mailbox: &str) -> bool {
        // Check mailbox
        if let Some(filter_mailbox) = &self.mailbox {
            if filter_mailbox != mailbox {
                return false;
            }
        }

        // Check from
        if let Some(from_addrs) = &self.from {
            let email_from_str = email.from.to_string();
            if !from_addrs.iter().any(|addr| email_from_str.contains(addr)) {
                return false;
            }
        }

        // Check to
        if let Some(to_addrs) = &self.to {
            let email_to_strings: Vec<String> = email.to.iter().map(|a| a.to_string()).collect();
            if !to_addrs.iter().any(|addr| email_to_strings.iter().any(|e| e.contains(addr))) {
                return false;
            }
        }

        // Check subject
        if let Some(subject_filter) = &self.subject {
            if !email.subject.to_lowercase().contains(&subject_filter.to_lowercase()) {
                return false;
            }
        }

        // Check since (created_at >= since)
        if let Some(since) = self.since {
            if email.created_at < since {
                return false;
            }
        }

        // Check until (created_at <= until)
        if let Some(until) = self.until {
            if email.created_at > until {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_address() {
        let addr = EmailAddress::from_string("user@example.com").unwrap();
        assert_eq!(addr.local, "user");
        assert_eq!(addr.domain, "example.com");
        assert_eq!(addr.to_string(), "user@example.com");
    }

    #[test]
    fn test_email_filter() {
        let email = Email {
            id: "test_id".to_string(),
            from: EmailAddress::from_string("sender@example.com").unwrap(),
            to: vec![EmailAddress::from_string("recipient@example.com").unwrap()],
            cc: vec![],
            bcc: vec![],
            subject: "Test Subject".to_string(),
            body: "Test body".to_string(),
            html_body: None,
            headers: HashMap::new(),
            created_at: 1000,
            attachments: vec![],
        };

        let filter = EmailFilter {
            from: Some(vec!["sender@example.com".to_string()]),
            ..Default::default()
        };
        assert!(filter.matches(&email, "INBOX"));

        let filter = EmailFilter {
            subject: Some("test".to_string()),
            ..Default::default()
        };
        assert!(filter.matches(&email, "INBOX"));
    }
}
