use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::database::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relay {
    pub url: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedImage {
    pub data_url: String,
    pub timestamp: u64, // Unix timestamp when cached
}

#[derive(Debug)]
pub struct AppState {
    pub relays: Arc<Mutex<Vec<Relay>>>,
    pub image_cache: Arc<Mutex<HashMap<String, CachedImage>>>, // pubkey -> cached image
    pub database: Arc<Mutex<Option<Database>>>,
}

impl AppState {
    pub fn new() -> Self {
        let default_relays = vec![
            Relay {
                url: "wss://nostr-pub.wellorder.net".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://relay.damus.io".to_string(),
                is_active: true,
            },
        ];
        Self {
            relays: Arc::new(Mutex::new(default_relays)),
            image_cache: Arc::new(Mutex::new(HashMap::new())),
            database: Arc::new(Mutex::new(None)),
        }
    }

    pub fn init_database(&self, db_path: &std::path::Path) -> Result<(), String> {
        let db = Database::new(db_path).map_err(|e| e.to_string())?;
        *self.database.lock().unwrap() = Some(db);
        Ok(())
    }

    pub fn get_database(&self) -> Result<Database, String> {
        self.database.lock().unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Database not initialized".to_string())
    }
} 