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
    
    match algorithm {
        "nip04" => {
            // Use NIP-04 encryption (legacy)
            let encrypted = nip04::encrypt(&secret_key, &public_key, message)?;
            Ok(encrypted)
        },
        "nip44" => {
            // Use NIP-44 encryption (the proper way)
            let encrypted = nip44::encrypt(
                &secret_key,
                &public_key,
                message,
                nip44::Version::default()
            )?;
            Ok(encrypted)
        },
        _ => {
            // Default to NIP-44 for unknown algorithms
            let encrypted = nip44::encrypt(
                &secret_key,
                &public_key,
                message,
                nip44::Version::default()
            )?;
            Ok(encrypted)
        }
    }
}

pub fn decrypt_message(private_key: &str, public_key: &str, encrypted_message: &str) -> Result<String> {
    // Parse the keys from bech32 format
    let secret_key = SecretKey::from_bech32(private_key)?;
    let public_key = PublicKey::from_bech32(public_key)?;
    
    // Use NIP-44 decryption (the proper way)
    let decrypted = nip44::decrypt(
        &secret_key,
        &public_key,
        encrypted_message
    )?;
    
    Ok(decrypted)
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
} 