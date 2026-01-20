use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for the mock relay server
#[derive(Debug, Clone, Parser)]
#[command(name = "mock-relay")]
#[command(about = "Mock Nostr relay server for testing")]
pub struct Config {
    /// Port to listen on (default: 8080)
    #[arg(short, long, default_value = "8080")]
    pub port: u16,

    /// Number of relays to start (default: 1)
    #[arg(short, long, default_value = "1")]
    pub relays: usize,

    /// Starting port for multiple relays
    #[arg(long, default_value = "8080")]
    pub start_port: u16,

    /// Path to JSON file with events to preload
    #[arg(long)]
    pub preload_events: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Log file path (default: relay.log)
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    /// Generate and preload fake events (count per kind)
    #[arg(long)]
    pub generate_fake_events: Option<usize>,

    /// Output file for generated events (JSON format)
    #[arg(long)]
    pub output_events: Option<PathBuf>,

    /// Seed for random number generator (for deterministic event generation)
    #[arg(long, default_value = "0")]
    pub seed: u64,
}

impl Config {
    /// Load configuration from file (future enhancement)
    pub fn from_file(_path: PathBuf) -> anyhow::Result<Self> {
        // For now, just use defaults
        // In the future, this could load from JSON/YAML
        Ok(Config::parse())
    }
}

/// Event preload configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Vec<Profile>>,
    pub events: Vec<PreloadEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relays: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadEvent {
    pub id: String,
    pub pubkey: String,
    pub created_at: i64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub pubkey: String,
    pub private_key: String, // nsec format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>, // User's display name from profile event
}

impl From<PreloadEvent> for crate::types::Event {
    fn from(event: PreloadEvent) -> Self {
        crate::types::Event {
            id: event.id,
            pubkey: event.pubkey,
            created_at: event.created_at,
            kind: event.kind,
            tags: event.tags,
            content: event.content,
            sig: event.sig,
        }
    }
}

/// Load events from a JSON file
pub fn load_preload_events(path: PathBuf) -> anyhow::Result<Vec<crate::types::Event>> {
    let content = std::fs::read_to_string(path)?;
    let config: PreloadConfig = serde_json::from_str(&content)?;
    Ok(config.events.into_iter().map(|e| e.into()).collect())
}

/// Write events to a JSON file
pub fn write_events_to_file(events: &[crate::types::Event], path: PathBuf) -> anyhow::Result<()> {
    let preload_events: Vec<PreloadEvent> = events.iter().map(|e| PreloadEvent {
        id: e.id.clone(),
        pubkey: e.pubkey.clone(),
        created_at: e.created_at,
        kind: e.kind,
        tags: e.tags.clone(),
        content: e.content.clone(),
        sig: e.sig.clone(),
    }).collect();
    
    let config = PreloadConfig {
        profiles: None,
        events: preload_events,
        relays: None,
    };
    
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Extract name from profile event content (JSON string)
fn extract_name_from_profile_content(content: &str) -> Option<String> {
    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(content) {
        // Try display_name first, then name
        if let Some(display_name) = metadata.get("display_name").and_then(|v| v.as_str()) {
            return Some(display_name.to_string());
        }
        if let Some(name) = metadata.get("name").and_then(|v| v.as_str()) {
            return Some(name.to_string());
        }
    }
    None
}

/// Write events with private keys to a JSON file
pub fn write_events_with_keys_to_file(
    events_with_keys: &[crate::test_utils::EventWithKey],
    path: PathBuf,
    relay_urls: Option<Vec<String>>,
) -> anyhow::Result<()> {
    // Extract events (without private keys)
    let preload_events: Vec<PreloadEvent> = events_with_keys.iter().map(|ewk| PreloadEvent {
        id: ewk.event.id.clone(),
        pubkey: ewk.event.pubkey.clone(),
        created_at: ewk.event.created_at,
        kind: ewk.event.kind,
        tags: ewk.event.tags.clone(),
        content: ewk.event.content.clone(),
        sig: ewk.event.sig.clone(),
    }).collect();
    
    // Extract unique profiles (pubkey -> (private_key, name) mapping)
    // Only include kind 0 events (profile events) in the profiles section
    // HashMap.insert() ensures deduplication by pubkey - if the same pubkey appears multiple times,
    // only the last one is kept. Different users can have the same name but different pubkeys.
    let mut profiles_map: HashMap<String, (String, Option<String>)> = HashMap::new();
    for ewk in events_with_keys {
        // Only process kind 0 events (profiles) for the profiles section
        if ewk.event.kind == 0 {
            // Try to extract name from profile content
            let name = extract_name_from_profile_content(&ewk.event.content);
            // Insert will overwrite if pubkey already exists, ensuring unique profiles by pubkey
            profiles_map.insert(ewk.event.pubkey.clone(), (ewk.private_key.clone(), name));
        }
    }
    
    let mut profiles: Vec<Profile> = profiles_map.into_iter()
        .map(|(pubkey, (private_key, name))| Profile { pubkey, private_key, name })
        .collect();
    
    // Sort profiles alphabetically by name (None values go last)
    profiles.sort_by(|a, b| {
        match (&a.name, &b.name) {
            (Some(name_a), Some(name_b)) => name_a.cmp(name_b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.pubkey.cmp(&b.pubkey), // If both have no name, sort by pubkey
        }
    });
    
    let config = PreloadConfig {
        profiles: Some(profiles),
        events: preload_events,
        relays: relay_urls,
    };
    
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        // Test that config can be created with defaults
        let _config = Config {
            port: 8080,
            relays: 1,
            start_port: 8080,
            preload_events: None,
            log_level: "info".to_string(),
            log_file: None,
            generate_fake_events: None,
            output_events: None,
            seed: 0,
        };
    }
}
