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
    pub async fn init_nostr_client(&self, private_key: &str) -> Result<(), String> {
        println!("[RUST] Initializing persistent Nostr client");
        
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
        // #region agent log
        let connect_start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/asherp/git/nostr-mail/.cursor/debug.log").and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"id":"log_connect_start","timestamp":{},"location":"state.rs:113","message":"Before client.connect()","data":{{"hypothesisId":"E"}},"sessionId":"debug-session","runId":"startup"}}"#, connect_start)
        });
        // #endregion
        client.connect().await;
        // #region agent log
        let connect_end = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/asherp/git/nostr-mail/.cursor/debug.log").and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"id":"log_connect_end","timestamp":{},"location":"state.rs:113","message":"After client.connect()","data":{{"duration":{},"hypothesisId":"E"}},"sessionId":"debug-session","runId":"startup"}}"#, connect_end, connect_end - connect_start)
        });
        // #endregion
        
        // Wait for connections to establish (with timeout)
        // WebSocket connections are asynchronous and need time to establish
        let max_wait_time = std::time::Duration::from_secs(10);
        let start_time = std::time::Instant::now();
        let check_interval = std::time::Duration::from_millis(500);
        
        // #region agent log
        let wait_start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/asherp/git/nostr-mail/.cursor/debug.log").and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"id":"log_wait_start","timestamp":{},"location":"state.rs:121","message":"Starting connection wait loop","data":{{"maxWaitSeconds":10,"hypothesisId":"E"}},"sessionId":"debug-session","runId":"startup"}}"#, wait_start)
        });
        // #endregion
        while start_time.elapsed() < max_wait_time {
            let relays = client.relays().await;
            
            // Check if we have at least one relay added (connection may still be establishing)
            if !relays.is_empty() {
                println!("[RUST] Relays added, giving connections brief moment to initialize...");
                // #region agent log
                let sleep_start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
                let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/asherp/git/nostr-mail/.cursor/debug.log").and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, r#"{{"id":"log_sleep_start","timestamp":{},"location":"state.rs:128","message":"Before connection wait","data":{{"relayCount":{},"hypothesisId":"E"}},"sessionId":"debug-session","runId":"startup"}}"#, sleep_start, relays.len())
                });
                // #endregion
                // Give connections a brief moment to initialize (reduced from 2 seconds to 100ms)
                // Connections will continue establishing in the background asynchronously
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                // #region agent log
                let sleep_end = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
                let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/asherp/git/nostr-mail/.cursor/debug.log").and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, r#"{{"id":"log_sleep_end","timestamp":{},"location":"state.rs:128","message":"After connection wait","data":{{"duration":{},"hypothesisId":"E"}},"sessionId":"debug-session","runId":"startup"}}"#, sleep_end, sleep_end - sleep_start)
                });
                // #endregion
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
        
        // Store client and keys
        *self.nostr_client.lock().unwrap() = Some(client.clone());
        *self.current_keys.lock().unwrap() = Some(keys);
        
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
    pub async fn get_nostr_client(&self, private_key: Option<&str>) -> Result<Client, String> {
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

    /// Update relays and reconnect client if needed
    pub async fn update_relays(&self, new_relays: Vec<Relay>) -> Result<(), String> {
        println!("[RUST] Updating relays");
        
        // Update stored relays
        *self.relays.lock().unwrap() = new_relays.clone();
        
        // If client exists, update its relays
        let client_clone = {
            let client_guard = self.nostr_client.lock().unwrap();
            client_guard.as_ref().cloned()
        };
        
        if let Some(client) = client_clone {
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
        
        // Update the client connections
        let client_option = {
            let client_guard = self.nostr_client.lock().unwrap();
            client_guard.as_ref().cloned()
        };
        
        if let Some(client) = client_option {
            if is_active {
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