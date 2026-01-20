// Example: Using mock-relay as a library in tests

use mock_relay::{start_relay, test_utils::*};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start a relay on an available port (port 0 = OS picks available port)
    let (addr, handle) = start_relay(0).await?;
    
    println!("Mock relay started on ws://{}", addr);
    
    // Create some test events
    let profile = create_profile_event(
        "profile_event_id",
        "npub1test123456789012345678901234567890123456789012345678901234567890",
        Some("alice"),
        Some("Alice"),
        Some("Test user"),
        None,
        Some("alice@example.com"),
    );
    
    let dm = create_dm_event(
        "dm_event_id",
        "npub1sender123456789012345678901234567890123456789012345678901234567890",
        "npub1recipient123456789012345678901234567890123456789012345678901234567890",
        "encrypted_content_here",
    );
    
    println!("Created test events:");
    println!("  Profile: kind={}, pubkey={}", profile.kind, profile.pubkey);
    println!("  DM: kind={}, content length={}", dm.kind, dm.content.len());
    
    // In a real test, you would:
    // 1. Connect your nostr client to ws://{addr}
    // 2. Publish events to the relay
    // 3. Query events from the relay
    // 4. Verify the results
    
    println!("\nRelay is running. Press Ctrl+C to stop.");
    
    // Wait a bit
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Stop the relay
    handle.abort();
    println!("Relay stopped.");
    
    Ok(())
}
