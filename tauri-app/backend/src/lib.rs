// NOTE: For Tauri commands, do NOT use 'pub' (do not export with pub) on the function definitions.
// Exporting Tauri commands with 'pub' can cause duplicate macro errors at compile time.
// Only use 'async fn' or 'fn' without 'pub' for #[tauri::command] functions.
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod crypto;
mod email;
mod nostr;
mod types;
pub mod state;
mod storage;
mod database;

use types::*;
pub use state::{AppState, Relay};
pub use types::KeyPair;
use storage::{Storage, Contact, Conversation, UserProfile, AppSettings, EmailDraft};
use database::{Contact as DbContact, Email as DbEmail, DirectMessage as DbDirectMessage, DbRelay};
use crate::types::{EmailMessage, RelayStatus, RelayConnectionStatus};

use nostr_sdk::Metadata;
use nostr_sdk::ToBech32;
use nostr_sdk::{PublicKey, Filter, Kind, FromBech32, SecretKey, Keys, RelayPoolNotification, Timestamp};
use std::str::FromStr;
use std::time::Duration;

use state::EventSubscription;
use serde_json;
use tauri::Emitter;

fn map_db_email_to_email_message(email: &DbEmail) -> EmailMessage {
    let raw_headers = email.raw_headers.clone().unwrap_or_default();
    EmailMessage {
        id: email.id.map(|id| id.to_string()).unwrap_or_else(|| email.message_id.clone()),
        from: email.from_address.clone(),
        to: email.to_address.clone(),
        subject: email.subject.clone(),
        body: email.body.clone(),
        raw_body: email.body.clone(),
        date: email.received_at,
        transport_auth_verified: email.transport_auth_verified,
        is_read: email.is_read,
        raw_headers: raw_headers.clone(),
        sender_pubkey: email.sender_pubkey.clone(),
        recipient_pubkey: email.recipient_pubkey.clone(),
        message_id: Some(email.message_id.clone()),
        signature_valid: email.signature_valid,
    }
}

/// Send a direct message using the persistent client
async fn send_direct_message_persistent(
    private_key: &str,
    recipient_pubkey: &str,
    content: &MessageContent,
    encryption_algorithm: Option<&str>,
    state: &AppState,
) -> Result<String, String> {
    use nostr_sdk::prelude::*;
    use crate::types::MessageContent;
    
    println!("[RUST] send_direct_message_persistent called");
    
    // Get or initialize the persistent client
    let client = state.get_nostr_client(Some(private_key)).await?;
    
    // Parse recipient pubkey
    let recipient = PublicKey::from_bech32(recipient_pubkey).map_err(|e| e.to_string())?;
    
    // Get keys for encryption
    let keys = state.get_current_keys().ok_or("No keys available")?;
    
    // Determine encryption algorithm (default to NIP-44)
    let algorithm = encryption_algorithm.unwrap_or("nip44");
    println!("[RUST] Using encryption algorithm: {}", algorithm);
    
    // Handle content based on type
    let encrypted_content = match content {
        MessageContent::Plaintext(text) => {
            println!("[RUST] Encrypting plaintext message");
            match algorithm {
                "nip04" => {
                    nip04::encrypt(keys.secret_key(), &recipient, text)
                        .map_err(|e| e.to_string())?
                },
                "nip44" => {
                    nip44::encrypt(keys.secret_key(), &recipient, text, nip44::Version::default())
                        .map_err(|e| e.to_string())?
                },
                _ => {
                    return Err(format!("Unsupported encryption algorithm: {}", algorithm));
                }
            }
        },
        MessageContent::Encrypted(encrypted) => {
            println!("[RUST] Using pre-encrypted content");
            encrypted.clone()
        }
    };
    
    // Create and send the event
    let event = EventBuilder::new(Kind::EncryptedDirectMessage, &encrypted_content)
        .tag(Tag::public_key(recipient));
    
    let output = client.send_event_builder(event).await.map_err(|e| e.to_string())?;
    
    println!("[RUST] Message sent successfully with output: {:?}", output);
    
    // The output.success contains relay URLs that successfully received the event
    // For now, we'll generate a placeholder event ID since we can't get the actual ID from Output
    // In a real implementation, you might want to store the event before sending
    let placeholder_id = format!("sent_to_{}_relays", output.success.len());
    Ok(placeholder_id)
}

#[tauri::command]
fn greet(name: &str) -> String {
    println!("[RUST] greet called with: {}", name);
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn generate_keypair() -> Result<KeyPair, String> {
    println!("[RUST] generate_keypair called");
    crypto::generate_keypair().map_err(|e| e.to_string())
}

#[tauri::command]
fn validate_private_key(private_key: String) -> Result<bool, String> {
    println!("[RUST] validate_private_key called with: {}...", &private_key[..10.min(private_key.len())]);
    crypto::validate_private_key(&private_key).map_err(|e| e.to_string())
}

#[tauri::command]
fn validate_public_key(public_key: String) -> Result<bool, String> {
    println!("[RUST] validate_public_key called with: {}...", &public_key[..10.min(public_key.len())]);
    crypto::validate_public_key(&public_key).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_public_key_from_private(private_key: String) -> Result<String, String> {
    println!("[RUST] get_public_key_from_private called");
    crypto::get_public_key_from_private(&private_key).map_err(|e| e.to_string())
}

#[tauri::command]
fn sign_data(private_key: String, data: String) -> Result<String, String> {
    println!("[RUST] sign_data called");
    crypto::sign_data(&private_key, &data).map_err(|e| e.to_string())
}

#[tauri::command]
fn verify_signature(public_key: String, signature: String, data: String) -> Result<bool, String> {
    println!("[RUST] verify_signature called");
    crypto::verify_signature(&public_key, &signature, &data).map_err(|e| e.to_string())
}

#[tauri::command]
fn recheck_email_signature(message_id: String, state: tauri::State<AppState>) -> Result<Option<bool>, String> {
    println!("[RUST] recheck_email_signature called for message_id: {}", message_id);
    let db = state.get_database().map_err(|e| e.to_string())?;
    
    // Get the email from database
    let email = db.get_email(&message_id).map_err(|e| e.to_string())?
        .ok_or_else(|| "Email not found".to_string())?;
    
    // Extract signature and pubkey from raw headers
    let raw_headers = email.raw_headers.as_deref().unwrap_or("");
    let sender_pubkey = email::extract_nostr_pubkey_from_headers(raw_headers);
    let signature = email::extract_nostr_sig_from_headers(raw_headers);
    
    if let (Some(pubkey), Some(sig)) = (sender_pubkey, signature) {
        // Verify the signature
        let is_valid = email::verify_email_signature(&pubkey, &sig, &email.body);
        
        // Update the database
        db.update_signature_valid(&message_id, Some(is_valid)).map_err(|e| e.to_string())?;
        
        println!("[RUST] recheck_email_signature: Signature verification result: {}", is_valid);
        Ok(Some(is_valid))
    } else {
        println!("[RUST] recheck_email_signature: Missing pubkey or signature");
        Ok(None)
    }
}

#[tauri::command]
async fn send_direct_message(request: DirectMessageRequest, state: tauri::State<'_, AppState>) -> Result<String, String> {
    println!("[RUST] send_direct_message called");
    println!("[RUST] Recipient: {}", request.recipient_pubkey);
    println!("[RUST] Content type: {:?}", request.content);
    println!("[RUST] Encryption algorithm: {:?}", request.encryption_algorithm);
    
    let result = send_direct_message_persistent(
        &request.sender_private_key,
        &request.recipient_pubkey,
        &request.content,
        request.encryption_algorithm.as_deref(),
        &state
    )
    .await
    .map(|event_id| {
        println!("[RUST] Successfully sent message, event ID: {}", event_id);
        event_id
    })
    .map_err(|e| {
        println!("[RUST] Failed to send message: {}", e);
        e.to_string()
    });
    
    println!("[RUST] Returning result: {:?}", result);
    result
}

#[tauri::command]
async fn fetch_direct_messages(private_key: String, relays: Vec<String>, since: Option<i64>) -> Result<Vec<NostrEvent>, String> {
    println!("[RUST] fetch_direct_messages called");
    nostr::fetch_direct_messages(&private_key, &relays, since).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_conversations(private_key: String, relays: Vec<String>) -> Result<Vec<nostr::Conversation>, String> {
    println!("[RUST] fetch_conversations called");
    nostr::fetch_conversations(&private_key, &relays).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_conversation_messages(private_key: String, contact_pubkey: String, relays: Vec<String>) -> Result<Vec<nostr::ConversationMessage>, String> {
    println!("[RUST] fetch_conversation_messages called for contact: {}", contact_pubkey);
    nostr::fetch_conversation_messages(&private_key, &contact_pubkey, &relays).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_profile(pubkey: String, relays: Vec<String>) -> Result<Option<ProfileResult>, String> {
    println!("[RUST] fetch_profile called for pubkey: {}", pubkey);
    let events = nostr::fetch_events(&pubkey, Some(0), &relays)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(latest_event) = events.into_iter().max_by_key(|e| e.created_at) {
        let profile = nostr::parse_profile_from_event(&latest_event).map_err(|e| e.to_string())?;
        Ok(Some(ProfileResult {
            pubkey: profile.pubkey,
            fields: profile.fields,
            raw_content: latest_event.content,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn fetch_profile_persistent(pubkey: String, state: tauri::State<'_, AppState>) -> Result<Option<ProfileResult>, String> {
    println!("[RUST] fetch_profile_persistent called for pubkey: {}", pubkey);
    
    // Get the persistent client
    let client = {
        let client_guard = state.nostr_client.lock().unwrap();
        match client_guard.as_ref() {
            Some(client) => client.clone(),
            None => return Err("Persistent Nostr client not initialized".to_string()),
        }
    };
    
    // Parse pubkey
    let public_key = PublicKey::from_bech32(&pubkey)
        .or_else(|_| PublicKey::from_hex(&pubkey))
        .map_err(|e| format!("Invalid pubkey {}: {}", pubkey, e))?;
    
    println!("[RUST] Using persistent client to fetch profile for: {}", pubkey);
    
    // Check relay connection status before fetching
    // Wait for at least one relay to be connected (up to 10 seconds)
    let max_wait_time = std::time::Duration::from_secs(10);
    let start_time = std::time::Instant::now();
    let check_interval = std::time::Duration::from_millis(500);
    
    let mut connected_relays = client.relays().await;
    println!("[RUST] Checking relay status before profile fetch...");
    println!("[RUST] Initial relays in client: {}", connected_relays.len());
    
    // Wait for at least one relay to connect
    while connected_relays.is_empty() && start_time.elapsed() < max_wait_time {
        println!("[RUST] No relays connected yet, waiting... ({:?} elapsed)", start_time.elapsed());
        tokio::time::sleep(check_interval).await;
        connected_relays = client.relays().await;
    }
    
    if connected_relays.is_empty() {
        println!("[RUST] WARNING: No relays connected after waiting! Profile fetch will likely fail.");
        return Err("No relays connected. Please check your relay configuration and network connection.".to_string());
    } else {
        println!("[RUST] Connected to {} relay(s):", connected_relays.len());
        for (url, _) in connected_relays.iter() {
            println!("[RUST]   ✓ {}", url);
        }
        
        // Give relays a moment to be fully ready for queries
        // WebSocket connections may be established but not ready to receive queries immediately
        println!("[RUST] Waiting 2 seconds for relays to be fully ready...");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    
    // Create filter for single profile
    let profile_filter = Filter::new()
        .author(public_key)
        .kind(Kind::from(0)) // Profile events are kind 0
        .limit(1);
    
    println!("[RUST] Fetching profile events with 15s timeout...");
    let fetch_start = std::time::Instant::now();
    let profile_events = client
        .fetch_events(profile_filter, Duration::from_secs(15))
        .await
        .map_err(|e| {
            println!("[RUST] ERROR during fetch_events: {}", e);
            e.to_string()
        })?;
    let fetch_duration = fetch_start.elapsed();
    println!("[RUST] Fetch completed in {:?}", fetch_duration);
    println!("[RUST] Found {} profile events using persistent client", profile_events.len());
    
    // Log if timeout was reached (suggests relays aren't responding)
    if fetch_duration.as_secs() >= 14 {
        println!("[RUST] WARNING: Fetch took nearly full timeout duration. Relays may not be responding to queries.");
        println!("[RUST] Attempting fallback: using fresh client connection to query relays directly...");
        
        // Fallback: Try using a fresh client connection
        let db = state.get_database().map_err(|e| format!("Failed to get database: {}", e))?;
        let db_relays = db.get_all_relays()
            .map_err(|e| format!("Failed to load relays from database: {}", e))?;
        let relay_urls: Vec<String> = db_relays.iter()
            .filter(|r| r.is_active)
            .map(|r| r.url.clone())
            .collect();
        
        if !relay_urls.is_empty() {
            println!("[RUST] Fallback: Querying {} relays with fresh connection", relay_urls.len());
            match nostr::fetch_events(&pubkey, Some(0), &relay_urls).await {
                Ok(events) => {
                    println!("[RUST] Fallback found {} profile events", events.len());
                    if let Some(latest_event) = events.into_iter().max_by_key(|e| e.created_at) {
                        let profile = nostr::parse_profile_from_event(&latest_event)
                            .map_err(|e| format!("Failed to parse profile: {}", e))?;
                        println!("[RUST] Fallback successfully retrieved profile!");
                        return Ok(Some(ProfileResult {
                            pubkey: pubkey.clone(),
                            fields: profile.fields,
                            raw_content: latest_event.content,
                        }));
                    }
                },
                Err(e) => {
                    println!("[RUST] Fallback also failed: {}", e);
                }
            }
        }
    }
    
    // Find latest profile event
    if let Some(latest_event) = profile_events.into_iter().max_by_key(|e| e.created_at) {
        println!("[RUST] Latest profile event ID: {}", latest_event.id.to_hex());
        
        // Parse profile content
        match serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&latest_event.content) {
            Ok(fields) => {
                println!("[RUST] Successfully parsed profile for: {}", pubkey);
                Ok(Some(ProfileResult {
                    pubkey: pubkey.clone(),
                    fields,
                    raw_content: latest_event.content,
                }))
            },
            Err(e) => {
                println!("[RUST] Failed to parse profile JSON for {}: {}", pubkey, e);
                Ok(Some(ProfileResult {
                    pubkey: pubkey.clone(),
                    fields: std::collections::HashMap::new(),
                    raw_content: latest_event.content,
                }))
            }
        }
    } else {
        println!("[RUST] No profile found for pubkey: {}", pubkey);
        Ok(None)
    }
}

#[tauri::command]
async fn fetch_nostr_following_pubkeys(pubkey: String, relays: Vec<String>) -> Result<Vec<String>, String> {
    nostr::fetch_following_pubkeys(&pubkey, &relays).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_following_pubkeys_persistent(pubkey: String, state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    println!("[RUST] fetch_following_pubkeys_persistent called for pubkey: {}", pubkey);
    
    // Get the persistent client
    let client = {
        let client_guard = state.nostr_client.lock().unwrap();
        match client_guard.as_ref() {
            Some(client) => client.clone(),
            None => return Err("Persistent Nostr client not initialized".to_string()),
        }
    };
    
    // Parse the pubkey
    let public_key = match PublicKey::from_bech32(&pubkey) {
        Ok(pk) => pk,
        Err(_) => PublicKey::from_hex(&pubkey).map_err(|e| e.to_string())?,
    };
    
    println!("[RUST] Using persistent client to fetch following for: {}", pubkey);
    
    // Fetch user's kind 3 event (contact list) using persistent client
    let contact_list_filter = Filter::new()
        .author(public_key)
        .kind(Kind::ContactList)
        .limit(1);

    println!("[RUST] Fetching contact list events using persistent client...");
    let contact_events = client
        .fetch_events(contact_list_filter, Duration::from_secs(10))
        .await
        .map_err(|e| e.to_string())?;
    
    println!("[RUST] Found {} contact list events", contact_events.len());
    
    let latest_contact_event = contact_events.into_iter().max_by_key(|e| e.created_at);

    if let Some(event) = latest_contact_event {
        println!("[RUST] Latest contact event ID: {}", event.id.to_hex());
        println!("[RUST] Contact event has {} tags", event.tags.len());
        
        // Get followed pubkeys from 'p' tags
        let followed_pubkeys: Vec<String> = event.tags
            .iter()
            .filter(|tag| tag.kind().as_str() == "p")
            .filter_map(|tag| {
                tag.content().and_then(|pk| {
                    // Try bech32 (npub) first, then hex, convert to bech32
                    PublicKey::from_bech32(pk)
                        .or_else(|_| PublicKey::from_hex(pk))
                        .ok()
                        .and_then(|pubkey| pubkey.to_bech32().ok())
                })
            })
            .collect();
        
        println!("[RUST] Found {} followed pubkeys using persistent client", followed_pubkeys.len());
        Ok(followed_pubkeys)
    } else {
        println!("[RUST] No contact list events found");
        Ok(vec![])
    }
}


#[tauri::command]
async fn fetch_following_profiles(private_key: String, relays: Vec<String>) -> Result<Vec<Profile>, String> {
    println!("[RUST] fetch_following_profiles called");
    nostr::fetch_following_profiles(&private_key, &relays).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn get_relays(state: tauri::State<AppState>) -> Result<Vec<Relay>, String> {
    Ok(state.relays.lock().unwrap().clone())
}

#[tauri::command]
async fn set_relays(relays: Vec<Relay>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.update_relays(relays).await
}

#[tauri::command]
async fn update_single_relay(relay_url: String, is_active: bool, state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] update_single_relay called for: {} (active: {})", relay_url, is_active);
    state.update_single_relay(&relay_url, is_active).await
}

#[tauri::command]
async fn sync_relay_states(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let client_option = {
        let client_guard = state.nostr_client.lock().unwrap();
        client_guard.clone()
    };
    
    let mut updated_relays = Vec::new();
    
    if let Some(client) = client_option {
        let connected_relays = client.relays().await;
        
        // Get current relay configuration from database (not just in-memory state)
        let db = state.get_database().map_err(|e| e.to_string())?;
        let all_db_relays = db.get_all_relays().map_err(|e| e.to_string())?;
        
        let mut relays_to_update = Vec::new();
        for db_relay in all_db_relays.iter() {
            if db_relay.is_active {
                let is_connected = connected_relays.iter().any(|(url, _)| url.to_string() == db_relay.url);
                if !is_connected {
                    // This relay is marked as active but not actually connected
                    relays_to_update.push(db_relay.url.clone());
                }
            }
        }
        
        // Update disconnected relays to inactive
        for relay_url in relays_to_update {
            // Update in-memory state (if relay exists there)
            {
                let mut relays_guard = state.relays.lock().unwrap();
                if let Some(relay) = relays_guard.iter_mut().find(|r| r.url == relay_url) {
                    relay.is_active = false;
                }
            }
            
            // Update in database
            let db = state.get_database().map_err(|e| e.to_string())?;
            // Try to get relay from database and update it
            let all_relays = db.get_all_relays().map_err(|e| e.to_string())?;
            if let Some(db_relay) = all_relays.iter().find(|r| r.url == relay_url) {
                let updated_relay = crate::database::DbRelay {
                    id: db_relay.id,
                    url: relay_url.clone(),
                    is_active: false,
                    created_at: db_relay.created_at,
                    updated_at: chrono::Utc::now(),
                };
                if let Err(e) = db.save_relay(&updated_relay) {
                    println!("[RUST] Failed to update relay in database: {}", e);
                }
            }
            
            updated_relays.push(relay_url);
        }
        
        // Only log when relays are actually updated
        if !updated_relays.is_empty() {
            println!("[RUST] sync_relay_states: Auto-disabled {} disconnected relay(s): {:?}", updated_relays.len(), updated_relays);
        }
    }
    
    Ok(updated_relays)
}

#[tauri::command]
async fn init_persistent_nostr_client(private_key: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] init_persistent_nostr_client called");
    state.init_nostr_client(&private_key).await
}

#[tauri::command]
async fn disconnect_nostr_client(state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] disconnect_nostr_client called");
    state.disconnect_nostr_client().await
}

#[tauri::command]
async fn get_nostr_client_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    println!("[RUST] get_nostr_client_status called");
    let client_option = {
        let client_guard = state.nostr_client.lock().unwrap();
        client_guard.clone()
    };
    
    if let Some(client) = client_option {
        let relay_count = client.relays().await.len();
        println!("[RUST] Nostr client connected to {} relays", relay_count);
        Ok(relay_count > 0)
    } else {
        println!("[RUST] Nostr client not initialized");
        Ok(false)
    }
}

#[tauri::command]
async fn get_relay_status(state: tauri::State<'_, AppState>) -> Result<Vec<RelayStatus>, String> {
    println!("[RUST] get_relay_status called");
    let client_option = {
        let client_guard = state.nostr_client.lock().unwrap();
        client_guard.clone()
    };
    
    let configured_relays = {
        let relays_guard = state.relays.lock().unwrap();
        relays_guard.clone()
    };
    
    let mut relay_statuses = Vec::new();
    
    if let Some(client) = client_option {
        let connected_relays = client.relays().await;
        
        for configured_relay in configured_relays {
            let is_connected = connected_relays.iter().any(|(url, _)| url.to_string() == configured_relay.url);
            
            let status = if !configured_relay.is_active {
                RelayConnectionStatus::Disabled
            } else if is_connected {
                RelayConnectionStatus::Connected
            } else {
                RelayConnectionStatus::Disconnected
            };
            
            relay_statuses.push(RelayStatus {
                url: configured_relay.url,
                is_active: configured_relay.is_active,
                status,
            });
        }
    } else {
        // No client initialized - all relays are disconnected
        for configured_relay in configured_relays {
            let status = if !configured_relay.is_active {
                RelayConnectionStatus::Disabled
            } else {
                RelayConnectionStatus::Disconnected
            };
            
            relay_statuses.push(RelayStatus {
                url: configured_relay.url,
                is_active: configured_relay.is_active,
                status,
            });
        }
    }
    
    Ok(relay_statuses)
}

#[tauri::command]
async fn test_relay_connection(relay_url: String) -> Result<bool, String> {
    println!("[RUST] test_relay_connection called for: {}", relay_url);
    
    use nostr_sdk::prelude::*;
    
    // Create a temporary client to test the connection
    let keys = Keys::generate(); // Use ephemeral keys for testing
    let client = Client::new(keys);
    
    // Try to add and connect to the relay
    match client.add_relay(&relay_url).await {
        Ok(_) => {
            println!("[RUST] Successfully added relay: {}", relay_url);
            
            // Try to connect
            client.connect().await;
            
            // Wait a moment for connection to establish
            tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
            
            // Check if connected
            let relays = client.relays().await;
            let is_connected = relays.iter().any(|(url, _)| url.to_string() == relay_url);
            
            // Disconnect the test client
            client.disconnect().await;
            
            println!("[RUST] Relay {} connection test result: {}", relay_url, is_connected);
            Ok(is_connected)
        },
        Err(e) => {
            println!("[RUST] Failed to add relay {}: {}", relay_url, e);
            Err(format!("Failed to connect to relay: {}", e))
        }
    }
}

#[tauri::command]
fn decrypt_dm_content(private_key: String, sender_pubkey: String, encrypted_content: String) -> Result<String, String> {
    println!("[RUST] decrypt_dm_content called");
    println!("[RUST] Decrypting with sender_pubkey: {}", sender_pubkey);
    println!("[RUST] Encrypted content: {}", encrypted_content);
    let result = nostr::decrypt_dm_content(&private_key, &sender_pubkey, &encrypted_content);
    match &result {
        Ok(decrypted) => println!("[RUST] Decryption successful: {}", decrypted),
        Err(e) => println!("[RUST] Decryption failed: {}", e),
    }
    result.map_err(|e| e.to_string())
}

#[tauri::command]
async fn publish_nostr_event(private_key: String, content: String, kind: u16, tags: Vec<Vec<String>>, relays: Vec<String>) -> Result<(), String> {
    println!("[RUST] publish_nostr_event called");
    nostr::publish_event(&private_key, &content, kind, tags, &relays)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_email(email_config: EmailConfig, to_address: String, subject: String, body: String, nostr_npub: Option<String>, message_id: Option<String>, attachments: Option<Vec<crate::types::EmailAttachment>>, _state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] send_email called with {} attachments", attachments.as_ref().map(|a| a.len()).unwrap_or(0));
    
    // Send the email via SMTP
    // Note: We don't save to database here - sent emails will be fetched from the server's sent folder via IMAP sync
    // This avoids duplicate entries and ensures we have the server's version with proper headers
    email::send_email(&email_config, &to_address, &subject, &body, nostr_npub.as_deref(), message_id.as_deref(), attachments.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    
    println!("[RUST] send_email: Email sent successfully. It will appear in sent folder after IMAP sync.");
    
    Ok(())
}

#[tauri::command]
async fn construct_email_headers(email_config: EmailConfig, to_address: String, subject: String, body: String, nostr_npub: Option<String>, message_id: Option<String>, attachments: Option<Vec<crate::types::EmailAttachment>>) -> Result<String, String> {
    println!("[RUST] construct_email_headers called with {} attachments", attachments.as_ref().map(|a| a.len()).unwrap_or(0));
    email::construct_email_headers(&email_config, &to_address, &subject, &body, nostr_npub.as_deref(), message_id.as_deref(), attachments.as_ref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_emails(email_config: EmailConfig, limit: usize, search_query: Option<String>, only_nostr: bool, require_signature: Option<bool>, state: tauri::State<'_, AppState>) -> Result<Vec<EmailMessage>, String> {
    println!("[RUST] fetch_emails called with search: {:?}, only_nostr: {}, require_signature: {:?}", search_query, only_nostr, require_signature);
    
    // Get latest email date from database
    let db = state.get_database().map_err(|e| e.to_string())?;
    let latest = db.get_latest_email_received_at().map_err(|e: rusqlite::Error| e.to_string())?;
    println!("[RUST] fetch_emails: Latest email date from DB: {:?}", latest);
    
    // Get sync_cutoff_days setting (similar to sync_nostr_emails_to_db)
    let sync_cutoff_days = match db.find_pubkeys_by_email_setting(&email_config.email_address) {
        Ok(pubkeys) => {
            // Try to get sync_cutoff_days from the first matching pubkey
            let mut cutoff = 365i64; // Default
            for pubkey in pubkeys {
                if let Ok(Some(value)) = db.get_setting(&pubkey, "sync_cutoff_days") {
                    if let Ok(parsed) = value.parse::<i64>() {
                        cutoff = parsed;
                        break; // Use first found setting
                    }
                }
            }
            Some(cutoff)
        }
        Err(_) => Some(365i64), // Default if we can't find pubkey
    };
    
    // Get require_signature setting if not provided (default: true)
    let require_signature = if let Some(req) = require_signature {
        req
    } else {
        // Try to get from user settings (default: true)
        match db.find_pubkeys_by_email_setting(&email_config.email_address) {
            Ok(pubkeys) => {
                let mut req_sig = true; // Default to true
                for pubkey in pubkeys {
                    if let Ok(Some(value)) = db.get_setting(&pubkey, "require_signature") {
                        req_sig = value == "true";
                        break; // Use first found setting
                    }
                }
                req_sig
            }
            Err(_) => true, // Default if we can't find pubkey
        }
    };
    
    let mut emails = email::fetch_emails(&email_config, limit, search_query, only_nostr, latest, sync_cutoff_days).await.map_err(|e| e.to_string())?;
    
    // Filter emails based on signature requirement
    if require_signature {
        emails.retain(|email| {
            // If email has X-Nostr-Pubkey header, it must have a valid signature
            if email.sender_pubkey.is_some() {
                // Email has pubkey, check signature
                if let Some(valid) = email.signature_valid {
                    valid // Only keep if signature is valid
                } else {
                    false // Reject emails without signature when require_signature is true
                }
            } else {
                // No pubkey header, allow (not a nostr email)
                true
            }
        });
    }
    // If require_signature is false, accept all emails regardless of signature
    
    Ok(emails)
}

#[tauri::command]
async fn fetch_image(url: String) -> Result<String, String> {
    println!("[RUST] fetch_image called for url: {}", url);
    nostr::fetch_image_as_data_url(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_multiple_images(urls: Vec<String>) -> Result<std::collections::HashMap<String, String>, String> {
    println!("[RUST] fetch_multiple_images called for {} urls", urls.len());
    nostr::fetch_multiple_images_as_data_urls(&urls).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn cache_profile_image(pubkey: String, data_url: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] cache_profile_image called for pubkey: {}", pubkey);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cached_image = state::CachedImage {
        data_url: data_url.clone(),
        timestamp,
    };
    state.image_cache.lock().unwrap().insert(pubkey.clone(), cached_image);
    println!("[RUST] Cached image for pubkey: {}", pubkey);
    // Persist to database as well
    if let Ok(db) = state.get_database() {
        if let Err(e) = db.update_contact_picture_data_url(&pubkey, &data_url) {
            println!("[RUST] Failed to persist picture_data_url to DB for pubkey {}: {}", pubkey, e);
        } else {
            println!("[RUST] Persisted picture_data_url to DB for pubkey: {}", pubkey);
        }
    }
    Ok(())
}

#[tauri::command]
fn get_cached_profile_image(pubkey: String, state: tauri::State<AppState>) -> Result<Option<String>, String> {
    println!("[RUST] get_cached_profile_image called for pubkey: {}", pubkey);
    
    let cache = state.image_cache.lock().unwrap();
    if let Some(cached_image) = cache.get(&pubkey) {
        // Check if cache is still valid (24 hours)
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let cache_age = current_time - cached_image.timestamp;
        let max_age = 24 * 60 * 60; // 24 hours in seconds
        
        if cache_age < max_age {
            println!("[RUST] Found valid cached image for pubkey: {}", pubkey);
            Ok(Some(cached_image.data_url.clone()))
        } else {
            println!("[RUST] Cached image expired for pubkey: {}", pubkey);
            Ok(None)
        }
    } else {
        println!("[RUST] No cached image found for pubkey: {}", pubkey);
        Ok(None)
    }
}

#[tauri::command]
fn clear_image_cache(state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] clear_image_cache called");
    state.image_cache.lock().unwrap().clear();
    println!("[RUST] Image cache cleared");
    Ok(())
}

#[tauri::command]
async fn fetch_profiles(pubkeys: Vec<String>, relays: Vec<String>) -> Result<Vec<ProfileResult>, String> {
    println!("[RUST] fetch_profiles called for {} pubkeys", pubkeys.len());
    let mut profiles = Vec::new();
    
    for pubkey in pubkeys {
        match fetch_profile(pubkey.clone(), relays.clone()).await {
            Ok(Some(profile)) => profiles.push(profile),
            Ok(None) => {
                // Create a placeholder profile for contacts without profiles
                profiles.push(ProfileResult {
                    pubkey: pubkey.clone(),
                    fields: std::collections::HashMap::new(),
                    raw_content: "{}".to_string(),
                });
            },
            Err(e) => {
                println!("[RUST] Error fetching profile for {}: {}", pubkey, e);
                // Create a placeholder profile for failed fetches
                profiles.push(ProfileResult {
                    pubkey: pubkey.clone(),
                    fields: std::collections::HashMap::new(),
                    raw_content: "{}".to_string(),
                });
            }
        }
    }
    
    Ok(profiles)
}

#[tauri::command]
async fn fetch_profiles_persistent(pubkeys: Vec<String>, state: tauri::State<'_, AppState>) -> Result<Vec<ProfileResult>, String> {
    println!("[RUST] fetch_profiles_persistent called for {} pubkeys", pubkeys.len());
    
    if pubkeys.is_empty() {
        return Ok(vec![]);
    }
    
    // Check database first for existing contacts
    let db = state.get_database()?;
    let mut results = Vec::new();
    let mut pubkeys_to_fetch = Vec::new();
    let mut public_keys_to_fetch = Vec::new();
    
    println!("[RUST] Checking database for existing contacts...");
    for pubkey_str in &pubkeys {
        match db.get_contact(pubkey_str) {
            Ok(Some(contact)) => {
                // Check if contact has profile data (name, email, picture_url, or about)
                let has_profile_data = contact.name.is_some() 
                    || contact.email.is_some() 
                    || contact.picture_url.is_some() 
                    || contact.about.is_some();
                
                if has_profile_data {
                    // Convert Contact to ProfileResult
                    let mut fields = std::collections::HashMap::new();
                    if let Some(name) = &contact.name {
                        fields.insert("name".to_string(), serde_json::Value::String(name.clone()));
                        fields.insert("display_name".to_string(), serde_json::Value::String(name.clone()));
                    }
                    if let Some(email) = &contact.email {
                        fields.insert("email".to_string(), serde_json::Value::String(email.clone()));
                    }
                    if let Some(picture) = &contact.picture_url {
                        fields.insert("picture".to_string(), serde_json::Value::String(picture.clone()));
                    }
                    if let Some(about) = &contact.about {
                        fields.insert("about".to_string(), serde_json::Value::String(about.clone()));
                    }
                    
                    // Create raw_content JSON from fields
                    let raw_content = serde_json::to_string(&fields)
                        .unwrap_or_else(|_| "{}".to_string());
                    
                    results.push(ProfileResult {
                        pubkey: pubkey_str.clone(),
                        fields: fields,
                        raw_content: raw_content,
                    });
                    
                    println!("[RUST] Using cached profile from database for: {}", pubkey_str);
                    continue;
                }
            },
            Ok(None) => {
                // Contact doesn't exist in database
            },
            Err(e) => {
                println!("[RUST] Error checking database for {}: {}", pubkey_str, e);
            }
        }
        
        // Need to fetch this profile from relays
        pubkeys_to_fetch.push(pubkey_str.clone());
    }
    
    println!("[RUST] Found {} profiles in database, need to fetch {} from relays", 
        results.len(), pubkeys_to_fetch.len());
    
    // If all profiles were found in database, return early
    if pubkeys_to_fetch.is_empty() {
        println!("[RUST] All profiles found in database, skipping relay fetch");
        return Ok(results);
    }
    
    // Get the persistent client for fetching remaining profiles
    let client = {
        let client_guard = state.nostr_client.lock().unwrap();
        match client_guard.as_ref() {
            Some(client) => client.clone(),
            None => return Err("Persistent Nostr client not initialized".to_string()),
        }
    };
    
    // Parse pubkeys that need fetching
    let parsed_keys: Result<Vec<PublicKey>, String> = pubkeys_to_fetch.iter()
        .map(|pubkey_str| {
            PublicKey::from_bech32(pubkey_str)
                .or_else(|_| PublicKey::from_hex(pubkey_str))
                .map_err(|e| format!("Invalid pubkey {}: {}", pubkey_str, e))
        })
        .collect();
    
    let parsed_keys = parsed_keys?;
    public_keys_to_fetch = parsed_keys;
    
    println!("[RUST] Fetching {} profiles from relays", public_keys_to_fetch.len());
    
    // Fetch profiles for remaining pubkeys in one request using persistent client
    let profiles_filter = Filter::new()
        .authors(public_keys_to_fetch.clone())
        .kind(Kind::from(0)) // Profile events are kind 0
        .limit(1000); // Allow for multiple profiles
        
    let profile_events = client
        .fetch_events(profiles_filter, Duration::from_secs(30))
        .await
        .map_err(|e| e.to_string())?;
    
    println!("[RUST] Found {} profile events from relays", profile_events.len());
    
    // Process each pubkey that needed fetching
    for (i, pubkey_str) in pubkeys_to_fetch.iter().enumerate() {
        let public_key = &public_keys_to_fetch[i];
        
        // Find the latest profile event for this pubkey
        let latest_event = profile_events.iter()
            .filter(|event| event.pubkey == *public_key)
            .max_by_key(|event| event.created_at);
        
        if let Some(event) = latest_event {
            // Parse profile content
            match serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&event.content) {
                Ok(fields) => {
                    results.push(ProfileResult {
                        pubkey: pubkey_str.clone(),
                        fields: fields,
                        raw_content: event.content.clone(),
                    });
                    
                    println!("[RUST] Fetched profile from relay for: {}", pubkey_str);
                },
                Err(e) => {
                    println!("[RUST] Failed to parse profile JSON for {}: {}", pubkey_str, e);
                    results.push(ProfileResult {
                        pubkey: pubkey_str.clone(),
                        fields: std::collections::HashMap::new(),
                        raw_content: event.content.clone(),
                    });
                }
            }
        } else {
            println!("[RUST] No profile found in relay for pubkey: {}", pubkey_str);
            results.push(ProfileResult {
                pubkey: pubkey_str.clone(),
                fields: std::collections::HashMap::new(),
                raw_content: "{}".to_string(),
            });
        }
    }
    
    println!("[RUST] fetch_profiles_persistent returning {} profiles ({} from DB, {} from relays)", 
        results.len(), results.len() - pubkeys_to_fetch.len(), pubkeys_to_fetch.len());
    Ok(results)
}

// Storage commands
#[tauri::command]
fn storage_save_contacts(contacts: Vec<Contact>) -> Result<(), String> {
    println!("[RUST] storage_save_contacts called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.save_contacts(contacts).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_get_contacts() -> Result<Vec<Contact>, String> {
    println!("[RUST] storage_get_contacts called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_contacts().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_clear_contacts() -> Result<(), String> {
    println!("[RUST] storage_clear_contacts called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.clear_contacts().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_save_conversations(conversations: Vec<Conversation>) -> Result<(), String> {
    println!("[RUST] storage_save_conversations called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.save_conversations(conversations).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_get_conversations() -> Result<Vec<Conversation>, String> {
    println!("[RUST] storage_get_conversations called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_conversations().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_clear_conversations() -> Result<(), String> {
    println!("[RUST] storage_clear_conversations called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.clear_conversations().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_save_user_profile(profile: UserProfile) -> Result<(), String> {
    println!("[RUST] storage_save_user_profile called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.save_user_profile(profile).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_get_user_profile() -> Result<Option<UserProfile>, String> {
    println!("[RUST] storage_get_user_profile called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_user_profile().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_clear_user_profile() -> Result<(), String> {
    println!("[RUST] storage_clear_user_profile called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.clear_user_profile().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_save_settings(settings: AppSettings) -> Result<(), String> {
    println!("[RUST] storage_save_settings called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.save_settings(settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_get_settings() -> Result<Option<AppSettings>, String> {
    println!("[RUST] storage_get_settings called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_save_email_draft(draft: EmailDraft) -> Result<(), String> {
    println!("[RUST] storage_save_email_draft called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.save_email_draft(draft).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_get_email_draft(id: String) -> Result<Option<EmailDraft>, String> {
    println!("[RUST] storage_get_email_draft called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_email_draft(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_clear_email_draft(id: String) -> Result<(), String> {
    println!("[RUST] storage_clear_email_draft called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.clear_email_draft(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_save_relays(relays: Vec<storage::Relay>) -> Result<(), String> {
    println!("[RUST] storage_save_relays called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.save_relays(relays).map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_get_relays() -> Result<Vec<storage::Relay>, String> {
    println!("[RUST] storage_get_relays called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_relays().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_clear_all_data() -> Result<(), String> {
    println!("[RUST] storage_clear_all_data called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.clear_all_data().map_err(|e| e.to_string())
}

#[tauri::command]
fn storage_get_data_size() -> Result<u64, String> {
    println!("[RUST] storage_get_data_size called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_data_size().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_contacts() -> Result<Vec<storage::Contact>, String> {
    println!("[RUST] get_contacts called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    let contacts = storage.get_contacts().map_err(|e| e.to_string())?;
    println!("[RUST] Retrieved {} contacts from storage", contacts.len());
    Ok(contacts)
}

#[tauri::command]
fn set_contacts(contacts: Vec<storage::Contact>) -> Result<(), String> {
    println!("[RUST] set_contacts called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    println!("[RUST] Saving {} contacts to storage", contacts.len());
    storage.save_contacts(contacts).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_contact(contact: storage::Contact) -> Result<(), String> {
    println!("[RUST] save_contact called for {}", contact.pubkey);
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.save_contact(contact).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_contact(pubkey: String) -> Result<Option<storage::Contact>, String> {
    println!("[RUST] get_contact called for {}", pubkey);
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_contact(&pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_contact_picture_data_url(pubkey: String, picture_data_url: String) -> Result<(), String> {
    println!("[RUST] update_contact_picture_data_url called for {}", pubkey);
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.update_contact_picture_data_url(&pubkey, picture_data_url).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_conversations() -> Result<Vec<Conversation>, String> {
    println!("[RUST] get_conversations called");
    let storage = Storage::new().map_err(|e| e.to_string())?;
    storage.get_conversations().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_conversations(conversations: Vec<Conversation>) -> Result<(), String> {
    println!("[RUST] set_conversations called with {} conversations", conversations.len());
    let storage = Storage::new().map_err(|e| e.to_string())?;
    println!("[RUST] set_conversations: Storage initialized, saving conversations...");
    let result = storage.save_conversations(conversations).map_err(|e| e.to_string());
    println!("[RUST] set_conversations: Save operation completed");
    result
}

#[tauri::command]
async fn test_imap_connection(email_config: EmailConfig) -> Result<(), String> {
    println!("[RUST] test_imap_connection called for: {}@{}:{}", 
        email_config.email_address, email_config.imap_host, email_config.imap_port);
    email::test_imap_connection(&email_config)
        .await
        .map_err(|e| {
            println!("[RUST] test_imap_connection failed: {}", e);
            e.to_string()
        })
}

#[tauri::command]
async fn test_smtp_connection(email_config: EmailConfig) -> Result<(), String> {
    email::test_smtp_connection(&email_config)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_message_confirmation(event_id: String, relays: Vec<String>) -> Result<bool, String> {
    println!("[RUST] check_message_confirmation called for event: {}", event_id);
    nostr::check_message_confirmation(&event_id, &relays)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn generate_qr_code(data: String, _size: Option<u32>) -> Result<String, String> {
    println!("[RUST] generate_qr_code called with data: '{}', length: {}", data, data.len());
    let qr = match qrcode::QrCode::new(data.as_bytes()) {
        Ok(qr) => qr,
        Err(e) => {
            println!("[RUST] QR code generation failed: {:?}", e);
            return Err(e.to_string());
        }
    };
    // Render to SVG string
    let svg = qr.render::<qrcode::render::svg::Color>().build();
    println!("[RUST] QR code SVG generated, length: {}", svg.len());
    let data_url = format!("data:image/svg+xml;utf8,{}", urlencoding::encode(&svg));
    println!("[RUST] Returning data_url, length: {}", data_url.len());
    Ok(data_url)
}

/// Get the app data directory path, handling Android-specific paths
fn get_app_data_dir() -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        // On Android, apps can write to their internal files directory
        // Path: /data/data/<package_name>/files/
        // Package name: com.nostr.mail
        // Android apps have write access to this directory by default
        
        let app_files_dir = std::path::PathBuf::from("/data/data/com.nostr.mail/files");
        let app_data_dir = app_files_dir.join("nostr-mail");
        
        // The files directory should exist, but create it if it doesn't
        // (This shouldn't fail as Android creates it automatically)
        std::fs::create_dir_all(&app_files_dir)
            .map_err(|e| format!("Cannot access app files directory {:?}: {}", app_files_dir, e))?;
        
        Ok(app_data_dir)
    }
    
    #[cfg(not(target_os = "android"))]
    {
        // On desktop platforms, use dirs crate
        dirs::data_dir()
            .ok_or_else(|| "Could not get app data directory".to_string())
            .map(|d| d.join("nostr-mail"))
    }
}

#[tauri::command]
fn init_database(state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] init_database called");
    
    // Get app data directory
    let app_dir = get_app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    println!("[RUST] App data directory: {:?}", app_dir);
    
    // Create directory if it doesn't exist
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create app data directory {:?}: {} (check permissions)", app_dir, e))?;
    println!("[RUST] App data directory created/verified: {:?}", app_dir);
    
    // Construct database path
    let db_path = app_dir.join("nostr_mail.db");
    println!("[RUST] Database path: {:?}", db_path);
    
    // Initialize database
    state.init_database(&db_path)
        .map_err(|e| format!("Failed to initialize database at {:?}: {} (check file permissions and disk space)", db_path, e))
}

// Database commands for contacts
#[tauri::command]
fn db_save_contact(contact: DbContact, state: tauri::State<AppState>) -> Result<i64, String> {
    println!("[RUST] db_save_contact called");
    let db = state.get_database()?;
    db.save_contact(&contact).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_contact(pubkey: String, state: tauri::State<AppState>) -> Result<Option<DbContact>, String> {
    println!("[RUST] db_get_contact called for pubkey: {}", pubkey);
    let db = state.get_database()?;
    db.get_contact(&pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_all_contacts(user_pubkey: String, state: tauri::State<AppState>) -> Result<Vec<DbContact>, String> {
    println!("[RUST] db_get_all_contacts called for user_pubkey: {}", user_pubkey);
    let db = state.get_database()?;
    db.get_all_contacts(&user_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_delete_contact(pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_delete_contact called for pubkey: {}", pubkey);
    let db = state.get_database()?;
    db.delete_contact(&pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_add_user_contact(user_pubkey: String, contact_pubkey: String, is_public: bool, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_add_user_contact called: user={}, contact={}, is_public={}", user_pubkey, contact_pubkey, is_public);
    let db = state.get_database()?;
    db.add_user_contact(&user_pubkey, &contact_pubkey, is_public).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_remove_user_contact(user_pubkey: String, contact_pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_remove_user_contact called: user={}, contact={}", user_pubkey, contact_pubkey);
    let db = state.get_database()?;
    db.remove_user_contact(&user_pubkey, &contact_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_public_contact_pubkeys(user_pubkey: String, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    println!("[RUST] db_get_public_contact_pubkeys called for user: {}", user_pubkey);
    let db = state.get_database()?;
    db.get_public_contact_pubkeys(&user_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_update_user_contact_public_status(user_pubkey: String, contact_pubkey: String, is_public: bool, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_update_user_contact_public_status called: user={}, contact={}, is_public={}", user_pubkey, contact_pubkey, is_public);
    let db = state.get_database()?;
    db.update_user_contact_public_status(&user_pubkey, &contact_pubkey, is_public).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_find_pubkeys_by_email(email: String, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let db = state.get_database()?;
    db.find_pubkeys_by_email(&email).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_find_pubkeys_by_email_including_dms(email: String, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let db = state.get_database()?;
    db.find_pubkeys_by_email_including_dms(&email).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_filter_new_contacts(user_pubkey: String, pubkeys: Vec<String>, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let db = state.get_database()?;
    let existing: std::collections::HashSet<String> = db.get_all_contacts(&user_pubkey)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|c| c.pubkey)
        .collect();
    let new: Vec<String> = pubkeys.into_iter().filter(|pk| !existing.contains(pk)).collect();
    Ok(new)
}

// Database commands for emails
#[tauri::command]
fn db_save_email(email: DbEmail, state: tauri::State<AppState>) -> Result<i64, String> {
    println!("[RUST] db_save_email called");
    let db = state.get_database()?;
    db.save_email(&email).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_email(message_id: String, state: tauri::State<AppState>) -> Result<Option<DbEmail>, String> {
    println!("[RUST] db_get_email called for message_id: {}", message_id);
    let db = state.get_database()?;
    db.get_email(&message_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_emails(limit: Option<i64>, offset: Option<i64>, nostr_only: Option<bool>, user_email: Option<String>, state: tauri::State<AppState>) -> Result<Vec<EmailMessage>, String> {
    println!("[RUST] db_get_emails called");
    if let Some(ref email) = user_email {
        println!("[RUST] db_get_emails: Filtering for user_email: {}", email);
    } else {
        println!("[RUST] db_get_emails: No user_email filter provided");
    }
    let db = state.get_database()?;
    let emails = db.get_emails(limit, offset, nostr_only, user_email.as_deref()).map_err(|e| e.to_string())?;
    let mapped: Vec<EmailMessage> = emails.iter().map(map_db_email_to_email_message).collect();
    println!("[RUST] Sending {} emails to frontend:", mapped.len());
    for (i, email) in mapped.iter().enumerate() {
        println!("[RUST] Email {}: {:#?}", i + 1, email);
    }
    Ok(mapped)
}

#[tauri::command]
async fn db_search_emails(
    search_query: String, 
    user_email: Option<String>, 
    private_key: Option<String>, 
    limit: Option<i64>,
    offset: Option<i64>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>
) -> Result<(usize, bool), String> {
    let page_size = limit.unwrap_or(50) as usize;
    let skip_count = offset.unwrap_or(0) as usize;
    println!("[RUST] db_search_emails called with query: '{}', limit: {}, offset: {}", search_query, page_size, skip_count);
    let db = state.get_database()?;
    
    // Emit search started event
    if let Err(e) = app_handle.emit("email-search-started", &serde_json::json!({})) {
        println!("[RUST] Failed to emit search-started event: {}", e);
    }
    
    // Process emails in batches - fetch more than we need since we filter after decryption
    let batch_size = (page_size * 3).max(100); // Fetch 3x the page size or at least 100
    let mut db_offset: i64 = 0;
    let search_query_lower = search_query.to_lowercase();
    let mut match_count = 0;
    let mut skipped_count = 0;
    let mut has_more = false;
    
    // Create email config for decryption
    let email_config = crate::types::EmailConfig {
        email_address: "".to_string(),
        password: "".to_string(),
        smtp_host: "".to_string(),
        smtp_port: 0,
        imap_host: "".to_string(),
        imap_port: 0,
        use_tls: false,
        private_key,
    };
    
    // Process emails in batches until we have enough results
    loop {
        let batch_emails = db.get_emails(Some(batch_size as i64), Some(db_offset), Some(true), user_email.as_deref())
            .map_err(|e| e.to_string())?;
        
        if batch_emails.is_empty() {
            break; // No more emails to process
        }
        
        for (index, email) in batch_emails.iter().enumerate() {
            // Emit progress update every 10 emails
            let processed = db_offset + index as i64;
            if processed % 10 == 0 {
                let progress = serde_json::json!({
                    "processed": processed,
                });
                if let Err(e) = app_handle.emit("email-search-progress", &progress) {
                    println!("[RUST] Failed to emit search-progress event: {}", e);
                }
                // Yield to allow UI updates
                tokio::task::yield_now().await;
            }
            
            // Stop if we've found enough results
            if match_count >= page_size {
                has_more = true; // There might be more results
                break;
            }
        
        // Decrypt subject and body for inbox emails
        // For inbox emails, shared secret = user's private key × sender's public key
        // So we use sender's pubkey (extracted from headers) for decryption
        let raw_headers = email.raw_headers.as_deref().unwrap_or("");
        let (decrypted_subject, decrypted_body) = if email.is_nostr_encrypted && email_config.private_key.is_some() {
            // decrypt_nostr_email_content extracts sender_pubkey from headers and uses it
            // Shared secret derivation: user's private key × sender's public key
            match email::decrypt_nostr_email_content(&email_config, raw_headers, &email.subject, &email.body) {
                Ok((subj, body)) => (subj, body),
                Err(e) => {
                    println!("[RUST] Failed to decrypt inbox email {}: {}", email.id.unwrap_or(0), e);
                    (email.subject.clone(), email.body.clone())
                }
            }
        } else {
            (email.subject.clone(), email.body.clone())
        };
        
        // Get attachments for this email and extract original filenames from manifest if available
        let mut attachment_filenames = Vec::new();
        if let Some(email_id) = email.id {
            match db.get_attachments_for_email(email_id) {
                Ok(attachments) => {
                    // Add encrypted filenames
                    attachment_filenames.extend(attachments.iter().map(|att| att.filename.to_lowercase()));
                    
                    // Try to extract original filenames from manifest if email has manifest-encrypted attachments
                    let has_manifest_attachments = attachments.iter().any(|att| att.encryption_method.as_deref() == Some("manifest_aes"));
                    if has_manifest_attachments && email_config.private_key.is_some() {
                        println!("[RUST] Inbox email {} has manifest-encrypted attachments, attempting to extract original filenames", email.id.unwrap_or(0));
                        println!("[RUST] Decrypted body length: {}, preview: {}", decrypted_body.len(), &decrypted_body.chars().take(200).collect::<String>());
                        
                        // Try to parse decrypted body as manifest JSON
                        match serde_json::from_str::<serde_json::Value>(&decrypted_body) {
                            Ok(manifest_json) => {
                                println!("[RUST] Successfully parsed decrypted body as JSON");
                                if let Some(manifest_obj) = manifest_json.as_object() {
                                    if let Some(attachments_array) = manifest_obj.get("attachments").and_then(|v| v.as_array()) {
                                        println!("[RUST] Found {} attachments in manifest", attachments_array.len());
                                        for attachment_obj in attachments_array {
                                            if let Some(orig_filename) = attachment_obj.get("orig_filename")
                                                .and_then(|v| v.as_str()) {
                                                println!("[RUST] Found original filename in manifest: {}", orig_filename);
                                                attachment_filenames.push(orig_filename.to_lowercase());
                                            } else {
                                                println!("[RUST] Attachment object missing orig_filename: {:?}", attachment_obj);
                                            }
                                        }
                                    } else {
                                        println!("[RUST] Manifest JSON does not have attachments array");
                                    }
                                } else {
                                    println!("[RUST] Manifest JSON is not an object");
                                }
                            }
                            Err(e) => {
                                println!("[RUST] Failed to parse decrypted body as JSON: {}", e);
                                println!("[RUST] Decrypted body (first 500 chars): {}", &decrypted_body.chars().take(500).collect::<String>());
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        }
        
        // Search in: from, to, subject, body, attachment filenames (both encrypted and decrypted)
        let from_matches = email.from_address.to_lowercase().contains(&search_query_lower);
        let to_matches = email.to_address.to_lowercase().contains(&search_query_lower);
        let subject_matches = decrypted_subject.to_lowercase().contains(&search_query_lower);
        let body_matches = decrypted_body.to_lowercase().contains(&search_query_lower);
        let attachment_matches = attachment_filenames.iter().any(|f| f.contains(&search_query_lower));
        
        if attachment_filenames.len() > 0 {
            println!("[RUST] Inbox email {} attachment filenames to search: {:?}", email.id.unwrap_or(0), attachment_filenames);
        }
        
        let matches = from_matches || to_matches || subject_matches || body_matches || attachment_matches;
        
        if matches {
            println!("[RUST] Inbox email {} matches search query '{}' (from: {}, to: {}, subject: {}, body: {}, attachments: {})", 
                email.id.unwrap_or(0), search_query, from_matches, to_matches, subject_matches, body_matches, attachment_matches);
        }
        
            if matches {
                // Skip results until we reach the offset
                if skipped_count < skip_count {
                    skipped_count += 1;
                    continue;
                }
                
                // Create EmailMessage with same format as normal emails (encrypted content, frontend will decrypt)
                // Use map_db_email_to_email_message to ensure consistent format
                let email_message = map_db_email_to_email_message(&email);
                
                // Emit this matching email immediately
                if let Err(e) = app_handle.emit("email-search-result", &email_message) {
                    println!("[RUST] Failed to emit search-result event: {}", e);
                }
                
                match_count += 1;
                
                // Stop if we've found enough results
                if match_count >= page_size {
                    has_more = true; // There might be more results
                    break;
                }
            }
        }
        
        // If we've found enough results or processed all emails, stop
        if match_count >= page_size || batch_emails.len() < batch_size {
            break;
        }
        
        db_offset += batch_size as i64;
    }
    
    // Emit search completed event
    let completion = serde_json::json!({
        "total_found": match_count,
        "has_more": has_more
    });
    if let Err(e) = app_handle.emit("email-search-completed", &completion) {
        println!("[RUST] Failed to emit search-completed event: {}", e);
    }
    
    println!("[RUST] Search found {} matching emails (has_more: {})", match_count, has_more);
    Ok((match_count, has_more))
}

#[tauri::command]
fn db_update_email_sender_pubkey(message_id: String, sender_pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.get_database()?;
    db.update_email_sender_pubkey(&message_id, &sender_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_update_email_sender_pubkey_by_id(id: i64, sender_pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_update_email_sender_pubkey_by_id called");
    let db = state.get_database().map_err(|e| e.to_string())?;
    db.update_email_sender_pubkey_by_id(id, &sender_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_update_email_recipient_pubkey(message_id: String, recipient_pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.get_database()?;
    db.update_email_recipient_pubkey(&message_id, &recipient_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_update_email_recipient_pubkey_by_id(id: i64, recipient_pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_update_email_recipient_pubkey_by_id called");
    let db = state.get_database().map_err(|e| e.to_string())?;
    db.update_email_recipient_pubkey_by_id(id, &recipient_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_find_emails_by_message_id(message_id: String, state: tauri::State<AppState>) -> Result<Vec<EmailMessage>, String> {
    println!("[RUST] db_find_emails_by_message_id called for message_id: {}", message_id);
    let db = state.get_database().map_err(|e| e.to_string())?;
    let emails = db.find_emails_by_message_id(&message_id).map_err(|e| e.to_string())?;
    let mapped: Vec<EmailMessage> = emails.iter().map(map_db_email_to_email_message).collect();
    Ok(mapped)
}

#[tauri::command]
fn db_get_sent_emails(limit: Option<i64>, offset: Option<i64>, user_email: Option<String>, state: tauri::State<AppState>) -> Result<Vec<EmailMessage>, String> {
    let db = state.get_database()?;
    let emails = db.get_sent_emails(limit, offset, user_email.as_deref()).map_err(|e| e.to_string())?;
    let mapped: Vec<EmailMessage> = emails.iter().map(map_db_email_to_email_message).collect();
    Ok(mapped)
}

#[tauri::command]
async fn db_search_sent_emails(
    search_query: String, 
    user_email: Option<String>, 
    private_key: Option<String>, 
    limit: Option<i64>,
    offset: Option<i64>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>
) -> Result<(usize, bool), String> {
    let page_size = limit.unwrap_or(50) as usize;
    let skip_count = offset.unwrap_or(0) as usize;
    println!("[RUST] db_search_sent_emails called with query: '{}', limit: {}, offset: {}", search_query, page_size, skip_count);
    let db = state.get_database()?;
    
    // Emit search started event
    if let Err(e) = app_handle.emit("sent-search-started", &serde_json::json!({})) {
        println!("[RUST] Failed to emit sent-search-started event: {}", e);
    }
    
    // Process emails in batches - fetch more than we need since we filter after decryption
    let batch_size = (page_size * 3).max(100); // Fetch 3x the page size or at least 100
    let mut db_offset: i64 = 0;
    let search_query_lower = search_query.to_lowercase();
    let mut match_count = 0;
    let mut skipped_count = 0;
    let mut has_more = false;
    
    // Create email config for decryption
    let email_config = crate::types::EmailConfig {
        email_address: "".to_string(),
        password: "".to_string(),
        smtp_host: "".to_string(),
        smtp_port: 0,
        imap_host: "".to_string(),
        imap_port: 0,
        use_tls: false,
        private_key,
    };
    
    // Process emails in batches until we have enough results
    loop {
        let batch_emails = db.get_sent_emails(Some(batch_size as i64), Some(db_offset), user_email.as_deref())
            .map_err(|e| e.to_string())?;
        
        if batch_emails.is_empty() {
            break; // No more emails to process
        }
        
        for (index, email) in batch_emails.iter().enumerate() {
            // Emit progress update every 10 emails
            let processed = db_offset + index as i64;
            if processed % 10 == 0 {
                let progress = serde_json::json!({
                    "processed": processed,
                });
                if let Err(e) = app_handle.emit("sent-search-progress", &progress) {
                    println!("[RUST] Failed to emit sent-search-progress event: {}", e);
                }
                // Yield to allow UI updates
                tokio::task::yield_now().await;
            }
            
            // Stop if we've found enough results
            if match_count >= page_size {
                has_more = true; // There might be more results
                break;
            }
            
            // For sent emails, decrypt using recipient's pubkey
            // Shared secret derivation: user's private key × recipient's public key
        let _raw_headers = email.raw_headers.as_deref().unwrap_or("");
        let (decrypted_subject, decrypted_body) = if email.is_nostr_encrypted && email_config.private_key.is_some() {
            // For sent emails, shared secret = user's private key × recipient's public key
            // So we need the recipient's pubkey to decrypt (not sender's pubkey)
            // Try to find recipient pubkey from contacts
            let recipient_email = &email.to_address;
            let mut decrypted_body_result = None;
            let mut decrypted_subject_result = email.subject.clone();
            
            // Try to find recipient pubkeys (including DMs)
            println!("[RUST] Searching for recipient pubkeys for email: {}", recipient_email);
            if let Ok(recipient_pubkeys) = db.find_pubkeys_by_email_including_dms(recipient_email) {
                println!("[RUST] Found {} recipient pubkey(s) for {} (including DMs)", recipient_pubkeys.len(), recipient_email);
                // Also try normalized Gmail address
                let normalized_email = if recipient_email.contains("@gmail.com") {
                    let parts: Vec<&str> = recipient_email.split('@').collect();
                    if parts.len() == 2 {
                        let local = parts[0].split('+').next().unwrap_or(parts[0]);
                        format!("{}@{}", local, parts[1]).to_lowercase()
                    } else {
                        recipient_email.to_lowercase()
                    }
                } else {
                    recipient_email.to_lowercase()
                };
                
                let mut all_pubkeys = recipient_pubkeys;
                if normalized_email != recipient_email.to_lowercase() {
                    println!("[RUST] Also searching for normalized email: {}", normalized_email);
                    if let Ok(normalized_pubkeys) = db.find_pubkeys_by_email_including_dms(&normalized_email) {
                        println!("[RUST] Found {} pubkey(s) for normalized email (including DMs)", normalized_pubkeys.len());
                        all_pubkeys.extend(normalized_pubkeys);
                    }
                }
                all_pubkeys.dedup();
                println!("[RUST] Total unique recipient pubkeys to try: {}", all_pubkeys.len());
                
                // Extract encrypted content from body (remove ASCII armor if present)
                let encrypted_content = match extract_encrypted_content_from_armor(&email.body) {
                    Some(content) => {
                        println!("[RUST] Extracted encrypted content from ASCII armor, length: {}", content.len());
                        content
                    },
                    None => {
                        println!("[RUST] No ASCII armor found, using raw body");
                        email.body.clone()
                    }
                };
                
                // Try decrypting with each recipient pubkey
                // Shared secret = user's private key × recipient's public key
                for recipient_pubkey in &all_pubkeys {
                    println!("[RUST] Trying to decrypt sent email body with recipient pubkey (shared secret: user_privkey × recipient_pubkey): {}", recipient_pubkey);
                    match nostr::decrypt_dm_content(
                        email_config.private_key.as_ref().unwrap(),
                        recipient_pubkey, // Using recipient's pubkey for shared secret derivation
                        &encrypted_content
                    ) {
                        Ok(decrypted) => {
                            println!("[RUST] Successfully decrypted sent email body with recipient pubkey, length: {}", decrypted.len());
                            // Save the recipient pubkey to database for future use
                            if !email.message_id.is_empty() {
                                if let Err(e) = db.update_email_recipient_pubkey(&email.message_id, recipient_pubkey) {
                                    println!("[RUST] Warning: Failed to save recipient pubkey to database: {}", e);
                                } else {
                                    println!("[RUST] Saved recipient pubkey {} to database for email {}", recipient_pubkey, email.message_id);
                                }
                            } else if let Some(email_id) = email.id {
                                if let Err(e) = db.update_email_recipient_pubkey_by_id(email_id, recipient_pubkey) {
                                    println!("[RUST] Warning: Failed to save recipient pubkey to database: {}", e);
                                } else {
                                    println!("[RUST] Saved recipient pubkey {} to database for email id {}", recipient_pubkey, email_id);
                                }
                            }
                            decrypted_body_result = Some(decrypted);
                            break;
                        }
                        Err(e) => {
                            println!("[RUST] Failed to decrypt with recipient pubkey {}: {}", recipient_pubkey, e);
                        }
                    }
                }
            } else {
                println!("[RUST] No recipient pubkeys found for email: {}", recipient_email);
            }
            
            // Fallback: Only for self-sent emails where recipient_pubkey lookup failed AND sender_pubkey == user's pubkey
            // For self-sent emails with same keypair: sender_pubkey == recipient_pubkey == user's pubkey
            // Shared secret = user's private key × sender_pubkey (which equals recipient_pubkey for self-sent)
            if decrypted_body_result.is_none() {
                println!("[RUST] No recipient pubkey decryption succeeded, checking if this is a self-sent email {}", email.id.unwrap_or(0));
                
                // Get user's public key from private key to verify if this is truly self-sent
                if let Ok(user_pubkey) = crypto::get_public_key_from_private(email_config.private_key.as_ref().unwrap()) {
                    // Only try sender_pubkey if it matches user's pubkey (truly self-sent with same keypair)
                    if let Some(sender_pubkey) = email.sender_pubkey.as_ref() {
                        if sender_pubkey == &user_pubkey {
                        println!("[RUST] Confirmed self-sent email (sender_pubkey == user_pubkey), trying sender_pubkey as recipient: {}", sender_pubkey);
                        
                        // Extract encrypted content from body (remove ASCII armor if present)
                        let encrypted_body_content = match extract_encrypted_content_from_armor(&email.body) {
                            Some(content) => {
                                println!("[RUST] Extracted encrypted content from ASCII armor for fallback, length: {}", content.len());
                                content
                            },
                            None => {
                                println!("[RUST] No ASCII armor found in body for fallback, using raw body");
                                email.body.clone()
                            }
                        };
                        
                        // Try decrypting body with sender_pubkey (only works for self-sent emails with same keypair)
                        // Shared secret = user's private key × sender_pubkey (same as recipient_pubkey for self-sent)
                        match nostr::decrypt_dm_content(
                            email_config.private_key.as_ref().unwrap(),
                            sender_pubkey, // For self-sent: sender_pubkey == recipient_pubkey == user_pubkey
                            &encrypted_body_content
                        ) {
                            Ok(decrypted) => {
                                println!("[RUST] Successfully decrypted sent email body with sender_pubkey (self-sent email with same keypair), length: {}", decrypted.len());
                                decrypted_body_result = Some(decrypted);
                                
                                // Also try to decrypt subject with sender_pubkey (same shared secret derivation)
                                if email::is_likely_encrypted_content(&email.subject) {
                                    let encrypted_subject_content = match extract_encrypted_content_from_armor(&email.subject) {
                                        Some(content) => content,
                                        None => email.subject.clone()
                                    };
                                    
                                    match nostr::decrypt_dm_content(
                                        email_config.private_key.as_ref().unwrap(),
                                        sender_pubkey, // Same shared secret: user_privkey × sender_pubkey
                                        &encrypted_subject_content
                                    ) {
                                        Ok(decrypted_subj) => {
                                            println!("[RUST] Successfully decrypted subject with sender_pubkey");
                                            decrypted_subject_result = decrypted_subj;
                                        }
                                        Err(_) => {
                                            println!("[RUST] Failed to decrypt subject with sender_pubkey, keeping original");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                println!("[RUST] Failed to decrypt with sender_pubkey fallback: {}", e);
                                println!("[RUST] Using encrypted body for search (search may not find decrypted content)");
                                decrypted_body_result = Some(email.body.clone());
                            }
                        }
                        } else {
                            println!("[RUST] sender_pubkey ({}) != user_pubkey ({}), this is NOT a self-sent email with same keypair", sender_pubkey, user_pubkey);
                            println!("[RUST] Cannot decrypt without recipient_pubkey. Recipient's pubkey must be in contacts for this email address: {}", recipient_email);
                            println!("[RUST] Using encrypted body for search (search may not find decrypted content)");
                            decrypted_body_result = Some(email.body.clone());
                        }
                    } else {
                        println!("[RUST] No sender_pubkey available for fallback, using encrypted body");
                        decrypted_body_result = Some(email.body.clone());
                    }
                } else {
                    println!("[RUST] Failed to get user's pubkey from private key, cannot verify self-sent email");
                    println!("[RUST] Cannot decrypt without recipient_pubkey. Recipient's pubkey must be in contacts for this email address: {}", recipient_email);
                    println!("[RUST] Using encrypted body for search (search may not find decrypted content)");
                    decrypted_body_result = Some(email.body.clone());
                }
            } else {
                println!("[RUST] Successfully decrypted sent email {} body with recipient pubkey", email.id.unwrap_or(0));
                
                // Subject decryption - try with recipient pubkeys first, then fallback to sender pubkey
                if email::is_likely_encrypted_content(&email.subject) {
                    let mut subject_decrypted = false;
                    
                    // Try recipient pubkeys first (same as body, including DMs)
                    if let Ok(recipient_pubkeys) = db.find_pubkeys_by_email_including_dms(recipient_email) {
                        let normalized_email = if recipient_email.contains("@gmail.com") {
                            let parts: Vec<&str> = recipient_email.split('@').collect();
                            if parts.len() == 2 {
                                let local = parts[0].split('+').next().unwrap_or(parts[0]);
                                format!("{}@{}", local, parts[1]).to_lowercase()
                            } else {
                                recipient_email.to_lowercase()
                            }
                        } else {
                            recipient_email.to_lowercase()
                        };
                        
                        let mut all_pubkeys = recipient_pubkeys;
                        if normalized_email != recipient_email.to_lowercase() {
                            if let Ok(normalized_pubkeys) = db.find_pubkeys_by_email_including_dms(&normalized_email) {
                                all_pubkeys.extend(normalized_pubkeys);
                            }
                        }
                        all_pubkeys.dedup();
                        
                        // Extract encrypted content from subject
                        let encrypted_subject_content = match extract_encrypted_content_from_armor(&email.subject) {
                            Some(content) => content,
                            None => email.subject.clone()
                        };
                        
                        for recipient_pubkey in &all_pubkeys {
                            match nostr::decrypt_dm_content(
                                email_config.private_key.as_ref().unwrap(),
                                recipient_pubkey,
                                &encrypted_subject_content
                            ) {
                                Ok(decrypted) => {
                                    decrypted_subject_result = decrypted;
                                    subject_decrypted = true;
                                    break;
                                }
                                Err(_) => {}
                            }
                        }
                    }
                    
                    // Fallback to sender pubkey if recipient pubkey decryption failed
                    if !subject_decrypted {
                        if let Some(sender_pubkey) = email.sender_pubkey.as_ref() {
                            if let Ok(decrypted) = crypto::decrypt_message(
                                email_config.private_key.as_ref().unwrap(),
                                sender_pubkey,
                                &email.subject
                            ) {
                                decrypted_subject_result = decrypted;
                            }
                        }
                    }
                }
            }
            
            (decrypted_subject_result, decrypted_body_result.unwrap_or_else(|| email.body.clone()))
        } else {
            (email.subject.clone(), email.body.clone())
        };
        
        // Get attachments for this email and extract original filenames from manifest if available
        let mut attachment_filenames = Vec::new();
        if let Some(email_id) = email.id {
            match db.get_attachments_for_email(email_id) {
                Ok(attachments) => {
                    // Add encrypted filenames
                    attachment_filenames.extend(attachments.iter().map(|att| att.filename.to_lowercase()));
                    
                    // Try to extract original filenames from manifest if email has manifest-encrypted attachments
                    let has_manifest_attachments = attachments.iter().any(|att| att.encryption_method.as_deref() == Some("manifest_aes"));
                    if has_manifest_attachments && email_config.private_key.is_some() {
                        println!("[RUST] Sent email {} has manifest-encrypted attachments, attempting to extract original filenames", email.id.unwrap_or(0));
                        println!("[RUST] Decrypted body length: {}, preview: {}", decrypted_body.len(), &decrypted_body.chars().take(200).collect::<String>());
                        
                        // Try to parse decrypted body as manifest JSON
                        match serde_json::from_str::<serde_json::Value>(&decrypted_body) {
                            Ok(manifest_json) => {
                                println!("[RUST] Successfully parsed decrypted body as JSON");
                                if let Some(manifest_obj) = manifest_json.as_object() {
                                    if let Some(attachments_array) = manifest_obj.get("attachments").and_then(|v| v.as_array()) {
                                        println!("[RUST] Found {} attachments in manifest", attachments_array.len());
                                        for attachment_obj in attachments_array {
                                            if let Some(orig_filename) = attachment_obj.get("orig_filename")
                                                .and_then(|v| v.as_str()) {
                                                println!("[RUST] Found original filename in manifest: {}", orig_filename);
                                                attachment_filenames.push(orig_filename.to_lowercase());
                                            } else {
                                                println!("[RUST] Attachment object missing orig_filename: {:?}", attachment_obj);
                                            }
                                        }
                                    } else {
                                        println!("[RUST] Manifest JSON does not have attachments array");
                                    }
                                } else {
                                    println!("[RUST] Manifest JSON is not an object");
                                }
                            }
                            Err(e) => {
                                println!("[RUST] Failed to parse decrypted body as JSON: {}", e);
                                println!("[RUST] Decrypted body (first 500 chars): {}", &decrypted_body.chars().take(500).collect::<String>());
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        }
        
        // Check if decrypted body is a manifest JSON - if so, we need to extract the actual body text
        // For manifest format, the body is encrypted with AES in manifest.body.ciphertext
        // For now, we'll search the JSON structure itself (which includes attachment metadata)
        // TODO: Decrypt manifest.body.ciphertext with AES to search actual body text
        let mut searchable_body = decrypted_body.clone();
        let mut is_manifest = false;
        
        // Try to detect if this is a manifest JSON
        if decrypted_body.trim_start().starts_with('{') {
            if let Ok(manifest_json) = serde_json::from_str::<serde_json::Value>(&decrypted_body) {
                if let Some(manifest_obj) = manifest_json.as_object() {
                    // Check if it has the manifest structure
                    if manifest_obj.contains_key("body") && manifest_obj.contains_key("attachments") {
                        is_manifest = true;
                        println!("[RUST] Sent email {} has manifest format body", email.id.unwrap_or(0));
                        
                        // For manifest format, search within the JSON structure
                        // The actual body text is in manifest.body.ciphertext but needs AES decryption
                        // For now, search the JSON string itself (includes attachment filenames which are already extracted)
                        // Also check if search query appears in any JSON values
                        let json_string = serde_json::to_string(&manifest_json).unwrap_or_default();
                        searchable_body = json_string;
                        
                        // Also check if the search query appears in any string values in the manifest
                        if let Some(body_obj) = manifest_obj.get("body").and_then(|v| v.as_object()) {
                            // Search in ciphertext (base64 encoded, might contain search terms)
                            if let Some(ciphertext) = body_obj.get("ciphertext").and_then(|v| v.as_str()) {
                                if ciphertext.to_lowercase().contains(&search_query_lower) {
                                    println!("[RUST] Search query found in manifest ciphertext");
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Search in: from, to, subject, body, attachment filenames (both encrypted and decrypted)
        let from_matches = email.from_address.to_lowercase().contains(&search_query_lower);
        let to_matches = email.to_address.to_lowercase().contains(&search_query_lower);
        let subject_matches = decrypted_subject.to_lowercase().contains(&search_query_lower);
        let body_matches = searchable_body.to_lowercase().contains(&search_query_lower);
        let attachment_matches = attachment_filenames.iter().any(|f| f.contains(&search_query_lower));
        
        let matches = from_matches || to_matches || subject_matches || body_matches || attachment_matches;
        
        // Always log search details for debugging
        println!("[RUST] Sent email {} search for '{}': from={}, to={}, subject={}, body={}, attachments={}, is_manifest={}", 
            email.id.unwrap_or(0), search_query, from_matches, to_matches, subject_matches, body_matches, attachment_matches, is_manifest);
        println!("[RUST] Decrypted subject: {}", decrypted_subject.chars().take(100).collect::<String>());
        println!("[RUST] Decrypted body (first 200 chars): {}", searchable_body.chars().take(200).collect::<String>());
        
            if matches {
                // Skip results until we reach the offset
                if skipped_count < skip_count {
                    skipped_count += 1;
                    continue;
                }
                
                println!("[RUST] ✓ Sent email {} MATCHES search query '{}'", email.id.unwrap_or(0), search_query);
                // Create EmailMessage with same format as normal emails (encrypted content, frontend will decrypt)
                // Use map_db_email_to_email_message to ensure consistent format
                let email_message = map_db_email_to_email_message(&email);
                
                // Emit this matching email immediately
                if let Err(e) = app_handle.emit("sent-search-result", &email_message) {
                    println!("[RUST] Failed to emit sent-search-result event: {}", e);
                }
                
                match_count += 1;
                
                // Stop if we've found enough results
                if match_count >= page_size {
                    has_more = true; // There might be more results
                    break;
                }
            }
        }
        
        // If we've found enough results or processed all emails, stop
        if match_count >= page_size || batch_emails.len() < batch_size {
            break;
        }
        
        db_offset += batch_size as i64;
    }
    
    // Emit search completed event
    let completion = serde_json::json!({
        "total_found": match_count,
        "has_more": has_more
    });
    if let Err(e) = app_handle.emit("sent-search-completed", &completion) {
        println!("[RUST] Failed to emit sent-search-completed event: {}", e);
    }
    
    println!("[RUST] Sent search found {} matching emails (has_more: {})", match_count, has_more);
    Ok((match_count, has_more))
}

// Database commands for direct messages
#[tauri::command]
fn db_save_dm(dm: DbDirectMessage, state: tauri::State<AppState>) -> Result<i64, String> {
    println!("[RUST] db_save_dm called");
    let db = state.get_database()?;
    db.save_dm(&dm).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_dms_for_conversation(user_pubkey: String, contact_pubkey: String, state: tauri::State<AppState>) -> Result<Vec<DbDirectMessage>, String> {
    println!("[RUST] db_get_dms_for_conversation called");
    let db = state.get_database()?;
    db.get_dms_for_conversation(&user_pubkey, &contact_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_decrypted_dms_for_conversation(
    private_key: String,
    user_pubkey: String,
    contact_pubkey: String,
    state: tauri::State<AppState>
) -> Result<Vec<crate::database::DirectMessage>, String> {
    let db = state.get_database().map_err(|e| e.to_string())?;
    let mut messages = db.get_dms_for_conversation(&user_pubkey, &contact_pubkey)
        .map_err(|e| e.to_string())?;
    for msg in &mut messages {
        let decrypt_sender_pubkey = if msg.sender_pubkey == user_pubkey {
            &msg.recipient_pubkey
        } else {
            &msg.sender_pubkey
        };
        let id_str = msg.id.map(|id| id.to_string()).unwrap_or_else(|| "<no id>".to_string());
        match crate::nostr::decrypt_dm_content(&private_key, decrypt_sender_pubkey, &msg.content) {
            Ok(decrypted) => {
                println!("[DM DECRYPT] Success: id={}, sender={}, recipient={}, decrypted='{}'", id_str, msg.sender_pubkey, msg.recipient_pubkey, &decrypted.chars().take(80).collect::<String>());
                msg.content = decrypted;
            },
            Err(e) => {
                println!(
                    "[DM DECRYPT] Failed: id={}, sender={}, recipient={}, error={}, sender_pubkey_raw={:?}, recipient_pubkey_raw={:?}, content_sample={:?}",
                    id_str,
                    msg.sender_pubkey,
                    msg.recipient_pubkey,
                    e,
                    msg.sender_pubkey,
                    msg.recipient_pubkey,
                    &msg.content.chars().take(40).collect::<String>()
                );
                msg.content = "[Failed to decrypt]".to_string();
            },
        }
    }
    Ok(messages)
}

// Database commands for attachments
#[tauri::command]
fn db_save_attachment(attachment: crate::database::Attachment, state: tauri::State<AppState>) -> Result<i64, String> {
    println!("[RUST] db_save_attachment called");
    let db = state.get_database()?;
    db.save_attachment(&attachment).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_attachments_for_email(email_id: i64, state: tauri::State<AppState>) -> Result<Vec<crate::database::Attachment>, String> {
    println!("[RUST] db_get_attachments_for_email called for email_id: {}", email_id);
    let db = state.get_database()?;
    db.get_attachments_for_email(email_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_attachment(attachment_id: i64, state: tauri::State<AppState>) -> Result<Option<crate::database::Attachment>, String> {
    println!("[RUST] db_get_attachment called for attachment_id: {}", attachment_id);
    let db = state.get_database()?;
    db.get_attachment(attachment_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_delete_attachment(attachment_id: i64, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_delete_attachment called for attachment_id: {}", attachment_id);
    let db = state.get_database()?;
    db.delete_attachment(attachment_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_attachment_to_disk(filename: String, data: String, _mime_type: String) -> Result<String, String> {
    println!("[RUST] save_attachment_to_disk called for filename: {}", filename);
    
    use base64::{Engine as _, engine::general_purpose};
    
    // Get user's Downloads directory
    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads directory".to_string())?;
    
    // Create full path for the file
    let mut file_path = downloads_dir.join(&filename);
    
    // If file exists, add a number to make it unique
    let mut counter = 1;
    while file_path.exists() {
        let stem = std::path::Path::new(&filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&filename);
        let extension = std::path::Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        
        let new_filename = if extension.is_empty() {
            format!("{} ({})", stem, counter)
        } else {
            format!("{} ({}).{}", stem, counter, extension)
        };
        
        file_path = downloads_dir.join(new_filename);
        counter += 1;
    }
    
    // Decode base64 data
    let decoded_data = general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Failed to decode base64 data: {}", e))?;
    
    // Write file to disk
    std::fs::write(&file_path, decoded_data)
        .map_err(|e| format!("Failed to write file: {}", e))?;
    
    let path_str = file_path.to_string_lossy().to_string();
    println!("[RUST] Successfully saved attachment to: {}", path_str);
    
    Ok(path_str)
}

#[derive(serde::Deserialize)]
struct AttachmentForZip {
    filename: String,
    data: String,
}

#[tauri::command]
async fn save_attachments_as_zip(zip_filename: String, attachments: Vec<AttachmentForZip>) -> Result<String, String> {
    println!("[RUST] save_attachments_as_zip called with {} attachments", attachments.len());
    
    use std::io::Write;
    use base64::{Engine as _, engine::general_purpose};
    
    // Get user's Downloads directory
    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads directory".to_string())?;
    
    // Create unique ZIP filename
    let mut zip_path = downloads_dir.join(&zip_filename);
    let mut counter = 1;
    while zip_path.exists() {
        let stem = std::path::Path::new(&zip_filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&zip_filename);
        let new_filename = format!("{} ({}).zip", stem, counter);
        zip_path = downloads_dir.join(new_filename);
        counter += 1;
    }
    
    // Create ZIP file
    let zip_file = std::fs::File::create(&zip_path)
        .map_err(|e| format!("Failed to create ZIP file: {}", e))?;
    
    let mut zip = zip::ZipWriter::new(zip_file);
    let options = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    
    // Add each attachment to the ZIP
    for attachment in attachments {
        println!("[RUST] Adding to ZIP: {}", attachment.filename);
        
        // Decode base64 data
        let decoded_data = general_purpose::STANDARD
            .decode(&attachment.data)
            .map_err(|e| format!("Failed to decode base64 data for {}: {}", attachment.filename, e))?;
        
        // Add file to ZIP
        zip.start_file(&attachment.filename, options)
            .map_err(|e| format!("Failed to start file in ZIP: {}", e))?;
        
        zip.write_all(&decoded_data)
            .map_err(|e| format!("Failed to write file data to ZIP: {}", e))?;
    }
    
    // Finish ZIP file
    zip.finish()
        .map_err(|e| format!("Failed to finish ZIP file: {}", e))?;
    
    let path_str = zip_path.to_string_lossy().to_string();
    println!("[RUST] Successfully created ZIP file: {}", path_str);
    
    Ok(path_str)
}

#[tauri::command]
fn db_save_email_with_attachments(email: crate::database::Email, attachments: Vec<crate::types::EmailAttachment>, state: tauri::State<AppState>) -> Result<i64, String> {
    println!("[RUST] db_save_email_with_attachments called with {} attachments", attachments.len());
    let db = state.get_database()?;
    db.save_email_with_attachments(&email, &attachments).map_err(|e| e.to_string())
}

// Database commands for settings
#[tauri::command]
fn db_save_setting(pubkey: String, key: String, value: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_save_setting called for pubkey: {}, key: {}", pubkey, key);
    let db = state.get_database()?;
    db.save_setting(&pubkey, &key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_setting(pubkey: String, key: String, state: tauri::State<AppState>) -> Result<Option<String>, String> {
    println!("[RUST] db_get_setting called for pubkey: {}, key: {}", pubkey, key);
    let db = state.get_database()?;
    db.get_setting(&pubkey, &key).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_all_settings(pubkey: String, private_key: Option<String>, state: tauri::State<AppState>) -> Result<std::collections::HashMap<String, String>, String> {
    println!("[RUST] db_get_all_settings called for pubkey: {}", pubkey);
    let db = state.get_database()?;
    let mut settings = db.get_all_settings(&pubkey).map_err(|e| e.to_string())?;
    
    // List of sensitive settings that should be decrypted
    let sensitive_keys = vec!["password"];
    
    // Decrypt sensitive settings if private key is provided
    if let Some(ref priv_key) = private_key {
        for key in sensitive_keys {
            if let Some(encrypted_value) = settings.get(key) {
                if !encrypted_value.is_empty() {
                    // Try to decrypt - if it fails, it might be plaintext (backward compatibility)
                    match crypto::decrypt_setting_value(priv_key, encrypted_value) {
                        Ok(decrypted) => {
                            println!("[RUST] Decrypted sensitive setting: {}", key);
                            settings.insert(key.to_string(), decrypted);
                        }
                        Err(e) => {
                            // If decryption fails, it might be plaintext (old data)
                            // Keep the original value for backward compatibility
                            println!("[RUST] Failed to decrypt setting {} (may be plaintext): {}", key, e);
                        }
                    }
                }
            }
        }
    }
    
    Ok(settings)
}

#[tauri::command]
fn db_save_settings_batch(pubkey: String, settings: std::collections::HashMap<String, String>, private_key: Option<String>, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_save_settings_batch called for pubkey: {}, {} settings", pubkey, settings.len());
    let db = state.get_database()?;
    
    // List of sensitive settings that should be encrypted
    let sensitive_keys = vec!["password"];
    
    for (key, value) in settings.iter() {
        let value_to_save = if sensitive_keys.contains(&key.as_str()) {
            // Encrypt sensitive settings if private key is provided
            if let Some(ref priv_key) = private_key {
                match crypto::encrypt_setting_value(priv_key, value) {
                    Ok(encrypted) => {
                        println!("[RUST] Encrypted sensitive setting: {}", key);
                        encrypted
                    }
                    Err(e) => {
                        eprintln!("[RUST] Failed to encrypt setting {}: {}", key, e);
                        return Err(format!("Failed to encrypt setting {}: {}", key, e));
                    }
                }
            } else {
                // No private key provided, save as-is (for backward compatibility)
                // But warn that sensitive data is not encrypted
                if sensitive_keys.contains(&key.as_str()) {
                    eprintln!("[RUST] WARNING: Saving sensitive setting '{}' without encryption (no private key provided)", key);
                }
                value.clone()
            }
        } else {
            // Non-sensitive setting, save as-is
            value.clone()
        };
        
        db.save_setting(&pubkey, key, &value_to_save).map_err(|e| format!("Failed to save setting {}: {}", key, e))?;
    }
    Ok(())
}

// Database commands for relays
#[tauri::command]
fn db_save_relay(relay: DbRelay, state: tauri::State<AppState>) -> Result<i64, String> {
    println!("[RUST] db_save_relay called");
    let db = state.get_database()?;
    db.save_relay(&relay).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_all_relays(state: tauri::State<AppState>) -> Result<Vec<DbRelay>, String> {
    println!("[RUST] db_get_all_relays called");
    let db = state.get_database()?;
    db.get_all_relays().map_err(|e| e.to_string())
}

#[tauri::command]
fn db_delete_relay(url: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_delete_relay called for url: {}", url);
    let db = state.get_database()?;
    db.delete_relay(&url).map_err(|e| e.to_string())
}

// Database utility commands
#[tauri::command]
fn db_get_database_size(state: tauri::State<AppState>) -> Result<u64, String> {
    println!("[RUST] db_get_database_size called");
    let db = state.get_database()?;
    db.get_database_size().map_err(|e| e.to_string())
}

#[tauri::command]
fn db_clear_all_data(state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_clear_all_data called");
    let db = state.get_database()?;
    db.clear_all_data().map_err(|e| e.to_string())
}

#[tauri::command]
async fn follow_user(private_key: String, pubkey_to_follow: String, relays: Vec<String>) -> Result<(), String> {
    println!("[RUST] follow_user called for pubkey: {}", pubkey_to_follow);
    
    // First, get the current follow list
    let current_follows = nostr::fetch_following_profiles(&private_key, &relays)
        .await
        .map_err(|e| format!("Failed to fetch current follows: {}", e))?;
    
    // Extract pubkeys from current follows
    let mut current_pubkeys: Vec<String> = current_follows.iter()
        .map(|profile| profile.pubkey.clone())
        .collect();
    
    // Add the new pubkey if it's not already in the list
    if !current_pubkeys.contains(&pubkey_to_follow) {
        current_pubkeys.push(pubkey_to_follow.clone());
        println!("[RUST] Added {} to follow list. Total follows: {}", pubkey_to_follow, current_pubkeys.len());
    } else {
        println!("[RUST] {} is already in follow list", pubkey_to_follow);
        return Ok(()); // Already following
    }
    
    // Create tags for the follow event (kind 3)
    let tags: Vec<Vec<String>> = current_pubkeys.iter()
        .map(|pubkey| vec!["p".to_string(), pubkey.clone()])
        .collect();
    
    // Empty content for follow events
    let content = "".to_string();
    
    // Publish the new follow event with the complete list
    nostr::publish_event(&private_key, &content, 3, tags, &relays)
        .await
        .map(|_| {
            println!("[RUST] Successfully published follow event with {} follows", current_pubkeys.len());
        })
        .map_err(|e| {
            println!("[RUST] Failed to publish follow event: {}", e);
            e.to_string()
        })
}

#[tauri::command]
async fn publish_follow_list(private_key: String, user_pubkey: String, relays: Vec<String>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] publish_follow_list called for user: {}", user_pubkey);
    
    // Get all public contacts from database
    let db = state.get_database()?;
    let public_pubkeys = db.get_public_contact_pubkeys(&user_pubkey)
        .map_err(|e| format!("Failed to get public contacts: {}", e))?;
    
    println!("[RUST] Found {} public contacts to publish", public_pubkeys.len());
    
    if public_pubkeys.is_empty() {
        println!("[RUST] No public contacts to publish");
        // Still publish an empty list to clear the follow list
    }
    
    // Create tags for the follow event (kind 3)
    let tags: Vec<Vec<String>> = public_pubkeys.iter()
        .map(|pubkey: &String| vec!["p".to_string(), pubkey.clone()])
        .collect();
    
    // Empty content for follow events
    let content = "".to_string();
    
    // Publish the follow event with the complete list
    nostr::publish_event(&private_key, &content, 3, tags, &relays)
        .await
        .map(|_| {
            println!("[RUST] Successfully published follow event with {} public contacts", public_pubkeys.len());
        })
        .map_err(|e| {
            println!("[RUST] Failed to publish follow event: {}", e);
            e.to_string()
        })
}

#[tauri::command]
fn encrypt_nip04_message(private_key: String, public_key: String, message: String) -> Result<String, String> {
    println!("[RUST] encrypt_nip04_message called");
    crypto::encrypt_message(&private_key, &public_key, &message, Some("nip04")).map_err(|e| e.to_string())
}

#[tauri::command]
fn encrypt_nip04_message_legacy(private_key: String, public_key: String, message: String) -> Result<String, String> {
    println!("[RUST] encrypt_nip04_message_legacy called");
    use nostr_sdk::prelude::*;
    
    // Parse the keys from bech32 format
    let secret_key = SecretKey::from_bech32(&private_key).map_err(|e| e.to_string())?;
    let public_key = PublicKey::from_bech32(&public_key).map_err(|e| e.to_string())?;
    
    // Use NIP-04 encryption (legacy)
    let encrypted = nip04::encrypt(&secret_key, &public_key, message).map_err(|e| e.to_string())?;
    
    Ok(encrypted)
}

#[tauri::command]
fn encrypt_message_with_algorithm(private_key: String, public_key: String, message: String, algorithm: String) -> Result<String, String> {
    println!("[RUST] encrypt_message_with_algorithm called with algorithm: {}", algorithm);
    crypto::encrypt_message(&private_key, &public_key, &message, Some(&algorithm)).map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_nostr_emails_last_24h(email_config: EmailConfig) -> Result<Vec<EmailMessage>, String> {
    email::fetch_nostr_emails_last_24h(&email_config)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_nostr_emails_smart(email_config: EmailConfig, state: tauri::State<'_, AppState>) -> Result<Vec<EmailMessage>, String> {
    let db = state.get_database().map_err(|e| e.to_string())?;
    email::fetch_nostr_emails_smart(&email_config, &db)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_nostr_emails(config: EmailConfig, state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let db = state.get_database().map_err(|e| e.to_string())?;
    email::sync_nostr_emails_to_db(&config, &db).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_sent_emails(config: EmailConfig, state: tauri::State<'_, AppState>) -> Result<usize, String> {
    println!("[RUST] sync_sent_emails command called");
    let db = state.get_database().map_err(|e| e.to_string())?;
    let result = email::sync_sent_emails_to_db(&config, &db).await.map_err(|e| e.to_string());
    println!("[RUST] sync_sent_emails command completed with result: {:?}", result);
    result
}

#[tauri::command]
async fn sync_all_emails(config: EmailConfig, state: tauri::State<'_, AppState>) -> Result<(usize, usize), String> {
    let db = state.get_database().map_err(|e| e.to_string())?;
    
    // Sync both inbox and sent emails
    let inbox_result = email::sync_nostr_emails_to_db(&config, &db).await;
    let sent_result = email::sync_sent_emails_to_db(&config, &db).await;
    
    match (inbox_result, sent_result) {
        (Ok(inbox_count), Ok(sent_count)) => Ok((inbox_count, sent_count)),
        (Err(e), _) => Err(format!("Inbox sync failed: {}", e)),
        (_, Err(e)) => Err(format!("Sent sync failed: {}", e)),
    }
}

#[tauri::command]
async fn sync_direct_messages_with_network(private_key: String, relays: Vec<String>, state: tauri::State<'_, AppState>) -> Result<usize, String> {
    println!("SYNC COMMAND CALLED");
    let db = state.get_database().map_err(|e| e.to_string())?;
    // 1. Get latest DM timestamp
    let latest = db.get_latest_dm_created_at().map_err(|e| e.to_string())?;
    let since = latest.map(|dt| dt.timestamp());
    // 2. Fetch new DMs from network
    let events = crate::nostr::fetch_direct_messages(&private_key, &relays, since)
        .await
        .map_err(|e| e.to_string())?;
    // 3. Convert NostrEvent to DirectMessage
    let dms: Vec<DbDirectMessage> = events.iter().map(|event| {
        // Convert sender_pubkey to npub format
        let sender_npub = if event.pubkey.starts_with("npub1") {
            event.pubkey.clone()
        } else if event.pubkey.len() == 64 && event.pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
            match nostr_sdk::prelude::PublicKey::from_hex(&event.pubkey) {
                Ok(pk) => pk.to_bech32().unwrap_or(event.pubkey.clone()),
                Err(_) => event.pubkey.clone(),
            }
        } else {
            event.pubkey.clone()
        };
        // Convert recipient_pubkey to npub format
        let raw_recipient = event.tags.iter()
            .find(|tag| tag.get(0) == Some(&"p".to_string()) && tag.get(1).is_some())
            .and_then(|tag| tag.get(1).cloned())
            .unwrap_or_default();
        let recipient_npub = if raw_recipient.starts_with("npub1") {
            raw_recipient.clone()
        } else if raw_recipient.len() == 64 && raw_recipient.chars().all(|c| c.is_ascii_hexdigit()) {
            match nostr_sdk::prelude::PublicKey::from_hex(&raw_recipient) {
                Ok(pk) => pk.to_bech32().unwrap_or(raw_recipient.clone()),
                Err(_) => raw_recipient.clone(),
            }
        } else {
            raw_recipient.clone()
        };
        DbDirectMessage {
            id: None,
            event_id: event.id.clone(),
            sender_pubkey: sender_npub,
            recipient_pubkey: recipient_npub,
            content: event.content.clone(),
            created_at: chrono::DateTime::from_timestamp(event.created_at, 0).unwrap_or_else(|| chrono::Utc::now()),
            received_at: chrono::Utc::now(),
        }
    }).collect();
    // 4. Save new DMs
    let inserted = db.save_dm_batch(&dms).map_err(|e| e.to_string())?;
    // 5. Return number of new messages
    Ok(inserted)
}

#[tauri::command]
fn db_get_all_dm_pubkeys(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let db = state.get_database().map_err(|e| e.to_string())?;
    db.get_all_dm_pubkeys().map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_all_dm_pubkeys_sorted(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let db = state.get_database().map_err(|e| e.to_string())?;
    db.get_all_dm_pubkeys_sorted().map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_profile(private_key: String, fields: std::collections::HashMap<String, serde_json::Value>, relays: Vec<String>) -> Result<(), String> {

    // Convert fields to JSON string
    let content = serde_json::to_string(&fields).map_err(|e| e.to_string())?;

    // Parse private key
    let secret_key = nostr_sdk::prelude::SecretKey::from_str(&private_key).map_err(|e| e.to_string())?;
    let keys = nostr_sdk::prelude::Keys::new(secret_key);

    // Build the profile event (kind 0)
    let metadata: Metadata = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let event = nostr_sdk::prelude::EventBuilder::metadata(&metadata);
    let event = event.build(keys.public_key()).sign_with_keys(&keys).map_err(|e| e.to_string())?;

    // Connect to relays and publish
    let client = nostr_sdk::prelude::Client::new(keys);
    for relay in &relays {
        client.add_relay(relay).await.map_err(|e| e.to_string())?;
    }
    client.connect().await;
    client.send_event(&event).await.map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn update_profile_persistent(private_key: String, fields: std::collections::HashMap<String, serde_json::Value>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] update_profile_persistent called");
    
    // Get the persistent client
    let client = {
        let client_guard = state.nostr_client.lock().unwrap();
        match client_guard.as_ref() {
            Some(client) => client.clone(),
            None => return Err("Persistent Nostr client not initialized".to_string()),
        }
    };
    
    // Convert fields to JSON string
    let content = serde_json::to_string(&fields).map_err(|e| e.to_string())?;
    println!("[RUST] Profile content: {}", content);

    // Parse private key and create keys
    let secret_key = nostr_sdk::prelude::SecretKey::from_str(&private_key).map_err(|e| e.to_string())?;
    let keys = nostr_sdk::prelude::Keys::new(secret_key);
    
    // Verify the client is using the same keys
    let current_keys = state.get_current_keys().ok_or("No current keys available")?;
    if current_keys.public_key() != keys.public_key() {
        return Err("Private key doesn't match the persistent client's keys".to_string());
    }

    // Build the profile event (kind 0)
    let metadata: Metadata = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let event = nostr_sdk::prelude::EventBuilder::metadata(&metadata);
    let event = event.build(keys.public_key()).sign_with_keys(&keys).map_err(|e| e.to_string())?;
    
    println!("[RUST] Built profile event with ID: {}", event.id.to_hex());

    // Get connected relays count for logging
    let connected_relays = client.relays().await;
    println!("[RUST] Publishing profile update to {} connected relays", connected_relays.len());
    
    // Publish event using persistent client
    let output = client.send_event(&event).await.map_err(|e| e.to_string())?;
    
    println!("[RUST] Profile update published successfully to {} relays", output.success.len());
    if !output.failed.is_empty() {
        println!("[RUST] Failed to publish to {} relays: {:?}", output.failed.len(), output.failed);
    }

    Ok(())
}

// Live Event Subscription System
#[tauri::command]
async fn start_live_event_subscription(
    private_key: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>
) -> Result<(), String> {
    println!("[RUST] start_live_event_subscription called");
    
    // Get the persistent client
    let client = state.get_nostr_client(Some(&private_key)).await?;
    
    // Parse user's public key from private key
    let secret_key = SecretKey::from_bech32(&private_key).map_err(|e| e.to_string())?;
    let keys = Keys::new(secret_key);
    let user_pubkey = keys.public_key();
    
    println!("[RUST] Starting subscription for user: {}", user_pubkey.to_bech32().unwrap_or_default());
    
    // Get the latest DM timestamp for 'since' parameter to avoid gaps
    let since_timestamp = {
        let db = state.get_database().map_err(|e| e.to_string())?;
        db.get_latest_dm_created_at().map_err(|e| e.to_string())?
            .map(|dt| dt.timestamp())
    };
    
    println!("[RUST] Using since timestamp: {:?}", since_timestamp);
    
    // Create separate filters - need individual subscriptions for each
    let mut filters = Vec::new();
    
    // Filter 1: Direct messages sent by user
    let mut dm_sent_filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(user_pubkey);
    
    if let Some(since) = since_timestamp {
        dm_sent_filter = dm_sent_filter.since(Timestamp::from(since as u64));
    }
    filters.push(dm_sent_filter);
    
    // Filter 2: Direct messages received by user (tagged with user's pubkey)
    let mut dm_received_filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(user_pubkey);
    
    if let Some(since) = since_timestamp {
        dm_received_filter = dm_received_filter.since(Timestamp::from(since as u64));
    }
    filters.push(dm_received_filter);
    
    // Filter 3: Profile updates for user's own profile
    let mut profile_filter = Filter::new()
        .kind(Kind::Metadata)
        .author(user_pubkey);
    
    if let Some(since) = since_timestamp {
        profile_filter = profile_filter.since(Timestamp::from(since as u64));
    }
    filters.push(profile_filter);
    
    println!("[RUST] Created {} filters for subscription", filters.len());
    
    // Subscribe to each filter separately
    let mut subscription_ids = Vec::new();
    for (i, filter) in filters.iter().enumerate() {
        match client.subscribe(filter.clone(), None).await {
            Ok(output) => {
                // output.val is a single SubscriptionId, not a HashMap
                let subscription_id = output.val;
                println!("[RUST] Filter {} created subscription ID: {}", i, subscription_id);
                subscription_ids.push(subscription_id);
            },
            Err(e) => {
                println!("[RUST] Failed to subscribe to filter {}: {}", i, e);
                return Err(format!("Failed to subscribe to filter {}: {}", i, e));
            }
        }
    }
    
    println!("[RUST] Started {} total subscriptions", subscription_ids.len());
    
    // Store subscription info
    let subscription = EventSubscription {
        subscription_ids: subscription_ids.clone(),
        is_active: true,
        filters,
        user_pubkey,
        since_timestamp,
    };
    
    *state.active_subscription.write().await = Some(subscription);
    
    // Spawn task to handle notifications
    let client_clone = client.clone();
    let app_handle_clone = app_handle.clone();
    let state_clone = state.inner().clone();
    let user_pubkey_clone = user_pubkey;
    
    tokio::spawn(async move {
        handle_subscription_notifications(client_clone, app_handle_clone, state_clone, user_pubkey_clone).await;
    });
    
    println!("[RUST] Live event subscription started successfully");
    Ok(())
}

#[tauri::command]
async fn stop_live_event_subscription(
    state: tauri::State<'_, AppState>
) -> Result<(), String> {
    println!("[RUST] stop_live_event_subscription called");
    
    let client = {
        let client_guard = state.nostr_client.lock().unwrap();
        match client_guard.as_ref() {
            Some(client) => client.clone(),
            None => {
                println!("[RUST] No client available");
                return Ok(());
            }
        }
    };
    
    let subscription_ids = {
        let mut subscription_guard = state.active_subscription.write().await;
        if let Some(subscription) = subscription_guard.take() {
            subscription.subscription_ids
        } else {
            println!("[RUST] No active subscription to stop");
            return Ok(());
        }
    };
    
    // Unsubscribe from all subscriptions (no Result to handle)
    for subscription_id in subscription_ids {
        client.unsubscribe(&subscription_id).await;
        println!("[RUST] Unsubscribed from: {}", subscription_id);
    }
    
    println!("[RUST] All live event subscriptions stopped");
    Ok(())
}

#[tauri::command]
async fn get_live_subscription_status(
    state: tauri::State<'_, AppState>
) -> Result<bool, String> {
    let subscription_guard = state.active_subscription.read().await;
    let is_active = subscription_guard.as_ref().map(|s| s.is_active).unwrap_or(false);
    println!("[RUST] Live subscription status: {}", is_active);
    Ok(is_active)
}

// Handle subscription notifications with automatic retry and exponential backoff
async fn handle_subscription_notifications(
    client: nostr_sdk::Client,
    app_handle: tauri::AppHandle,
    state: AppState,
    user_pubkey: PublicKey,
) {
    println!("[RUST] Starting notification handler for user: {}", user_pubkey.to_bech32().unwrap_or_default());
    
    let mut retry_count = 0;
    let max_retries = 5;
    let base_delay = Duration::from_secs(1);
    
    loop {
        let mut notifications = client.notifications();
        
        while let Ok(notification) = notifications.recv().await {
            match notification {
                RelayPoolNotification::Event { event, .. } => {
                    match event.kind {
                        Kind::EncryptedDirectMessage => {
                            println!("[RUST] Processing DM: {}", event.id.to_hex()[..8].to_string() + "...");
                            if let Err(e) = handle_live_direct_message(&event, &app_handle, &state, &user_pubkey).await {
                                println!("[RUST] Error handling DM: {}", e);
                            }
                        },
                        Kind::Metadata => {
                            println!("[RUST] Processing profile update: {}", event.id.to_hex()[..8].to_string() + "...");
                            if let Err(e) = handle_live_profile_update(&event, &app_handle, &state, &user_pubkey).await {
                                println!("[RUST] Error handling profile update: {}", e);
                            }
                        },
                        _ => {
                            // Silently ignore other event types - no logging
                        }
                    }
                },
                RelayPoolNotification::Message { .. } => {
                    // Silently handle relay messages - no logging to reduce verbosity
                },
                RelayPoolNotification::Shutdown => {
                    println!("[RUST] Received shutdown notification");
                    break;
                }
            }
        }
        
        // If we get here, the notification stream ended - implement retry logic
        retry_count += 1;
        if retry_count > max_retries {
            println!("[RUST] Max retries exceeded, stopping notification handler");
            // Emit error event to frontend
            if let Err(e) = app_handle.emit("subscription-error", "Max retries exceeded") {
                println!("[RUST] Failed to emit subscription error: {}", e);
            }
            break;
        }
        
        // Exponential backoff
        let delay = base_delay * (2_u32.pow(retry_count.min(5)));
        println!("[RUST] Notification stream ended, retrying in {:?} (attempt {})", delay, retry_count);
        tokio::time::sleep(delay).await;
        
        // Check if subscription is still active
        let is_still_active = {
            let subscription_guard = state.active_subscription.read().await;
            subscription_guard.as_ref().map(|s| s.is_active).unwrap_or(false)
        };
        
        if !is_still_active {
            println!("[RUST] Subscription no longer active, stopping notification handler");
            break;
        }
    }
    
    println!("[RUST] Notification handler stopped");
}

// Handle live direct message events
async fn handle_live_direct_message(
    event: &nostr_sdk::Event,
    app_handle: &tauri::AppHandle,
    state: &AppState,
    user_pubkey: &PublicKey,
) -> Result<(), String> {
    // Processing live DM - logging handled by caller
    
    // Convert sender_pubkey to npub format
    let sender_npub = event.pubkey.to_bech32().map_err(|e| e.to_string())?;
    
    // Find recipient from p tags
    let recipient_npub = event.tags.iter()
        .find(|tag| tag.kind().as_str() == "p")
        .and_then(|tag| tag.content())
        .and_then(|pk| {
            PublicKey::from_bech32(pk)
                .or_else(|_| PublicKey::from_hex(pk))
                .ok()
                .and_then(|pubkey| pubkey.to_bech32().ok())
        })
        .unwrap_or_else(|| user_pubkey.to_bech32().unwrap_or_default());
    
    // Create DirectMessage for database
    let dm = DbDirectMessage {
        id: None,
        event_id: event.id.to_hex(),
        sender_pubkey: sender_npub.clone(),
        recipient_pubkey: recipient_npub.clone(),
        content: event.content.clone(),
        created_at: chrono::DateTime::from_timestamp(event.created_at.as_u64() as i64, 0)
            .unwrap_or_else(|| chrono::Utc::now()),
        received_at: chrono::Utc::now(),
    };
    
    // Save to database (will handle deduplication via UNIQUE constraint)
    let db = state.get_database().map_err(|e| e.to_string())?;
    match db.save_dm(&dm) {
        Ok(_) => {
            println!("[RUST] Saved live DM to database");
            
            // Emit event to frontend
            let dm_payload = serde_json::json!({
                "event_id": event.id.to_hex(),
                "sender_pubkey": sender_npub,
                "recipient_pubkey": recipient_npub,
                "content": event.content,
                "created_at": event.created_at.as_u64(),
                "is_live": true
            });
            
            if let Err(e) = app_handle.emit("dm-received", &dm_payload) {
                println!("[RUST] Failed to emit dm-received event: {}", e);
            } else {
                println!("[RUST] Emitted dm-received event");
            }
        },
        Err(e) => {
            // Check if it's a duplicate (UNIQUE constraint violation)
            if e.to_string().contains("UNIQUE constraint failed") {
                println!("[RUST] DM already exists in database (duplicate): {}", event.id.to_hex());
            } else {
                println!("[RUST] Failed to save DM: {}", e);
                return Err(format!("Failed to save DM: {}", e));
            }
        }
    }
    
    Ok(())
}

// Handle live profile update events
async fn handle_live_profile_update(
    event: &nostr_sdk::Event,
    app_handle: &tauri::AppHandle,
    state: &AppState,
    user_pubkey: &PublicKey,
) -> Result<(), String> {
    println!("[RUST] Processing live profile update: {}", event.id.to_hex());
    
    let event_pubkey = event.pubkey;
    let is_current_user = event_pubkey == *user_pubkey;
    
    // Check if this is a contact's profile update (not current user)
    if !is_current_user {
        // Check if this pubkey is in the user's contacts
        let db = match state.get_database() {
            Ok(db) => db,
            Err(e) => {
                println!("[RUST] Failed to get database to check contact: {}", e);
                return Ok(()); // Silently ignore if DB unavailable
            }
        };
        
        let event_pubkey_str = event_pubkey.to_bech32().unwrap_or_else(|_| event_pubkey.to_hex());
        match db.user_follows_contact(&user_pubkey.to_bech32().unwrap_or_else(|_| user_pubkey.to_hex()), &event_pubkey_str) {
            Ok(true) => {
                println!("[RUST] Processing profile update for contact: {}", event_pubkey_str);
            },
            Ok(false) => {
                println!("[RUST] Ignoring profile update from non-contact user");
                return Ok(());
            },
            Err(e) => {
                println!("[RUST] Error checking if contact: {}", e);
                return Ok(());
            }
        }
    }
    
    // Parse profile content
    let fields: std::collections::HashMap<String, serde_json::Value> = 
        serde_json::from_str(&event.content).unwrap_or_default();
    
    // Emit event to frontend
    let profile_payload = serde_json::json!({
        "pubkey": event_pubkey.to_bech32().unwrap_or_default(),
        "fields": fields,
        "created_at": event.created_at.as_u64(),
        "raw_content": event.content,
        "is_live": true,
        "is_current_user": is_current_user
    });
    
    if let Err(e) = app_handle.emit("profile-updated", &profile_payload) {
        println!("[RUST] Failed to emit profile-updated event: {}", e);
    } else {
        if is_current_user {
            println!("[RUST] Emitted profile-updated event for current user");
        } else {
            println!("[RUST] Emitted profile-updated event for contact");
        }
    }
    
    Ok(())
}

// Draft operations
#[tauri::command]
fn db_save_draft(draft: DbEmail, state: tauri::State<AppState>) -> Result<i64, String> {
    let db = state.get_database()?;
    db.save_draft(&draft).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_drafts(limit: Option<i64>, offset: Option<i64>, user_email: Option<String>, state: tauri::State<AppState>) -> Result<Vec<DbEmail>, String> {
    let db = state.get_database()?;
    db.get_drafts(limit, offset, user_email.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_delete_draft(message_id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.get_database()?;
    db.delete_draft(&message_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn db_delete_sent_email(message_id: String, user_email: Option<String>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] db_delete_sent_email called for message_id: {}", message_id);
    
    let db = state.get_database()?;
    
    // First, try to delete from the email server
    // Get the email from database to find the from_address
    if let Ok(Some(email)) = db.get_email(&message_id) {
        let from_email = email.from_address.trim().to_lowercase();
        
        // Try to find settings that match this email address
        // We'll need to check settings for all pubkeys, but that's complex
        // For now, let's try a simpler approach: get all unique pubkeys and check their settings
        // Actually, let's just try to get settings - if user_email is provided, we can use it to match
        // But settings are keyed by pubkey, not email...
        
        // Simplified approach: Try to construct EmailConfig if we have user_email
        // The frontend should pass the correct user_email that matches the email's from_address
        if let Some(ref user_email_param) = user_email {
            if user_email_param.trim().to_lowercase() == from_email {
                // Try to get settings from database by querying for email_address setting
                // Use a helper function to find pubkeys with matching email
                let pubkeys_vec = match db.find_pubkeys_by_email_setting(user_email_param) {
                    Ok(p) => p,
                    Err(_) => vec![],
                };
                
                if !pubkeys_vec.is_empty() {
                    // Try each pubkey's settings
                    for pubkey in pubkeys_vec {
                        if let Ok(all_settings) = db.get_all_settings(&pubkey) {
                            let email_address = all_settings.get("email_address").cloned();
                            let password = all_settings.get("password").cloned();
                            let imap_host = all_settings.get("imap_host").cloned();
                            let imap_port = all_settings.get("imap_port").and_then(|s| s.parse::<u16>().ok());
                            let use_tls = all_settings.get("imap_use_tls").map(|s| s == "true").unwrap_or(true);
                            
                            // Only attempt server deletion if we have the necessary config
                            if let (Some(email_addr), Some(pwd), Some(host), Some(port)) = (email_address, password, imap_host, imap_port) {
                                if email_addr.to_lowercase() == from_email {
                                    let email_config = crate::types::EmailConfig {
                                        email_address: email_addr,
                                        password: pwd,
                                        smtp_host: all_settings.get("smtp_host").cloned().unwrap_or_default(),
                                        smtp_port: all_settings.get("smtp_port").and_then(|s| s.parse::<u16>().ok()).unwrap_or(587),
                                        imap_host: host,
                                        imap_port: port,
                                        use_tls,
                                        private_key: all_settings.get("nostr_private_key").cloned(),
                                    };
                                    
                                    println!("[RUST] db_delete_sent_email: Attempting to delete from email server");
                                    // Use tokio::time::timeout to prevent hanging
                                    match tokio::time::timeout(
                                        std::time::Duration::from_secs(30),
                                        crate::email::delete_sent_email_from_server(&email_config, &message_id)
                                    ).await {
                                        Ok(Ok(_)) => {
                                            println!("[RUST] db_delete_sent_email: Successfully deleted from email server");
                                        }
                                        Ok(Err(e)) => {
                                            // Log error but continue with local deletion
                                            println!("[RUST] db_delete_sent_email: Failed to delete from email server: {}, continuing with local deletion", e);
                                        }
                                        Err(_) => {
                                            // Timeout occurred
                                            println!("[RUST] db_delete_sent_email: Server deletion timed out after 30 seconds, continuing with local deletion");
                                        }
                                    }
                                    break; // Found matching settings, stop searching
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Always delete locally, even if server deletion failed
    db.delete_sent_email(&message_id, user_email.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_mark_as_read(message_id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.get_database()?;
    db.mark_as_read(&message_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_check_dm_matches_email_encrypted(dm_event_id: String, _user_pubkey: String, _contact_pubkey: String, state: tauri::State<AppState>) -> Result<bool, String> {
    println!("[RUST] db_check_dm_matches_email_encrypted called for DM event_id: {}", dm_event_id);
    let db = state.get_database().map_err(|e| e.to_string())?;
    
    // Get the DM content hash for fast lookup
    let dm_content_hash = match db.get_dm_content_hash_by_event_id(&dm_event_id)
        .map_err(|e| e.to_string())? {
        Some(hash) => hash,
        None => {
            println!("[RUST] DM not found or hash not available");
            return Ok(false);
        }
    };
    
    println!("[RUST] DM content hash: {}", dm_content_hash);
    
    // Use hash-based lookup instead of scanning all emails
    match db.find_email_by_subject_hash(&dm_content_hash)
        .map_err(|e| e.to_string())? {
        Some(email) => {
            println!("[RUST] Found matching email for DM content. Email ID: {}, Subject length: {}", 
                email.id.unwrap_or(0), email.subject.len());
            Ok(true)
        },
        None => {
            println!("[RUST] No matching email found for DM content hash");
            Ok(false)
        }
    }
}

/// Extract encrypted content from ASCII armor
fn extract_encrypted_content_from_armor(body: &str) -> Option<String> {
    // Look for the start and end markers
    let start_marker = "-----BEGIN NOSTR NIP-";
    let end_marker = "-----END NOSTR NIP-";
    
    if let Some(start_pos) = body.find(start_marker) {
        if let Some(end_pos) = body.find(end_marker) {
            // Find the actual start of encrypted content (after the header line)
            if let Some(content_start) = body[start_pos..].find('\n') {
                let content_start = start_pos + content_start + 1;
                let encrypted_content = &body[content_start..end_pos];
                
                // Remove whitespace from the encrypted content
                let cleaned_content = encrypted_content.replace(|c: char| c.is_whitespace(), "");
                return Some(cleaned_content);
            }
        }
    }
    
    None
}

#[tauri::command]
fn db_get_matching_email_id(dm_event_id: String, state: tauri::State<AppState>) -> Result<Option<types::MatchingEmailIdResult>, String> {
    println!("[RUST] db_get_matching_email_id called for DM event_id: {}", dm_event_id);
    let db = state.get_database().map_err(|e| e.to_string())?;
    
    // Get the DM content hash for fast lookup
    let dm_content_hash = match db.get_dm_content_hash_by_event_id(&dm_event_id)
        .map_err(|e| e.to_string())? {
        Some(hash) => hash,
        None => {
            println!("[RUST] DM not found or hash not available");
            return Ok(None);
        }
    };
    
    println!("[RUST] DM content hash: {}", dm_content_hash);
    
    // Use hash-based lookup instead of scanning all emails
    match db.find_email_by_subject_hash(&dm_content_hash)
        .map_err(|e| e.to_string())? {
        Some(email) => {
            let email_id = email.id;
            let message_id = email.message_id.clone();
            println!("[RUST] Found matching email ID: {:?}, message_id: {} for DM event_id: {}", email_id, message_id, dm_event_id);
            Ok(Some(types::MatchingEmailIdResult {
                email_id: email_id,
                message_id: message_id,
            }))
        },
        None => {
            println!("[RUST] No matching email found for DM event_id: {}", dm_event_id);
            Ok(None)
        }
    }
}

#[tauri::command]
fn db_get_matching_email_body(dm_event_id: String, private_key: String, _user_pubkey: String, _contact_pubkey: String, state: tauri::State<AppState>) -> Result<Option<types::MatchingEmailBodyResult>, String> {
    println!("[RUST] db_get_matching_email_body called for DM event_id: {}", dm_event_id);
    let db = state.get_database().map_err(|e| e.to_string())?;
    
    // Get the DM content hash for fast lookup
    let dm_content_hash = match db.get_dm_content_hash_by_event_id(&dm_event_id)
        .map_err(|e| e.to_string())? {
        Some(hash) => hash,
        None => {
            println!("[RUST] DM not found or hash not available");
            return Ok(None);
        }
    };
    
    println!("[RUST] DM content hash: {}", dm_content_hash);
    
    // Use hash-based lookup instead of scanning all emails
    let email = match db.find_email_by_subject_hash(&dm_content_hash)
        .map_err(|e| e.to_string())? {
        Some(email) => email,
        None => {
            println!("[RUST] No matching email found for DM content");
            return Ok(None);
        }
    };
    
    let email_id = email.id;
    println!("[RUST] Found matching email for DM content. Email ID: {:?}, Subject length: {}", 
        email_id, email.subject.len());
    
    // Determine if user sent or received the email
    let user_sent_email = email.sender_pubkey.as_ref().map(|s| s == &_user_pubkey).unwrap_or(false);
    let user_received_email = email.recipient_pubkey.as_ref().map(|s| s == &_user_pubkey).unwrap_or(false);
    
    println!("[RUST] Email sender_pubkey: {:?}, recipient_pubkey: {:?}, user_pubkey: {}", 
        email.sender_pubkey, email.recipient_pubkey, _user_pubkey);
    println!("[RUST] User sent email: {}, User received email: {}", user_sent_email, user_received_email);
    
    // Determine which pubkey to use for decryption
    let pubkeys_to_try: Vec<String> = if user_sent_email {
        // User sent the email: decrypt with user's private key × recipient's public key
        if let Some(ref recipient_pubkey) = email.recipient_pubkey {
            vec![recipient_pubkey.clone()]
        } else {
            // Fallback: find recipient pubkey by email address
            println!("[RUST] Recipient pubkey not set, looking up by email: {}", email.to_address);
            db.find_pubkeys_by_email(&email.to_address).map_err(|e| e.to_string())?
        }
    } else if user_received_email {
        // User received the email: decrypt with user's private key × sender's public key
        if let Some(ref sender_pubkey) = email.sender_pubkey {
            vec![sender_pubkey.clone()]
        } else {
            // Fallback: find sender pubkey by email address
            println!("[RUST] Sender pubkey not set, looking up by email: {}", email.from_address);
            db.find_pubkeys_by_email(&email.from_address).map_err(|e| e.to_string())?
        }
    } else {
        // Unknown direction: try both sender and recipient pubkeys
        println!("[RUST] Cannot determine email direction, trying both sender and recipient pubkeys");
        let mut pubkeys = Vec::new();
        if let Some(ref sender_pubkey) = email.sender_pubkey {
            pubkeys.push(sender_pubkey.clone());
        }
        if let Some(ref recipient_pubkey) = email.recipient_pubkey {
            pubkeys.push(recipient_pubkey.clone());
        }
        // Also try looking up by email addresses (including DMs for recipient, contacts only for sender)
        let sender_pubkeys = db.find_pubkeys_by_email(&email.from_address).map_err(|e| e.to_string())?;
        let recipient_pubkeys = db.find_pubkeys_by_email_including_dms(&email.to_address).map_err(|e| e.to_string())?;
        pubkeys.extend(sender_pubkeys);
        pubkeys.extend(recipient_pubkeys);
        pubkeys
    };
    
    if pubkeys_to_try.is_empty() {
        println!("[RUST] No pubkeys found for decryption");
        return Ok(None);
    }
    
    // Extract encrypted content from ASCII armor
    let encrypted_content = match extract_encrypted_content_from_armor(&email.body) {
        Some(content) => {
            println!("[RUST] Extracted encrypted content from ASCII armor, length: {}", content.len());
            content
        },
        None => {
            println!("[RUST] No ASCII armor found in email body, trying raw body");
            email.body.clone()
        }
    };
    
    // Try to decrypt the email body using each pubkey
    for pubkey in pubkeys_to_try {
        println!("[RUST] Trying to decrypt with pubkey: {}", pubkey);
        match nostr::decrypt_dm_content(&private_key, &pubkey, &encrypted_content) {
            Ok(decrypted_body) => {
                println!("[RUST] Successfully decrypted email body with pubkey: {}", pubkey);
                return Ok(Some(types::MatchingEmailBodyResult {
                    body: decrypted_body,
                    email_id: email_id,
                }));
            },
            Err(e) => {
                println!("[RUST] Failed to decrypt with pubkey {}: {}", pubkey, e);
                // Continue to try the next pubkey
            }
        }
    }
    
    println!("[RUST] Failed to decrypt email body with any pubkey");
    Ok(None)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("[RUST] Starting nostr-mail application...");
    println!("[RUST] Registering Tauri commands...");

    // Create AppState and initialize the database
    let app_state = AppState::new();
    // Use platform-specific app data directory
    match get_app_data_dir() {
        Ok(app_dir) => {
            println!("[RUST] App data directory: {:?}", app_dir);
            if let Err(e) = std::fs::create_dir_all(&app_dir) {
                println!("[RUST] Failed to create app data directory {:?}: {}", app_dir, e);
            } else {
                let db_path = app_dir.join("nostr_mail.db");
                println!("[RUST] Initializing database at: {:?}", db_path);
                match app_state.init_database(&db_path) {
                    Ok(()) => {
                        // Database initialized successfully
                        // Contacts will be loaded per-user when needed
                        println!("[RUST] Database initialized successfully");
                    },
                    Err(e) => println!("[RUST] Failed to initialize database at startup: {}", e),
                }
            }
        },
        Err(e) => println!("[RUST] Could not get app data directory: {}", e),
    }

    let builder = tauri::Builder::default();
    println!("[RUST] Builder created successfully");

    let builder = builder.manage(app_state);
    println!("[RUST] AppState managed successfully");
    
    let builder = builder.plugin(tauri_plugin_opener::init());
    println!("[RUST] Plugin added successfully");
    
    let builder = builder.plugin(tauri_plugin_dialog::init());
    println!("[RUST] Plugin added successfully");
    
    let builder = builder.invoke_handler(tauri::generate_handler![
        greet,
        generate_keypair,
        validate_private_key,
        validate_public_key,
        get_public_key_from_private,
        sign_data,
        verify_signature,
        recheck_email_signature,
        send_direct_message,
        fetch_direct_messages,
        fetch_conversations,
        fetch_conversation_messages,
        fetch_profile,
        fetch_profile_persistent,
        fetch_following_profiles,
        fetch_nostr_following_pubkeys,
        fetch_following_pubkeys_persistent,
        fetch_profiles_persistent,
        get_relays,
        set_relays,
        update_single_relay,
        sync_relay_states,
        init_persistent_nostr_client,
        disconnect_nostr_client,
        get_nostr_client_status,
        get_relay_status,
        test_relay_connection,
        decrypt_dm_content,
        publish_nostr_event,
        send_email,
        construct_email_headers,
        fetch_emails,
        fetch_nostr_emails_last_24h,
        fetch_nostr_emails_smart,
        fetch_image,
        fetch_multiple_images,
        fetch_profiles,
        cache_profile_image,
        get_cached_profile_image,
        clear_image_cache,
        storage_save_contacts,
        storage_get_contacts,
        storage_clear_contacts,
        storage_save_conversations,
        storage_get_conversations,
        storage_clear_conversations,
        storage_save_user_profile,
        storage_get_user_profile,
        storage_clear_user_profile,
        storage_save_settings,
        storage_get_settings,
        storage_save_email_draft,
        storage_get_email_draft,
        storage_clear_email_draft,
        storage_save_relays,
        storage_get_relays,
        storage_clear_all_data,
        storage_get_data_size,
        get_contacts,
        set_contacts,
        save_contact,
        get_contact,
        update_contact_picture_data_url,
        get_conversations,
        set_conversations,
        test_imap_connection,
        test_smtp_connection,
        check_message_confirmation,
        generate_qr_code,
        init_database,
        db_save_contact,
        db_get_contact,
        db_get_all_contacts,
        db_delete_contact,
        db_add_user_contact,
        db_remove_user_contact,
        db_get_public_contact_pubkeys,
        db_update_user_contact_public_status,
        db_find_pubkeys_by_email,
        db_find_pubkeys_by_email_including_dms,
        db_filter_new_contacts,
        db_save_email,
        db_get_email,
        db_get_emails,
        db_search_emails,
        db_update_email_sender_pubkey,
        db_update_email_sender_pubkey_by_id,
        db_update_email_recipient_pubkey,
        db_update_email_recipient_pubkey_by_id,
        db_find_emails_by_message_id,
        db_get_sent_emails,
        db_search_sent_emails,
        db_save_dm,
        db_get_dms_for_conversation,
        db_get_decrypted_dms_for_conversation,
        db_save_attachment,
        db_get_attachments_for_email,
        db_get_attachment,
        db_delete_attachment,
        save_attachment_to_disk,
        save_attachments_as_zip,
        db_save_email_with_attachments,
        db_save_setting,
        db_get_setting,
        db_get_all_settings,
        db_save_settings_batch,
        db_get_database_size,
        db_clear_all_data,
        follow_user,
        publish_follow_list,
        encrypt_nip04_message,
        encrypt_nip04_message_legacy,
        encrypt_message_with_algorithm,
        db_save_relay,
        db_get_all_relays,
        db_delete_relay,
        sync_nostr_emails,
        sync_sent_emails,
        sync_all_emails,
        sync_direct_messages_with_network,
        db_get_all_dm_pubkeys,
        db_get_all_dm_pubkeys_sorted,
        update_profile,
        update_profile_persistent,
        start_live_event_subscription,
        stop_live_event_subscription,
        get_live_subscription_status,
        db_save_draft,
        db_get_drafts,
        db_delete_draft,
        db_delete_sent_email,
        db_mark_as_read,
        db_check_dm_matches_email_encrypted,
        db_get_matching_email_id,
        db_get_matching_email_body,
    ]);
    println!("[RUST] Invoke handler registered successfully");
    
    println!("[RUST] Generating Tauri context...");
    let context = tauri::generate_context!();
    println!("[RUST] Context generated successfully");
    
    println!("[RUST] Starting Tauri application...");
    builder.run(context)
        .expect("error while running tauri application");
}

// HTTP Server Support Functions
// These functions wrap Tauri commands for use in HTTP handlers

pub async fn init_app_state() -> AppState {
    let app_state = AppState::new();
    match get_app_data_dir() {
        Ok(app_dir) => {
            if let Err(e) = std::fs::create_dir_all(&app_dir) {
                println!("[RUST] Failed to create app data directory {:?}: {}", app_dir, e);
            } else {
                let db_path = app_dir.join("nostr_mail.db");
                if let Err(e) = app_state.init_database(&db_path) {
                    println!("[RUST] Failed to initialize database at {:?}: {}", db_path, e);
                }
            }
        },
        Err(e) => println!("[RUST] Could not get app data directory: {}", e),
    }
    app_state
}

// HTTP wrapper functions - these call the same underlying crypto functions
pub fn generate_keypair_http() -> Result<KeyPair, String> {
    crypto::generate_keypair().map_err(|e| e.to_string())
}

pub fn validate_private_key_http(private_key: &str) -> Result<bool, String> {
    crypto::validate_private_key(private_key).map_err(|e| e.to_string())
}

pub fn validate_public_key_http(public_key: &str) -> Result<bool, String> {
    crypto::validate_public_key(public_key).map_err(|e| e.to_string())
}

pub fn get_public_key_from_private_http(private_key: &str) -> Result<String, String> {
    crypto::get_public_key_from_private(private_key).map_err(|e| e.to_string())
}

pub async fn http_init_database(_app_state: std::sync::Arc<AppState>) -> Result<(), String> {
    // Database should already be initialized in init_app_state
    // This is just a placeholder for consistency
    Ok(())
}

pub async fn http_db_get_all_contacts(user_pubkey: String, app_state: std::sync::Arc<AppState>) -> Result<Vec<DbContact>, String> {
    let db = app_state.get_database()?;
    db.get_all_contacts(&user_pubkey).map_err(|e| e.to_string())
}

pub async fn http_db_get_all_relays(app_state: std::sync::Arc<AppState>) -> Result<Vec<DbRelay>, String> {
    let db = app_state.get_database()?;
    db.get_all_relays().map_err(|e| e.to_string())
}

pub async fn http_db_get_emails(
    app_state: std::sync::Arc<AppState>,
    limit: usize,
    offset: usize,
    nostr_only: bool,
    user_email: Option<String>,
) -> Result<Vec<DbEmail>, String> {
    let db = app_state.get_database()?;
    db.get_emails(
        Some(limit as i64),
        Some(offset as i64),
        Some(nostr_only),
        user_email.as_deref(),
    )
    .map_err(|e| e.to_string())
}

pub async fn http_get_relays(app_state: std::sync::Arc<AppState>) -> Result<Vec<Relay>, String> {
    Ok(app_state.relays.lock().unwrap().clone())
}

pub async fn http_set_relays(app_state: std::sync::Arc<AppState>, relays: Vec<Relay>) -> Result<(), String> {
    app_state.update_relays(relays).await
}

pub async fn http_init_persistent_nostr_client(app_state: std::sync::Arc<AppState>, private_key: String) -> Result<(), String> {
    app_state.init_nostr_client(&private_key).await
}

pub async fn http_sync_relay_states(app_state: std::sync::Arc<AppState>) -> Result<Vec<String>, String> {
    // This is the same logic as sync_relay_states command
    let client_option = {
        let client_guard = app_state.nostr_client.lock().unwrap();
        client_guard.clone()
    };
    
    let mut updated_relays = Vec::new();
    
    if let Some(client) = client_option {
        let connected_relays = client.relays().await;
        
        let db = app_state.get_database().map_err(|e| e.to_string())?;
        let all_db_relays = db.get_all_relays().map_err(|e| e.to_string())?;
        
        let mut relays_to_update = Vec::new();
        for db_relay in all_db_relays.iter() {
            if db_relay.is_active {
                let is_connected = connected_relays.iter().any(|(url, _)| url.to_string() == db_relay.url);
                if !is_connected {
                    relays_to_update.push(db_relay.url.clone());
                }
            }
        }
        
        for relay_url in relays_to_update {
            let mut relays_guard = app_state.relays.lock().unwrap();
            if let Some(relay) = relays_guard.iter_mut().find(|r| r.url == relay_url) {
                relay.is_active = false;
            }
            
            let db = app_state.get_database().map_err(|e| e.to_string())?;
            let all_relays = db.get_all_relays().map_err(|e| e.to_string())?;
            if let Some(db_relay) = all_relays.iter().find(|r| r.url == relay_url) {
                let updated_relay = crate::database::DbRelay {
                    id: db_relay.id,
                    url: relay_url.clone(),
                    is_active: false,
                    created_at: db_relay.created_at,
                    updated_at: chrono::Utc::now(),
                };
                if let Err(e) = db.save_relay(&updated_relay) {
                    println!("[RUST] Failed to update relay in database: {}", e);
                }
            }
            
            updated_relays.push(relay_url);
        }
    }
    
    Ok(updated_relays)
}

pub async fn http_get_relay_status(app_state: std::sync::Arc<AppState>) -> Result<Vec<RelayStatus>, String> {
    let client_option = {
        let client_guard = app_state.nostr_client.lock().unwrap();
        client_guard.clone()
    };
    
    let configured_relays = {
        let relays_guard = app_state.relays.lock().unwrap();
        relays_guard.clone()
    };
    
    let mut relay_statuses = Vec::new();
    
    if let Some(client) = client_option {
        let connected_relays = client.relays().await;
        
        for configured_relay in configured_relays {
            let is_connected = connected_relays.iter().any(|(url, _)| url.to_string() == configured_relay.url);
            
            let status = if !configured_relay.is_active {
                RelayConnectionStatus::Disabled
            } else if is_connected {
                RelayConnectionStatus::Connected
            } else {
                RelayConnectionStatus::Disconnected
            };
            
            relay_statuses.push(RelayStatus {
                url: configured_relay.url,
                is_active: configured_relay.is_active,
                status,
            });
        }
    } else {
        for configured_relay in configured_relays {
            let status = if !configured_relay.is_active {
                RelayConnectionStatus::Disabled
            } else {
                RelayConnectionStatus::Disconnected
            };
            
            relay_statuses.push(RelayStatus {
                url: configured_relay.url,
                is_active: configured_relay.is_active,
                status,
            });
        }
    }
    
    Ok(relay_statuses)
}

// Note: start_live_event_subscription requires Tauri AppHandle for event emission
// In HTTP mode, we can't emit Tauri events, so this will return an error
pub async fn http_start_live_event_subscription(_app_state: std::sync::Arc<AppState>, _private_key: String) -> Result<(), String> {
    Err("Live event subscriptions are not supported in HTTP mode. Please use the Tauri app for real-time updates.".to_string())
} 