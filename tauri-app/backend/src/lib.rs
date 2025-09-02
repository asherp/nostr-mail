// NOTE: For Tauri commands, do NOT use 'pub' (do not export with pub) on the function definitions.
// Exporting Tauri commands with 'pub' can cause duplicate macro errors at compile time.
// Only use 'async fn' or 'fn' without 'pub' for #[tauri::command] functions.
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod crypto;
mod email;
mod nostr;
mod types;
mod state;
mod storage;
mod database;

use types::*;
use state::{AppState, Relay};
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
        is_read: true, // or use a real field if available
        raw_headers: raw_headers.clone(),
        nostr_pubkey: email.nostr_pubkey.clone(),
        message_id: Some(email.message_id.clone()),
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
    
    // Create filter for single profile
    let profile_filter = Filter::new()
        .author(public_key)
        .kind(Kind::from(0)) // Profile events are kind 0
        .limit(1);
        
    let profile_events = client
        .fetch_events(profile_filter, Duration::from_secs(30))
        .await
        .map_err(|e| e.to_string())?;
    
    println!("[RUST] Found {} profile events using persistent client", profile_events.len());
    
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
    println!("[RUST] sync_relay_states called - auto-disabling disconnected relays");
    
    let client_option = {
        let client_guard = state.nostr_client.lock().unwrap();
        client_guard.clone()
    };
    
    let mut updated_relays = Vec::new();
    
    if let Some(client) = client_option {
        let connected_relays = client.relays().await;
        println!("[RUST] Currently connected relays: {} relays", connected_relays.len());
        for (url, _) in connected_relays.iter() {
            println!("[RUST]   Connected: {}", url);
        }
        
        // Get current relay configuration from database (not just in-memory state)
        let db = state.get_database().map_err(|e| e.to_string())?;
        let all_db_relays = db.get_all_relays().map_err(|e| e.to_string())?;
        println!("[RUST] Database relays: {} total", all_db_relays.len());
        for db_relay in all_db_relays.iter() {
            println!("[RUST]   DB relay: {} (active: {})", db_relay.url, db_relay.is_active);
        }
        
        let mut relays_to_update = Vec::new();
        for db_relay in all_db_relays.iter() {
            if db_relay.is_active {
                let is_connected = connected_relays.iter().any(|(url, _)| url.to_string() == db_relay.url);
                if !is_connected {
                    // This relay is marked as active but not actually connected
                    println!("[RUST] Found disconnected active relay: {}", db_relay.url);
                    relays_to_update.push(db_relay.url.clone());
                }
            }
        }
        
        // Update disconnected relays to inactive
        for relay_url in relays_to_update {
            println!("[RUST] Auto-disabling disconnected relay: {}", relay_url);
            
            // Update in-memory state (if relay exists there)
            {
                let mut relays_guard = state.relays.lock().unwrap();
                if let Some(relay) = relays_guard.iter_mut().find(|r| r.url == relay_url) {
                    relay.is_active = false;
                    println!("[RUST] Updated in-memory state for relay: {}", relay_url);
                } else {
                    println!("[RUST] Relay {} not found in in-memory state (database-only relay)", relay_url);
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
async fn send_email(email_config: EmailConfig, to_address: String, subject: String, body: String, nostr_npub: Option<String>, message_id: Option<String>, attachments: Option<Vec<crate::types::EmailAttachment>>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[RUST] send_email called with {} attachments", attachments.as_ref().map(|a| a.len()).unwrap_or(0));
    
    // First send the email via SMTP
    email::send_email(&email_config, &to_address, &subject, &body, nostr_npub.as_deref(), message_id.as_deref(), attachments.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    
    // If sending was successful, save the email to the database
    if let Some(msg_id) = &message_id {
        println!("[RUST] send_email: Saving sent email to database with message_id: {}", msg_id);
        
        let db = state.get_database().map_err(|e| e.to_string())?;
        
        // Create the email record
        let email_record = crate::database::Email {
            id: None,
            message_id: msg_id.clone(),
            from_address: email_config.email_address.clone(),
            to_address: to_address.clone(),
            subject: subject.clone(),
            body: body.clone(),
            body_plain: Some(body.clone()),
            body_html: None,
            received_at: chrono::Utc::now(),
            is_nostr_encrypted: nostr_npub.is_some(),
            nostr_pubkey: if let Some(private_key) = &email_config.private_key {
                crate::crypto::get_public_key_from_private(private_key).ok()
            } else {
                None
            },
            raw_headers: None, // We don't have the raw headers here
            is_draft: false,
            is_read: true, // Mark sent emails as read
            updated_at: None,
            created_at: chrono::Utc::now(),
        };
        
        // Save email with attachments if any
        if let Some(attachments) = &attachments {
            if !attachments.is_empty() {
                db.save_email_with_attachments(&email_record, attachments)
                    .map_err(|e| {
                        println!("[RUST] send_email: Failed to save email with attachments: {}", e);
                        format!("Failed to save sent email to database: {}", e)
                    })?;
                println!("[RUST] send_email: Successfully saved sent email with {} attachments to database", attachments.len());
            } else {
                db.save_email(&email_record)
                    .map_err(|e| {
                        println!("[RUST] send_email: Failed to save email: {}", e);
                        format!("Failed to save sent email to database: {}", e)
                    })?;
                println!("[RUST] send_email: Successfully saved sent email to database");
            }
        } else {
            db.save_email(&email_record)
                .map_err(|e| {
                    println!("[RUST] send_email: Failed to save email: {}", e);
                    format!("Failed to save sent email to database: {}", e)
                })?;
            println!("[RUST] send_email: Successfully saved sent email to database");
        }
    } else {
        println!("[RUST] send_email: No message_id provided, skipping database save");
    }
    
    Ok(())
}

#[tauri::command]
async fn construct_email_headers(email_config: EmailConfig, to_address: String, subject: String, body: String, nostr_npub: Option<String>, message_id: Option<String>, attachments: Option<Vec<crate::types::EmailAttachment>>) -> Result<String, String> {
    println!("[RUST] construct_email_headers called with {} attachments", attachments.as_ref().map(|a| a.len()).unwrap_or(0));
    email::construct_email_headers(&email_config, &to_address, &subject, &body, nostr_npub.as_deref(), message_id.as_deref(), attachments.as_ref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_emails(email_config: EmailConfig, limit: usize, search_query: Option<String>, only_nostr: bool) -> Result<Vec<EmailMessage>, String> {
    println!("[RUST] fetch_emails called with search: {:?}, only_nostr: {}", search_query, only_nostr);
    email::fetch_emails(&email_config, limit, search_query, only_nostr).await.map_err(|e| e.to_string())
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
    
    // Get the persistent client
    let client = {
        let client_guard = state.nostr_client.lock().unwrap();
        match client_guard.as_ref() {
            Some(client) => client.clone(),
            None => return Err("Persistent Nostr client not initialized".to_string()),
        }
    };
    
    // Parse all pubkeys
    let public_keys: Result<Vec<PublicKey>, String> = pubkeys.iter()
        .map(|pubkey_str| {
            PublicKey::from_bech32(pubkey_str)
                .or_else(|_| PublicKey::from_hex(pubkey_str))
                .map_err(|e| format!("Invalid pubkey {}: {}", pubkey_str, e))
        })
        .collect();
    
    let public_keys = public_keys?;
    
    println!("[RUST] Using persistent client to fetch {} profiles", public_keys.len());
    
    // Fetch profiles for all pubkeys in one request using persistent client
    let profiles_filter = Filter::new()
        .authors(public_keys.clone())
        .kind(Kind::from(0)) // Profile events are kind 0
        .limit(1000); // Allow for multiple profiles
        
    let profile_events = client
        .fetch_events(profiles_filter, Duration::from_secs(30))
        .await
        .map_err(|e| e.to_string())?;
    
    println!("[RUST] Found {} profile events using persistent client", profile_events.len());
    
    let mut results = Vec::new();
    
    // Process each requested pubkey
    for (i, pubkey_str) in pubkeys.iter().enumerate() {
        let public_key = &public_keys[i];
        
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
                    
                    println!("[RUST] Found profile for: {}", pubkey_str);
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
            println!("[RUST] No profile found for pubkey: {}", pubkey_str);
            results.push(ProfileResult {
                pubkey: pubkey_str.clone(),
                fields: std::collections::HashMap::new(),
                raw_content: "{}".to_string(),
            });
        }
    }
    
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
    email::test_imap_connection(&email_config)
        .await
        .map_err(|e| e.to_string())
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

#[tauri::command]
fn init_database(state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] init_database called");
    // Use dirs crate for app data directory
    let app_dir = dirs::data_dir()
        .ok_or_else(|| "Could not get app data directory".to_string())?
        .join("nostr-mail");
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    let db_path = app_dir.join("nostr_mail.db");
    println!("[RUST] Database path: {:?}", db_path);
    state.init_database(&db_path)
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
fn db_get_all_contacts(state: tauri::State<AppState>) -> Result<Vec<DbContact>, String> {
    println!("[RUST] db_get_all_contacts called");
    let db = state.get_database()?;
    db.get_all_contacts().map_err(|e| e.to_string())
}

#[tauri::command]
fn db_delete_contact(pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_delete_contact called for pubkey: {}", pubkey);
    let db = state.get_database()?;
    db.delete_contact(&pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_find_pubkeys_by_email(email: String, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let db = state.get_database()?;
    db.find_pubkeys_by_email(&email).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_filter_new_contacts(pubkeys: Vec<String>, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let db = state.get_database()?;
    let existing: std::collections::HashSet<String> = db.get_all_contacts()
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
fn db_update_email_nostr_pubkey(message_id: String, nostr_pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.get_database()?;
    db.update_email_nostr_pubkey(&message_id, &nostr_pubkey).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_update_email_nostr_pubkey_by_id(id: i64, nostr_pubkey: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_update_email_nostr_pubkey_by_id called");
    let db = state.get_database().map_err(|e| e.to_string())?;
    db.update_email_nostr_pubkey_by_id(id, &nostr_pubkey).map_err(|e| e.to_string())
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
    println!("[RUST] db_get_sent_emails called");
    if let Some(ref email) = user_email {
        println!("[RUST] db_get_sent_emails: Filtering for user_email: {}", email);
    } else {
        println!("[RUST] db_get_sent_emails: No user_email filter provided");
    }
    let db = state.get_database()?;
    let emails = db.get_sent_emails(limit, offset, user_email.as_deref()).map_err(|e| e.to_string())?;
    let mapped: Vec<EmailMessage> = emails.iter().map(map_db_email_to_email_message).collect();
    println!("[RUST] Sending {} sent emails to frontend:", mapped.len());
    for (i, email) in mapped.iter().enumerate() {
        println!("[RUST] Sent Email {}: {:#?}", i + 1, email);
    }
    Ok(mapped)
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
fn db_save_setting(key: String, value: String, state: tauri::State<AppState>) -> Result<(), String> {
    println!("[RUST] db_save_setting called for key: {}", key);
    let db = state.get_database()?;
    db.save_setting(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_setting(key: String, state: tauri::State<AppState>) -> Result<Option<String>, String> {
    println!("[RUST] db_get_setting called for key: {}", key);
    let db = state.get_database()?;
    db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_get_all_settings(state: tauri::State<AppState>) -> Result<std::collections::HashMap<String, String>, String> {
    println!("[RUST] db_get_all_settings called");
    let db = state.get_database()?;
    db.get_all_settings().map_err(|e| e.to_string())
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
    let db = state.get_database().map_err(|e| e.to_string())?;
    email::sync_sent_emails_to_db(&config, &db).await.map_err(|e| e.to_string())
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
    _state: &AppState,
    user_pubkey: &PublicKey,
) -> Result<(), String> {
    println!("[RUST] Processing live profile update: {}", event.id.to_hex());
    
    // Only process profile updates for the current user
    if event.pubkey != *user_pubkey {
        println!("[RUST] Ignoring profile update from different user");
        return Ok(());
    }
    
    // Parse profile content
    let fields: std::collections::HashMap<String, serde_json::Value> = 
        serde_json::from_str(&event.content).unwrap_or_default();
    
    // Emit event to frontend
    let profile_payload = serde_json::json!({
        "pubkey": event.pubkey.to_bech32().unwrap_or_default(),
        "fields": fields,
        "created_at": event.created_at.as_u64(),
        "raw_content": event.content,
        "is_live": true
    });
    
    if let Err(e) = app_handle.emit("profile-updated", &profile_payload) {
        println!("[RUST] Failed to emit profile-updated event: {}", e);
    } else {
        println!("[RUST] Emitted profile-updated event");
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
fn db_get_drafts(user_email: Option<String>, state: tauri::State<AppState>) -> Result<Vec<DbEmail>, String> {
    let db = state.get_database()?;
    db.get_drafts(user_email.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn db_delete_draft(message_id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.get_database()?;
    db.delete_draft(&message_id).map_err(|e| e.to_string())
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
    
    // Get the encrypted DM content from the database using a proper method
    // We'll need to add this method to the database module
    let encrypted_dm_content = db.get_dm_encrypted_content_by_event_id(&dm_event_id)
        .map_err(|e| e.to_string())?;
    
    println!("[RUST] Found encrypted DM content length: {}", encrypted_dm_content.len());
    println!("[RUST] Encrypted DM content sample: {}", encrypted_dm_content.chars().take(50).collect::<String>());
    
    // Get all emails for this user that have encrypted subjects
    let emails = db.get_emails(None, None, Some(true), None).map_err(|e| e.to_string())?;
    println!("[RUST] Found {} emails with encrypted subjects", emails.len());
    
    for email in emails {
        println!("[RUST] Checking email ID {} with subject length: {}", email.id.unwrap_or(0), email.subject.len());
        println!("[RUST] Email subject sample: {}", email.subject.chars().take(50).collect::<String>());
        
        // Check if the encrypted email subject matches the encrypted DM content
        if email.subject == encrypted_dm_content {
            println!("[RUST] Found matching email for DM content. Email ID: {}, Subject length: {}", 
                email.id.unwrap_or(0), email.subject.len());
            return Ok(true);
        }
    }
    
    println!("[RUST] No matching email found for DM content");
    Ok(false)
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
fn db_get_matching_email_body(dm_event_id: String, private_key: String, _user_pubkey: String, _contact_pubkey: String, state: tauri::State<AppState>) -> Result<Option<String>, String> {
    println!("[RUST] db_get_matching_email_body called for DM event_id: {}", dm_event_id);
    let db = state.get_database().map_err(|e| e.to_string())?;
    
    // Get the encrypted DM content from the database
    let encrypted_dm_content = db.get_dm_encrypted_content_by_event_id(&dm_event_id)
        .map_err(|e| e.to_string())?;
    
    println!("[RUST] Found encrypted DM content length: {}", encrypted_dm_content.len());
    
    // Get all emails for this user that have encrypted subjects
    let emails = db.get_emails(None, None, Some(true), None).map_err(|e| e.to_string())?;
    println!("[RUST] Found {} emails with encrypted subjects", emails.len());
    
    for email in emails {
        // Check if the encrypted email subject matches the encrypted DM content
        if email.subject == encrypted_dm_content {
            println!("[RUST] Found matching email for DM content. Email ID: {}, Subject length: {}", 
                email.id.unwrap_or(0), email.subject.len());
            
            // Extract recipient email from the email
            let recipient_email = email.to_address;
            println!("[RUST] Recipient email: {}", recipient_email);
            
            // Find the recipient's pubkey using the email address
            let recipient_pubkeys = db.find_pubkeys_by_email(&recipient_email)
                .map_err(|e| e.to_string())?;
            
            if recipient_pubkeys.is_empty() {
                println!("[RUST] No pubkeys found for recipient email: {}", recipient_email);
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
            
            // Try to decrypt the email body using each recipient pubkey
            for recipient_pubkey in recipient_pubkeys {
                println!("[RUST] Trying to decrypt with recipient pubkey: {}", recipient_pubkey);
                match nostr::decrypt_dm_content(&private_key, &recipient_pubkey, &encrypted_content) {
                    Ok(decrypted_body) => {
                        println!("[RUST] Successfully decrypted email body with pubkey: {}", recipient_pubkey);
                        return Ok(Some(decrypted_body));
                    },
                    Err(e) => {
                        println!("[RUST] Failed to decrypt with pubkey {}: {}", recipient_pubkey, e);
                        // Continue to try the next pubkey
                    }
                }
            }
            
            println!("[RUST] Failed to decrypt email body with any recipient pubkey");
            return Ok(None);
        }
    }
    
    println!("[RUST] No matching email found for DM content");
    Ok(None)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("[RUST] Starting nostr-mail application...");
    println!("[RUST] Registering Tauri commands...");

    // Create AppState and initialize the database
    let app_state = AppState::new();
    // Use dirs crate for app data directory
    let app_dir = dirs::data_dir()
        .ok_or_else(|| "Could not get app data directory".to_string())
        .map(|d| d.join("nostr-mail"));
    match app_dir {
        Ok(app_dir) => {
            if let Err(e) = std::fs::create_dir_all(&app_dir) {
                println!("[RUST] Failed to create app data directory: {}", e);
            } else {
                let db_path = app_dir.join("nostr_mail.db");
                match app_state.init_database(&db_path) {
                    Ok(()) => {
                        match app_state.get_database() {
                            Ok(db) => match db.get_all_contacts() {
                                Ok(contacts) => println!("[RUST] Database contains {} contacts at startup", contacts.len()),
                                Err(e) => println!("[RUST] Failed to get contacts from database at startup: {}", e),
                            },
                            Err(e) => println!("[RUST] Database not initialized at startup: {}", e),
                        }
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
        db_find_pubkeys_by_email,
        db_filter_new_contacts,
        db_save_email,
        db_get_email,
        db_get_emails,
        db_update_email_nostr_pubkey,
        db_update_email_nostr_pubkey_by_id,
        db_find_emails_by_message_id,
        db_get_sent_emails,
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
        db_get_database_size,
        db_clear_all_data,
        follow_user,
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
        db_mark_as_read,
        db_check_dm_matches_email_encrypted,
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