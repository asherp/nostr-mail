pub mod config;
pub mod smtp;
pub mod imap;
pub mod store;
pub mod test_utils;
pub mod types;

// Re-export commonly used types
pub use store::{EmailStore, Mailbox};
pub use test_utils::EmailWithMetadata;
pub use types::{Email, EmailAddress, EmailFilter};
pub use smtp::SmtpServer;
pub use imap::ImapServer;
