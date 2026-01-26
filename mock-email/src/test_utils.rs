use crate::types::{Email, EmailAddress, Attachment};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::collections::HashMap;

/// Email with metadata (for testing)
#[derive(Debug, Clone)]
pub struct EmailWithMetadata {
    pub email: Email,
    pub mailbox: String,
}

/// Generate a fake email with random data
#[allow(dead_code)]
pub fn generate_fake_email() -> Email {
    generate_fake_email_with_rng(&mut rand::thread_rng(), None, None)
}

/// Generate a fake email with random data (with seeded RNG)
pub fn generate_fake_email_with_rng(
    rng: &mut impl Rng,
    from: Option<EmailAddress>,
    to: Option<Vec<EmailAddress>>,
) -> Email {
    let subjects = [
        "Meeting Tomorrow",
        "Project Update",
        "Quick Question",
        "Follow Up",
        "Important Notice",
        "Weekly Report",
        "Action Required",
        "Thank You",
        "Reminder",
        "New Opportunity",
    ];

    let bodies = [
        "Hi there,\n\nJust wanted to follow up on our previous conversation.\n\nBest regards",
        "Hello,\n\nI hope this email finds you well.\n\nLooking forward to your response.",
        "Dear colleague,\n\nI wanted to share some important information with you.\n\nBest",
        "Hi,\n\nQuick question - could you please provide an update?\n\nThanks!",
        "Hello,\n\nThis is a friendly reminder about the upcoming deadline.\n\nRegards",
    ];

    let domains = ["example.com", "test.org", "demo.net", "fake.io", "sample.com"];
    let names = [
        "alice", "bob", "charlie", "diana", "eve", "frank", "grace", "henry",
        "iris", "jack", "kate", "liam", "mia", "noah", "olivia", "paul",
    ];

    let from_addr = from.unwrap_or_else(|| {
        let name = names[rng.gen_range(0..names.len())];
        let domain = domains[rng.gen_range(0..domains.len())];
        EmailAddress::new(name.to_string(), domain.to_string())
    });

    let to_addrs = to.unwrap_or_else(|| {
        vec![EmailAddress::new(
            names[rng.gen_range(0..names.len())].to_string(),
            domains[rng.gen_range(0..domains.len())].to_string(),
        )]
    });

    let subject = subjects[rng.gen_range(0..subjects.len())];
    let body = bodies[rng.gen_range(0..bodies.len())];

    let mut headers = HashMap::new();
    headers.insert("message-id".to_string(), format!("<{}@mock-email>", uuid::Uuid::new_v4()));
    headers.insert("user-agent".to_string(), "mock-email/1.0".to_string());

    Email {
        id: uuid::Uuid::new_v4().to_string(),
        from: from_addr,
        to: to_addrs,
        cc: vec![],
        bcc: vec![],
        subject: subject.to_string(),
        body: body.to_string(),
        html_body: None,
        headers,
        created_at: chrono::Utc::now().timestamp() - rng.gen_range(0..86400 * 30), // Random time in last 30 days
        attachments: vec![],
    }
}

/// Generate multiple fake emails
#[allow(dead_code)]
pub fn generate_fake_emails(count: usize) -> Vec<Email> {
    generate_fake_emails_with_seed(count, None)
}

/// Generate multiple fake emails with a seed for deterministic generation
#[allow(dead_code)]
pub fn generate_fake_emails_with_seed(count: usize, seed: Option<u64>) -> Vec<Email> {
    let mut rng = if let Some(s) = seed {
        StdRng::seed_from_u64(s)
    } else {
        StdRng::from_entropy()
    };

    let mut emails = Vec::new();
    for _ in 0..count {
        emails.push(generate_fake_email_with_rng(&mut rng, None, None));
    }
    emails
}

/// Generate fake emails with a pool of email addresses
pub fn generate_fake_emails_with_pool(
    count: usize,
    seed: Option<u64>,
    email_pool: &[(String, String)], // Vec of (email_address, name)
) -> Vec<EmailWithMetadata> {
    let mut rng = if let Some(s) = seed {
        StdRng::seed_from_u64(s)
    } else {
        StdRng::from_entropy()
    };

    let mut emails_with_metadata = Vec::new();
    
    for _ in 0..count {
        // Randomly select from and to addresses from the pool
        let from_idx = rng.gen_range(0..email_pool.len());
        let to_idx = rng.gen_range(0..email_pool.len());
        
        // Ensure from and to are different
        let to_idx = if from_idx == to_idx {
            (to_idx + 1) % email_pool.len()
        } else {
            to_idx
        };

        let from_addr = EmailAddress::from_string(&email_pool[from_idx].0)
            .unwrap_or_else(|| EmailAddress::new("unknown".to_string(), "unknown".to_string()));
        let to_addr = EmailAddress::from_string(&email_pool[to_idx].0)
            .unwrap_or_else(|| EmailAddress::new("unknown".to_string(), "unknown".to_string()));

        let email = generate_fake_email_with_rng(&mut rng, Some(from_addr.clone()), Some(vec![to_addr]));
        
        // Determine mailbox (INBOX for recipient, SENT for sender)
        emails_with_metadata.push(EmailWithMetadata {
            email: email.clone(),
            mailbox: "INBOX".to_string(),
        });
        
        emails_with_metadata.push(EmailWithMetadata {
            email,
            mailbox: "SENT".to_string(),
        });
    }

    emails_with_metadata
}

/// Generate fake email with attachments
#[allow(dead_code)]
pub fn generate_fake_email_with_attachments(num_attachments: usize) -> Email {
    let mut email = generate_fake_email();
    
    let mut attachments = Vec::new();
    for i in 0..num_attachments {
        let content = format!("Fake attachment content {}", i).into_bytes();
        attachments.push(Attachment {
            filename: format!("attachment_{}.txt", i),
            content_type: "text/plain".to_string(),
            content,
        });
    }
    
    email.attachments = attachments;
    email
}

/// Generate fake email addresses
#[allow(dead_code)]
pub fn generate_fake_email_addresses(count: usize) -> Vec<(String, String)> {
    generate_fake_email_addresses_with_seed(count, None)
}

/// Generate fake email addresses with a seed
pub fn generate_fake_email_addresses_with_seed(count: usize, seed: Option<u64>) -> Vec<(String, String)> {
    let mut rng = if let Some(s) = seed {
        StdRng::seed_from_u64(s)
    } else {
        StdRng::from_entropy()
    };

    let domains = ["example.com", "test.org", "demo.net", "fake.io", "sample.com"];
    let names = [
        "alice", "bob", "charlie", "diana", "eve", "frank", "grace", "henry",
        "iris", "jack", "kate", "liam", "mia", "noah", "olivia", "paul",
        "quinn", "ruby", "sam", "tina", "uma", "vince", "willa", "xavier",
    ];

    let mut addresses = Vec::new();
    for i in 0..count {
        let name = if i < names.len() {
            names[i].to_string()
        } else {
            format!("user{}", i)
        };
        let domain = domains[rng.gen_range(0..domains.len())];
        let email = format!("{}@{}", name, domain);
        addresses.push((email, name));
    }

    addresses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_fake_email() {
        let email = generate_fake_email();
        assert!(!email.id.is_empty());
        assert!(!email.from.to_string().is_empty());
        assert!(!email.to.is_empty());
    }

    #[test]
    fn test_generate_fake_emails() {
        let emails = generate_fake_emails(10);
        assert_eq!(emails.len(), 10);
    }

    #[test]
    fn test_generate_fake_email_addresses() {
        let addresses = generate_fake_email_addresses(5);
        assert_eq!(addresses.len(), 5);
        assert!(addresses[0].0.contains('@'));
    }
}
