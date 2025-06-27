use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

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

#[derive(Debug, Default)]
pub struct AppState {
    pub relays: Arc<Mutex<Vec<Relay>>>,
    pub image_cache: Arc<Mutex<HashMap<String, CachedImage>>>, // pubkey -> cached image
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
        }
    }
} 