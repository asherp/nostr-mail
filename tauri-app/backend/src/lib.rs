// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod crypto;
mod email;
mod nostr;
mod types;
mod state;
mod storage;

use types::*;
use state::{AppState, Relay};
use storage::{Storage, Contact, Conversation, UserProfile, AppSettings, EmailDraft};

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
async fn send_direct_message(private_key: String, recipient_pubkey: String, message: String, relays: Vec<String>) -> Result<(), String> {
    println!("[RUST] send_direct_message called");
    nostr::send_direct_message(&private_key, &recipient_pubkey, &message, &relays)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_direct_messages(private_key: String, relays: Vec<String>) -> Result<Vec<NostrEvent>, String> {
    println!("[RUST] fetch_direct_messages called");
    nostr::fetch_direct_messages(&private_key, &relays).await.map_err(|e| e.to_string())
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
async fn fetch_following_profiles(private_key: String, relays: Vec<String>) -> Result<Vec<Profile>, String> {
    println!("[RUST] fetch_following_profiles called");
    nostr::fetch_following_profiles(&private_key, &relays).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn get_relays(state: tauri::State<AppState>) -> Result<Vec<Relay>, String> {
    Ok(state.relays.lock().unwrap().clone())
}

#[tauri::command]
fn set_relays(relays: Vec<Relay>, state: tauri::State<AppState>) -> Result<(), String> {
    *state.relays.lock().unwrap() = relays;
    Ok(())
}

#[tauri::command]
fn decrypt_dm_content(private_key: String, sender_pubkey: String, encrypted_content: String) -> Result<String, String> {
    println!("[RUST] decrypt_dm_content called");
    nostr::decrypt_dm_content(&private_key, &sender_pubkey, &encrypted_content).map_err(|e| e.to_string())
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
async fn send_email(email_config: EmailConfig, to_address: String, subject: String, body: String) -> Result<(), String> {
    println!("[RUST] send_email called");
    email::send_email(&email_config, &to_address, &subject, &body)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_emails(email_config: EmailConfig, limit: usize, search_query: Option<String>) -> Result<Vec<EmailMessage>, String> {
    println!("[RUST] fetch_emails called with search: {:?}", search_query);
    email::fetch_emails(&email_config, limit, search_query).await.map_err(|e| e.to_string())
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
        data_url,
        timestamp,
    };
    
    state.image_cache.lock().unwrap().insert(pubkey.clone(), cached_image);
    println!("[RUST] Cached image for pubkey: {}", pubkey);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("[RUST] Starting nostr-mail application...");
    println!("[RUST] Registering Tauri commands...");
    
    let builder = tauri::Builder::default();
    println!("[RUST] Builder created successfully");
    
    let builder = builder.manage(AppState::new());
    println!("[RUST] AppState managed successfully");
    
    let builder = builder.plugin(tauri_plugin_opener::init());
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
        fetch_following_profiles,
        get_relays,
        set_relays,
        decrypt_dm_content,
        publish_nostr_event,
        send_email,
        fetch_emails,
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
    ]);
    println!("[RUST] Invoke handler registered successfully");
    
    println!("[RUST] Generating Tauri context...");
    let context = tauri::generate_context!();
    println!("[RUST] Context generated successfully");
    
    println!("[RUST] Starting Tauri application...");
    builder.run(context)
        .expect("error while running tauri application");
} 