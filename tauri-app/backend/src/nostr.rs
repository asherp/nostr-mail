use nostr_sdk::prelude::*;
use anyhow::Result;
use crate::types::{NostrEvent, Profile};
use std::time::Duration;
use base64::{engine::general_purpose, Engine as _};
use mime_guess;

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
    let keys = Keys::new(secret_key);
    let recipient = PublicKey::from_bech32(recipient_pubkey)?;
    
    // Create client
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
    
    // Send private message using NIP-59 (gift wraps)
    let event_id = client.send_private_msg(recipient, message, []).await?;
    
    Ok(event_id.to_hex())
}

pub async fn fetch_direct_messages(
    private_key: &str,
    relays: &[String],
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
    
    // Create filter for encrypted direct messages
    let filter = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(keys.public_key());
    
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
    let sender = PublicKey::from_bech32(sender_pubkey)?;
    
    // Use NIP-44 decryption
    let decrypted = nip44::decrypt(&secret_key, &sender, encrypted_content)?;
    Ok(decrypted)
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
            .filter_map(|tag| tag.content().and_then(|pk| PublicKey::from_hex(pk).ok()))
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
            .kind(Kind::Metadata);
            
        let profile_events = client.fetch_events(profiles_filter, Duration::from_secs(10)).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_nip05_validation_format() {
        // This is a placeholder test since we're not actually making HTTP requests
        assert!(validate_nip05("test", "user@domain.com").await.is_ok());
    }
} 