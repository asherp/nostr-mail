# Mock Nostr Relay Server

A testing-focused mock Nostr relay server implementation in Rust. This server implements the core nostr protocol messages (REQ, EVENT, CLOSE, EOSE, OK, AUTH) and supports the event kinds used by nostr-mail.

## Features

- **Nostr Protocol Support**: Implements REQ, EVENT, CLOSE, EOSE, OK, and AUTH messages
- **Event Kinds**: Supports kinds 0 (profile metadata), 3 (contact list), 4 (encrypted DMs), and 22242 (AUTH)
- **Multiple Relays**: Can run multiple relay instances simultaneously
- **Filtering**: Full support for nostr filters (kinds, authors, pubkeys, time ranges, limits)
- **Testing Utilities**: Helper functions for creating test events
- **Preload Events**: Load test events from JSON files
- **Real Event Generation**: Generate random events with real secp256k1 keypairs and proper signatures
- **Signature Verification**: All events are verified using nostr-sdk (same library as your backend)

## Installation

```bash
cd mock-relay
cargo build --release
```

**Note**: The mock relay uses `nostr-sdk` (same as your backend) to:
- Generate real secp256k1 keypairs (real npub/nsec format)
- Verify all event signatures using the same logic as real nostr relays

## Usage

### Basic Usage

Start a single relay on the default port (8080):

```bash
cargo run -- --port 8080
```

Start multiple relays:

```bash
cargo run -- --relays 3 --start-port 8080
```

This will start 3 relays on ports 8080, 8081, and 8082.

### Command Line Options

- `--port <PORT>`: Port to listen on (default: 8080)
- `--relays <COUNT>`: Number of relays to start (default: 1)
- `--start-port <PORT>`: Starting port for multiple relays (default: 8080)
- `--preload-events <PATH>`: Path to JSON file with events to preload
- `--generate-fake-events <COUNT>`: Generate and preload fake events (count per kind: profiles, contacts, DMs). Events are automatically written to `events.json` (or `--output-events` path) with private keys included.
- `--output-events <PATH>`: Override default output file for generated events (default: `events.json`)
- `--seed <SEED>`: Seed for random number generator (for deterministic event generation). Default is 0. The same seed will generate the same events every time.
- `--log-level <LEVEL>`: Log level (trace, debug, info, warn, error) (default: info)
- `--log-file <PATH>`: Log file path (default: relay.log). Logs are written to both stdout and the file.

### Examples

```bash
# Start relay on port 8080
cargo run -- --port 8080

# Start 3 relays on ports 8080-8082
cargo run -- --relays 3 --start-port 8080

# Start relay with debug logging
cargo run -- --port 8080 --log-level debug

# Start relay and preload events
cargo run -- --port 8080 --preload-events test_events.json

# Start relay with 10 fake events per kind (30 total)
# Events are automatically written to events.json with private keys included
cargo run -- --port 8080 --generate-fake-events 10

# Generate events and write to a custom file
cargo run -- --port 8080 --generate-fake-events 10 --output-events my-events.json

# Generate deterministic events using a seed (same seed = same events)
cargo run -- --port 8080 --generate-fake-events 10 --seed 12345

# Write logs to file (logs also go to stdout)
cargo run -- --port 8080 --log-file relay.log

# Later, you can preload those events from the file
cargo run -- --port 8080 --preload-events events.json
```

## Preload Events Format

The JSON file format includes events, profiles section with private keys, and relay URLs:

```json
{
  "events": [
    {
      "id": "event_id_here",
      "pubkey": "npub1...",
      "created_at": 1234567890,
      "kind": 0,
      "tags": [],
      "content": "{\"name\":\"Alice\",\"about\":\"Test user\"}",
      "sig": "signature_here"
    }
  ],
  "profiles": [
    {
      "pubkey": "npub1...",
      "private_key": "nsec1..."
    }
  ],
  "relays": [
    "ws://127.0.0.1:8080"
  ]
}
```

**Note**: When you generate events with `--generate-fake-events`:
- A `profiles` section is automatically included mapping each unique pubkey to its private key
- A `relays` section is automatically included with the relay URLs where the events are served
- This allows you to use the private keys in your tests (e.g., to decrypt DMs, sign new events, etc.) by looking up the pubkey in the profiles section
- You can use the relay URLs to connect your test client to the mock relays

## Library Usage

You can also use mock-relay as a library in your tests:

```rust
use mock_relay::start_relay;

#[tokio::test]
async fn test_nostr_client() {
    // Start a relay on an available port
    let (addr, _handle) = start_relay(0).await.unwrap();
    
    // Connect your nostr client to ws://{addr}
    // ... your test code ...
}
```

## Supported Event Kinds

- **Kind 0**: Profile metadata (SET_METADATA)
- **Kind 3**: Contact list (ContactList)
- **Kind 4**: Encrypted direct messages (ENCRYPTED_DIRECT_MESSAGE)
- **Kind 22242**: AUTH events (NIP-42)

## Protocol Implementation

### REQ Message

Request events matching filters:

```json
["REQ", "subscription_id", {"kinds": [4], "authors": ["npub1..."]}]
```

The relay responds with:
- `EVENT` messages for each matching event
- `EOSE` message when done

### EVENT Message

Publish an event:

```json
["EVENT", {"id": "...", "pubkey": "...", "created_at": 1234567890, "kind": 4, "tags": [], "content": "...", "sig": "..."}]
```

The relay responds with:
- `OK` message indicating acceptance/rejection

### CLOSE Message

Close a subscription:

```json
["CLOSE", "subscription_id"]
```

### AUTH Message

Authenticate with the relay (NIP-42):

```json
["AUTH", {"id": "...", "kind": 22242, "tags": [["relay", "..."], ["challenge", "..."]], ...}]
```

## Testing Utilities

The `test_utils` module provides helper functions for creating test events and generating fake data:

### Manual Event Creation

```rust
use mock_relay::test_utils::*;

// Create a profile event
let profile = create_profile_event(
    "event_id",
    "npub1...",
    Some("alice"),
    Some("Alice"),
    Some("About Alice"),
    None,
    Some("alice@example.com"),
);

// Create a contact list event
let contacts = create_contact_list_event(
    "event_id",
    "npub1...",
    vec!["npub1contact1", "npub1contact2"],
);

// Create a DM event
let dm = create_dm_event(
    "event_id",
    "sender_npub",
    "recipient_npub",
    "encrypted_content",
);
```

### Fake Event Generation

Generate random fake events for testing:

```rust
use mock_relay::test_utils::*;

// Generate a single fake profile event
let fake_profile = generate_fake_profile_event();

// Generate a fake contact list with 5 contacts
let fake_contacts = generate_fake_contact_list_event(5);

// Generate a fake DM event
let fake_dm = generate_fake_dm_event();

// Generate 10 fake events of a specific kind
let fake_profiles = generate_fake_events(0, 10); // 10 profile events
let fake_dms = generate_fake_events(4, 10);      // 10 DM events

// Generate a mix of events
let mix = generate_fake_event_mix(5, 3, 10); // 5 profiles, 3 contacts, 10 DMs

// Generate fake identifiers
let pubkey = generate_fake_pubkey();    // Random npub (fake, not real bech32)
let event_id = generate_fake_event_id(); // Random event ID
let sig = generate_fake_sig();          // Random signature

// Generate REAL keypairs (requires "real-keys" feature)
#[cfg(feature = "real-keys")]
{
    use mock_relay::test_utils::generate_real_keypair;
    let (nsec, npub) = generate_real_keypair().unwrap();
    println!("Real nsec: {}", nsec);
    println!("Real npub: {}", npub);
}
```

### Important Notes

- **Fake Keys**: `generate_fake_pubkey()` creates strings that look like npubs but are NOT valid bech32-encoded keys. They're just for testing appearance.
- **Real Keys**: Use `generate_real_keypair()` (with "real-keys" feature) to generate actual secp256k1 keypairs that can be used with real nostr clients.
- **Signature Verification**: By default, signatures are NOT verified. Enable "verify-signatures" feature to verify event signatures.

```rust
use mock_relay::test_utils::*;

// Create a profile event
let profile = create_profile_event(
    "event_id",
    "npub1...",
    Some("alice"),
    Some("Alice"),
    Some("About Alice"),
    None,
    Some("alice@example.com"),
);

// Create a contact list event
let contacts = create_contact_list_event(
    "event_id",
    "npub1...",
    vec!["npub1contact1", "npub1contact2"],
);

// Create a DM event
let dm = create_dm_event(
    "event_id",
    "sender_npub",
    "recipient_npub",
    "encrypted_content",
);
```

## Integration with nostr-mail

To use this mock relay with nostr-mail tests:

1. Start the mock relay:
   ```bash
   cd mock-relay
   cargo run -- --port 8080
   ```

2. Configure your test to use `ws://127.0.0.1:8080` as a relay URL

3. The mock relay will handle all nostr protocol messages transparently

## Key Generation and Signature Verification

### Real Keys and Signature Verification (Default)

The mock relay **always** uses real secp256k1 keypairs and verifies all signatures:

- **Real Keys**: All generated events use actual secp256k1 keypairs created with `nostr-sdk::Keys::generate()` (same as your backend)
- **Signature Verification**: All incoming events are verified using `nostr-sdk::Event::verify()` (same as your backend)

This ensures:
- Generated events have valid signatures that your backend will accept
- Events sent to the mock relay are verified using the same logic as real relays
- Full compatibility with your nostr-mail backend

### Generating Real Keypairs

```rust
use mock_relay::test_utils::generate_real_keypair;
let (nsec, npub) = generate_real_keypair().unwrap();
// nsec is a real nsec (e.g., "nsec1...")
// npub is a real npub (e.g., "npub1...")
```

All event generation functions (`generate_fake_profile_event()`, `generate_fake_dm_event()`, etc.) automatically use real keypairs and create properly signed events.

## Limitations

- **In-Memory Storage**: Events are stored in memory and lost on restart
- **No Persistence**: No database or file persistence (intentional for testing)
- **Simplified AUTH**: AUTH messages are accepted without challenge verification (signature is still verified)

## Architecture

```
mock-relay/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Library exports
│   ├── server.rs        # WebSocket server
│   ├── protocol.rs      # Nostr protocol handler
│   ├── store.rs         # Event storage
│   ├── relay.rs         # Relay management
│   ├── config.rs        # Configuration
│   ├── test_utils.rs     # Test utilities
│   └── types.rs          # Core types
```

## Development

### Running Tests

```bash
cargo test
```

### Building

```bash
cargo build
```

### Logging

By default, all logs are written to both stdout and a file (`relay.log`). The mock server logs all incoming requests and outgoing responses, including:
- `[REQUEST]` - All incoming WebSocket messages from clients
- `[RESPONSE]` - All outgoing responses to clients
- `[REQ]` - Subscription requests with details
- `[EVENT]` - Event submissions with event details
- `[CLOSE]` - Subscription close requests
- `[AUTH]` - Authentication requests

To specify a custom log file path, use the `--log-file` option:

```bash
cargo run -- --port 8080 --log-file custom.log
```

Logs are written to the specified file path (created if it doesn't exist, appended to if it does). All logs are written to both stdout and the log file.

### Running with Logging

```bash
RUST_LOG=debug cargo run -- --port 8080
```

## License

Same as nostr-mail project.
