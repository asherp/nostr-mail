# Connecting to Mock Email Servers

This guide explains how to connect your nostr-mail application to the mock email servers for testing.

## Starting the Mock Email Server

First, start the mock email server:

```bash
cd mock-email
cargo run
```

The server will start with:
- **SMTP server** on `127.0.0.1:2525` (for sending emails)
- **IMAP server** on `127.0.0.1:1143` (for receiving emails)

You should see output like:
```
[INFO] Starting mock-email server
[INFO] Mock email server running:
[INFO]   SMTP: smtp://127.0.0.1:2525
[INFO]   IMAP: imap://127.0.0.1:1143
```

## Configuring nostr-mail (Tauri App)

### Via Settings UI

1. **Open nostr-mail** and navigate to **Settings → Email Settings**

2. **Select "Custom"** as your email provider

3. **Configure SMTP settings**:
   - **SMTP Host**: `127.0.0.1`
   - **SMTP Port**: `2525`
   - **SMTP Username**: Any value (e.g., `test@example.com`)
   - **SMTP Password**: Any value (e.g., `password`)
   - **Use TLS**: **Disable** (mock server doesn't support TLS)

4. **Configure IMAP settings**:
   - **IMAP Host**: `127.0.0.1`
   - **IMAP Port**: `1143`
   - **IMAP Username**: Any value (e.g., `test@example.com`)
   - **IMAP Password**: Any value (e.g., `password`)
   - **Use TLS**: **Disable** (mock server doesn't support TLS)

5. **Click "Test Email Connection"** to verify the connection

### Via Environment Variables (Legacy Python App)

If you're using the legacy Python/Dash app, set these environment variables:

```bash
export EMAIL_ADDRESS=test@example.com
export EMAIL_PASSWORD=password
export SMTP_HOST=127.0.0.1
export SMTP_PORT=2525
export IMAP_HOST=127.0.0.1
export IMAP_PORT=1143
```

## Important Notes

### Authentication

The mock email server **accepts any credentials** for testing purposes. You can use:
- Username: `test@example.com` (or any email address)
- Password: `password` (or any string)

### TLS/SSL

**Disable TLS/SSL** in your email client settings. The mock server doesn't support encrypted connections (intentional for local testing).

### Email Addresses

When sending emails:
- **From address**: Use any email address (e.g., `sender@example.com`)
- **To address**: Use any email address (e.g., `recipient@example.com`)

The mock server will:
- Store emails sent to recipients in their **INBOX**
- Store emails sent from senders in their **SENT** mailbox

### Testing Workflow

1. **Start mock server**: `cd mock-email && cargo run`

2. **Configure nostr-mail** with the mock server settings (see above)

3. **Send a test email**:
   - Go to Compose tab
   - Enter recipient: `test@example.com`
   - Write subject and body
   - Click Send

4. **Check the email was received**:
   - Go to Inbox tab
   - Click Refresh
   - You should see the email you just sent

5. **Check mock server logs**: Look at `email.log` or the console output to see SMTP/IMAP activity

## Preloading Test Emails

You can preload the mock server with test emails:

```bash
# Generate 50 fake emails
cargo run -- --generate-fake-emails 50

# Or preload from a JSON file
cargo run -- --preload-emails emails.json
```

Then when you connect via IMAP, you'll see these preloaded emails in the INBOX.

## Troubleshooting

### Connection Refused

- Make sure the mock server is running (`cargo run` in the `mock-email` directory)
- Check that ports 2525 (SMTP) and 1143 (IMAP) are not already in use
- Verify you're using `127.0.0.1` (not `localhost`)

### Authentication Failed

- The mock server accepts any credentials, so this shouldn't happen
- Make sure TLS/SSL is **disabled** in your email settings

### Emails Not Appearing

- Click "Refresh" in the Inbox tab
- Check the mock server logs (`email.log`) to see if emails were received
- Verify the email addresses match (case-sensitive in some cases)

### Port Already in Use

If ports 2525 or 1143 are already in use, start the mock server with custom ports:

```bash
cargo run -- --smtp-port 2526 --imap-port 1144
```

Then update your email settings to use these ports.

## Example Configuration

Here's a complete example configuration for the Tauri app:

```json
{
  "smtp_host": "127.0.0.1",
  "smtp_port": 2525,
  "smtp_username": "test@example.com",
  "smtp_password": "password",
  "smtp_use_tls": false,
  "imap_host": "127.0.0.1",
  "imap_port": 1143,
  "imap_username": "test@example.com",
  "imap_password": "password",
  "imap_use_tls": false
}
```

## Integration with Tests

For automated testing, you can start the mock server programmatically:

```rust
use mock_email::{EmailStore, SmtpServer, ImapServer};
use std::sync::Arc;

#[tokio::test]
async fn test_email_sending() {
    // Create store
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    // Start SMTP server
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
    
    // Now connect your email client to the servers
    // Use smtp_server.addr() and imap_server.addr() to get the actual ports
}
```
