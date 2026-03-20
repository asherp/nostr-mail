// Simple test script to verify connection to mock relay
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let relay_url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:8080".to_string());
    
    println!("🔌 Connecting to mock relay: {}", relay_url);
    
    match connect_async(&relay_url).await {
        Ok((ws_stream, _)) => {
            println!("✅ Successfully connected to relay!");
            
            let (mut write, mut read) = ws_stream.split();
            
            // Send a simple REQ message to test the connection
            let test_req = r#"["REQ", "test_sub", {"kinds": [0], "limit": 1}]"#;
            println!("📤 Sending test REQ: {}", test_req);
            
            if let Err(e) = write.send(Message::Text(test_req.to_string())).await {
                eprintln!("❌ Failed to send message: {}", e);
                return Err(e.into());
            }
            
            println!("⏳ Waiting for response...");
            
            // Wait for a response (with timeout)
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(5),
                read.next()
            ).await {
                Ok(Some(Ok(Message::Text(response)))) => {
                    println!("✅ Received response: {}", response);
                    println!("✅ Connection test successful!");
                }
                Ok(Some(Ok(Message::Close(_)))) => {
                    println!("⚠️  Connection closed by server");
                }
                Ok(Some(Ok(Message::Binary(_)))) => {
                    println!("✅ Received binary response (connection working)");
                }
                Ok(Some(Ok(Message::Ping(_)))) => {
                    println!("✅ Received ping (connection working)");
                }
                Ok(Some(Ok(Message::Pong(_)))) => {
                    println!("✅ Received pong (connection working)");
                }
                Ok(Some(Ok(Message::Frame(_)))) => {
                    println!("✅ Received frame (connection working)");
                }
                Ok(Some(Err(e))) => {
                    eprintln!("❌ WebSocket error: {}", e);
                    return Err(e.into());
                }
                Ok(None) => {
                    println!("⚠️  No response received (connection closed)");
                }
                Err(_) => {
                    println!("⏱️  Timeout waiting for response (relay may be slow or not responding)");
                }
            }
            
            // Close the connection
            let _ = write.close().await;
            println!("👋 Connection closed");
        }
        Err(e) => {
            eprintln!("❌ Failed to connect: {}", e);
            eprintln!("💡 Make sure the mock relay is running:");
            eprintln!("   cd mock-relay");
            eprintln!("   cargo run -- --port 8080");
            return Err(e.into());
        }
    }
    
    Ok(())
}
