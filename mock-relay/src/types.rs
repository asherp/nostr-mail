use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use hex;

/// Normalize a pubkey string (hex or npub) to npub format
/// Returns the npub format if conversion succeeds, otherwise returns the original string
pub fn normalize_pubkey(pubkey: &str) -> String {
    use nostr_sdk::prelude::*;
    
    // If already npub format, return as-is
    if pubkey.starts_with("npub") {
        return pubkey.to_string();
    }
    
    // Try to parse as bech32 npub (in case it's malformed)
    if let Ok(pk) = PublicKey::from_bech32(pubkey) {
        if let Ok(npub) = pk.to_bech32() {
            return npub;
        }
    }
    
    // Try to parse as hex and convert to npub
    if let Ok(bytes) = hex::decode(pubkey) {
        if bytes.len() == 32 {
            if let Ok(pk) = PublicKey::from_slice(&bytes) {
                if let Ok(npub) = pk.to_bech32() {
                    return npub;
                }
            }
        }
    }
    
    // If all conversions fail, return original
    pubkey.to_string()
}

/// Nostr event structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub pubkey: String,
    pub created_at: i64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

/// Filter for querying events
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Filter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<u16>>,
    #[serde(rename = "#p", skip_serializing_if = "Option::is_none")]
    pub pubkey_refs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Subscription with ID and filters
#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: String,
    pub filters: Vec<Filter>,
}

/// Nostr protocol message types
#[derive(Debug, Clone)]
pub enum NostrMessage {
    Req {
        subscription_id: String,
        filters: Vec<Filter>,
    },
    Event {
        event: Event,
    },
    Close {
        subscription_id: String,
    },
    Auth {
        event: Event,
    },
}

/// Response message types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ResponseMessage {
    #[serde(rename = "EVENT")]
    Event {
        #[serde(rename = "0")]
        subscription_id: String,
        #[serde(rename = "1")]
        event: Event,
    },
    #[serde(rename = "EOSE")]
    Eose {
        #[serde(rename = "0")]
        subscription_id: String,
    },
    #[serde(rename = "OK")]
    Ok {
        #[serde(rename = "0")]
        event_id: String,
        #[serde(rename = "1")]
        accepted: bool,
        #[serde(rename = "2")]
        message: String,
    },
    #[serde(rename = "NOTICE")]
    Notice {
        #[serde(rename = "0")]
        message: String,
    },
    #[serde(rename = "AUTH")]
    Auth {
        #[serde(rename = "0")]
        challenge: String,
    },
}

impl ResponseMessage {
    /// Convert to JSON array format for nostr protocol
    pub fn to_json_array(&self) -> serde_json::Value {
        match self {
            ResponseMessage::Event {
                subscription_id,
                event,
            } => {
                serde_json::json!(["EVENT", subscription_id, event])
            }
            ResponseMessage::Eose { subscription_id } => {
                serde_json::json!(["EOSE", subscription_id])
            }
            ResponseMessage::Ok {
                event_id,
                accepted,
                message,
            } => {
                serde_json::json!(["OK", event_id, accepted, message])
            }
            ResponseMessage::Notice { message } => {
                serde_json::json!(["NOTICE", message])
            }
            ResponseMessage::Auth { challenge } => {
                serde_json::json!(["AUTH", challenge])
            }
        }
    }
}

impl Filter {
    /// Check if an event matches this filter
    pub fn matches(&self, event: &Event) -> bool {
        // Check IDs
        if let Some(ids) = &self.ids {
            if !ids.contains(&event.id) {
                return false;
            }
        }

        // Check authors (normalize both event pubkey and filter authors for comparison)
        if let Some(authors) = &self.authors {
            let normalized_event_pubkey = normalize_pubkey(&event.pubkey);
            let normalized_authors: Vec<String> = authors.iter().map(|a| normalize_pubkey(a)).collect();
            if !normalized_authors.contains(&normalized_event_pubkey) {
                return false;
            }
        }

        // Check kinds
        if let Some(kinds) = &self.kinds {
            if !kinds.contains(&event.kind) {
                return false;
            }
        }

        // Check pubkey references (p tags) - normalize both event p-tags and filter pubkey_refs
        if let Some(pubkey_refs) = &self.pubkey_refs {
            let event_p_tags: HashSet<String> = event
                .tags
                .iter()
                .filter(|tag| tag.len() >= 2 && tag[0] == "p")
                .filter_map(|tag| tag.get(1).map(|pk| normalize_pubkey(pk)))
                .collect();

            let normalized_pubkey_refs: Vec<String> = pubkey_refs.iter().map(|pk| normalize_pubkey(pk)).collect();
            let has_match = normalized_pubkey_refs.iter().any(|pk| event_p_tags.contains(pk));
            if !has_match {
                return false;
            }
        }

        // Check since (created_at >= since)
        if let Some(since) = self.since {
            if event.created_at < since {
                return false;
            }
        }

        // Check until (created_at <= until)
        if let Some(until) = self.until {
            if event.created_at > until {
                return false;
            }
        }

        true
    }
}

impl TryFrom<serde_json::Value> for NostrMessage {
    type Error = anyhow::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        let arr = value
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Message must be an array"))?;

        if arr.is_empty() {
            return Err(anyhow::anyhow!("Message array cannot be empty"));
        }

        let msg_type = arr[0]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("First element must be a string"))?;

        match msg_type {
            "REQ" => {
                if arr.len() < 3 {
                    return Err(anyhow::anyhow!("REQ message must have at least 3 elements"));
                }
                let subscription_id = arr[1]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Subscription ID must be a string"))?
                    .to_string();

                let mut filters = Vec::new();
                for filter_val in arr.iter().skip(2) {
                    let filter: Filter = serde_json::from_value(filter_val.clone())?;
                    filters.push(filter);
                }

                Ok(NostrMessage::Req {
                    subscription_id,
                    filters,
                })
            }
            "EVENT" => {
                if arr.len() < 2 {
                    return Err(anyhow::anyhow!("EVENT message must have at least 2 elements"));
                }
                let event: Event = serde_json::from_value(arr[1].clone())?;
                Ok(NostrMessage::Event { event })
            }
            "CLOSE" => {
                if arr.len() < 2 {
                    return Err(anyhow::anyhow!("CLOSE message must have at least 2 elements"));
                }
                let subscription_id = arr[1]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Subscription ID must be a string"))?
                    .to_string();
                Ok(NostrMessage::Close { subscription_id })
            }
            "AUTH" => {
                if arr.len() < 2 {
                    return Err(anyhow::anyhow!("AUTH message must have at least 2 elements"));
                }
                let event: Event = serde_json::from_value(arr[1].clone())?;
                Ok(NostrMessage::Auth { event })
            }
            _ => Err(anyhow::anyhow!("Unknown message type: {}", msg_type)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_matches() {
        let event = Event {
            id: "test_id".to_string(),
            pubkey: "test_pubkey".to_string(),
            created_at: 1000,
            kind: 4,
            tags: vec![vec!["p".to_string(), "recipient_pubkey".to_string()]],
            content: "encrypted_content".to_string(),
            sig: "test_sig".to_string(),
        };

        // Test kind filter
        let filter = Filter {
            kinds: Some(vec![4]),
            ..Default::default()
        };
        assert!(filter.matches(&event));

        let filter = Filter {
            kinds: Some(vec![0, 3]),
            ..Default::default()
        };
        assert!(!filter.matches(&event));

        // Test author filter
        let filter = Filter {
            authors: Some(vec!["test_pubkey".to_string()]),
            ..Default::default()
        };
        assert!(filter.matches(&event));

        // Test pubkey ref filter
        let filter = Filter {
            pubkey_refs: Some(vec!["recipient_pubkey".to_string()]),
            ..Default::default()
        };
        assert!(filter.matches(&event));

        // Test since filter
        let filter = Filter {
            since: Some(500),
            ..Default::default()
        };
        assert!(filter.matches(&event));

        let filter = Filter {
            since: Some(1500),
            ..Default::default()
        };
        assert!(!filter.matches(&event));
    }
}
