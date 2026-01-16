# Settings Page

The Settings page configures all application settings.

## Purpose

Configure email, Nostr, relays, and application preferences.

## Key Features

- **Nostr Settings**: Keypair management, encryption algorithm selection
- **Email Settings**: SMTP and IMAP configuration
- **Relay Settings**: Add, remove, and manage Nostr relays
- **Advanced Settings**: Email preferences, filtering, sync options
- **Appearance**: Dark mode toggle

## Usage Instructions

### Nostr Settings

#### Private Key Management

- Enter your nsec/npriv key or generate a new one
- Toggle visibility, copy, or show QR code
- Public key (npub) is automatically derived

#### Encryption Algorithm

- Select "NIP-44 (Recommended)" or "NIP-04 (Legacy)"
- NIP-44 is the modern, secure standard

#### Generate Keypair

- Click "Generate New Keypair" to create a new keypair
- **Important**: Save your private key securely!

### Email Settings

#### Email Provider

- Select your provider (Gmail, Outlook, Yahoo, or Custom)
- Provider-specific settings are auto-filled

#### Email Configuration

- Enter email address and password/app password
- Configure SMTP host and port (default: 587)
- Configure IMAP host and port (default: 993)
- Enable TLS/SSL (recommended)

#### Test Connection

- Click "Test Email Connection" to verify settings

### Relay Settings

#### Add Relay

- Enter relay URL (e.g., `wss://relay.damus.io`)
- Click "Add" to add relay
- Relays are used for DMs and profile updates

#### Manage Relays

- Enable/disable relays by toggling switches
- Remove relays you no longer need

### Advanced Settings

#### Inbox Filter

- Choose "Nostr Emails Only" or "All Emails"

#### Email Preferences

- **Send Matching DM**: Automatically send DM when emailing Nostr contacts
- **Require Signatures**: Only accept emails with valid signatures
- **Hide Undecryptable Emails**: Hide emails that can't be decrypted
- **Automatically Encrypt**: Encrypt all outgoing emails
- **Automatically Sign**: Sign all outgoing emails
- **Hide Unverified**: Hide messages without verified signatures

#### Sync Settings

- **Sync Cutoff**: How far back to sync emails (default: 365 days)
- **Emails Per Page**: Number of emails per page (default: 50)

### Appearance

#### Dark Mode

- Toggle dark mode on/off
- Preference is saved and persists across sessions

## Configuration Tips

### Gmail Setup

1. Generate an app password: https://support.google.com/accounts/answer/185833
2. Remove spaces from the app password
3. Enable IMAP in Gmail settings
4. Use SMTP: `smtp.gmail.com:587`, IMAP: `imap.gmail.com:993`

### Security

- Never share your private key (nsec/npriv)
- Use app passwords instead of your main email password
- Enable TLS/SSL for all email connections
- Use NIP-44 encryption (not NIP-04)

### Relays

- Add multiple relays for redundancy
- Use reliable, well-maintained relays
- Test relay connections regularly
