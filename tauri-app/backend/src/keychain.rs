use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SERVICE_NAME: &str = "nostr-mail";
const VAULT_ACCOUNT: &str = "vault";

/// All private keys stored as a single JSON blob in one keychain entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Vault {
    /// pubkey -> private key
    keys: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct KeychainManager;

impl KeychainManager {
    pub fn new() -> Self {
        KeychainManager
    }

    fn entry() -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE_NAME, VAULT_ACCOUNT)
    }

    fn load_vault(&self) -> Result<Vault, String> {
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse vault: {}", e)),
            Err(keyring::Error::NoEntry) => Ok(Vault::default()),
            Err(e) => Err(format!("Keychain read error: {}", e)),
        }
    }

    fn save_vault(&self, vault: &Vault) -> Result<(), String> {
        let json = serde_json::to_string(vault)
            .map_err(|e| format!("Failed to serialize vault: {}", e))?;
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        entry.set_password(&json).map_err(|e| format!("Keychain store error: {}", e))
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
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Keychain delete error: {}", e)),
        }
    }
}
