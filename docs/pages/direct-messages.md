# Direct Messages Page

The Direct Messages page (the **Messages** tab) manages Nostr direct message conversations.

## Purpose

Send and receive encrypted direct messages via the Nostr protocol.

## Key Features

- **Conversation List**: View all DM conversations sorted by most recent
- **Message Threading**: View full conversation threads
- **Send DMs**: Send encrypted direct messages to Nostr contacts
- **Real-time Updates**: Receive new DMs in real-time via relay subscriptions
- **Search**: Search conversations by contact name
- **Email Matching**: See which DMs match sent emails

## Usage Instructions

### 1. View Conversations

- Navigate to Direct Messages tab
- View list of all conversations sorted by most recent message
- Click on a conversation to view messages

### 2. Send Direct Message

- Select a conversation or start a new one
- Type your message in the input field
- Press Enter or click Send
- Messages are sent as **NIP-17 gift-wrapped** DMs (kind 1059), which hide message metadata from relays. If the recipient hasn't published a NIP-17 inbox, nostr-mail transparently falls back to NIP-04. See [Encryption & Signing](../encryption.md#nip-17-gift-wrapped-direct-messages).

### 3. Search Conversations

- Click "Search" button
- Type to filter conversations by contact name

### 4. Refresh

- Click "Refresh" to reload conversations from relays

## Configuration Options

- **Encryption Algorithm**: Choose NIP-44 (recommended) or NIP-04 in Settings → Advanced → Encryption Algorithm
- **Relays**: Configure relays in Settings → Relay Settings

## Tips and Best Practices

- DMs are encrypted end-to-end and gift-wrapped (NIP-17), with NIP-04 fallback for recipients who don't support it
- Conversations are automatically synced from configured relays
- DMs matching email subjects are marked for easy identification
- Use DMs for quick Nostr-native communication
- Multiple relays ensure message delivery reliability
- Real-time updates keep conversations current
