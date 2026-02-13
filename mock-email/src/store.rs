use crate::types::{Email, EmailFilter};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use log::info;

/// Mailbox structure
#[derive(Debug, Clone)]
pub struct Mailbox {
    #[allow(dead_code)]
    pub name: String,
    pub emails: Vec<String>, // Email IDs
}

/// In-memory email store
#[derive(Debug, Clone)]
pub struct EmailStore {
    emails: Arc<RwLock<HashMap<String, Email>>>,
    // Index by recipient for faster filtering
    emails_by_recipient: Arc<RwLock<HashMap<String, Vec<String>>>>,
    // Index by sender for faster filtering
    emails_by_sender: Arc<RwLock<HashMap<String, Vec<String>>>>,
    // Mailboxes: INBOX, SENT, DRAFTS, etc.
    mailboxes: Arc<RwLock<HashMap<String, Mailbox>>>,
}

impl EmailStore {
    pub fn new() -> Self {
        Self {
            emails: Arc::new(RwLock::new(HashMap::new())),
            emails_by_recipient: Arc::new(RwLock::new(HashMap::new())),
            emails_by_sender: Arc::new(RwLock::new(HashMap::new())),
            mailboxes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize default mailboxes (async)
    pub async fn init(&self) {
        self.init_default_mailboxes().await;
    }

    async fn init_default_mailboxes(&self) {
        let mut mailboxes = self.mailboxes.write().await;
        // INBOX must be uppercase per IMAP standard
        mailboxes.insert("INBOX".to_string(), Mailbox {
            name: "INBOX".to_string(),
            emails: vec![],
        });
        // Standard mailbox names (case-insensitive, but we'll store in standard case)
        mailboxes.insert("Sent".to_string(), Mailbox {
            name: "Sent".to_string(),
            emails: vec![],
        });
        mailboxes.insert("Drafts".to_string(), Mailbox {
            name: "Drafts".to_string(),
            emails: vec![],
        });
        mailboxes.insert("Trash".to_string(), Mailbox {
            name: "Trash".to_string(),
            emails: vec![],
        });
    }

    /// Add an email to the store
    pub async fn add_email(&self, email: Email, mailbox: &str) {
        let email_id = email.id.clone();
        let from_str = email.from.to_string();
        let to_strings: Vec<String> = email.to.iter().map(|a| a.to_string()).collect();

        // Add to main store
        let mut emails = self.emails.write().await;
        emails.insert(email_id.clone(), email.clone());

        // Update sender index
        let mut by_sender = self.emails_by_sender.write().await;
        by_sender
            .entry(from_str)
            .or_insert_with(Vec::new)
            .push(email_id.clone());

        // Update recipient index
        let mut by_recipient = self.emails_by_recipient.write().await;
        for to_str in to_strings {
            by_recipient
                .entry(to_str)
                .or_insert_with(Vec::new)
                .push(email_id.clone());
        }

        // Add to mailbox
        let mut mailboxes = self.mailboxes.write().await;
        let mailbox_entry = mailboxes.entry(mailbox.to_string()).or_insert_with(|| Mailbox {
            name: mailbox.to_string(),
            emails: vec![],
        });
        mailbox_entry.emails.push(email_id.clone());
        info!("[STORE] Added email {} to mailbox '{}' (mailbox now has {} emails)", 
            email_id, mailbox, mailbox_entry.emails.len());
    }

    /// Query emails matching filters
    #[allow(dead_code)]
    pub async fn query(&self, filter: &EmailFilter) -> Vec<Email> {
        let emails = self.emails.read().await;
        let mailboxes = self.mailboxes.read().await;
        let mut matching_emails = Vec::new();
        let mut seen_ids = HashSet::new();

        // Determine which mailboxes to search
        let mailboxes_to_search: Vec<String> = if let Some(mailbox_name) = &filter.mailbox {
            vec![mailbox_name.clone()]
        } else {
            mailboxes.keys().cloned().collect()
        };

        // Get candidate email IDs based on filter optimizations
        let candidates = self.get_candidate_ids(filter).await;

        // Check each candidate email
        for email_id in candidates {
            if seen_ids.contains(&email_id) {
                continue;
            }

            if let Some(email) = emails.get(&email_id) {
                // Check if email is in one of the mailboxes we're searching
                let email_in_mailbox = mailboxes_to_search.iter().any(|mb_name| {
                    mailboxes
                        .get(mb_name)
                        .map(|mb| mb.emails.contains(&email_id))
                        .unwrap_or(false)
                });

                if email_in_mailbox && filter.matches(email, &mailboxes_to_search[0]) {
                    matching_emails.push(email.clone());
                    seen_ids.insert(email_id);
                }
            }
        }

        // Sort by created_at (newest first)
        matching_emails.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply limit if specified
        if let Some(limit) = filter.limit {
            matching_emails.truncate(limit);
        }

        matching_emails
    }

    /// Get candidate email IDs based on filter optimizations
    #[allow(dead_code)]
    async fn get_candidate_ids(&self, filter: &EmailFilter) -> Vec<String> {
        let emails = self.emails.read().await;
        let mut candidates = HashSet::new();

        // If filter specifies from addresses, use sender index
        if let Some(from_addrs) = &filter.from {
            let by_sender = self.emails_by_sender.read().await;
            for from_addr in from_addrs {
                if let Some(email_ids) = by_sender.get(from_addr) {
                    for email_id in email_ids {
                        candidates.insert(email_id.clone());
                    }
                }
            }
        }

        // If filter specifies to addresses, use recipient index
        if let Some(to_addrs) = &filter.to {
            let by_recipient = self.emails_by_recipient.read().await;
            for to_addr in to_addrs {
                if let Some(email_ids) = by_recipient.get(to_addr) {
                    for email_id in email_ids {
                        candidates.insert(email_id.clone());
                    }
                }
            }
        }

        // If no specific filters, consider all emails
        if filter.from.is_none() && filter.to.is_none() {
            candidates.extend(emails.keys().cloned());
        }

        candidates.into_iter().collect()
    }

    /// Get an email by ID
    #[allow(dead_code)]
    pub async fn get_email(&self, email_id: &str) -> Option<Email> {
        let emails = self.emails.read().await;
        emails.get(email_id).cloned()
    }

    /// Get all emails in a mailbox (case-insensitive lookup, except INBOX which is case-sensitive)
    pub async fn get_mailbox_emails(&self, mailbox_name: &str) -> Vec<Email> {
        let mailboxes = self.mailboxes.read().await;
        let emails = self.emails.read().await;
        
        info!("[STORE] get_mailbox_emails: requested mailbox='{}', available mailboxes: {:?}", 
            mailbox_name, mailboxes.keys().collect::<Vec<_>>());
        
        // INBOX is case-sensitive per IMAP standard, others are case-insensitive
        let mailbox = if mailbox_name.to_uppercase() == "INBOX" {
            mailboxes.get("INBOX")
        } else {
            // Case-insensitive lookup for other mailboxes
            mailboxes.get(mailbox_name)
                .or_else(|| {
                    // Try with first letter capitalized (standard case)
                    let mut chars: Vec<char> = mailbox_name.chars().collect();
                    if !chars.is_empty() {
                        chars[0] = chars[0].to_uppercase().next().unwrap();
                        let capitalized: String = chars.into_iter().collect();
                        info!("[STORE] Trying capitalized mailbox name: '{}'", capitalized);
                        mailboxes.get(&capitalized)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let upper = mailbox_name.to_uppercase();
                    info!("[STORE] Trying uppercase mailbox name: '{}'", upper);
                    mailboxes.get(&upper)
                })
                .or_else(|| {
                    let lower = mailbox_name.to_lowercase();
                    info!("[STORE] Trying lowercase mailbox name: '{}'", lower);
                    mailboxes.get(&lower)
                })
        };
        
        if let Some(mailbox) = mailbox {
            let result: Vec<Email> = mailbox
                .emails
                .iter()
                .filter_map(|id| emails.get(id).cloned())
                .collect();
            info!("[STORE] Found mailbox '{}' with {} email IDs, returning {} emails", 
                mailbox.name, mailbox.emails.len(), result.len());
            result
        } else {
            info!("[STORE] Mailbox '{}' not found, returning empty vec", mailbox_name);
            vec![]
        }
    }

    /// List all mailboxes
    pub async fn list_mailboxes(&self) -> Vec<String> {
        let mailboxes = self.mailboxes.read().await;
        mailboxes.keys().cloned().collect()
    }

    /// Get all emails (for debugging/testing)
    #[allow(dead_code)]
    pub async fn get_all_emails(&self) -> Vec<Email> {
        let emails = self.emails.read().await;
        emails.values().cloned().collect()
    }

    /// Clear all emails (for testing)
    #[allow(dead_code)]
    pub async fn clear(&self) {
        let mut emails = self.emails.write().await;
        emails.clear();
        let mut by_sender = self.emails_by_sender.write().await;
        by_sender.clear();
        let mut by_recipient = self.emails_by_recipient.write().await;
        by_recipient.clear();
        let mut mailboxes = self.mailboxes.write().await;
        for mailbox in mailboxes.values_mut() {
            mailbox.emails.clear();
        }
    }

    /// Get count of emails
    #[allow(dead_code)]
    pub async fn count(&self) -> usize {
        let emails = self.emails.read().await;
        emails.len()
    }
}

impl Default for EmailStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EmailAddress;

    #[tokio::test]
    async fn test_add_and_query() {
        let store = EmailStore::new();

        let email = Email {
            id: "test_id".to_string(),
            from: EmailAddress::from_string("sender@example.com").unwrap(),
            to: vec![EmailAddress::from_string("recipient@example.com").unwrap()],
            cc: vec![],
            bcc: vec![],
            subject: "Test Subject".to_string(),
            body: "Test body".to_string(),
            html_body: None,
            headers: std::collections::HashMap::new(),
            created_at: 1000,
            attachments: vec![],
        };

        store.add_email(email, "INBOX").await;

        let filter = EmailFilter {
            mailbox: Some("INBOX".to_string()),
            ..Default::default()
        };

        let results = store.query(&filter).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "test_id");
    }
}
