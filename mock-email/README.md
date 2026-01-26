# Mock Email Server

A testing-focused mock email server implementation in Rust. This server implements SMTP (for receiving emails) and IMAP (for fetching emails) protocols, designed for testing email functionality in nostr-mail and other email applications.

## Features

- **SMTP Server**: Receives emails via SMTP protocol (default port 2525)
- **IMAP Server**: Fetches emails via IMAP protocol (default port 1143)
- **Multiple Mailboxes**: Supports INBOX, SENT, DRAFTS, TRASH, and custom mailboxes
- **Email Storage**: In-memory email storage with indexing by sender and recipient
- **Testing Utilities**: Helper functions for creating test emails
- **Preload Emails**: Load test emails from JSON files
- **Fake Email Generation**: Generate random fake emails for testing
- **Email Parsing**: Full email parsing with support for attachments, HTML, and headers

## Installation

```bash
cd mock-email
cargo build --release
```

## Usage

### Basic Usage

Start the mock email server with default ports (SMTP: 2525, IMAP: 1143):

```bash
cargo run
```

Start with custom ports:

```bash
cargo run -- --smtp-port 2525 --imap-port 1143
```

### Command Line Options

- `--smtp-port <PORT>`: SMTP port to listen on (default: 2525)
- `--imap-port <PORT>`: IMAP port to listen on (default: 1143)
- `--preload-emails <PATH>`: Path to JSON file with emails to preload
- `--generate-fake-emails <COUNT>`: Generate and preload fake emails. Emails are automatically written to `emails.json` (or `--output-emails` path).
- `--output-emails <PATH>`: Override default output file for generated emails (default: `emails.json`)
- `--seed <SEED>`: Seed for random number generator (for deterministic email generation). Default is 0. The same seed will generate the same emails every time.
- `--log-level <LEVEL>`: Log level (trace, debug, info, warn, error) (default: info)
- `--log-file <PATH>`: Log file path (default: email.log). Logs are written to both stdout and the file.

### Examples

```bash
# Start server with default ports
cargo run

# Start server with custom ports
cargo run -- --smtp-port 2525 --imap-port 1143

# Start server with debug logging
cargo run -- --log-level debug

# Start server and preload emails
cargo run -- --preload-emails test_emails.json

# Start server with 50 fake emails
# Emails are automatically written to emails.json
cargo run -- --generate-fake-emails 50

# Generate emails and write to a custom file
cargo run -- --generate-fake-emails 50 --output-emails my-emails.json

# Generate deterministic emails using a seed (same seed = same emails)
cargo run -- --generate-fake-emails 50 --seed 12345

# Write logs to file (logs also go to stdout)
cargo run -- --log-file email.log

# Later, you can preload those emails from the file
cargo run -- --preload-emails emails.json
```

## Preload Emails Format

The JSON file format includes emails with their mailbox assignments:

```json
{
  "emails": [
    {
      "id": "email_id_here",
      "from": "sender@example.com",
      "to": ["recipient@example.com"],
      "cc": [],
      "bcc": [],
      "subject": "Test Subject",
      "body": "Test body content",
      "html_body": null,
      "headers": {
        "message-id": "<test@mock-email>",
        "user-agent": "mock-email/1.0"
      },
      "created_at": 1234567890,
      "attachments": [],
      "mailbox": "INBOX"
    }
  ],
  "mailboxes": null
}
```

**Note**: When you generate emails with `--generate-fake-emails`:
- Emails are automatically assigned to appropriate mailboxes (INBOX for recipients, SENT for senders)
- A `mailboxes` section can be included to specify custom mailbox assignments
- This allows you to use the emails in your tests by loading them from the file

## Library Usage

You can also use mock-email as a library in your tests:

```rust
use mock_email::{EmailStore, SmtpServer, ImapServer};
use std::sync::Arc;

#[tokio::test]
async fn test_email_client() {
    // Create email store
    let store = Arc::new(EmailStore::new());
    
    // Start SMTP server on an available port
    let smtp_addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let smtp_server = SmtpServer::new(smtp_addr, store.clone());
    let smtp_handle = tokio::spawn(async move {
        smtp_server.start().await.unwrap();
    });
    
    // Start IMAP server
    let imap_addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let imap_server = ImapServer::new(imap_addr, store.clone());
    let imap_handle = tokio::spawn(async move {
        imap_server.start().await.unwrap();
    });
    
    // Connect your email client to the servers
    // ... your test code ...
}
```

## Supported SMTP Commands

- `EHLO` / `HELO`: Greeting
- `MAIL FROM:`: Specify sender
- `RCPT TO:`: Specify recipient
- `DATA`: Start email data transmission
- `QUIT`: Close connection
- `RSET`: Reset session
- `NOOP`: No operation

## Supported IMAP Commands

- `CAPABILITY`: List server capabilities
- `LOGIN`: Authenticate (accepts any credentials for testing)
- `AUTHENTICATE`: Authenticate (simplified, accepts any auth)
- `SELECT` / `EXAMINE`: Select a mailbox
- `LIST`: List mailboxes
- `FETCH`: Fetch email data (BODY[], RFC822, ENVELOPE, BODYSTRUCTURE)
- `SEARCH`: Search emails (simplified, returns all emails)
- `STATUS`: Get mailbox status
- `NOOP`: No operation
- `LOGOUT`: Close connection

## Testing Utilities

The `test_utils` module provides helper functions for creating test emails and generating fake data:

### Manual Email Creation

```rust
use mock_email::test_utils::*;
use mock_email::types::{Email, EmailAddress};

// Create a simple email
let email = generate_fake_email();

// Create email with specific from/to
let from = EmailAddress::from_string("sender@example.com").unwrap();
let to = vec![EmailAddress::from_string("recipient@example.com").unwrap()];
let email = generate_fake_email_with_rng(&mut rand::thread_rng(), Some(from), Some(to));
```

### Fake Email Generation

Generate random fake emails for testing:

```rust
use mock_email::test_utils::*;

// Generate a single fake email
let fake_email = generate_fake_email();

// Generate multiple fake emails
let fake_emails = generate_fake_emails(10);

// Generate deterministic emails using a seed
let fake_emails = generate_fake_emails_with_seed(10, Some(12345));

// Generate emails with attachments
let email_with_attachments = generate_fake_email_with_attachments(3);

// Generate email addresses
let addresses = generate_fake_email_addresses(5);
```

## Integration with nostr-mail

To use this mock email server with nostr-mail:

1. Start the mock email server:
   ```bash
   cd mock-email
   cargo run
   ```

2. Configure nostr-mail email settings:
   - **SMTP Host**: `127.0.0.1`
   - **SMTP Port**: `2525`
   - **IMAP Host**: `127.0.0.1`
   - **IMAP Port**: `1143`
   - **Disable TLS/SSL** (mock server doesn't support encryption)
   - **Any username/password** (mock server accepts any credentials)

3. The mock server will handle all SMTP/IMAP protocol messages transparently

See [CONNECTING.md](CONNECTING.md) for detailed connection instructions.

## Limitations

- **In-Memory Storage**: Emails are stored in memory and lost on restart
- **No Persistence**: No database or file persistence (intentional for testing)
- **Simplified Authentication**: IMAP authentication accepts any credentials (for testing)
- **Basic IMAP**: Not all IMAP features are implemented (sufficient for basic testing)
- **No TLS/SSL**: TLS/SSL is not supported (use for local testing only)

## Architecture

```
mock-email/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Library exports
│   ├── smtp.rs          # SMTP server
│   ├── imap.rs          # IMAP server
│   ├── store.rs         # Email storage
│   ├── types.rs         # Core types
│   ├── test_utils.rs    # Test utilities
│   └── config.rs        # Configuration
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

By default, all logs are written to both stdout and a file (`email.log`). The mock server logs all incoming requests and outgoing responses, including:
- `[SMTP]` - All incoming SMTP commands and email data
- `[IMAP]` - All incoming IMAP commands and responses

To specify a custom log file path, use the `--log-file` option:

```bash
cargo run -- --log-file custom.log
```

Logs are written to the specified file path (created if it doesn't exist, appended to if it does). All logs are written to both stdout and the log file.

### Running with Logging

```bash
RUST_LOG=debug cargo run
```

## License

Same as nostr-mail project.
