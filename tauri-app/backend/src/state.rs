use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relay {
    pub url: String,
    pub is_active: bool,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub relays: Arc<Mutex<Vec<Relay>>>,
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
        }
    }
} 