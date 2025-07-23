use nostr_sdk::prelude::*;
use anyhow::Result;
use crate::types::{NostrEvent, Profile};
use std::time::Duration;
use serde::{Serialize, Deserialize};
use base64::engine::{general_purpose, Engine as _};

fn parse_pubkey(pubkey: &str) -> anyhow::Result<nostr_sdk::prelude::PublicKey> {
    if pubkey.starts_with("npub1") {
        nostr_sdk::prelude::PublicKey::from_bech32(pubkey).map_err(|e| anyhow::anyhow!(e))
    } else if pubkey.len() == 64 && pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        nostr_sdk::prelude::PublicKey::from_hex(pubkey).map_err(|e| anyhow::anyhow!(e))
    } else {
        Err(anyhow::anyhow!("Unknown pubkey format: {}", pubkey))
    }
}

pub async fn publish_event(
    private_key: &str,
    content: &str,
    kind: u16,
    tags: Vec<Vec<String>>,
    relays: &[String],
) -> Result<String> {
    // Parse private key from bech32 format
    let secret_key = SecretKey::from_bech32(private_key)?;
    let keys = Keys::new(secret_key);
    
    let event_builder = if kind == 0 {
        // Kind 0: Profile metadata
        let metadata: Metadata = serde_json::from_str(content)?;
        EventBuilder::metadata(&metadata)
    } else {
        // Other kinds (e.g., text note)
        let mut builder = EventBuilder::new(Kind::from(kind), content.to_string());
        for tag_vec in tags {
            if let Ok(tag) = Tag::parse(tag_vec) {
                builder = builder.tag(tag);
            }
        }
        builder
    };
    
    // Build and sign event
    let event = event_builder.build(keys.public_key()).sign_with_keys(&keys)?;
    
    // Connect to relays and publish
    let client = Client::new(keys);
    if relays.is_empty() {
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    } else {
        for relay in relays {
            client.add_relay(relay.clone()).await?;
        }
    }
    client.connect().await;
    
    // Publish event
    let event_id = client.send_event(&event).await?;
    
    Ok(event_id.to_hex())
}

pub async fn send_direct_message(
    private_key: &str,
    recipient_pubkey: &str,
    message: &str,
    relays: &[String],
) -> Result<String> {
    // Parse keys from bech32 format
    let secret_key = SecretKey::from_bech32(private_key)?;
    let keys = Keys::new(secret_key.clone());
    let recipient = PublicKey::from_bech32(recipient_pubkey)?;
    
    // Create client
    let client = Client::new(keys.clone());
    if relays.is_empty() {
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    } else {
        for relay in relays {
            client.add_relay(relay.clone()).await?;
        }
    }
    client.connect().await;
    
    // Encrypt the message using NIP-04
    let encrypted_content = nip04::encrypt(&secret_key, &recipient, message)?;
    println!("[NOSTR] Encrypted message using NIP-04: {}", &encrypted_content[..encrypted_content.len().min(50)]);
    
    // Create the encrypted direct message event manually
    let event = EventBuilder::new(Kind::EncryptedDirectMessage, encrypted_content)
        .tag(Tag::public_key(recipient))
        .build(keys.public_key())
        .sign_with_keys(&keys)?;
    
    println!("[NOSTR] Created encrypted direct message event with ID: {}", event.id.to_hex());
    println!("[NOSTR] Event content length: {}", event.content.len());
    println!("[NOSTR] Event tags: {:?}", event.tags);
    
    // Send the event to relays
    let event_id = client.send_event(&event).await?;
    
    println!("[NOSTR] Successfully sent encrypted direct message, event ID: {}", event_id.to_hex());
    
    Ok(event_id.to_hex())
}

pub async fn check_message_confirmation(event_id: &str, relays: &[String]) -> Result<bool> {
    println!("[NOSTR] check_message_confirmation called for event: {}", event_id);
    
    // Parse event ID
    let event_id = EventId::from_hex(event_id)?;
    println!("[NOSTR] Parsed event ID successfully");
    
    // Create a client with generated keys (we don't need our own keys for this)
    let keys = Keys::generate();
    let client = Client::new(keys);
    
    // Add relays
    for relay in relays {
        println!("[NOSTR] Adding relay for confirmation check: {}", relay);
        client.add_relay(relay.clone()).await?;
    }
    
    // If no relays provided, use defaults
    if relays.is_empty() {
        println!("[NOSTR] No relays provided, using defaults");
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    }
    
    client.connect().await;
    println!("[NOSTR] Connected to relays for confirmation check");
    
    // Create filter to look for this specific event
    let filter = Filter::new().id(event_id);
    println!("[NOSTR] Created filter for event ID: {}", event_id.to_hex());
    
    // Try to fetch the event with a short timeout
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            println!("[NOSTR] Found {} events for confirmation check", events.len());
            // If we found the event, it's confirmed
            let confirmed = !events.is_empty();
            println!("[NOSTR] Message confirmation result: {}", confirmed);
            Ok(confirmed)
        },
        Err(e) => {
            println!("[NOSTR] Error fetching events for confirmation: {}", e);
            // If we can't fetch it, it's not confirmed yet
            Ok(false)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub contact_pubkey: String,
    pub contact_name: Option<String>,
    pub last_message: String,
    pub last_timestamp: i64,
    pub message_count: usize,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: String,
    pub sender_pubkey: String,
    pub receiver_pubkey: String,
    pub content: String,
    pub timestamp: i64,
    pub is_sent: bool,
}

pub async fn fetch_direct_messages(
    private_key: &str,
    relays: &[String],
    since: Option<i64>,
) -> Result<Vec<NostrEvent>> {
    // Parse private key from bech32 format
    let secret_key = SecretKey::from_bech32(private_key)?;
    let keys = Keys::new(secret_key);
    let client = Client::new(keys.clone());
    
    // Add relays
    for relay in relays {
        client.add_relay(relay.clone()).await?;
    }
    
    // If no relays provided, use defaults
    if relays.is_empty() {
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    }
    
    client.connect().await;
    
    // Create filter for encrypted direct messages - fetch both sent and received
    let mut sent_filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(keys.public_key());
    let mut received_filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(keys.public_key());
    if let Some(since_ts) = since {
        let ts = Timestamp::from(since_ts as u64);
        sent_filter = sent_filter.since(ts);
        received_filter = received_filter.since(ts);
    }
    
    // Get events for both sent and received
    let sent_events = client.fetch_events(sent_filter, Duration::from_secs(10)).await?;
    let received_events = client.fetch_events(received_filter, Duration::from_secs(10)).await?;
    
    // Combine and deduplicate events
    let mut all_events = sent_events;
    all_events.extend(received_events);
    
    // Remove duplicates based on event ID
    let mut unique_events = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    
    for event in all_events {
        if seen_ids.insert(event.id) {
            unique_events.push(event);
        }
    }
    
    // Sort by timestamp (newest first)
    unique_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    
    // Convert to our format
    let nostr_events: Vec<NostrEvent> = unique_events.into_iter().map(|event| {
        NostrEvent {
            id: event.id.to_hex(),
            pubkey: event.pubkey.to_bech32().unwrap_or_default(),
            created_at: event.created_at.as_u64() as i64,
            kind: event.kind.as_u16(),
            tags: event.tags.into_iter().map(|tag| tag.to_vec()).collect(),
            content: event.content,
            sig: event.sig.to_string(),
        }
    }).collect();
    
    Ok(nostr_events)
}

pub async fn fetch_events(
    pubkey: &str,
    kind: Option<u16>,
    relays: &[String],
) -> Result<Vec<NostrEvent>> {
    let keys = Keys::generate();
    let client = Client::new(keys);
    
    // Add relays
    for relay in relays {
        client.add_relay(relay.clone()).await?;
    }
    
    // If no relays provided, use defaults
    if relays.is_empty() {
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    }
    
    client.connect().await;
    
    // Parse pubkey from bech32 format
    let pubkey = PublicKey::from_bech32(pubkey)?;
    
    // Create filter
    let mut filter = Filter::new();
    filter = filter.author(pubkey);
    
    if let Some(k) = kind {
        filter = filter.kind(Kind::from(k));
    }
    
    // Get events
    let events = client.fetch_events(filter, Duration::from_secs(10)).await?;
    
    // Convert to our format
    let nostr_events: Vec<NostrEvent> = events.into_iter().map(|event| {
        NostrEvent {
            id: event.id.to_hex(),
            pubkey: event.pubkey.to_bech32().unwrap_or_default(),
            created_at: event.created_at.as_u64() as i64,
            kind: event.kind.as_u16(),
            tags: event.tags.into_iter().map(|tag| tag.to_vec()).collect(),
            content: event.content,
            sig: event.sig.to_string(),
        }
    }).collect();
    
    Ok(nostr_events)
}

#[allow(dead_code)]
pub async fn validate_nip05(_pubkey: &str, nip05: &str) -> Result<bool> {
    if !nip05.contains('@') {
        return Ok(false);
    }
    
    let parts: Vec<&str> = nip05.split('@').collect();
    if parts.len() != 2 {
        return Ok(false);
    }
    
    let _username = parts[0];
    let _domain = parts[1];
    
    let _url = format!("https://{}/.well-known/nostr.json?name={}", _domain, _username);
    
    // For now, we'll use a simple approach without reqwest
    // In a full implementation, you'd want to make an HTTP request
    // For now, return true as a placeholder
    Ok(true)
}

pub fn parse_profile_from_event(event: &NostrEvent) -> Result<Profile> {
    if event.kind != 0 {
        return Err(anyhow::anyhow!("Event is not a profile event"));
    }
    // Parse JSON content
    let content: serde_json::Value = serde_json::from_str(&event.content)?;
    let fields = content.as_object()
        .map(|m| m.clone().into_iter().collect())
        .unwrap_or_default();
    Ok(Profile {
        pubkey: event.pubkey.clone(),
        fields,
    })
}

// Utility function to decrypt DM content
pub fn decrypt_dm_content(
    private_key: &str,
    sender_pubkey: &str,
    encrypted_content: &str,
) -> Result<String> {
    let secret_key = SecretKey::from_bech32(private_key)?;
    let sender = parse_pubkey(sender_pubkey)?;
    
    // Try NIP-44 first (newer standard)
    if let Ok(decrypted) = nip44::decrypt(&secret_key, &sender, encrypted_content) {
        return Ok(decrypted);
    }
    
    // Try NIP-04 format: base64(encrypted_content)?iv=base64(iv)
    if encrypted_content.contains("?iv=") {
        println!("[NOSTR] Attempting NIP-04 decryption");
        println!("[NOSTR] Encrypted content: {}", encrypted_content);
        
        // Use the actual NIP-04 decryption from nostr-sdk
        match nip04::decrypt(&secret_key, &sender, encrypted_content) {
            Ok(decrypted) => {
                println!("[NOSTR] NIP-04 decryption successful");
                return Ok(decrypted);
            }
            Err(e) => {
                println!("[NOSTR] NIP-04 decryption failed: {:?}", e);
                return Err(anyhow::anyhow!("NIP-04 decryption failed: {:?}", e));
            }
        }
    }
    
    // If both fail, return error
    Err(anyhow::anyhow!("Failed to decrypt with both NIP-04 and NIP-44"))
}

pub async fn fetch_following_profiles(
    private_key: &str,
    relays: &[String],
) -> Result<Vec<Profile>> {
    println!("[NOSTR] fetch_following_profiles called");
    println!("[NOSTR] Using relays: {:?}", relays);
    
    let secret_key = SecretKey::from_bech32(private_key)?;
    let keys = Keys::new(secret_key);
    let client = Client::new(keys.clone());
    
    println!("[NOSTR] User public key: {}", keys.public_key().to_bech32()?);

    // Connect to relays
    if relays.is_empty() {
        println!("[NOSTR] No relays provided, using defaults");
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    } else {
        for relay in relays {
            println!("[NOSTR] Adding relay: {}", relay);
            client.add_relay(relay.clone()).await?;
        }
    }
    client.connect().await;
    println!("[NOSTR] Connected to relays");

    // Fetch user's kind 3 event (contact list)
    let contact_list_filter = Filter::new()
        .author(keys.public_key())
        .kind(Kind::ContactList)
        .limit(1);

    println!("[NOSTR] Fetching contact list events...");
    let contact_events = client.fetch_events(contact_list_filter, Duration::from_secs(10)).await?;
    println!("[NOSTR] Found {} contact list events", contact_events.len());
    
    let latest_contact_event = contact_events.into_iter().max_by_key(|e| e.created_at);

    if let Some(event) = latest_contact_event {
        println!("[NOSTR] Latest contact event ID: {}", event.id.to_hex());
        println!("[NOSTR] Contact event created at: {}", event.created_at.as_u64());
        println!("[NOSTR] Contact event has {} tags", event.tags.len());
        
        // Debug: Show all tag kinds
        println!("[NOSTR] Tag kinds found:");
        for (i, tag) in event.tags.iter().enumerate() {
            println!("[NOSTR]   Tag {}: kind='{}', content='{}'", 
                i, 
                tag.kind().as_str(), 
                tag.content().unwrap_or("None")
            );
        }
        
        // Get followed pubkeys from 'p' tags
        let followed_pubkeys: Vec<PublicKey> = event.tags
            .iter()
            .filter(|tag| tag.kind().as_str() == "p")
            .filter_map(|tag| {
                tag.content().and_then(|pk| {
                    // Try bech32 (npub) first, then hex
                    PublicKey::from_bech32(pk)
                        .or_else(|_| PublicKey::from_hex(pk))
                        .ok()
                })
            })
            .collect();
        
        println!("[NOSTR] Found {} followed pubkeys", followed_pubkeys.len());
        for (i, pk) in followed_pubkeys.iter().enumerate() {
            println!("[NOSTR]   {}. {}", i + 1, pk.to_bech32()?);
        }
        
        if followed_pubkeys.is_empty() {
            println!("[NOSTR] No followed pubkeys found, returning empty list");
            return Ok(vec![]);
        }

        // Fetch profiles for all followed pubkeys
        println!("[NOSTR] Fetching profiles for {} pubkeys...", followed_pubkeys.len());
        let profiles_filter = Filter::new()
            .authors(followed_pubkeys)
            .kind(Kind::from(0)); // Profile events are kind 0
            
        let profile_events = client.fetch_events(profiles_filter, Duration::from_secs(30)).await?;
        println!("[NOSTR] Found {} profile events", profile_events.len());
        
        // Use a map to store only the latest profile for each pubkey
        let mut latest_profiles: std::collections::HashMap<PublicKey, Event> = std::collections::HashMap::new();
        for profile_event in profile_events {
            latest_profiles.entry(profile_event.pubkey)
                .and_modify(|e| {
                    if profile_event.created_at > e.created_at {
                        *e = profile_event.clone();
                    }
                })
                .or_insert(profile_event);
        }
        
        println!("[NOSTR] Unique profiles found: {}", latest_profiles.len());

        // Parse events into Profile structs
        let profiles: Vec<Profile> = latest_profiles.values().map(|event| {
            let nostr_event = NostrEvent {
                id: event.id.to_hex(),
                pubkey: event.pubkey.to_bech32().unwrap_or_default(),
                created_at: event.created_at.as_u64() as i64,
                kind: event.kind.as_u16(),
                tags: event.tags.iter().map(|t| t.clone().to_vec()).collect(),
                content: event.content.clone(),
                sig: event.sig.to_string(),
            };
            parse_profile_from_event(&nostr_event)
        }).filter_map(Result::ok).collect();
        
        println!("[NOSTR] Successfully parsed {} profiles", profiles.len());
        Ok(profiles)
    } else {
        println!("[NOSTR] No contact list event found for user");
        Ok(vec![]) // No contact list found
    }
}

pub async fn fetch_image_as_data_url(url: &str) -> Result<String> {
    let response = reqwest::get(url).await?;

    // Try to guess mime type from header, fallback to guessing from URL path
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| mime_guess::from_path(url).first_or_octet_stream().to_string());

    let bytes = response.bytes().await?;
    let encoded = general_purpose::STANDARD.encode(&bytes);

    Ok(format!("data:{};base64,{}", content_type, encoded))
}

pub async fn fetch_multiple_images_as_data_urls(urls: &[String]) -> Result<std::collections::HashMap<String, String>> {
    println!("[NOSTR] Fetching {} images concurrently", urls.len());
    
    let mut results = std::collections::HashMap::new();
    let mut futures = Vec::new();
    
    // Create futures for all image fetches
    for url in urls {
        let url_clone = url.clone();
        let future = async move {
            match fetch_image_as_data_url(&url_clone).await {
                Ok(data_url) => Some((url_clone, data_url)),
                Err(e) => {
                    println!("[NOSTR] Failed to fetch image {}: {}", url_clone, e);
                    None
                }
            }
        };
        futures.push(future);
    }
    
    // Execute all futures concurrently
    let results_vec = futures::future::join_all(futures).await;
    
    // Collect successful results
    for result in results_vec {
        if let Some((url, data_url)) = result {
            results.insert(url, data_url);
        }
    }
    
    println!("[NOSTR] Successfully fetched {} out of {} images", results.len(), urls.len());
    Ok(results)
}

pub async fn fetch_conversations(
    private_key: &str,
    relays: &[String],
) -> Result<Vec<Conversation>> {
    println!("[NOSTR] fetch_conversations called");
    
    let secret_key = SecretKey::from_bech32(private_key)?;
    let keys = Keys::new(secret_key);
    let client = Client::new(keys.clone());
    
    // Add relays
    for relay in relays {
        client.add_relay(relay.clone()).await?;
    }
    
    // If no relays provided, use defaults
    if relays.is_empty() {
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    }
    
    client.connect().await;
    
    // Create filter for encrypted direct messages - fetch both sent and received
    let sent_filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(keys.public_key());
    
    let received_filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(keys.public_key());
    
    // Get events for both sent and received
    let sent_events = client.fetch_events(sent_filter, Duration::from_secs(10)).await?;
    let received_events = client.fetch_events(received_filter, Duration::from_secs(10)).await?;
    
    // Combine and deduplicate events
    let mut all_events = sent_events;
    all_events.extend(received_events);
    
    // Remove duplicates based on event ID
    let mut unique_events = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    
    for event in all_events {
        if seen_ids.insert(event.id) {
            unique_events.push(event);
        }
    }
    
    // Group messages by conversation (contact)
    let mut conversations: std::collections::HashMap<String, Vec<ConversationMessage>> = std::collections::HashMap::new();
    
    for event in unique_events {
        // Extract the other party's pubkey from the tags
        let other_party = if event.pubkey == keys.public_key() {
            // This is a sent message, find the recipient in tags
            let raw_pubkey = event.tags.iter()
                .find(|tag| tag.kind().as_str() == "p")
                .and_then(|tag| tag.content())
                .unwrap_or_default();
            // Log for debugging self-DMs and recipient extraction
            println!("[NOSTR DEBUG] Event {}: sender={}, raw recipient tag='{}'", event.id.to_hex(), event.pubkey.to_bech32().unwrap_or_default(), raw_pubkey);
            // Convert hex to bech32 if needed
            if raw_pubkey.len() == 64 && raw_pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
                match PublicKey::from_hex(raw_pubkey) {
                    Ok(pk) => pk.to_bech32().unwrap_or_default(),
                    Err(_) => raw_pubkey.to_string(),
                }
            } else {
                raw_pubkey.to_string()
            }
        } else {
            // This is a received message, the sender is the event pubkey
            event.pubkey.to_bech32().unwrap_or_default()
        };
        
        if !other_party.is_empty() {
            // Try to decrypt the message
            let sender_pubkey = if event.pubkey == keys.public_key() {
                // This is a message we sent, so we need to decrypt as if we're the sender
                // But for received messages, the sender is the other party
                &other_party
            } else {
                // This is a message we received, so the sender is the event pubkey
                &event.pubkey.to_bech32().unwrap_or_default()
            };
            
            println!("[NOSTR] Attempting to decrypt message from {} to {}", 
                sender_pubkey, keys.public_key().to_bech32().unwrap_or_default());
            
            let decrypted_content = match decrypt_dm_content(private_key, sender_pubkey, &event.content) {
                Ok(content) => {
                    println!("[NOSTR] Successfully decrypted message: {}", content);
                    content
                },
                Err(e) => {
                    println!("[NOSTR] Failed to decrypt message: {}", e);
                    "[Encrypted message]".to_string()
                },
            };
            
            let message = ConversationMessage {
                id: event.id.to_hex(),
                sender_pubkey: event.pubkey.to_bech32().unwrap_or_default(),
                receiver_pubkey: other_party.clone(),
                content: decrypted_content,
                timestamp: event.created_at.as_u64() as i64,
                is_sent: event.pubkey == keys.public_key(),
            };
            
            conversations.entry(other_party)
                .or_insert_with(Vec::new)
                .push(message);
        }
    }
    
    // Convert to conversation list
    let mut conversation_list = Vec::new();
    
    for (contact_pubkey, messages) in conversations {
        if !messages.is_empty() {
            // Sort messages by timestamp (newest first)
            let mut sorted_messages = messages;
            sorted_messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            
            let last_message = &sorted_messages[0];
            
            // For now, use a simple name from the pubkey
            // In a full implementation, you'd want to fetch the profile
            let contact_name = Some(format!("Contact {}", &contact_pubkey[..8.min(contact_pubkey.len())]));
            
            conversation_list.push(Conversation {
                contact_pubkey,
                contact_name,
                last_message: last_message.content.clone(),
                last_timestamp: last_message.timestamp,
                message_count: sorted_messages.len(),
                messages: sorted_messages,
            });
        }
    }
    
    // Sort conversations by last message timestamp (newest first)
    conversation_list.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
    
    println!("[NOSTR] Found {} conversations", conversation_list.len());
    Ok(conversation_list)
}

pub async fn fetch_conversation_messages(
    private_key: &str,
    contact_pubkey: &str,
    relays: &[String],
) -> Result<Vec<ConversationMessage>> {
    println!("[NOSTR] fetch_conversation_messages called for contact: {}", contact_pubkey);
    
    let secret_key = SecretKey::from_bech32(private_key)?;
    let keys = Keys::new(secret_key);
    let client = Client::new(keys.clone());
    
    // Add relays
    for relay in relays {
        client.add_relay(relay.clone()).await?;
    }
    
    // If no relays provided, use defaults
    if relays.is_empty() {
        client.add_relay("wss://nostr-pub.wellorder.net").await?;
        client.add_relay("wss://relay.damus.io").await?;
    }
    
    client.connect().await;
    
    // Create filter for messages between these two users
    let contact_pubkey_parsed = PublicKey::from_bech32(contact_pubkey)?;
    let filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .authors([keys.public_key(), contact_pubkey_parsed])
        .pubkeys([keys.public_key(), contact_pubkey_parsed]);
    
    // Get events
    let events = client.fetch_events(filter, Duration::from_secs(10)).await?;
    
    // Convert to conversation messages
    let mut messages = Vec::new();
    
    for event in events {
        // Try to decrypt the message
        let sender_pubkey = if event.pubkey == keys.public_key() {
            // This is a message we sent, so the sender for decryption is the contact
            contact_pubkey
        } else {
            // This is a message we received, so the sender is the event pubkey
            &event.pubkey.to_bech32().unwrap_or_default()
        };
        
        println!("[NOSTR] Attempting to decrypt conversation message from {} to {}", 
            sender_pubkey, keys.public_key().to_bech32().unwrap_or_default());
        println!("[NOSTR] Encrypted content: {}", &event.content[..event.content.len().min(100)]);
        println!("[NOSTR] Content length: {}, starts with: {}", event.content.len(), &event.content[..event.content.len().min(10)]);
        
        let decrypted_content = match decrypt_dm_content(private_key, sender_pubkey, &event.content) {
            Ok(content) => {
                println!("[NOSTR] Successfully decrypted conversation message: {}", content);
                content
            },
            Err(e) => {
                println!("[NOSTR] Failed to decrypt conversation message: {}", e);
                // Try alternative decryption methods or show raw content
                if event.content.starts_with("AES") || event.content.contains("|") {
                    // This might be a different encryption format
                    format!("[Alternative encrypted format: {}]", &event.content[..event.content.len().min(50)])
                } else {
                    "[Encrypted message]".to_string()
                }
            },
        };
        
        let message = ConversationMessage {
            id: event.id.to_hex(),
            sender_pubkey: event.pubkey.to_bech32().unwrap_or_default(),
            receiver_pubkey: contact_pubkey.to_string(),
            content: decrypted_content,
            timestamp: event.created_at.as_u64() as i64,
            is_sent: event.pubkey == keys.public_key(),
        };
        
        messages.push(message);
    }
    
    // Sort by timestamp (oldest first for conversation view)
    messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    
    println!("[NOSTR] Found {} messages in conversation", messages.len());
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_nip05_validation_format() {
        // This is a placeholder test since we're not actually making HTTP requests
        assert!(validate_nip05("test", "user@domain.com").await.is_ok());
    }
} 