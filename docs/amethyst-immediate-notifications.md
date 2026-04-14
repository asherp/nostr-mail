# How Amethyst Gets Immediate Notification of Sent Messages

## Overview

Amethyst (a Nostr client for Android) achieves immediate notification of sent messages through a combination of **optimistic UI updates**, **persistent WebSocket subscriptions**, and **relay echo mechanisms**. This document explains how this works and compares it to the current implementation in nostr-mail.

## Key Mechanisms

### 1. **Optimistic UI / Local Echo**

Amethyst immediately displays sent messages in the UI before waiting for relay confirmation:

- When you tap "send", the message appears instantly in the chat view
- The message is stored in the local database immediately
- This provides instant visual feedback without network latency
- The message is marked as "pending" or "sending" until relay confirmation

**Implementation**: The frontend adds the message to the UI and local state immediately after signing the event, before sending to relays.

### 2. **Persistent WebSocket Subscriptions**

Amethyst maintains persistent WebSocket connections to configured relays and subscribes to events matching specific filters:

```rust
// Example subscription filters (similar to nostr-mail's implementation)
Filter 1: Messages sent by user
  - kind: EncryptedDirectMessage
  - author: <user_pubkey>
  
Filter 2: Messages received by user  
  - kind: EncryptedDirectMessage
  - pubkey: <user_pubkey>  // Messages tagged with user's pubkey
```

**How it works:**
- Client sends `REQ` messages to relays with subscription filters
- Relays maintain the subscription and send matching events via WebSocket
- Client listens to `client.notifications()` stream for incoming events
- When a sent message matches the subscription filter, it arrives via the notification stream

### 3. **Relay Echo Mechanism**

When you publish an event to a relay:

1. **Publish**: Client sends `PUBLISH` event to relay via WebSocket
2. **Relay Acceptance**: Relay validates and accepts the event (may send `OK` response)
3. **Relay Echo**: Some relays echo back events that match active subscriptions
   - If you're subscribed to messages from your own pubkey (Filter 1 above)
   - The relay may immediately send your own message back through the subscription
   - This provides confirmation that the relay has received and stored the message

**Important Note**: Not all relays reliably echo sent messages immediately. Some relays:
- Only echo messages after a delay
- Don't echo messages from the same connection that published them
- Have inconsistent behavior across different relay implementations

### 4. **Local Database Storage**

Amethyst stores messages immediately in the local database:
- Sent messages are saved locally before relay confirmation
- This ensures messages persist even if the app closes before confirmation
- Database queries show messages immediately, regardless of relay state

## Comparison: Amethyst vs nostr-mail

### Amethyst's Approach

1. **Optimistic UI**: Shows message immediately
2. **Relies on Relay Echo**: Waits for relay to echo back the message via subscription
3. **Subscription Filters**: Subscribes to own messages (`.author(user_pubkey)`)
4. **Fast Relays**: Uses low-latency relays for better echo timing

### nostr-mail's Current Approach

Looking at `tauri-app/backend/src/lib.rs` lines 140-174:

```rust
// CRITICAL FIX: Relays don't immediately echo sent messages back to the sender.
// Manually trigger the live handler immediately after successful send to avoid delay.
if output.success.len() > 0 {
    // Process the sent message through the live handler immediately
    // This uses the same code path as relay notifications, ensuring consistency
    tokio::spawn(async move {
        handle_live_direct_message(&event_clone, &app_handle_clone, &state_clone, &user_pubkey_clone).await
    });
}
```

**Key Difference**: nostr-mail **manually triggers** the live handler because relays don't reliably echo sent messages immediately.

### Why nostr-mail Uses Manual Trigger

The codebase comment explains:
> "Relays don't immediately echo sent messages back to the sender."

This means:
- Waiting for relay echo can cause delays (seconds or more)
- Some relays never echo messages from the same connection
- Manual trigger ensures consistent immediate feedback

## How Amethyst Might Achieve Better Results

Based on research, Amethyst likely:

1. **Uses Multiple Relays**: Publishes to several relays simultaneously
   - Some relays may echo faster than others
   - First echo wins, providing immediate confirmation

2. **Optimized Relay Selection**: Chooses relays known for fast echo behavior
   - Some relay implementations are better at echoing
   - May use relays with lower latency

3. **Subscription Timing**: Maintains subscriptions before sending
   - Ensures subscription is active when message is published
   - Reduces delay between publish and echo

4. **Local State Management**: Aggressive local caching
   - Messages appear immediately from local database
   - Relay echo just confirms persistence

## Technical Implementation Details

### Subscription Setup (from nostr-mail's code)

```rust
// Filter for sent messages
let dm_sent_filter = Filter::new()
    .kind(Kind::EncryptedDirectMessage)
    .author(user_pubkey)
    .since(Timestamp::from(since_timestamp));

// Subscribe to filter
client.subscribe(dm_sent_filter, None).await?;

// Listen for notifications
let mut notifications = client.notifications();
while let Ok(notification) = notifications.recv().await {
    match notification {
        RelayPoolNotification::Event { event, .. } => {
            // Handle echoed message
        }
    }
}
```

### Message Flow

```
User sends message
    ↓
1. Sign event locally
    ↓
2. Add to UI immediately (optimistic)
    ↓
3. Save to local database
    ↓
4. Publish to relays via WebSocket
    ↓
5a. [Amethyst] Wait for relay echo via subscription
5b. [nostr-mail] Manually trigger handler immediately
    ↓
6. Update UI with confirmation status
```

## Recommendations for nostr-mail

The current manual trigger approach is actually **more reliable** than waiting for relay echo:

1. **Consistent Behavior**: Works regardless of relay implementation
2. **Immediate Feedback**: No waiting for relay echo delays
3. **Same Code Path**: Uses same handler as relay notifications, ensuring consistency

However, you could improve by:

1. **Adding Optimistic UI**: Show message immediately before backend confirmation
2. **Dual Strategy**: Try manual trigger first, but also listen for relay echo as backup
3. **Better Relay Selection**: Choose relays known for fast echo (if available)

## References

- Nostr Protocol: https://github.com/nostr-protocol/nips
- Amethyst GitHub: https://github.com/vitorpamplona/amethyst
- nostr-mail implementation: `tauri-app/backend/src/lib.rs` (lines 140-174, 3258-3347)
