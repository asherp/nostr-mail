use crate::types::{Event, Filter, normalize_pubkey};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory event store for a relay
#[derive(Debug, Clone)]
pub struct EventStore {
    events: Arc<RwLock<HashMap<String, Event>>>,
    // Index by kind for faster filtering
    events_by_kind: Arc<RwLock<HashMap<u16, Vec<String>>>>,
    // Index by author for faster filtering
    events_by_author: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl EventStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(HashMap::new())),
            events_by_kind: Arc::new(RwLock::new(HashMap::new())),
            events_by_author: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add an event to the store
    pub async fn add_event(&self, mut event: Event) {
        let event_id = event.id.clone();
        let kind = event.kind;
        
        // Normalize pubkey to npub format before storing
        let normalized_pubkey = normalize_pubkey(&event.pubkey);
        event.pubkey = normalized_pubkey.clone();

        // Add to main store
        let mut events = self.events.write().await;
        events.insert(event_id.clone(), event.clone());

        // Update kind index
        let mut by_kind = self.events_by_kind.write().await;
        by_kind.entry(kind).or_insert_with(Vec::new).push(event_id.clone());

        // Update author index with normalized pubkey
        let mut by_author = self.events_by_author.write().await;
        by_author
            .entry(normalized_pubkey)
            .or_insert_with(Vec::new)
            .push(event_id);
    }

    /// Query events matching filters
    pub async fn query(&self, filters: &[Filter]) -> Vec<Event> {
        let events = self.events.read().await;
        let mut matching_events = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // If no filters, return all events
        if filters.is_empty() {
            return events.values().cloned().collect();
        }

        // For each filter, find matching events
        for filter in filters {
            let candidates = self.get_candidate_ids(filter).await;
            
            for event_id in candidates {
                if seen_ids.contains(&event_id) {
                    continue;
                }

                if let Some(event) = events.get(&event_id) {
                    if filter.matches(event) {
                        matching_events.push(event.clone());
                        seen_ids.insert(event_id);
                    }
                }
            }
        }

        // Sort by created_at (newest first)
        matching_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply limit if specified (use the first filter's limit)
        if let Some(filter) = filters.first() {
            if let Some(limit) = filter.limit {
                matching_events.truncate(limit);
            }
        }

        matching_events
    }

    /// Get candidate event IDs based on filter optimizations
    async fn get_candidate_ids(&self, filter: &Filter) -> Vec<String> {
        let events = self.events.read().await;
        let mut candidates = std::collections::HashSet::new();

        // If filter specifies kinds, use kind index
        if let Some(kinds) = &filter.kinds {
            let by_kind = self.events_by_kind.read().await;
            for kind in kinds {
                if let Some(event_ids) = by_kind.get(kind) {
                    for event_id in event_ids {
                        candidates.insert(event_id.clone());
                    }
                }
            }
        }

        // If filter specifies authors, normalize them and use author index
        if let Some(authors) = &filter.authors {
            let by_author = self.events_by_author.read().await;
            for author in authors {
                // Normalize author pubkey to npub format before lookup
                let normalized_author = normalize_pubkey(author);
                if let Some(event_ids) = by_author.get(&normalized_author) {
                    for event_id in event_ids {
                        candidates.insert(event_id.clone());
                    }
                }
            }
        }

        // If we have specific IDs, use those
        if let Some(ids) = &filter.ids {
            for id in ids {
                if events.contains_key(id) {
                    candidates.insert(id.clone());
                }
            }
        }

        // If no specific filters, consider all events
        if filter.kinds.is_none() && filter.authors.is_none() && filter.ids.is_none() {
            candidates.extend(events.keys().cloned());
        }

        candidates.into_iter().collect()
    }

    /// Get an event by ID
    pub async fn get_event(&self, event_id: &str) -> Option<Event> {
        let events = self.events.read().await;
        events.get(event_id).cloned()
    }

    /// Get all events (for debugging/testing)
    pub async fn get_all_events(&self) -> Vec<Event> {
        let events = self.events.read().await;
        events.values().cloned().collect()
    }

    /// Clear all events (for testing)
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
        let mut by_kind = self.events_by_kind.write().await;
        by_kind.clear();
        let mut by_author = self.events_by_author.write().await;
        by_author.clear();
    }

    /// Get count of events
    pub async fn count(&self) -> usize {
        let events = self.events.read().await;
        events.len()
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_query() {
        let store = EventStore::new();

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

        let results = store.query(&[filter]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "test_id");
    }

    #[tokio::test]
    async fn test_filter_by_author() {
        let store = EventStore::new();

        let event1 = Event {
            id: "id1".to_string(),
            pubkey: "pubkey1".to_string(),
            created_at: 1000,
            kind: 4,
            tags: vec![],
            content: "test1".to_string(),
            sig: "sig1".to_string(),
        };

        let event2 = Event {
            id: "id2".to_string(),
            pubkey: "pubkey2".to_string(),
            created_at: 1000,
            kind: 4,
            tags: vec![],
            content: "test2".to_string(),
            sig: "sig2".to_string(),
        };

        store.add_event(event1).await;
        store.add_event(event2).await;

        let filter = Filter {
            authors: Some(vec!["pubkey1".to_string()]),
            ..Default::default()
        };

        let results = store.query(&[filter]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].pubkey, "pubkey1");
    }
}
