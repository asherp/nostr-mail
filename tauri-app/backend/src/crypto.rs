use anyhow::Result;
use nostr_sdk::prelude::*;
use crate::types::KeyPair;

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