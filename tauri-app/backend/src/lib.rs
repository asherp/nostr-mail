// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod crypto;
mod email;
mod nostr;
mod types;
mod state;

use types::*;
use state::{AppState, Relay};

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
async fn fetch_emails(email_config: EmailConfig, limit: usize) -> Result<Vec<EmailMessage>, String> {
    println!("[RUST] fetch_emails called");
    email::fetch_emails(&email_config, limit).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_image(url: String) -> Result<String, String> {
    println!("[RUST] fetch_image called for url: {}", url);
    nostr::fetch_image_as_data_url(&url).await.map_err(|e| e.to_string())
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
        fetch_profile,
        fetch_following_profiles,
        get_relays,
        set_relays,
        decrypt_dm_content,
        publish_nostr_event,
        send_email,
        fetch_emails,
        fetch_image,
    ]);
    println!("[RUST] Invoke handler registered successfully");
    
    println!("[RUST] Generating Tauri context...");
    let context = tauri::generate_context!();
    println!("[RUST] Context generated successfully");
    
    println!("[RUST] Starting Tauri application...");
    builder.run(context)
        .expect("error while running tauri application");
} 