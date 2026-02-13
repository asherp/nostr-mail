use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::database::Database;
use nostr_sdk::prelude::*;
use anyhow::Result;
use tokio::sync::RwLock;

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

#[derive(Debug, Clone)]
pub struct EventSubscription {
    pub subscription_ids: Vec<SubscriptionId>,
    pub is_active: bool,
    pub filters: Vec<Filter>,
    pub user_pubkey: PublicKey,
    pub since_timestamp: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub relays: Arc<Mutex<Vec<Relay>>>,
    pub image_cache: Arc<Mutex<HashMap<String, CachedImage>>>, // pubkey -> cached image
    pub database: Arc<Mutex<Option<Database>>>,
    pub nostr_client: Arc<Mutex<Option<Client>>>,
    pub current_keys: Arc<Mutex<Option<Keys>>>,
    pub active_subscription: Arc<RwLock<Option<EventSubscription>>>,
    pub failed_relays: Arc<Mutex<std::collections::HashMap<String, String>>>, // Track relays that failed to connect: URL -> error message
    pub relay_auth_status: Arc<Mutex<std::collections::HashMap<String, bool>>>, // Track authentication status per relay: URL -> authenticated boolean
    pub pending_auth_challenges: Arc<Mutex<std::collections::HashMap<String, String>>>, // Track pending AUTH challenges: URL -> challenge string
    pub current_private_key: Arc<Mutex<Option<String>>>, // Store current private key in bech32 format for AUTH
}

impl AppState {
    pub fn new() -> Self {
        // Start with empty relays - will be loaded from database when available
        Self {
            relays: Arc::new(Mutex::new(Vec::new())),
            image_cache: Arc::new(Mutex::new(HashMap::new())),
            database: Arc::new(Mutex::new(None)),
            nostr_client: Arc::new(Mutex::new(None)),
            current_keys: Arc::new(Mutex::new(None)),
            active_subscription: Arc::new(RwLock::new(None)),
            failed_relays: Arc::new(Mutex::new(std::collections::HashMap::new())),
            relay_auth_status: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pending_auth_challenges: Arc::new(Mutex::new(std::collections::HashMap::new())),
            current_private_key: Arc::new(Mutex::new(None)),
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

    /// Initialize or update the persistent Nostr client with new keys and relays
    /// Requires a private key - connections are only established when a private key is provided
    pub async fn init_nostr_client(&self, private_key: &str) -> Result<(), String> {
        println!("[RUST] Initializing persistent Nostr client with private key");
        
        // Initialize rustls on Android before creating client
        crate::nostr::init_android_rustls();
        
        // Parse private key
        let secret_key = SecretKey::from_bech32(private_key).map_err(|e| e.to_string())?;
        let keys = Keys::new(secret_key);
        
        // Create new client
        let client = Client::new(keys.clone());
        
        // Load relays from database (required - no fallback)
        let db = self.get_database()?;
        let db_relays = db.get_all_relays()
            .map_err(|e| format!("Failed to load relays from database: {}", e))?;
        
        println!("[RUST] Loaded {} relays from database for client init", db_relays.len());
        
        // Convert DbRelay to Relay and update in-memory state
        let state_relays: Vec<Relay> = db_relays.iter().map(|db_relay| Relay {
            url: db_relay.url.clone(),
            is_active: db_relay.is_active,
        }).collect();
        
        // Update in-memory state
        *self.relays.lock().unwrap() = state_relays.clone();
        
        // Add active relays to client
        let mut added_relays = false;
        for relay in state_relays.iter().filter(|r| r.is_active) {
            match client.add_relay(&relay.url).await {
                Ok(_) => {
                    println!("[RUST] Added relay: {}", relay.url);
                    added_relays = true;
                },
                Err(e) => {
                    println!("[RUST] Failed to add relay {}: {}", relay.url, e);
                }
            }
        }
        
        // Require at least one relay to be added
        if !added_relays {
            return Err("No active relays found in database. Please add relays in settings.".to_string());
        }
        
        // Connect to relays
        client.connect().await;
        
        // Wait for connections to establish (with timeout)
        // WebSocket connections are asynchronous and need time to establish
        let max_wait_time = std::time::Duration::from_secs(10);
        let start_time = std::time::Instant::now();
        let check_interval = std::time::Duration::from_millis(500);
        while start_time.elapsed() < max_wait_time {
            let relays = client.relays().await;
            
            // Check if we have at least one relay added (connection may still be establishing)
            if !relays.is_empty() {
                println!("[RUST] Relays added, giving connections brief moment to initialize...");
                // Give connections a brief moment to initialize (reduced from 2 seconds to 100ms)
                // Connections will continue establishing in the background asynchronously
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                break;
            }
            
            // Wait before checking again
            tokio::time::sleep(check_interval).await;
        }
        
        // Log final connection status
        let final_relays = client.relays().await;
        let relay_urls: Vec<String> = final_relays.iter()
            .map(|(url, _)| url.to_string())
            .collect();
        
        println!("[RUST] Connection attempt completed. Total relays added: {}", final_relays.len());
        if !relay_urls.is_empty() {
            println!("[RUST] Relay URLs: {:?}", relay_urls);
            println!("[RUST] Note: Connection status will be checked asynchronously");
        } else {
            println!("[RUST] Warning: No relays were added");
        }
        
        // Store client, keys, and private key
        *self.nostr_client.lock().unwrap() = Some(client.clone());
        *self.current_keys.lock().unwrap() = Some(keys.clone());
        *self.current_private_key.lock().unwrap() = Some(private_key.to_string());
        
        // Check if there was an active subscription that needs to be restarted
        let subscription_to_restart = {
            let subscription_guard = self.active_subscription.read().await;
            subscription_guard.clone()
        };
        
        if let Some(subscription) = subscription_to_restart {
            println!("[RUST] Restarting active subscription after client reconnection");
            // Restart the subscription with the new client
            let mut new_subscription_ids = Vec::new();
            let mut restart_success = true;
            
            for filter in &subscription.filters {
                match client.subscribe(filter.clone(), None).await {
                    Ok(output) => {
                        // output.val is a single SubscriptionId, not a HashMap
                        new_subscription_ids.push(output.val);
                    },
                    Err(e) => {
                        println!("[RUST] Failed to restart subscription for filter: {}", e);
                        restart_success = false;
                        break;
                    }
                }
            }
            
            if restart_success {
                let mut subscription_guard = self.active_subscription.write().await;
                if let Some(ref mut sub) = subscription_guard.as_mut() {
                    sub.subscription_ids = new_subscription_ids.clone();
                    sub.is_active = true;
                    println!("[RUST] Subscription restarted with {} IDs", new_subscription_ids.len());
                }
            } else {
                println!("[RUST] Failed to restart some subscriptions");
            }
        }
        
        Ok(())
    }

    /// Get the persistent Nostr client (initialize if needed)
    /// Requires a private key to initialize the client
    /// If a private key is provided and doesn't match the current client's keys, reinitializes the client
    pub async fn get_nostr_client(&self, private_key: Option<&str>) -> Result<Client, String> {
        use nostr_sdk::prelude::*;
        
        // If a private key is provided, check if we need to reinitialize
        if let Some(pk) = private_key {
            // Parse the provided private key
            let secret_key = SecretKey::from_bech32(pk).map_err(|e| e.to_string())?;
            let provided_keys = Keys::new(secret_key);
            
            // Check if keys match current client
            let needs_reinit = {
                let current_keys = self.get_current_keys();
                match current_keys {
                    Some(current_keys) => current_keys.public_key() != provided_keys.public_key(),
                    None => true, // No keys stored, need to initialize
                }
            };
            
            if needs_reinit {
                println!("[RUST] Private key changed, reinitializing client connections");
                self.init_nostr_client(pk).await?;
            }
        }
        
        // First, try to get the existing client
        let existing_client = {
            let client_guard = self.nostr_client.lock().unwrap();
            client_guard.as_ref().cloned()
        };
        
        if let Some(client) = existing_client {
            // Check if client is still connected
            if client.relays().await.is_empty() {
                // Reconnect if no relays are connected
                if let Some(pk) = private_key {
                    self.init_nostr_client(pk).await?;
                    let reconnected_client = {
                        let client_guard = self.nostr_client.lock().unwrap();
                        client_guard.as_ref().cloned()
                    };
                    return reconnected_client.ok_or_else(|| "Failed to reconnect client".to_string());
                } else {
                    return Err("Nostr client not connected and no private key provided".to_string());
                }
            }
            Ok(client)
        } else {
            // Initialize client if not exists
            if let Some(pk) = private_key {
                self.init_nostr_client(pk).await?;
                let new_client = {
                    let client_guard = self.nostr_client.lock().unwrap();
                    client_guard.as_ref().cloned()
                };
                new_client.ok_or_else(|| "Failed to initialize client".to_string())
            } else {
                Err("Nostr client not initialized and no private key provided".to_string())
            }
        }
    }

    /// Get current keys
    pub fn get_current_keys(&self) -> Option<Keys> {
        self.current_keys.lock().unwrap().clone()
    }

    /// Update relays configuration without connecting
    /// Connections are only established when init_nostr_client is called with a private key
    pub async fn update_relays(&self, new_relays: Vec<Relay>) -> Result<(), String> {
        println!("[RUST] Updating relay configuration (stored, not connecting yet)");
        
        // Update stored relays
        *self.relays.lock().unwrap() = new_relays.clone();
        
        // If client exists and is initialized (has private key), update its relays
        let client_clone = {
            let client_guard = self.nostr_client.lock().unwrap();
            client_guard.as_ref().cloned()
        };
        
        // Only update connections if client exists (meaning we have a private key)
        if let Some(client) = client_clone {
            println!("[RUST] Client exists, updating relay connections");
            // Disconnect from all current relays
            client.disconnect().await;
            
            // Add new active relays
            for relay in new_relays.iter().filter(|r| r.is_active) {
                match client.add_relay(&relay.url).await {
                    Ok(_) => {
                        println!("[RUST] Added relay: {}", relay.url);
                    },
                    Err(e) => {
                        println!("[RUST] Failed to add relay {}: {}", relay.url, e);
                    }
                }
            }
            
            // Reconnect
            client.connect().await;
            
            // Wait for connections to establish (with timeout)
            // WebSocket connections are asynchronous and need time to establish
            let max_wait_time = std::time::Duration::from_secs(10);
            let start_time = std::time::Instant::now();
            let check_interval = std::time::Duration::from_millis(500);
            
            while start_time.elapsed() < max_wait_time {
                let relays = client.relays().await;
                
                // Check if we have at least one relay added (connection may still be establishing)
                if !relays.is_empty() {
                    println!("[RUST] Relays added after update, waiting for connections to establish...");
                    // Give connections more time to establish
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    break;
                }
                
                // Wait before checking again
                tokio::time::sleep(check_interval).await;
            }
            
            // Log final connection status
            let final_relays = client.relays().await;
            let relay_urls: Vec<String> = final_relays.iter()
                .map(|(url, _)| url.to_string())
                .collect();
            
            println!("[RUST] Reconnection attempt completed. Total relays added: {}", final_relays.len());
            if !relay_urls.is_empty() {
                println!("[RUST] Relay URLs: {:?}", relay_urls);
                println!("[RUST] Note: Connection status will be checked asynchronously");
            } else {
                println!("[RUST] Warning: No relays were added after update");
            }
        } else {
            println!("[RUST] No client initialized yet - relay configuration stored, will connect when private key is provided");
        }
        
        Ok(())
    }

    /// Disconnect the Nostr client
    pub async fn disconnect_nostr_client(&self) -> Result<(), String> {
        println!("[RUST] Disconnecting Nostr client");
        let client_option = {
            let client_guard = self.nostr_client.lock().unwrap();
            client_guard.as_ref().cloned()
        };
        
        if let Some(client) = client_option {
            client.disconnect().await;
        }
        Ok(())
    }

    /// Update a single relay's connection status
    pub async fn update_single_relay(&self, relay_url: &str, is_active: bool) -> Result<(), String> {
        println!("[RUST] Updating single relay: {} (active: {})", relay_url, is_active);
        
        // Update the relay in local state, or add it if it doesn't exist
        {
            let mut relays_guard = self.relays.lock().unwrap();
            if let Some(relay) = relays_guard.iter_mut().find(|r| r.url == relay_url) {
                relay.is_active = is_active;
            } else {
                // Relay doesn't exist in in-memory state, add it
                // This can happen when a new relay is added to the database but state hasn't been synced
                println!("[RUST] Relay {} not found in in-memory state, adding it", relay_url);
                relays_guard.push(Relay {
                    url: relay_url.to_string(),
                    is_active,
                });
            }
        }
        
        // Update the client connections only if client exists (has private key)
        let client_option = {
            let client_guard = self.nostr_client.lock().unwrap();
            client_guard.as_ref().cloned()
        };
        
        if let Some(client) = client_option {
            if is_active {
                // Clear from failed_relays map when reconnecting
                {
                    let mut failed_map = self.failed_relays.lock().unwrap();
                    if failed_map.remove(relay_url).is_some() {
                        println!("[RUST] Cleared relay {} from failed_relays map (reconnecting)", relay_url);
                    }
                }
                
                // Connect to this relay
                match client.add_relay(relay_url).await {
                    Ok(_) => {
                        println!("[RUST] Successfully added relay: {}", relay_url);
                        // Try to connect - this is async and may not complete immediately
                        // The connection status will be checked later via get_relay_status
                    },
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                            // Relay was already added - this is fine
                            println!("[RUST] Relay {} was already added", relay_url);
                        } else {
                            // This is a real error
                            println!("[RUST] Failed to add relay {}: {}", relay_url, e);
                            return Err(format!("Failed to add relay: {}", e));
                        }
                    }
                }
            } else {
                // Disconnect from this relay
                match client.remove_relay(relay_url).await {
                    Ok(_) => {
                        println!("[RUST] Successfully disconnected from relay: {}", relay_url);
                    },
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("relay not found") || error_msg.contains("not found") {
                            // Relay was already disconnected or never connected - this is fine
                            println!("[RUST] Relay {} was already disconnected (not found)", relay_url);
                        } else {
                            // This is a real error
                            println!("[RUST] Failed to disconnect from relay {}: {}", relay_url, e);
                            return Err(format!("Failed to disconnect from relay: {}", e));
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: point NOSTR_MAIL_CONFIG at a minimal config file inside a temp dir
    /// and return the temp dir (keep alive for test lifetime).
    fn setup_test_env() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = dir.path().join("test-config.json");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(f, r#"{{"relays":[]}}"#).unwrap();
        std::env::set_var("NOSTR_MAIL_CONFIG", config_path.to_str().unwrap());
        dir
    }

    // =====================
    // AppState::new
    // =====================

    #[test]
    fn test_appstate_new_initial_state() {
        let state = AppState::new();

        // Relays should be empty
        let relays = state.relays.lock().unwrap();
        assert!(relays.is_empty());

        // Image cache should be empty
        let cache = state.image_cache.lock().unwrap();
        assert!(cache.is_empty());

        // Database should be None
        let db = state.database.lock().unwrap();
        assert!(db.is_none());

        // Nostr client should be None
        let client = state.nostr_client.lock().unwrap();
        assert!(client.is_none());

        // Current keys should be None
        let keys = state.current_keys.lock().unwrap();
        assert!(keys.is_none());

        // Failed relays should be empty
        let failed = state.failed_relays.lock().unwrap();
        assert!(failed.is_empty());

        // Relay auth status should be empty
        let auth = state.relay_auth_status.lock().unwrap();
        assert!(auth.is_empty());

        // Pending auth challenges should be empty
        let pending = state.pending_auth_challenges.lock().unwrap();
        assert!(pending.is_empty());

        // Current private key should be None
        let pk = state.current_private_key.lock().unwrap();
        assert!(pk.is_none());
    }

    // =====================
    // AppState::get_current_keys
    // =====================

    #[test]
    fn test_get_current_keys_none_initially() {
        let state = AppState::new();
        assert!(state.get_current_keys().is_none());
    }

    #[test]
    fn test_get_current_keys_after_set() {
        let state = AppState::new();
        let keys = nostr_sdk::prelude::Keys::generate();
        *state.current_keys.lock().unwrap() = Some(keys.clone());

        let retrieved = state.get_current_keys();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().public_key(), keys.public_key());
    }

    // =====================
    // AppState::get_database
    // =====================

    #[test]
    fn test_get_database_before_init() {
        let state = AppState::new();
        let result = state.get_database();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Database not initialized");
    }

    // =====================
    // AppState::init_database
    // =====================

    #[test]
    fn test_init_database_success() {
        let dir = setup_test_env();
        let state = AppState::new();
        let db_path = dir.path().join("state_test.db");

        let result = state.init_database(&db_path);
        assert!(result.is_ok());

        // Database should now be accessible
        let db_result = state.get_database();
        assert!(db_result.is_ok());
    }

    #[test]
    fn test_init_database_then_get() {
        let dir = setup_test_env();
        let state = AppState::new();
        let db_path = dir.path().join("state_test2.db");

        state.init_database(&db_path).unwrap();
        let db = state.get_database().unwrap();

        // Verify the database is functional
        let relays = db.get_all_relays().unwrap();
        assert!(relays.is_empty()); // empty config -> no seeded relays
    }

    #[test]
    fn test_init_database_can_be_called_twice() {
        let dir = setup_test_env();
        let state = AppState::new();
        let db_path = dir.path().join("state_test3.db");

        state.init_database(&db_path).unwrap();
        // Calling again should succeed (reinitializes)
        state.init_database(&db_path).unwrap();

        let db = state.get_database();
        assert!(db.is_ok());
    }

    // =====================
    // AppState relay/failed_relays state
    // =====================

    #[test]
    fn test_relay_state_manipulation() {
        let state = AppState::new();

        // Add relays to in-memory state
        {
            let mut relays = state.relays.lock().unwrap();
            relays.push(Relay {
                url: "wss://relay1.example.com".to_string(),
                is_active: true,
            });
            relays.push(Relay {
                url: "wss://relay2.example.com".to_string(),
                is_active: false,
            });
        }

        let relays = state.relays.lock().unwrap();
        assert_eq!(relays.len(), 2);
        assert!(relays[0].is_active);
        assert!(!relays[1].is_active);
    }

    #[test]
    fn test_failed_relays_tracking() {
        let state = AppState::new();

        {
            let mut failed = state.failed_relays.lock().unwrap();
            failed.insert("wss://bad-relay.example.com".to_string(), "Connection refused".to_string());
        }

        let failed = state.failed_relays.lock().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(
            failed.get("wss://bad-relay.example.com").unwrap(),
            "Connection refused"
        );
    }

    #[test]
    fn test_relay_auth_status_tracking() {
        let state = AppState::new();

        {
            let mut auth = state.relay_auth_status.lock().unwrap();
            auth.insert("wss://authed.example.com".to_string(), true);
            auth.insert("wss://unauthed.example.com".to_string(), false);
        }

        let auth = state.relay_auth_status.lock().unwrap();
        assert_eq!(*auth.get("wss://authed.example.com").unwrap(), true);
        assert_eq!(*auth.get("wss://unauthed.example.com").unwrap(), false);
    }

    #[test]
    fn test_pending_auth_challenges() {
        let state = AppState::new();

        {
            let mut pending = state.pending_auth_challenges.lock().unwrap();
            pending.insert("wss://relay.example.com".to_string(), "challenge_abc".to_string());
        }

        let pending = state.pending_auth_challenges.lock().unwrap();
        assert_eq!(
            pending.get("wss://relay.example.com").unwrap(),
            "challenge_abc"
        );
    }

    #[test]
    fn test_current_private_key_storage() {
        let state = AppState::new();

        // Initially None
        assert!(state.current_private_key.lock().unwrap().is_none());

        // Set a value
        *state.current_private_key.lock().unwrap() = Some("nsec1testkey".to_string());
        assert_eq!(
            state.current_private_key.lock().unwrap().as_deref(),
            Some("nsec1testkey")
        );
    }

    #[test]
    fn test_image_cache() {
        let state = AppState::new();

        {
            let mut cache = state.image_cache.lock().unwrap();
            cache.insert("pubkey123".to_string(), CachedImage {
                data_url: "data:image/png;base64,abc".to_string(),
                timestamp: 1700000000,
            });
        }

        let cache = state.image_cache.lock().unwrap();
        let cached = cache.get("pubkey123").unwrap();
        assert_eq!(cached.data_url, "data:image/png;base64,abc");
        assert_eq!(cached.timestamp, 1700000000);
    }
}