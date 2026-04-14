use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE_NAME: &str = "nostr-mail";
const VAULT_ACCOUNT: &str = "vault";

/// All private keys stored as a single JSON blob in one keychain entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Vault {
    /// pubkey -> private key
    keys: HashMap<String, String>,
}

/// Manages private keys in the OS keychain with an in-memory cache.
/// The cache ensures only one keychain access prompt per app launch.
#[derive(Debug)]
pub struct KeychainManager {
    cache: Mutex<Option<Vault>>,
}

impl Clone for KeychainManager {
    fn clone(&self) -> Self {
        let cached = self.cache.lock().unwrap().clone();
        KeychainManager {
            cache: Mutex::new(cached),
        }
    }
}

impl KeychainManager {
    pub fn new() -> Self {
        KeychainManager {
            cache: Mutex::new(None),
        }
    }

    fn entry() -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE_NAME, VAULT_ACCOUNT)
    }

    /// Read the vault from the OS keychain (bypassing cache).
    fn read_vault_from_keychain() -> Result<Vault, String> {
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse vault: {}", e)),
            Err(keyring::Error::NoEntry) => Ok(Vault::default()),
            Err(e) => Err(format!("Keychain read error: {}", e)),
        }
    }

    /// Get the vault, reading from keychain only on first access.
    fn load_vault(&self) -> Result<Vault, String> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(ref vault) = *cache {
            return Ok(vault.clone());
        }
        let vault = Self::read_vault_from_keychain()?;
        *cache = Some(vault.clone());
        Ok(vault)
    }

    /// Write the vault to the OS keychain and update the cache.
    fn save_vault(&self, vault: &Vault) -> Result<(), String> {
        let json = serde_json::to_string(vault)
            .map_err(|e| format!("Failed to serialize vault: {}", e))?;
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        entry.set_password(&json).map_err(|e| format!("Keychain store error: {}", e))?;
        *self.cache.lock().unwrap() = Some(vault.clone());
        Ok(())
    }

    pub fn store_key(&self, public_key: &str, private_key: &str) -> Result<(), String> {
        let mut vault = self.load_vault()?;
        vault.keys.insert(public_key.to_string(), private_key.to_string());
        self.save_vault(&vault)
    }

    pub fn get_key(&self, public_key: &str) -> Result<Option<String>, String> {
        let vault = self.load_vault()?;
        Ok(vault.keys.get(public_key).cloned())
    }

    pub fn delete_key(&self, public_key: &str) -> Result<(), String> {
        let mut vault = self.load_vault()?;
        vault.keys.remove(public_key);
        self.save_vault(&vault)
    }

    pub fn list_pubkeys(&self) -> Result<Vec<String>, String> {
        let vault = self.load_vault()?;
        Ok(vault.keys.keys().cloned().collect())
    }

    pub fn clear_all(&self) -> Result<(), String> {
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.delete_credential() {
            Ok(()) => {
                *self.cache.lock().unwrap() = Some(Vault::default());
                Ok(())
            },
            Err(keyring::Error::NoEntry) => {
                *self.cache.lock().unwrap() = Some(Vault::default());
                Ok(())
            },
            Err(e) => Err(format!("Keychain delete error: {}", e)),
        }
    }
}
