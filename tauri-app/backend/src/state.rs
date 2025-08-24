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
        
        // Load relays from database first (don't rely on in-memory state)
        let db_relays = match self.get_database() {
            Ok(db) => {
                match db.get_all_relays() {
                    Ok(relays) => {
                        println!("[RUST] Loaded {} relays from database for client init", relays.len());
                        // Convert DbRelay to Relay and update in-memory state
                        let state_relays: Vec<Relay> = relays.iter().map(|db_relay| Relay {
                            url: db_relay.url.clone(),
                            is_active: db_relay.is_active,
                        }).collect();
                        
                        // Update in-memory state
                        *self.relays.lock().unwrap() = state_relays.clone();
                        
                        state_relays
                    },
                    Err(e) => {
                        println!("[RUST] Failed to load relays from database: {}, using in-memory state", e);
                        self.relays.lock().unwrap().clone()
                    }
                }
            },
            Err(e) => {
                println!("[RUST] Database not available: {}, using in-memory state", e);
                self.relays.lock().unwrap().clone()
            }
        };
        
        // Use the loaded relays (either from DB or in-memory fallback)
        let relays = db_relays;
        
        // Add active relays to client
        let mut added_relays = false;
        for relay in relays.iter().filter(|r| r.is_active) {
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
        
        // Add default relays if no active relays were added
        if !added_relays {
            println!("[RUST] No active relays found, using defaults");
            client.add_relay("wss://nostr-pub.wellorder.net").await.map_err(|e| e.to_string())?;
            client.add_relay("wss://relay.damus.io").await.map_err(|e| e.to_string())?;
        }
        
        // Connect to relays
        client.connect().await;
        println!("[RUST] Connected to {} relays", client.relays().await.len());
        
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
            let mut added_relays = false;
            for relay in new_relays.iter().filter(|r| r.is_active) {
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
            
            // Add defaults if no active relays
            if !added_relays {
                client.add_relay("wss://nostr-pub.wellorder.net").await.map_err(|e| e.to_string())?;
                client.add_relay("wss://relay.damus.io").await.map_err(|e| e.to_string())?;
            }
            
            // Reconnect
            client.connect().await;
            println!("[RUST] Reconnected to {} relays", client.relays().await.len());
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
        
        // Update the relay in local state
        {
            let mut relays_guard = self.relays.lock().unwrap();
            if let Some(relay) = relays_guard.iter_mut().find(|r| r.url == relay_url) {
                relay.is_active = is_active;
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