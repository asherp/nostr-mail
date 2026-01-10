use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub email_address: String,
    pub password: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub imap_host: String,
    pub imap_port: u16,
    pub use_tls: bool,
    pub private_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub raw_body: String,
    pub date: DateTime<Utc>,
    pub is_read: bool,
    pub raw_headers: String,
    pub nostr_pubkey: Option<String>,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrEvent {
    pub id: String,
    pub pubkey: String,
    pub created_at: i64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub pubkey: String,
    pub fields: HashMap<String, serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectMessage {
    pub id: String,
    pub from_pubkey: String,
    pub to_pubkey: String,
    pub content: String,
    pub created_at: i64,
    pub is_valid: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    pub url: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub pubkey: Option<String>,
    pub contact: Option<String>,
    pub supported_nips: Vec<u16>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default_relays: Vec<String>,
    pub contacts_file: Option<String>,
    pub cache_dir: Option<String>,
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResult {
    pub pubkey: String,
    pub fields: HashMap<String, serde_json::Value>,
    pub raw_content: String,
}

/// Represents the content of a message with explicit encryption state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    /// Plaintext content that needs to be encrypted
    Plaintext(String),
    /// Already encrypted content that should be sent as-is
    Encrypted(String),
}

/// Request structure for sending direct messages with explicit content type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectMessageRequest {
    pub sender_private_key: String,
    pub recipient_pubkey: String,
    pub content: MessageContent,
    pub relays: Vec<String>,
    pub encryption_algorithm: Option<String>, // "nip04" or "nip44"
}

/// Connection status for a relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelayConnectionStatus {
    Connected,
    Disconnected,
    Disabled,
}

/// Status information for a relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayStatus {
    pub url: String,
    pub is_active: bool,
    pub status: RelayConnectionStatus,
}

/// Represents an email attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachment {
    pub filename: String,
    pub content_type: String,
    pub data: String, // Base64 encoded data
    pub size: usize,
    pub is_encrypted: bool,
    pub encryption_method: Option<String>,
    pub algorithm: Option<String>,
    pub original_filename: Option<String>,
    pub original_type: Option<String>,
    pub original_size: Option<usize>,
}

/// Result structure for matching email body lookup (includes email ID for fetching attachments)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchingEmailBodyResult {
    pub body: String,
    pub email_id: Option<i64>,
} 