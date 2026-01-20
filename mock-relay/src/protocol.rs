use crate::store::EventStore;
use crate::types::{Event, Filter, NostrMessage, ResponseMessage, Subscription};
use hex;
use log::info;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Convert a pubkey string (hex or npub) to npub format for logging
fn pubkey_to_npub(pubkey: &str) -> String {
    use nostr_sdk::prelude::*;
    
    // If already npub format, return as-is
    if pubkey.starts_with("npub") {
        return pubkey.to_string();
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
    
    // If conversion fails, return original
    pubkey.to_string()
}

/// Convert filter pubkeys to npub format for logging
fn filter_to_npub_format(filter: &Filter) -> serde_json::Value {
    let mut json = serde_json::to_value(filter).unwrap_or(serde_json::json!({}));
    
    // Convert authors
    if let Some(authors) = json.get_mut("authors").and_then(|a| a.as_array_mut()) {
        for author in authors {
            if let Some(author_str) = author.as_str() {
                *author = serde_json::Value::String(pubkey_to_npub(author_str));
            }
        }
    }
    
    // Convert #p (pubkey_refs)
    if let Some(pubkey_refs) = json.get_mut("#p").and_then(|p| p.as_array_mut()) {
        for pubkey_ref in pubkey_refs {
            if let Some(pubkey_str) = pubkey_ref.as_str() {
                *pubkey_ref = serde_json::Value::String(pubkey_to_npub(pubkey_str));
            }
        }
    }
    
    json
}

/// Handler for nostr protocol messages
pub struct ProtocolHandler {
    store: Arc<EventStore>,
    subscriptions: Arc<RwLock<HashMap<String, Subscription>>>,
}

impl ProtocolHandler {
    pub fn new(store: Arc<EventStore>) -> Self {
        Self {
            store,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle an incoming nostr message and return response messages
    pub async fn handle_message(
        &self,
        message: NostrMessage,
        client_addr: SocketAddr,
    ) -> Vec<ResponseMessage> {
        let mut responses = Vec::new();

        match &message {
            NostrMessage::Req {
                subscription_id,
                filters,
            } => {
                // Convert filters to npub format for logging
                let filters_npub: Vec<serde_json::Value> = filters.iter()
                    .map(|f| filter_to_npub_format(f))
                    .collect();
                info!("[REQ] {} subscription_id={}, filters={}", 
                    client_addr, subscription_id, serde_json::to_string(&filters_npub).unwrap_or_default());
                responses.extend(self.handle_req(subscription_id.clone(), filters.clone()).await);
            }
            NostrMessage::Event { event } => {
                info!("[EVENT] {} event_id={}, pubkey={}, kind={}", 
                    client_addr, event.id, pubkey_to_npub(&event.pubkey), event.kind);
                responses.extend(self.handle_event(event.clone()).await);
            }
            NostrMessage::Close { subscription_id } => {
                info!("[CLOSE] {} subscription_id={}", client_addr, subscription_id);
                self.handle_close(subscription_id.clone()).await;
            }
            NostrMessage::Auth { event } => {
                info!("[AUTH] {} event_id={}, pubkey={}", 
                    client_addr, event.id, pubkey_to_npub(&event.pubkey));
                responses.extend(self.handle_auth(event.clone()).await);
            }
        }

        responses
    }

    /// Handle REQ message - create subscription and send matching events
    async fn handle_req(
        &self,
        subscription_id: String,
        filters: Vec<Filter>,
    ) -> Vec<ResponseMessage> {
        let mut responses = Vec::new();

        // Store subscription
        let subscription = Subscription {
            id: subscription_id.clone(),
            filters: filters.clone(),
        };
        let mut subs = self.subscriptions.write().await;
        subs.insert(subscription_id.clone(), subscription);

        // Query matching events
        let events = self.store.query(&filters).await;
        info!("[REQ] subscription_id={} matched {} events", subscription_id, events.len());

        // Send matching events
        for event in events {
            responses.push(ResponseMessage::Event {
                subscription_id: subscription_id.clone(),
                event,
            });
        }

        // Send EOSE
        responses.push(ResponseMessage::Eose {
            subscription_id: subscription_id.clone(),
        });
        info!("[REQ] subscription_id={} sent EOSE", subscription_id);

        responses
    }

    /// Handle EVENT message - store event and broadcast to matching subscriptions
    async fn handle_event(&self, event: Event) -> Vec<ResponseMessage> {
        let mut responses = Vec::new();

        // Basic validation
        if !self.validate_event(&event) {
            info!("[EVENT] event_id={} REJECTED: validation failed", event.id);
            responses.push(ResponseMessage::Ok {
                event_id: event.id.clone(),
                accepted: false,
                message: "invalid: event validation failed".to_string(),
            });
            return responses;
        }

        // Store event
        self.store.add_event(event.clone()).await;
        info!("[EVENT] event_id={} ACCEPTED and stored", event.id);

        // Broadcast to matching subscriptions
        let subs = self.subscriptions.read().await;
        for subscription in subs.values() {
            let matching = subscription
                .filters
                .iter()
                .any(|filter| filter.matches(&event));

            if matching {
                responses.push(ResponseMessage::Event {
                    subscription_id: subscription.id.clone(),
                    event: event.clone(),
                });
            }
        }

        // Send OK response
        responses.push(ResponseMessage::Ok {
            event_id: event.id,
            accepted: true,
            message: "".to_string(),
        });

        responses
    }

    /// Handle CLOSE message - remove subscription
    async fn handle_close(&self, subscription_id: String) {
        let mut subs = self.subscriptions.write().await;
        subs.remove(&subscription_id);
    }

    /// Handle AUTH message - respond with OK (simplified for testing)
    async fn handle_auth(&self, event: Event) -> Vec<ResponseMessage> {
        let mut responses = Vec::new();

        // For testing, accept all AUTH events
        // In a real relay, this would verify the challenge and signature
        responses.push(ResponseMessage::Ok {
            event_id: event.id,
            accepted: true,
            message: "".to_string(),
        });

        responses
    }

    /// Basic event validation with signature verification
    fn validate_event(&self, event: &Event) -> bool {
        // Check required fields
        if event.id.is_empty() {
            return false;
        }
        if event.pubkey.is_empty() {
            return false;
        }
        if event.sig.is_empty() {
            return false;
        }

        // Check that created_at is reasonable (not too far in future)
        let now = chrono::Utc::now().timestamp();
        if event.created_at > now + 3600 {
            // More than 1 hour in the future
            return false;
        }

        // Verify signature using nostr-sdk
        if let Err(e) = self.verify_signature(event) {
            log::warn!("Signature verification failed: {}", e);
            return false;
        }

        true
    }

    /// Verify event signature using nostr-sdk
    fn verify_signature(&self, event: &Event) -> anyhow::Result<()> {
        use nostr_sdk::prelude::*;
        
        // Convert our Event to JSON and parse as nostr-sdk Event
        // This ensures we're using the same serialization format
        let event_json = serde_json::json!({
            "id": event.id,
            "pubkey": event.pubkey,
            "created_at": event.created_at,
            "kind": event.kind,
            "tags": event.tags,
            "content": event.content,
            "sig": event.sig
        });
        
        // Parse as nostr-sdk Event
        let nostr_event: nostr_sdk::Event = serde_json::from_value(event_json)
            .map_err(|e| anyhow::anyhow!("Failed to parse event: {}", e))?;
        
        // Verify the signature using nostr-sdk's verify method
        nostr_event.verify()
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))?;
        
        Ok(())
    }

    /// Get active subscription count (for debugging)
    pub async fn subscription_count(&self) -> usize {
        let subs = self.subscriptions.read().await;
        subs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EventStore;

    #[tokio::test]
    async fn test_handle_req() {
        let store = Arc::new(EventStore::new());
        let handler = ProtocolHandler::new(store.clone());

        let event = Event {
            id: "test_id".to_string(),
            pubkey: "test_pubkey".to_string(),
            created_at: 1000,
            kind: 4,
            tags: vec![],
            content: "test".to_string(),
            sig: "test_sig".to_string(),
        };

        store.add_event(event).await;

        let filter = Filter {
            kinds: Some(vec![4]),
            ..Default::default()
        };

        let message = NostrMessage::Req {
            subscription_id: "sub1".to_string(),
            filters: vec![filter],
        };

        let responses = handler.handle_message(message).await;
        assert_eq!(responses.len(), 2); // EVENT + EOSE
    }

    #[tokio::test]
    async fn test_handle_event() {
        let store = Arc::new(EventStore::new());
        let handler = ProtocolHandler::new(store.clone());

        let event = Event {
            id: "new_event".to_string(),
            pubkey: "test_pubkey".to_string(),
            created_at: 1000,
            kind: 4,
            tags: vec![],
            content: "test".to_string(),
            sig: "test_sig".to_string(),
        };

        let message = NostrMessage::Event { event };
        let responses = handler.handle_message(message).await;

        // Should get OK response
        assert!(!responses.is_empty());
        assert_eq!(store.count().await, 1);
    }
}
