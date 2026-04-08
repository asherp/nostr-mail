use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub public_key: String,
    pub label: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationAccount {
    pub public_key: String,
    pub private_key: String,
    pub label: String,
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
    pub html_body: Option<String>,
    pub date: DateTime<Utc>,
    pub is_read: bool,
    pub raw_headers: String,
    pub sender_pubkey: Option<String>,
    pub recipient_pubkey: Option<String>,
    pub message_id: Option<String>,
    pub signature_valid: Option<bool>,
    pub signature_source: Option<String>,
    pub transport_auth_verified: Option<bool>,
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
    pub sender_private_key: Option<String>,
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
    pub error_message: Option<String>, // Error message if the relay failed to connect
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
    /// True when the body could not be decrypted server-side (e.g. glossia-encoded)
    /// and the frontend should attempt glossia decode + NIP decrypt itself.
    #[serde(default)]
    pub encrypted: bool,
    /// Sender pubkey from the email (for frontend decryption key lookup)
    #[serde(default)]
    pub sender_pubkey: Option<String>,
    /// Manifest attachment metadata (if the email was manifest-encrypted)
    #[serde(default)]
    pub attachments: Vec<ManifestAttachmentInfo>,
}

/// Result structure for matching email ID lookup (includes both ID and message_id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchingEmailIdResult {
    pub email_id: Option<i64>,
    pub message_id: String,
}

/// Transport authentication method used for verification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransportAuthMethod {
    #[serde(rename = "dmarc")]
    Dmarc,
    #[serde(rename = "dkim")]
    Dkim,
    #[serde(rename = "none")]
    None,
}

/// Verdict from transport authentication verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportAuthVerdict {
    pub transport_verified: bool,
    pub method: TransportAuthMethod,
    pub reason: String,
}

/// Result structure for follow list fetch that distinguishes between no event found vs empty event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowListResult {
    /// List of followed pubkeys (empty if event has no tags)
    pub pubkeys: Vec<String>,
    /// True if a kind 3 event was found on relays (even if it has 0 tags)
    pub event_found: bool,
    /// ID of the latest contact list event found (if any)
    pub event_id: Option<String>,
}

/// Parsed representation of an ASCII-armored nostr-mail message.
/// Populated from a capnp ArmorMessage (schema validation) then serialized as JSON for Tauri IPC.
/// The body_type and encryption_nip fields derive from the capnp Body union and NipVersion enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedArmorMessage {
    /// Raw encoded body content (glossia words, base64, or plaintext)
    pub body_text: String,
    /// Body variant from capnp Body union: "encrypted", "signed", or "plain"
    pub body_type: String,
    /// NIP version if encrypted, from capnp NipVersion enum: "nip04" or "nip44"
    pub encryption_nip: Option<String>,
    /// Schnorr signature hex (128 hex chars = 64 bytes), from capnp SignatureBlock.signature
    pub signature_hex: Option<String>,
    /// Pubkey hex from SIGNATURE block (64 hex chars = 32 bytes), from capnp SignatureBlock.pubkey
    pub sig_pubkey_hex: Option<String>,
    /// Pubkey hex from SEAL block (64 hex chars = 32 bytes), from capnp SealBlock.pubkey
    pub seal_pubkey_hex: Option<String>,
    /// Profile name from @ProfileName line, from capnp SignatureBlock.profileName
    pub profile_name: Option<String>,
    /// Display name from @DisplayName line, from capnp SealBlock.displayName (falls back to profile_name)
    pub display_name: Option<String>,
    /// Raw encoded sig+pubkey content before decode (for JS fallback paths)
    pub raw_sig_pubkey: Option<String>,
    /// Plaintext that appeared before the first armor delimiter
    pub prefix_text: Option<String>,
    /// Recursively parsed nested quoted armor, from capnp Body.quoted -> ArmorMessage
    pub quoted: Option<Box<ParsedArmorMessage>>,
    /// Raw nested armor text string (for JS compatibility during incremental migration)
    pub quoted_armor_text: Option<String>,
    /// Decoded body bytes as base64 (for signature verification without re-decoding)
    pub body_bytes_b64: Option<String>,
}

/// Per-block decrypt result (one per nesting level in the armor chain).
/// Array is ordered innermost-first, matching the JS decryptAllEncryptedBlocks convention.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedBlock {
    /// Decrypted plaintext body (after manifest AES decrypt if applicable)
    pub decrypted_text: Option<String>,
    /// Error message if decryption failed at this level
    pub error: Option<String>,
    /// Whether this block was encrypted (false for signed/plain blocks → null in JS array)
    pub was_encrypted: bool,
    /// Profile name from the signature/seal block at this level
    pub profile_name: Option<String>,
    /// Body type at this level: "encrypted", "signed", "plain"
    pub body_type: String,
}

/// Manifest attachment metadata — carries the AES key needed for separate decryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestAttachmentInfo {
    pub id: String,
    pub orig_filename: String,
    pub orig_mime: String,
    /// Base64-encoded AES-256 key for this attachment
    pub key_wrap_b64: String,
    /// Hex SHA-256 of the encrypted file (for integrity check)
    pub cipher_sha256_hex: Option<String>,
    pub cipher_size: u64,
}

/// Full result from the decrypt_email_body Tauri command.
/// Replaces the JS decryptManifestMessage + decryptAllEncryptedBlocks pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptEmailResult {
    /// Decrypted subject (or original if not encrypted / decryption failed)
    pub subject: String,
    /// Decrypted outermost body text
    pub body: String,
    /// Whether the body came from a manifest (vs legacy direct encryption)
    pub is_manifest: bool,
    /// Attachment metadata from manifest (empty if legacy or no attachments)
    pub attachments: Vec<ManifestAttachmentInfo>,
    /// Per-block decrypt results, innermost-first (for lock/unlock icons)
    pub block_results: Vec<DecryptedBlock>,
    /// Whether overall decryption succeeded
    pub success: bool,
    /// Error message if overall decryption failed
    pub error: Option<String>,
    /// Subject ciphertext (after glossia decode, before NIP decrypt) for DM↔email hash matching
    pub subject_ciphertext: Option<String>,
}

/// Result from the decrypt_manifest_attachment Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedAttachment {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    /// Base64-encoded decrypted file bytes
    pub data_b64: String,
    /// Original (unpadded) file size
    pub size: usize,
}