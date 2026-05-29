# Settings Page

The Settings page configures all application settings.

## Purpose

Configure email, Nostr, relays, and application preferences.

## Key Features

- **Nostr Settings**: Account login (import or generate a key)
- **Email Settings**: SMTP and IMAP configuration
- **Advanced Settings**: Encryption algorithm, Glossia encoding, inbox filtering, email preferences, sync options
- **Relay Settings**: Add, remove, and manage Nostr relays; live-event status
- **Appearance**: Dark mode toggle

Settings are saved per account (keyed by your public key), so each identity keeps its own configuration.

## Usage Instructions

### Nostr Settings

This is where you log in to your Nostr account. For multi-account management and how keys are stored, see [Accounts & Keys](accounts.md).

#### Private Key (login)

- Enter your nsec/npriv key to log in, or generate a new one below
- Toggle visibility, copy, show a QR code, or scan a QR code from another device
- Public key (npub) is automatically derived and displayed
- Keys are stored in the **OS secure keychain**, not in the browser

#### Generate Keypair

- Click "Generate New Keypair" to create a new account
- **Important**: Save your private key securely — there is no recovery if it is lost!

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

#### Encryption Algorithm

- Select "NIP-44 (Recommended)" or "NIP-04 (Legacy)"
- NIP-44 is the modern, secure standard. Incoming mail is decrypted with either scheme automatically. See [Encryption & Signing](../encryption.md).

#### Glossia Encoding

Choose how each email component is encoded so it survives forwarding and reply quoting. You can set the **Ciphertext**, **Signature**, and **Pubkey** independently to:

- **Latin** — natural-language encoding (default, survives email chains)
- **English – BIP39** — BIP39 word-list encoding (survives email chains)
- **Base64 / npub** — no word encoding; compact but ⚠️ breaks on forward/reply

See the [Glossia Encoding](../glossia.md) guide for details.

#### Inbox Filter

- Choose "Nostr Emails Only" (encrypted, signed, or from Nostr contacts) or "All Emails"

#### Inbox Folder

- Choose which IMAP folder to scan for new mail. "Default" scans INBOX + the `nostr-mail` folder. The folder list loads from your IMAP server.

#### Email Preferences

- **Send Matching DM**: Automatically send a DM with the same subject when emailing Nostr contacts
- **Require Signatures**: Only accept emails with valid signatures
- **Hide Undecryptable Emails**: Hide emails that can't be decrypted with your key
- **Automatically Encrypt**: Encrypt outgoing emails when composing
- **Automatically Sign**: Sign outgoing emails with your Nostr key
- **Hide Unverified**: Hide messages with missing or invalid signatures
- **Include X-Nostr-Pubkey**: Attach the sender's public key as a header so recipients can derive the shared secret without scanning the body
- **Include X-Nostr-Sig**: Attach a signature header over the body (requires X-Nostr-Pubkey)
- **Include X-Nostr-Recipient**: Attach the pubkey the body was encrypted to, allowing decryption without a matching Nostr DM (useful when not using relays)

See [Encryption & Signing](../encryption.md#x-nostr-email-headers) for more on the X-Nostr headers.

#### Sync Settings

- **Sync Cutoff (Days)**: How far back to sync emails on a new device (default: 30 days; set to 0 to sync all)
- **Emails Per Page**: Number of emails per page in the inbox (default: 50, range 10–500)

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
