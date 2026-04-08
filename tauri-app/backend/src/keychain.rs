use std::collections::HashMap;

const SERVICE_NAME: &str = "nostr-mail";
const ACCOUNT_LIST_KEY: &str = "_account_list";
const ACCOUNT_LABELS_KEY: &str = "_account_labels";

#[derive(Debug, Clone)]
pub struct KeychainManager;

impl KeychainManager {
    pub fn new() -> Self {
        KeychainManager
    }

    fn entry(account: &str) -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE_NAME, account)
    }

    pub fn store_key(&self, public_key: &str, private_key: &str) -> Result<(), String> {
        let entry = Self::entry(public_key).map_err(|e| format!("Keychain entry error: {}", e))?;
        entry.set_password(private_key).map_err(|e| format!("Keychain store error: {}", e))?;

        // Update account list
        let mut accounts = self.list_accounts().unwrap_or_default();
        if !accounts.contains(&public_key.to_string()) {
            accounts.push(public_key.to_string());
            self.save_account_list(&accounts)?;
        }

        Ok(())
    }

    pub fn get_key(&self, public_key: &str) -> Result<Option<String>, String> {
        let entry = Self::entry(public_key).map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Keychain read error: {}", e)),
        }
    }

    pub fn delete_key(&self, public_key: &str) -> Result<(), String> {
        let entry = Self::entry(public_key).map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.delete_credential() {
            Ok(()) => {},
            Err(keyring::Error::NoEntry) => {}, // already gone
            Err(e) => return Err(format!("Keychain delete error: {}", e)),
        }

        // Update account list
        let mut accounts = self.list_accounts().unwrap_or_default();
        accounts.retain(|a| a != public_key);
        self.save_account_list(&accounts)?;

        // Remove label
        let mut labels = self.get_labels().unwrap_or_default();
        if labels.remove(public_key).is_some() {
            self.save_labels(&labels)?;
        }

        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<String>, String> {
        let entry = Self::entry(ACCOUNT_LIST_KEY).map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse account list: {}", e)),
            Err(keyring::Error::NoEntry) => Ok(Vec::new()),
            Err(e) => Err(format!("Keychain read error: {}", e)),
        }
    }

    pub fn get_labels(&self) -> Result<HashMap<String, String>, String> {
        let entry = Self::entry(ACCOUNT_LABELS_KEY).map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse labels: {}", e)),
            Err(keyring::Error::NoEntry) => Ok(HashMap::new()),
            Err(e) => Err(format!("Keychain read error: {}", e)),
        }
    }

    pub fn set_label(&self, public_key: &str, label: &str) -> Result<(), String> {
        let mut labels = self.get_labels().unwrap_or_default();
        if label.is_empty() {
            labels.remove(public_key);
        } else {
            labels.insert(public_key.to_string(), label.to_string());
        }
        self.save_labels(&labels)
    }

    pub fn clear_all(&self) -> Result<(), String> {
        let accounts = self.list_accounts().unwrap_or_default();
        for pubkey in &accounts {
            let entry = Self::entry(pubkey).map_err(|e| format!("Keychain entry error: {}", e))?;
            let _ = entry.delete_credential(); // ignore errors for individual deletions
        }

        // Delete metadata entries
        for key in &[ACCOUNT_LIST_KEY, ACCOUNT_LABELS_KEY] {
            let entry = Self::entry(key).map_err(|e| format!("Keychain entry error: {}", e))?;
            let _ = entry.delete_credential();
        }

        Ok(())
    }

    fn save_account_list(&self, accounts: &[String]) -> Result<(), String> {
        let json = serde_json::to_string(accounts)
            .map_err(|e| format!("Failed to serialize account list: {}", e))?;
        let entry = Self::entry(ACCOUNT_LIST_KEY).map_err(|e| format!("Keychain entry error: {}", e))?;
        entry.set_password(&json).map_err(|e| format!("Keychain store error: {}", e))
    }

    fn save_labels(&self, labels: &HashMap<String, String>) -> Result<(), String> {
        let json = serde_json::to_string(labels)
            .map_err(|e| format!("Failed to serialize labels: {}", e))?;
        let entry = Self::entry(ACCOUNT_LABELS_KEY).map_err(|e| format!("Keychain entry error: {}", e))?;
        entry.set_password(&json).map_err(|e| format!("Keychain store error: {}", e))
    }
}
