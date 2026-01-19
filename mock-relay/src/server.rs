use crate::protocol::ProtocolHandler;
use crate::store::EventStore;
use crate::types::{NostrMessage, ResponseMessage};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use hex;
use log::{debug, error, info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    accept_async,
    tungstenite::Message,
};

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

/// Convert pubkeys in JSON request to npub format for logging
fn convert_request_to_npub_format(json: &serde_json::Value) -> serde_json::Value {
    let mut converted = json.clone();
    
    // Handle REQ messages: convert pubkeys in filters
    if let Some(arr) = converted.as_array_mut() {
        if arr.len() >= 3 && arr[0].as_str() == Some("REQ") {
            // Convert filters (skip subscription_id at index 1)
            for filter_val in arr.iter_mut().skip(2) {
                if let Some(filter_obj) = filter_val.as_object_mut() {
                    // Convert authors
                    if let Some(authors) = filter_obj.get_mut("authors").and_then(|a| a.as_array_mut()) {
                        for author in authors {
                            if let Some(author_str) = author.as_str() {
                                *author = serde_json::Value::String(pubkey_to_npub(author_str));
                            }
                        }
                    }
                    // Convert #p (pubkey_refs)
                    if let Some(pubkey_refs) = filter_obj.get_mut("#p").and_then(|p| p.as_array_mut()) {
                        for pubkey_ref in pubkey_refs {
                            if let Some(pubkey_str) = pubkey_ref.as_str() {
                                *pubkey_ref = serde_json::Value::String(pubkey_to_npub(pubkey_str));
                            }
                        }
                    }
                }
            }
        }
        // Handle EVENT messages: convert pubkey in event
        else if arr.len() >= 2 && arr[0].as_str() == Some("EVENT") {
            if let Some(event_obj) = arr[1].as_object_mut() {
                if let Some(pubkey) = event_obj.get_mut("pubkey") {
                    if let Some(pubkey_str) = pubkey.as_str() {
                        *pubkey = serde_json::Value::String(pubkey_to_npub(pubkey_str));
                    }
                }
            }
        }
        // Handle AUTH messages: convert pubkey in event
        else if arr.len() >= 2 && arr[0].as_str() == Some("AUTH") {
            if let Some(event_obj) = arr[1].as_object_mut() {
                if let Some(pubkey) = event_obj.get_mut("pubkey") {
                    if let Some(pubkey_str) = pubkey.as_str() {
                        *pubkey = serde_json::Value::String(pubkey_to_npub(pubkey_str));
                    }
                }
            }
        }
    }
    
    converted
}

/// WebSocket server for a nostr relay
pub struct RelayServer {
    handler: Arc<ProtocolHandler>,
    addr: SocketAddr,
}

impl RelayServer {
    pub fn new(addr: SocketAddr, store: Arc<EventStore>) -> Self {
        let handler = Arc::new(ProtocolHandler::new(store));
        Self { handler, addr }
    }

    /// Start the server and listen for connections
    pub async fn start(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.addr).await?;
        info!("Mock relay server listening on ws://{}", self.addr);

        while let Ok((stream, addr)) = listener.accept().await {
            let handler = self.handler.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, addr, handler).await {
                    error!("Error handling connection from {}: {}", addr, e);
                }
            });
        }

        Ok(())
    }

    /// Get the address the server is bound to
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Handle a WebSocket connection
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    handler: Arc<ProtocolHandler>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    info!("New WebSocket connection from {}", addr);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    while let Some(msg_result) = ws_receiver.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                // Parse JSON and convert pubkeys to npub format for logging
                let json_value: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => {
                        // Log with npub format
                        let npub_json = convert_request_to_npub_format(&v);
                        info!("[REQUEST] {} -> {}", addr, serde_json::to_string(&npub_json).unwrap_or(text));
                        v
                    },
                    Err(e) => {
                        warn!("Failed to parse message from {}: {}", addr, e);
                        let notice = ResponseMessage::Notice {
                            message: format!("invalid: failed to parse message: {}", e),
                        };
                        let notice_json = notice.to_json_array();
                        if let Err(e) = ws_sender
                            .send(Message::Text(serde_json::to_string(&notice_json)?))
                            .await
                        {
                            error!("Failed to send notice: {}", e);
                            break;
                        }
                        continue;
                    }
                };

                // Convert to NostrMessage
                let nostr_msg = match NostrMessage::try_from(json_value) {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!("Invalid nostr message from {}: {}", addr, e);
                        let notice = ResponseMessage::Notice {
                            message: format!("invalid: {}", e),
                        };
                        let notice_json = notice.to_json_array();
                        if let Err(e) = ws_sender
                            .send(Message::Text(serde_json::to_string(&notice_json)?))
                            .await
                        {
                            error!("Failed to send notice: {}", e);
                            break;
                        }
                        continue;
                    }
                };

                // Handle message and get responses
                let responses = handler.handle_message(nostr_msg, addr).await;

                // Send responses
                for response in responses {
                    let response_json = response.to_json_array();
                    let response_text = serde_json::to_string(&response_json)?;
                    info!("[RESPONSE] {} <- {}", addr, response_text);

                    if let Err(e) = ws_sender.send(Message::Text(response_text)).await {
                        error!("Failed to send response to {}: {}", addr, e);
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("Connection closed by {}", addr);
                break;
            }
            Ok(Message::Ping(data)) => {
                debug!("Received ping from {}", addr);
                if let Err(e) = ws_sender.send(Message::Pong(data)).await {
                    error!("Failed to send pong: {}", e);
                    break;
                }
            }
            Ok(Message::Pong(_)) => {
                debug!("Received pong from {}", addr);
            }
            Ok(Message::Binary(_)) => {
                warn!("Received binary message from {}, ignoring", addr);
            }
            Ok(Message::Frame(_)) => {
                // Internal frame, ignore
            }
            Err(e) => {
                error!("WebSocket error from {}: {}", addr, e);
                break;
            }
        }
    }

    info!("Connection to {} closed", addr);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EventStore;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_server_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        let store = Arc::new(EventStore::new());
        let server = RelayServer::new(addr, store);
        assert_eq!(server.addr(), addr);
    }
}
