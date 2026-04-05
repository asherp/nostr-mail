use anyhow::Result;
use nostr_sdk::prelude::*;
use crate::types::KeyPair;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};

pub fn generate_keypair() -> Result<KeyPair> {
    // Generate a proper secp256k1 keypair using nostr-sdk
    let keys = Keys::generate();
    
    Ok(KeyPair {
        private_key: keys.secret_key().to_bech32()?,
        public_key: keys.public_key().to_bech32()?,
    })
}

pub fn encrypt_message(private_key: &str, public_key: &str, message: &str, algorithm: Option<&str>) -> Result<String> {
    // Parse the keys from bech32 format
    let secret_key = SecretKey::from_bech32(private_key)?;
    let public_key = PublicKey::from_bech32(public_key)?;
    
    // Determine encryption algorithm (default to NIP-44)
    let algorithm = algorithm.unwrap_or("nip44");
    println!("[CRYPTO] Encrypting message with algorithm: {} (message length: {} bytes)", algorithm, message.len());
    
    match algorithm {
        "nip04" => {
            // Use NIP-04 encryption (legacy)
            println!("[CRYPTO] Using NIP-04 encryption (legacy format)");
            let encrypted = nip04::encrypt(&secret_key, &public_key, message)?;
            println!("[CRYPTO] NIP-04 encryption successful, encrypted length: {} chars", encrypted.len());
            Ok(encrypted)
        },
        "nip44" => {
            // Use NIP-44 encryption (the proper way)
            println!("[CRYPTO] Using NIP-44 encryption (modern format)");
            let encrypted = nip44::encrypt(
                &secret_key,
                &public_key,
                message,
                nip44::Version::default()
            )?;
            println!("[CRYPTO] NIP-44 encryption successful, encrypted length: {} chars", encrypted.len());
            Ok(encrypted)
        },
        _ => {
            // Default to NIP-44 for unknown algorithms
            println!("[CRYPTO] Unknown algorithm '{}', defaulting to NIP-44", algorithm);
            let encrypted = nip44::encrypt(
                &secret_key,
                &public_key,
                message,
                nip44::Version::default()
            )?;
            println!("[CRYPTO] NIP-44 encryption successful (default), encrypted length: {} chars", encrypted.len());
            Ok(encrypted)
        }
    }
}

pub fn decrypt_message(private_key: &str, public_key: &str, encrypted_message: &str) -> Result<String> {
    // Parse the keys from bech32 format
    let secret_key = SecretKey::from_bech32(private_key)?;
    let public_key = PublicKey::from_bech32(public_key)?;
    
    // Try NIP-44 first (newer standard)
    match nip44::decrypt(&secret_key, &public_key, encrypted_message) {
        Ok(decrypted) => {
            println!("[CRYPTO] Successfully decrypted with NIP-44");
            return Ok(decrypted);
        }
        Err(e) => {
            println!("[CRYPTO] NIP-44 decryption failed: {:?}, trying NIP-04", e);
        }
    }
    
    // Try NIP-04 format: base64(encrypted_content)?iv=base64(iv)
    match nip04::decrypt(&secret_key, &public_key, encrypted_message) {
        Ok(decrypted) => {
            println!("[CRYPTO] Successfully decrypted with NIP-04");
            return Ok(decrypted);
        }
        Err(e) => {
            println!("[CRYPTO] NIP-04 decryption also failed: {:?}", e);
        }
    }
    
    Err(anyhow::anyhow!("Failed to decrypt with both NIP-04 and NIP-44. Content length: {}, Has '?iv=': {}", 
        encrypted_message.len(), 
        encrypted_message.contains("?iv=")))
}

/// Detect encryption format from encrypted content
/// Returns "nip04", "nip44", or "unknown"
pub fn detect_encryption_format(content: &str) -> String {
    if content.is_empty() {
        return "unknown".to_string();
    }

    // Remove ASCII armor if present (for body content)
    let cleaned = content
        .replace("-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
        .replace("-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----", "")
        .replace("-----BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE-----", "")
        .replace("-----END NOSTR NIP-44 ENCRYPTED MESSAGE-----", "");
    let clean_content = cleaned.trim();

    // Check for NIP-04 format: base64?iv=base64
    if clean_content.contains("?iv=") {
        // Split on ?iv= to verify format
        if let Some(pos) = clean_content.find("?iv=") {
            let before_iv = &clean_content[..pos];
            let after_iv = &clean_content[pos + 4..];
            // Check if both parts are valid base64-like strings
            if before_iv.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=') &&
               after_iv.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=') {
                return "nip04".to_string();
            }
        }
    }

    // Check for NIP-44 format: versioned format (starts with version byte 1 or 2)
    // NIP-44 is base64 encoded, and when decoded, the first byte indicates version
    if clean_content.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=') {
        if let Ok(decoded) = general_purpose::STANDARD.decode(&clean_content) {
            if !decoded.is_empty() {
                let version_byte = decoded[0];
                // NIP-44 v1 uses version byte 1 (0x01), v2 uses version byte 2 (0x02)
                if version_byte == 1 || version_byte == 2 {
                    return "nip44".to_string();
                }
            }
        }
    }

    "unknown".to_string()
}

// Additional utility functions for working with nostr-sdk

pub fn get_public_key_from_private(private_key: &str) -> Result<String> {
    let secret_key = SecretKey::from_bech32(private_key)?;
    let keys = Keys::new(secret_key);
    Ok(keys.public_key().to_bech32()?)
}

pub fn validate_private_key(private_key: &str) -> Result<bool> {
    match SecretKey::from_bech32(private_key) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

pub fn validate_public_key(public_key: &str) -> Result<bool> {
    match PublicKey::from_bech32(public_key) {
        Ok(_) => Ok(true),
        Err(e) => {
            // Log the error for debugging
            eprintln!("Public key validation failed for '{}': {:?}", public_key, e);
            Ok(false)
        }
    }
}

/// Sign arbitrary data (email body) using nostr schnorr signature
pub fn sign_data(private_key: &str, data: &str) -> Result<String> {
    use ::secp256k1::{Message, Secp256k1, SecretKey as SecpSecretKey, Keypair, rand::rngs::OsRng};
    use sha2::{Sha256, Digest};
    
    let secret_key = SecretKey::from_bech32(private_key)?;
    let secp = Secp256k1::new();
    
    // Convert nostr SecretKey to secp256k1 SecretKey
    // SecretKey has secret_bytes() method that returns [u8; 32]
    let secp_secret_key = SecpSecretKey::from_slice(&secret_key.secret_bytes())?;
    
    // Hash the data using SHA256
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let message_hash = hasher.finalize();
    let message = Message::from_digest_slice(&message_hash)?;
    
    // Create a keypair from the secret key and sign the message
    let keypair = Keypair::from_secret_key(&secp, &secp_secret_key);
    let mut rng = OsRng;
    let signature = secp.sign_schnorr_with_rng(&message, &keypair, &mut rng);
    
    // Return signature as hex string (schnorr signatures are 64 bytes)
    Ok(hex::encode(signature.as_ref()))
}

/// Sign raw binary data using nostr schnorr signature
pub fn sign_data_bytes(private_key: &str, data: &[u8]) -> Result<String> {
    use ::secp256k1::{Message, Secp256k1, SecretKey as SecpSecretKey, Keypair, rand::rngs::OsRng};
    use sha2::{Sha256, Digest};

    let secret_key = SecretKey::from_bech32(private_key)?;
    let secp = Secp256k1::new();
    let secp_secret_key = SecpSecretKey::from_slice(&secret_key.secret_bytes())?;

    let mut hasher = Sha256::new();
    hasher.update(data);
    let message_hash = hasher.finalize();
    let message = Message::from_digest_slice(&message_hash)?;

    let keypair = Keypair::from_secret_key(&secp, &secp_secret_key);
    let mut rng = OsRng;
    let signature = secp.sign_schnorr_with_rng(&message, &keypair, &mut rng);

    Ok(hex::encode(signature.as_ref()))
}

/// Verify a signature for raw binary data
pub fn verify_signature_bytes(public_key: &str, signature: &str, data: &[u8]) -> Result<bool> {
    use ::secp256k1::{Message, Secp256k1, XOnlyPublicKey};
    use sha2::{Sha256, Digest};

    let pubkey = PublicKey::from_bech32(public_key)?;
    let secp = Secp256k1::verification_only();

    let sig_bytes = hex::decode(signature).map_err(|e| anyhow::anyhow!("Invalid hex signature: {}", e))?;
    if sig_bytes.len() != 64 {
        return Ok(false);
    }
    let sig = ::secp256k1::schnorr::Signature::from_slice(&sig_bytes)?;

    let mut hasher = Sha256::new();
    hasher.update(data);
    let message_hash = hasher.finalize();
    let message = Message::from_digest_slice(&message_hash)?;

    let pubkey_hex = pubkey.to_hex();
    let pubkey_bytes = hex::decode(&pubkey_hex).map_err(|e| anyhow::anyhow!("Invalid pubkey hex: {}", e))?;
    if pubkey_bytes.len() != 32 {
        return Ok(false);
    }
    let xonly_pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes)?;

    match secp.verify_schnorr(&sig, &message, &xonly_pubkey) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Verify a signature for arbitrary data (email body)
pub fn verify_signature(public_key: &str, signature: &str, data: &str) -> Result<bool> {
    use ::secp256k1::{Message, Secp256k1, XOnlyPublicKey};
    use sha2::{Sha256, Digest};
    
    let pubkey = PublicKey::from_bech32(public_key)?;
    let secp = Secp256k1::verification_only();
    
    // Parse signature from hex (schnorr signatures are 64 bytes = 128 hex chars)
    let sig_bytes = hex::decode(signature).map_err(|e| anyhow::anyhow!("Invalid hex signature: {}", e))?;
    if sig_bytes.len() != 64 {
        return Ok(false);
    }
    let sig = ::secp256k1::schnorr::Signature::from_slice(&sig_bytes)?;
    
    // Hash the data using SHA256
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let message_hash = hasher.finalize();
    let message = Message::from_digest_slice(&message_hash)?;
    
    // Get the XOnlyPublicKey from nostr PublicKey
    // PublicKey.to_hex() returns the 64-character hex string of the XOnlyPublicKey
    let pubkey_hex = pubkey.to_hex();
    let pubkey_bytes = hex::decode(&pubkey_hex).map_err(|e| anyhow::anyhow!("Invalid pubkey hex: {}", e))?;
    if pubkey_bytes.len() != 32 {
        return Ok(false);
    }
    let xonly_pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes)?;
    
    // Verify the signature using XOnlyPublicKey directly
    match secp.verify_schnorr(&sig, &message, &xonly_pubkey) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Derive a symmetric encryption key from a private key
/// Uses SHA256 to derive a 32-byte key from the private key bytes
fn derive_encryption_key(private_key: &str) -> Result<[u8; 32]> {
    let secret_key = SecretKey::from_bech32(private_key)?;
    let secret_bytes = secret_key.secret_bytes();
    
    // Use SHA256 to derive a key from the private key bytes
    // Add a context string to ensure this key is only used for settings encryption
    let mut hasher = Sha256::new();
    hasher.update(b"nostr-mail-settings-encryption-v1:");
    hasher.update(&secret_bytes);
    let hash = hasher.finalize();
    
    // Convert hash to fixed-size array
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    Ok(key)
}

/// Encrypt sensitive data using a key derived from the user's private key
/// This allows the user to encrypt their own data using their own keypair
pub fn encrypt_setting_value(private_key: &str, value: &str) -> Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    
    // Derive encryption key from private key
    let key_bytes = derive_encryption_key(private_key)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    
    // Generate a random nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    
    // Encrypt the value
    let ciphertext = cipher.encrypt(&nonce, value.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
    
    // Combine nonce and ciphertext, encode as base64
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    
    Ok(general_purpose::STANDARD.encode(&combined))
}

/// Decrypt sensitive data using a key derived from the user's private key
pub fn decrypt_setting_value(private_key: &str, encrypted_value: &str) -> Result<String> {
    if encrypted_value.is_empty() {
        return Ok(String::new());
    }
    
    // Decode base64
    let combined = general_purpose::STANDARD.decode(encrypted_value)
        .map_err(|e| anyhow::anyhow!("Base64 decode failed: {}", e))?;
    
    if combined.len() < 12 {
        return Err(anyhow::anyhow!("Encrypted value too short"));
    }
    
    // Extract nonce (first 12 bytes) and ciphertext (rest)
    let nonce = Nonce::from_slice(&combined[0..12]);
    let ciphertext = &combined[12..];
    
    // Derive encryption key from private key
    let key_bytes = derive_encryption_key(private_key)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    
    // Decrypt the value
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
    
    String::from_utf8(plaintext)
        .map_err(|e| anyhow::anyhow!("UTF-8 decode failed: {}", e))
}

/// AES-256-GCM decrypt with raw key bytes (not derived from private key).
/// Input format: 12-byte nonce || ciphertext+tag (same layout as decrypt_setting_value).
pub fn aes_gcm_decrypt_raw(key_bytes: &[u8], encrypted_data: &[u8]) -> Result<Vec<u8>> {
    if key_bytes.len() != 32 {
        return Err(anyhow::anyhow!("AES key must be 32 bytes, got {}", key_bytes.len()));
    }
    if encrypted_data.len() < 12 {
        return Err(anyhow::anyhow!("Encrypted data too short (need at least 12-byte nonce)"));
    }
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&encrypted_data[..12]);
    let ciphertext = &encrypted_data[12..];
    cipher.decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("AES-GCM decryption failed: {}", e))
}

/// AES-256-GCM decrypt + remove padding.
/// After decryption, first 4 bytes are original size (little-endian u32),
/// followed by the original data padded to 64 KiB boundaries.
pub fn aes_gcm_decrypt_padded(key_bytes: &[u8], encrypted_data: &[u8]) -> Result<Vec<u8>> {
    let plaintext = aes_gcm_decrypt_raw(key_bytes, encrypted_data)?;
    if plaintext.len() < 4 {
        return Err(anyhow::anyhow!("Decrypted data too short for padding header"));
    }
    let original_size = u32::from_le_bytes([plaintext[0], plaintext[1], plaintext[2], plaintext[3]]) as usize;
    if 4 + original_size > plaintext.len() {
        return Err(anyhow::anyhow!("Original size {} exceeds plaintext length {}", original_size, plaintext.len() - 4));
    }
    Ok(plaintext[4..4 + original_size].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_keypair_generation() {
        let keypair = generate_keypair().unwrap();
        assert!(!keypair.private_key.is_empty());
        assert!(!keypair.public_key.is_empty());
        
        // Verify the keys are valid
        assert!(validate_private_key(&keypair.private_key).unwrap());
        assert!(validate_public_key(&keypair.public_key).unwrap());
    }
    
    #[test]
    fn test_encryption_decryption() {
        let keypair1 = generate_keypair().unwrap();
        let keypair2 = generate_keypair().unwrap();
        
        let message = "Hello, this is a test message!";
        
        let encrypted = encrypt_message(&keypair1.private_key, &keypair2.public_key, message, None).unwrap();
        let decrypted = decrypt_message(&keypair2.private_key, &keypair1.public_key, &encrypted).unwrap();
        
        assert_eq!(message, decrypted);
    }
    
    #[test]
    fn test_public_key_derivation() {
        let keypair = generate_keypair().unwrap();
        let derived_public = get_public_key_from_private(&keypair.private_key).unwrap();
        assert_eq!(keypair.public_key, derived_public);
    }
    
    #[test]
    fn test_user_public_key() {
        // Test the specific public key provided by the user
        let user_pubkey = "npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r";
        let is_valid = validate_public_key(user_pubkey).unwrap();
        println!("User public key validation result: {}", is_valid);
        assert!(is_valid, "User public key should be valid");
    }

    #[test]
    fn test_aes_gcm_decrypt_raw_roundtrip() {
        // Encrypt then decrypt with raw key
        let key_bytes = [0x42u8; 32];
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let plaintext = b"Hello, nostr-mail!";
        let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).unwrap();

        // Combine nonce + ciphertext (same format as encrypt_setting_value)
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);

        let decrypted = aes_gcm_decrypt_raw(&key_bytes, &combined).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_gcm_decrypt_raw_wrong_key() {
        let key_bytes = [0x42u8; 32];
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, b"secret".as_ref()).unwrap();

        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);

        // Decrypt with wrong key
        let wrong_key = [0xFFu8; 32];
        assert!(aes_gcm_decrypt_raw(&wrong_key, &combined).is_err());
    }

    #[test]
    fn test_aes_gcm_decrypt_raw_bad_key_length() {
        assert!(aes_gcm_decrypt_raw(&[0u8; 16], &[0u8; 28]).is_err());
    }

    #[test]
    fn test_aes_gcm_decrypt_padded_roundtrip() {
        // Simulate manifest attachment format: encrypt [size_le_u32 || data || padding]
        let key_bytes = [0x42u8; 32];
        let original_data = b"This is the original file content";
        let original_size = original_data.len() as u32;

        // Build padded plaintext: 4-byte LE size + data + zero padding to 64 KiB
        let mut padded = Vec::new();
        padded.extend_from_slice(&original_size.to_le_bytes());
        padded.extend_from_slice(original_data);
        // Pad to next 64 KiB boundary
        let block = 65536;
        let total = 4 + original_data.len();
        let pad_len = if total % block == 0 { 0 } else { block - (total % block) };
        padded.resize(padded.len() + pad_len, 0u8);

        // Encrypt
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, padded.as_ref()).unwrap();
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);

        // Decrypt with padding removal
        let decrypted = aes_gcm_decrypt_padded(&key_bytes, &combined).unwrap();
        assert_eq!(decrypted, original_data);
    }
} 