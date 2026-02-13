use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use nostr::prelude::*;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// Initialize the WASM module. Call this once before using other functions.
/// Sets up better panic messages for debugging.
#[wasm_bindgen]
pub fn init() {
    console_error_panic_hook::set_once();
}

// ---------------------------------------------------------------------------
// KeyPair type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

// We expose KeyPair fields to JS via explicit wasm_bindgen getters so the
// object is easy to use from JavaScript without going through serde.
#[wasm_bindgen]
pub struct WasmKeyPair {
    private_key: String,
    public_key: String,
}

#[wasm_bindgen]
impl WasmKeyPair {
    #[wasm_bindgen(getter)]
    pub fn private_key(&self) -> String {
        self.private_key.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn public_key(&self) -> String {
        self.public_key.clone()
    }
}

// ---------------------------------------------------------------------------
// Key Generation & Derivation
// ---------------------------------------------------------------------------

/// Generate a new secp256k1 keypair.
/// Returns a `WasmKeyPair` with bech32-encoded `nsec` and `npub` strings.
#[wasm_bindgen]
pub fn generate_keypair() -> Result<WasmKeyPair, JsValue> {
    let keys = Keys::generate();

    let private_key = keys
        .secret_key()
        .to_bech32()
        .map_err(|e| JsValue::from_str(&format!("Failed to encode secret key: {}", e)))?;
    let public_key = keys
        .public_key()
        .to_bech32()
        .map_err(|e| JsValue::from_str(&format!("Failed to encode public key: {}", e)))?;

    Ok(WasmKeyPair {
        private_key,
        public_key,
    })
}

/// Derive the bech32 `npub` public key from a bech32 `nsec` private key.
#[wasm_bindgen]
pub fn get_public_key_from_private(private_key: &str) -> Result<String, JsValue> {
    let secret_key = SecretKey::from_bech32(private_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid private key: {}", e)))?;
    let keys = Keys::new(secret_key);
    keys.public_key()
        .to_bech32()
        .map_err(|e| JsValue::from_str(&format!("Failed to encode public key: {}", e)))
}

// ---------------------------------------------------------------------------
// Key Validation
// ---------------------------------------------------------------------------

/// Validate a bech32 `nsec` private key. Returns `true` if valid.
#[wasm_bindgen]
pub fn validate_private_key(private_key: &str) -> bool {
    SecretKey::from_bech32(private_key).is_ok()
}

/// Validate a bech32 `npub` public key. Returns `true` if valid.
#[wasm_bindgen]
pub fn validate_public_key(public_key: &str) -> bool {
    PublicKey::from_bech32(public_key).is_ok()
}

// ---------------------------------------------------------------------------
// NIP-04 / NIP-44 Encryption
// ---------------------------------------------------------------------------

/// Encrypt a message using NIP-04 or NIP-44.
///
/// * `private_key` - sender's bech32 `nsec`
/// * `public_key`  - recipient's bech32 `npub`
/// * `message`     - plaintext to encrypt
/// * `algorithm`   - `"nip04"` or `"nip44"` (defaults to `"nip44"`)
#[wasm_bindgen]
pub fn encrypt_message(
    private_key: &str,
    public_key: &str,
    message: &str,
    algorithm: Option<String>,
) -> Result<String, JsValue> {
    let secret_key = SecretKey::from_bech32(private_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid private key: {}", e)))?;
    let pub_key = PublicKey::from_bech32(public_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid public key: {}", e)))?;

    let algo = algorithm.as_deref().unwrap_or("nip44");

    match algo {
        "nip04" => {
            let encrypted = nip04::encrypt(&secret_key, &pub_key, message)
                .map_err(|e| JsValue::from_str(&format!("NIP-04 encryption failed: {}", e)))?;
            Ok(encrypted)
        }
        _ => {
            // Default to NIP-44 for "nip44" or any unknown algorithm string
            let encrypted = nip44::encrypt(
                &secret_key,
                &pub_key,
                message,
                nip44::Version::default(),
            )
            .map_err(|e| JsValue::from_str(&format!("NIP-44 encryption failed: {}", e)))?;
            Ok(encrypted)
        }
    }
}

/// Decrypt a message, auto-detecting NIP-44 or NIP-04 format.
///
/// Tries NIP-44 first (newer standard), then falls back to NIP-04.
#[wasm_bindgen]
pub fn decrypt_message(
    private_key: &str,
    public_key: &str,
    encrypted_message: &str,
) -> Result<String, JsValue> {
    let secret_key = SecretKey::from_bech32(private_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid private key: {}", e)))?;
    let pub_key = PublicKey::from_bech32(public_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid public key: {}", e)))?;

    // Try NIP-44 first
    if let Ok(decrypted) = nip44::decrypt(&secret_key, &pub_key, encrypted_message) {
        return Ok(decrypted);
    }

    // Fall back to NIP-04
    if let Ok(decrypted) = nip04::decrypt(&secret_key, &pub_key, encrypted_message) {
        return Ok(decrypted);
    }

    Err(JsValue::from_str(&format!(
        "Failed to decrypt with both NIP-04 and NIP-44. Content length: {}, Has '?iv=': {}",
        encrypted_message.len(),
        encrypted_message.contains("?iv=")
    )))
}

/// Detect the encryption format of a ciphertext string.
///
/// Returns `"nip04"`, `"nip44"`, or `"unknown"`.
#[wasm_bindgen]
pub fn detect_encryption_format(content: &str) -> String {
    if content.is_empty() {
        return "unknown".to_string();
    }

    // Remove ASCII armor if present
    let cleaned = content
        .replace("-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
        .replace("-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
        .replace("-----BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE-----", "")
        .replace("-----END NOSTR NIP-44 ENCRYPTED MESSAGE-----", "");
    let clean_content = cleaned.trim();

    // Check for NIP-04 format: base64?iv=base64
    if clean_content.contains("?iv=") {
        if let Some(pos) = clean_content.find("?iv=") {
            let before_iv = &clean_content[..pos];
            let after_iv = &clean_content[pos + 4..];
            if before_iv
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
                && after_iv
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
            {
                return "nip04".to_string();
            }
        }
    }

    // Check for NIP-44 format: base64 that decodes to a versioned payload (version byte 1 or 2)
    if clean_content
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    {
        if let Ok(decoded) = general_purpose::STANDARD.decode(clean_content) {
            if !decoded.is_empty() {
                let version_byte = decoded[0];
                if version_byte == 1 || version_byte == 2 {
                    return "nip44".to_string();
                }
            }
        }
    }

    "unknown".to_string()
}

// ---------------------------------------------------------------------------
// Schnorr Signatures
// ---------------------------------------------------------------------------

/// Sign arbitrary data with a Schnorr signature.
///
/// Returns the 64-byte signature as a hex string.
#[wasm_bindgen]
pub fn sign_data(private_key: &str, data: &str) -> Result<String, JsValue> {
    use secp256k1::{Keypair, Message, Secp256k1, SecretKey as SecpSecretKey};

    let secret_key = SecretKey::from_bech32(private_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid private key: {}", e)))?;
    let secp = Secp256k1::new();

    let secp_secret_key = SecpSecretKey::from_slice(&secret_key.secret_bytes())
        .map_err(|e| JsValue::from_str(&format!("secp256k1 key error: {}", e)))?;

    // SHA-256 hash of the data
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let message_hash = hasher.finalize();

    let message = Message::from_digest_slice(&message_hash)
        .map_err(|e| JsValue::from_str(&format!("Message hash error: {}", e)))?;

    let keypair = Keypair::from_secret_key(&secp, &secp_secret_key);

    // Use getrandom-backed OsRng for auxiliary randomness
    let mut rng = secp256k1::rand::rngs::OsRng;
    let signature = secp.sign_schnorr_with_rng(&message, &keypair, &mut rng);

    Ok(hex::encode(signature.as_ref()))
}

/// Verify a Schnorr signature.
///
/// * `public_key` - bech32 `npub`
/// * `signature`  - hex-encoded 64-byte Schnorr signature
/// * `data`       - the original data that was signed
#[wasm_bindgen]
pub fn verify_signature(public_key: &str, signature: &str, data: &str) -> Result<bool, JsValue> {
    use secp256k1::{Message, Secp256k1, XOnlyPublicKey};

    let pubkey = PublicKey::from_bech32(public_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid public key: {}", e)))?;
    let secp = Secp256k1::verification_only();

    // Decode hex signature
    let sig_bytes = hex::decode(signature)
        .map_err(|e| JsValue::from_str(&format!("Invalid hex signature: {}", e)))?;
    if sig_bytes.len() != 64 {
        return Ok(false);
    }
    let sig = secp256k1::schnorr::Signature::from_slice(&sig_bytes)
        .map_err(|e| JsValue::from_str(&format!("Invalid signature format: {}", e)))?;

    // SHA-256 hash of the data
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let message_hash = hasher.finalize();

    let message = Message::from_digest_slice(&message_hash)
        .map_err(|e| JsValue::from_str(&format!("Message hash error: {}", e)))?;

    // Convert nostr PublicKey -> secp256k1 XOnlyPublicKey
    let pubkey_hex = pubkey.to_hex();
    let pubkey_bytes = hex::decode(&pubkey_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid pubkey hex: {}", e)))?;
    if pubkey_bytes.len() != 32 {
        return Ok(false);
    }
    let xonly_pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes)
        .map_err(|e| JsValue::from_str(&format!("Invalid XOnly public key: {}", e)))?;

    match secp.verify_schnorr(&sig, &message, &xonly_pubkey) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

// ---------------------------------------------------------------------------
// Settings Encryption (AES-256-GCM with key derived from nsec)
// ---------------------------------------------------------------------------

/// Derive a 32-byte symmetric key from a bech32 `nsec` private key.
/// Uses SHA-256 with a domain-separation prefix.
fn derive_encryption_key(private_key: &str) -> Result<[u8; 32], JsValue> {
    let secret_key = SecretKey::from_bech32(private_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid private key: {}", e)))?;
    let secret_bytes = secret_key.secret_bytes();

    let mut hasher = Sha256::new();
    hasher.update(b"nostr-mail-settings-encryption-v1:");
    hasher.update(&secret_bytes);
    let hash = hasher.finalize();

    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    Ok(key)
}

/// Encrypt a settings value using AES-256-GCM with a key derived from the
/// user's private key. Returns a base64-encoded string (nonce + ciphertext).
///
/// Returns an empty string if the input is empty.
#[wasm_bindgen]
pub fn encrypt_setting_value(private_key: &str, value: &str) -> Result<String, JsValue> {
    if value.is_empty() {
        return Ok(String::new());
    }

    let key_bytes = derive_encryption_key(private_key)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, value.as_bytes())
        .map_err(|e| JsValue::from_str(&format!("AES-GCM encryption failed: {}", e)))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(general_purpose::STANDARD.encode(&combined))
}

/// Decrypt a settings value previously encrypted with `encrypt_setting_value`.
///
/// Returns an empty string if the input is empty.
#[wasm_bindgen]
pub fn decrypt_setting_value(private_key: &str, encrypted_value: &str) -> Result<String, JsValue> {
    if encrypted_value.is_empty() {
        return Ok(String::new());
    }

    let combined = general_purpose::STANDARD
        .decode(encrypted_value)
        .map_err(|e| JsValue::from_str(&format!("Base64 decode failed: {}", e)))?;

    if combined.len() < 12 {
        return Err(JsValue::from_str("Encrypted value too short"));
    }

    let nonce = Nonce::from_slice(&combined[0..12]);
    let ciphertext = &combined[12..];

    let key_bytes = derive_encryption_key(private_key)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| JsValue::from_str(&format!("AES-GCM decryption failed: {}", e)))?;

    String::from_utf8(plaintext)
        .map_err(|e| JsValue::from_str(&format!("UTF-8 decode failed: {}", e)))
}

// ---------------------------------------------------------------------------
// Native-only test helpers (run with `cargo test`, not wasm-bindgen-test)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = generate_keypair().unwrap();
        assert!(!kp.private_key().is_empty());
        assert!(!kp.public_key().is_empty());
        assert!(validate_private_key(&kp.private_key()));
        assert!(validate_public_key(&kp.public_key()));
    }

    #[test]
    fn test_public_key_derivation() {
        let kp = generate_keypair().unwrap();
        let derived = get_public_key_from_private(&kp.private_key()).unwrap();
        assert_eq!(kp.public_key(), derived);
    }

    #[test]
    fn test_nip44_round_trip() {
        let kp1 = generate_keypair().unwrap();
        let kp2 = generate_keypair().unwrap();
        let msg = "Hello NIP-44!";
        let enc = encrypt_message(&kp1.private_key(), &kp2.public_key(), msg, Some("nip44".into())).unwrap();
        let dec = decrypt_message(&kp2.private_key(), &kp1.public_key(), &enc).unwrap();
        assert_eq!(msg, dec);
    }

    #[test]
    fn test_nip04_round_trip() {
        let kp1 = generate_keypair().unwrap();
        let kp2 = generate_keypair().unwrap();
        let msg = "Hello NIP-04!";
        let enc = encrypt_message(&kp1.private_key(), &kp2.public_key(), msg, Some("nip04".into())).unwrap();
        let dec = decrypt_message(&kp2.private_key(), &kp1.public_key(), &enc).unwrap();
        assert_eq!(msg, dec);
    }

    #[test]
    fn test_sign_verify_round_trip() {
        let kp = generate_keypair().unwrap();
        let data = "Sign this message";
        let sig = sign_data(&kp.private_key(), data).unwrap();
        assert!(verify_signature(&kp.public_key(), &sig, data).unwrap());
        // Tampered data should fail
        assert!(!verify_signature(&kp.public_key(), &sig, "tampered").unwrap());
    }

    #[test]
    fn test_settings_round_trip() {
        let kp = generate_keypair().unwrap();
        let value = "my-secret-setting";
        let enc = encrypt_setting_value(&kp.private_key(), value).unwrap();
        let dec = decrypt_setting_value(&kp.private_key(), &enc).unwrap();
        assert_eq!(value, dec);
    }

    #[test]
    fn test_settings_empty_string() {
        let kp = generate_keypair().unwrap();
        let enc = encrypt_setting_value(&kp.private_key(), "").unwrap();
        assert_eq!(enc, "");
        let dec = decrypt_setting_value(&kp.private_key(), "").unwrap();
        assert_eq!(dec, "");
    }

    #[test]
    fn test_detect_nip04() {
        // Synthetic NIP-04 content: base64?iv=base64
        let nip04_content = "dGVzdA==?iv=dGVzdA==";
        assert_eq!(detect_encryption_format(nip04_content), "nip04");
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(detect_encryption_format(""), "unknown");
        assert_eq!(detect_encryption_format("just plain text with spaces"), "unknown");
    }

    #[test]
    fn test_key_validation() {
        assert!(!validate_private_key("not-a-key"));
        assert!(!validate_public_key("not-a-key"));

        let kp = generate_keypair().unwrap();
        assert!(validate_private_key(&kp.private_key()));
        assert!(validate_public_key(&kp.public_key()));
    }
}
