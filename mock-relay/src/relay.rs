use crate::server::RelayServer;
use crate::store::EventStore;
use anyhow::Result;
use log::{info, warn};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Manages multiple relay instances
pub struct RelayManager {
    relays: Vec<RelayInstance>,
}

struct RelayInstance {
    server: RelayServer,
    store: Arc<EventStore>,
    handle: Option<JoinHandle<Result<()>>>,
}

impl RelayManager {
    pub fn new() -> Self {
        Self {
            relays: Vec::new(),
        }
    }

    /// Start a new relay on the given port
    /// Binds to 0.0.0.0 to allow connections from other devices (e.g., Android emulator/physical device)
    pub async fn start_relay(&mut self, port: u16) -> Result<SocketAddr> {
        // Bind to 0.0.0.0 to allow connections from other devices on the network
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
        let store = Arc::new(EventStore::new());
        let server = RelayServer::new(addr, store.clone());

        let server_addr = server.addr();
        let handle = tokio::spawn(async move {
            server.start().await
        });

        let instance = RelayInstance {
            server: RelayServer::new(server_addr, store.clone()),
            store,
            handle: Some(handle),
        };

        self.relays.push(instance);

        info!("Started relay on ws://{}", server_addr);
        Ok(server_addr)
    }

    /// Start multiple relays starting from a base port
    pub async fn start_relays(&mut self, count: usize, start_port: u16) -> Result<Vec<SocketAddr>> {
        let mut addrs = Vec::new();

        for i in 0..count {
            let port = start_port + i as u16;
            match self.start_relay(port).await {
                Ok(addr) => {
                    addrs.push(addr);
                    // Small delay between starting relays
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
                Err(e) => {
                    warn!("Failed to start relay on port {}: {}", port, e);
                }
            }
        }

        Ok(addrs)
    }

    /// Get the store for a specific relay by index
    pub fn get_store(&self, index: usize) -> Option<Arc<EventStore>> {
        self.relays.get(index).map(|instance| instance.store.clone())
    }

    /// Stop a relay by index
    pub async fn stop_relay(&mut self, index: usize) -> Result<()> {
        if let Some(instance) = self.relays.get_mut(index) {
            if let Some(handle) = instance.handle.take() {
                handle.abort();
                info!("Stopped relay {}", index);
            }
        }
        Ok(())
    }

    /// Stop all relays
    pub async fn stop_all(&mut self) {
        for (i, instance) in self.relays.iter_mut().enumerate() {
            if let Some(handle) = instance.handle.take() {
                handle.abort();
                info!("Stopped relay {}", i);
            }
        }
    }

    /// Get the number of active relays
    pub fn count(&self) -> usize {
        self.relays.len()
    }

    /// Get addresses of all relays
    pub fn addresses(&self) -> Vec<SocketAddr> {
        self.relays.iter().map(|r| r.server.addr()).collect()
    }
}

impl Default for RelayManager {
    fn default() -> Self {
        Self::new()
    }
}

// Note: RelayServer doesn't need to be Clone - we'll pass Arc instead

/// Start a single relay instance (library function for testing)
pub async fn start_relay(port: u16) -> Result<(SocketAddr, JoinHandle<Result<()>>)> {
    // Bind to 0.0.0.0 to allow connections from other devices (e.g., Android emulator/physical device)
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
    let store = Arc::new(EventStore::new());
    let server = RelayServer::new(addr, store);

    let server_addr = server.addr();
    let handle = tokio::spawn(async move {
        server.start().await
    });

    Ok((server_addr, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_relay_manager() {
        let mut manager = RelayManager::new();
        assert_eq!(manager.count(), 0);

        // Note: This test would require binding to a port, which might fail
        // In a real test, you'd use port 0 to get an available port
    }
}
