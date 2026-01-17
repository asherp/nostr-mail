# nostr-mail

A modern, cross-platform email encryption tool that bridges Nostr and email protocols, providing end-to-end encrypted email communication using secp256k1 key pairs and the Nostr social key registry.

## Overview

Nostr-mail encrypts email content using a symmetric key derived from a combination of the sender's private key and the receiver's public key. Both sender and receiver derive a shared secret known only to them, which is used to protect their communications.

This application can use any email server for delivery while leveraging Nostr's decentralized social key registry for key discovery and management.

### Why nostr-mail?

Nostr-mail aims to improve privacy for the average person by bridging the gap between Nostr and email. The two protocols serve different purposes, but they also solve each other's problems. For example, PGP does exist for email but it has not seen mainstream adoption because it relies on an existing key registry.

| Feature            | Nostr                               | Email                             | nostr-mail                  |
| -------------------|-------------------------------------| --------------------------------- |--------------------------- |
| Social Key Registry| ✓                                   | ✗                                 | ✓                          |
| PGP                | ✓                                   | ✓                                 | ✓                          |
| Long form content  | ✗                                   | ✓                                 | ✓                          |
| Archival Storage   | ✗                                   | ✓                                 | ✓                          |
| Ubiquitous         | ✗                                   | ✓                                 | ✓                          |

## Features

### 📧 Email Functionality
- **SMTP Email Sending**: Send emails through configured SMTP servers
- **IMAP Email Fetching**: Fetch and display emails from IMAP servers
- **Email Composition**: Rich email composition with subject, body, and recipient fields
- **Draft Saving**: Save email drafts locally for later completion
- **Email Preview**: View email list with sender, subject, and preview text
- **Email Details**: Full email viewing with formatted content
- **Attachment Support**: Encrypt and send file attachments using hybrid encryption

### 🔐 Nostr Integration
- **Key Management**: Generate and manage Nostr keypairs (nsec/npriv format)
- **Profile Management**: Create and update Nostr profiles with metadata
- **Direct Messages**: Send and receive encrypted direct messages via Nostr
- **Relay Management**: Configure and manage Nostr relays with enable/disable controls
- **Contact Discovery**: Automatically load contacts from your Nostr follow list
- **Profile Caching**: Cache profile data and images for offline access

### 👥 Contact Management
- **Contact List**: View all your Nostr contacts in a clean, organized interface
- **Profile Details**: View complete contact profiles with all available metadata
- **Contact Actions**: Send emails, direct messages, or copy public keys
- **Smart Caching**: Contacts load instantly from cache with progressive image loading
- **In-Place Updates**: Refresh contacts without clearing the list
- **Alphabetical Sorting**: Contacts are consistently sorted by name

### 🎨 User Interface
- **Modern Design**: Clean, responsive interface with gradient accents
- **Dark Mode**: Toggle between light and dark themes
- **Tabbed Interface**: Organized sections for Compose, Inbox, Sent, Drafts, DMs, Contacts, Profile, and Settings
- **Responsive Layout**: Works on desktop and mobile devices
- **Loading States**: Visual feedback during data loading and operations
- **Notifications**: Success and error notifications for user actions

## Tech Stack

### Frontend
- **Vanilla JavaScript**: No framework dependencies for maximum performance
- **HTML5**: Semantic markup with accessibility features
- **CSS3**: Modern styling with CSS Grid, Flexbox, and custom properties
- **Font Awesome**: Icon library for consistent UI elements

### Backend (Tauri)
- **Rust**: High-performance backend for email and Nostr operations
- **Tauri**: Cross-platform desktop app framework (Windows, macOS, Linux, Android)
- **Nostr Libraries**: Rust Nostr implementation with NIP-44 and NIP-04 support
- **Email Libraries**: SMTP and IMAP support for email functionality
- **SQLite**: Local database for caching contacts, messages, and settings

### Encryption
- **NIP-44**: Default encryption standard (recommended)
- **NIP-04**: Legacy encryption support for backward compatibility
- **Hybrid Encryption**: AES-256 for large attachments, NIP-44 for keys

## Installation

### Prerequisites

- **Rust** (latest stable version)
- **Cargo** (comes with Rust)
- **Tauri CLI** (install with `cargo install tauri-cli`)
- **System Dependencies**:
  - **macOS**: Xcode Command Line Tools
  - **Linux**: `libwebkit2gtk-4.0-dev`, `build-essential`, `curl`, `wget`, `libssl-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
  - **Windows**: Microsoft Visual Studio C++ Build Tools

### Installation Steps

1. **Clone the repository**:
   ```bash
   git clone https://github.com/asherp/nostr-mail
   cd nostr-mail/tauri-app
   ```

2. **Install Tauri CLI** (if not already installed):
   ```bash
   cargo install tauri-cli
   ```

3. **Start development server**:
   ```bash
   cargo tauri dev
   ```

## Quick Start

1. **Generate or Import Keypair**: 
   - Navigate to Settings → Nostr Settings
   - Click "Generate New Keypair" or paste your existing nsec/npriv key

2. **Configure Email**:
   - Navigate to Settings → Email Settings
   - Enter your email provider details (SMTP and IMAP)
   - Test your connection

3. **Add Relays**:
   - Navigate to Settings → Relay Settings
   - Add Nostr relays (e.g., `wss://relay.damus.io`)

4. **Load Contacts**:
   - Navigate to Contacts tab
   - Click "Refresh" to load contacts from your Nostr follow list

5. **Compose Your First Email**:
   - Navigate to Compose tab
   - Select a Nostr contact or enter an email address
   - Write your message and send!

## Page Documentation

### Compose Page

The Compose page is where you create and send encrypted emails.

#### Purpose
Compose and send encrypted emails to recipients, with optional Nostr direct message notifications.

#### Key Features
- **Recipient Selection**: Choose from Nostr contacts with email addresses or enter any email address manually
- **Email Composition**: Write subject and message body
- **Encryption**: Encrypt email content using NIP-44 (default) or NIP-04
- **Signing**: Sign emails with your Nostr private key for authentication
- **Attachments**: Add file attachments with automatic hybrid encryption (AES-256 for files, NIP-44 for keys)
- **Draft Saving**: Save drafts locally for later completion
- **Preview Headers**: Preview email headers before sending

#### Usage Instructions

1. **Select Recipient**:
   - Use the dropdown to select a Nostr contact (if they have an email in their profile)
   - Or manually enter an email address in the "To:" field
   - When a Nostr contact is selected, their public key is displayed

2. **Compose Message**:
   - Enter a subject line
   - Write your message in the text area

3. **Add Attachments** (optional):
   - Click "Add Attachments" button
   - Select one or more files
   - Attachments are automatically encrypted when you encrypt the message

4. **Encrypt and Sign**:
   - Click "Encrypt" to encrypt the message (or enable automatic encryption in Settings)
   - Click "Sign" to sign the message (or enable automatic signing in Settings)
   - The Send button shows icons indicating what actions will be performed (🔒 encrypt, ✍️ sign, ✈️ send)

5. **Send Email**:
   - Click "Send" to send the encrypted email
   - If a Nostr contact is selected and encryption is enabled, you can optionally send a matching DM notification

6. **Save Draft**:
   - Click "Save Draft" to save your work for later
   - Drafts can be resumed from the Drafts page

#### Configuration Options
- **Automatic Encryption**: Enable in Settings → Advanced → Automatically Encrypt
- **Automatic Signing**: Enable in Settings → Advanced → Automatically Sign
- **Send Matching DM**: Enable in Settings → Advanced → Send Matching DM (sends DM with same subject when emailing Nostr contacts)

#### Tips and Best Practices
- Always verify the recipient's public key when selecting a Nostr contact
- Use descriptive subject lines as they may be visible in DM notifications
- Large attachments are automatically handled with hybrid encryption
- Preview headers before sending to verify encryption and signing status

### Inbox Page

The Inbox page displays all received emails, with filtering and decryption capabilities.

#### Purpose
View, search, and decrypt received emails from your IMAP server.

#### Key Features
- **Email List**: View all emails with sender, subject, and preview
- **Email Details**: Full email viewing with decrypted content
- **Search**: Search emails by sender email address
- **Filtering**: Filter to show only Nostr-encrypted emails or all emails
- **Decryption**: Automatic decryption of encrypted emails using your private key
- **Pagination**: Load emails in pages (configurable page size)
- **Signature Verification**: Verify Nostr signatures on received emails

#### Usage Instructions

1. **View Inbox**:
   - Navigate to Inbox tab
   - Emails are automatically loaded from your IMAP server
   - Click on any email to view details

2. **Search Emails**:
   - Use the search box to filter by sender email address
   - Results update as you type

3. **View Email Details**:
   - Click on an email in the list
   - View full email content with decrypted body
   - See encryption status and signature verification

4. **Refresh Inbox**:
   - Click "Refresh" button to fetch new emails from server

5. **Navigate Back**:
   - Click "Back to Inbox" to return to the email list

#### Configuration Options
- **Email Filter**: Choose "Nostr Emails Only" or "All Emails" in Settings → Advanced → Inbox Filter
- **Hide Undecryptable**: Enable in Settings → Advanced → Hide Undecryptable Emails
- **Require Signatures**: Enable in Settings → Advanced → Require Signatures
- **Hide Unverified**: Enable in Settings → Advanced → Hide Unverified
- **Emails Per Page**: Configure in Settings → Advanced → Emails Per Page (default: 50)

#### Tips and Best Practices
- Use "Nostr Emails Only" filter to focus on encrypted communications
- Enable signature verification to ensure email authenticity
- Undecryptable emails are hidden by default (can be shown by disabling the setting)
- Refresh regularly to stay up-to-date with new emails

### Sent Page

The Sent page displays all emails you've sent.

#### Purpose
View and manage sent email history.

#### Key Features
- **Sent Email List**: View all sent emails with recipient, subject, and timestamp
- **Email Details**: View full sent email content
- **Search**: Search sent emails by recipient or subject
- **Pagination**: Load sent emails in pages

#### Usage Instructions

1. **View Sent Emails**:
   - Navigate to Sent tab
   - View list of all sent emails
   - Click on any email to view details

2. **Search Sent Emails**:
   - Use the search box to filter sent emails
   - Search by recipient or subject

3. **View Email Details**:
   - Click on an email to view full content
   - See encryption and signing status

4. **Refresh**:
   - Click "Refresh" to reload sent emails

#### Tips and Best Practices
- Sent emails are stored locally in the database
- Use search to quickly find specific sent emails
- Sent emails show encryption and signing status

### Drafts Page

The Drafts page manages saved email drafts.

#### Purpose
View, edit, and manage saved email drafts.

#### Key Features
- **Draft List**: View all saved drafts
- **Edit Drafts**: Resume editing any draft
- **Delete Drafts**: Remove drafts you no longer need
- **Draft Details**: View draft content before resuming

#### Usage Instructions

1. **View Drafts**:
   - Navigate to Drafts tab
   - View list of all saved drafts

2. **Resume Draft**:
   - Click on a draft to view details
   - Click "Edit" or navigate to Compose to resume editing
   - Draft content is automatically loaded into the compose form

3. **Delete Draft**:
   - Click on a draft to view details
   - Click "Delete" to remove the draft

4. **Refresh**:
   - Click "Refresh" to reload drafts list

#### Tips and Best Practices
- Drafts are saved automatically when you click "Save Draft"
- Drafts are stored locally and persist across app restarts
- Delete old drafts to keep your list organized

### Direct Messages Page

The Direct Messages page manages Nostr direct message conversations.

#### Purpose
Send and receive encrypted direct messages via the Nostr protocol.

#### Key Features
- **Conversation List**: View all DM conversations sorted by most recent
- **Message Threading**: View full conversation threads
- **Send DMs**: Send encrypted direct messages to Nostr contacts
- **Real-time Updates**: Receive new DMs in real-time via relay subscriptions
- **Search**: Search conversations by contact name
- **Email Matching**: See which DMs match sent emails

#### Usage Instructions

1. **View Conversations**:
   - Navigate to Direct Messages tab
   - View list of all conversations sorted by most recent message
   - Click on a conversation to view messages

2. **Send Direct Message**:
   - Select a conversation or start a new one
   - Type your message in the input field
   - Press Enter or click Send
   - Messages are automatically encrypted with NIP-44

3. **Search Conversations**:
   - Click "Search" button
   - Type to filter conversations by contact name

4. **Refresh**:
   - Click "Refresh" to reload conversations from relays

#### Configuration Options
- **Encryption Algorithm**: Choose NIP-44 (recommended) or NIP-04 in Settings → Nostr Settings → Encryption Algorithm
- **Relays**: Configure relays in Settings → Relay Settings

#### Tips and Best Practices
- DMs are encrypted end-to-end using NIP-44
- Conversations are automatically synced from configured relays
- DMs matching email subjects are marked for easy identification
- Use DMs for quick Nostr-native communication

### Contacts Page

The Contacts page displays and manages your Nostr contacts.

#### Purpose
View, search, and interact with your Nostr contacts.

#### Key Features
- **Contact List**: View all contacts from your Nostr follow list
- **Profile Details**: View complete contact profiles with metadata
- **Contact Actions**: Send emails, DMs, or copy public keys
- **Search**: Search contacts by name
- **Add Contacts**: Manually add contacts by public key
- **Export Contacts**: Export contact list as npub list
- **Progressive Loading**: Images load progressively for better performance

#### Usage Instructions

1. **View Contacts**:
   - Navigate to Contacts tab
   - View list of all contacts sorted alphabetically
   - Click on a contact to view profile details

2. **View Profile Details**:
   - Click on any contact in the list
   - View full profile including name, picture, about, email, and public key
   - See all available profile metadata

3. **Contact Actions**:
   - **Send Email**: Click "Send Email" to compose an email to this contact
   - **Send DM**: Click "Send DM" to start a direct message conversation
   - **Copy Pubkey**: Click "Copy Pubkey" to copy the contact's public key

4. **Search Contacts**:
   - Click "Search" button
   - Type to filter contacts by name

5. **Add Contact**:
   - Click "Add Contact" button
   - Enter a public key (npub format) or scan QR code
   - Contact will be added to your list

6. **Export Contacts**:
   - Click "Export Contacts" to download a list of all npubs

7. **Refresh**:
   - Click "Refresh" to reload contacts from Nostr relays

#### Tips and Best Practices
- Contacts are automatically loaded from your Nostr follow list
- Profile pictures are cached locally for offline access
- Only contacts with email addresses appear in the Compose page dropdown
- Use search to quickly find specific contacts

### Profile Page

The Profile page manages your Nostr profile.

#### Purpose
Create and update your Nostr profile metadata.

#### Key Features
- **Profile Fields**: Edit name, display name, picture, about, email, and custom fields
- **Profile Picture**: Upload and set profile picture
- **Publish Profile**: Publish profile updates to Nostr relays
- **Profile Preview**: See your profile as others see it

#### Usage Instructions

1. **View Profile**:
   - Navigate to Profile tab
   - View your current profile fields

2. **Edit Profile**:
   - Click on any field to edit
   - Update name, display name, about, email, etc.
   - Add custom profile fields

3. **Set Profile Picture**:
   - Click on profile picture area
   - Select an image file
   - Picture is automatically uploaded and cached

4. **Update Profile**:
   - Click "Update Profile" button
   - Profile is published to configured Nostr relays
   - Changes are visible to others immediately

5. **Refresh**:
   - Click "Refresh" to reload profile from relays

#### Tips and Best Practices
- Profile updates are published as Nostr kind 0 events
- Include your email address so others can send you encrypted emails
- Profile pictures are cached and displayed in contacts
- Custom fields allow you to add any additional metadata

### Settings Page

The Settings page configures all application settings.

#### Purpose
Configure email, Nostr, relays, and application preferences.

#### Key Features
- **Nostr Settings**: Keypair management, encryption algorithm selection
- **Email Settings**: SMTP and IMAP configuration
- **Relay Settings**: Add, remove, and manage Nostr relays
- **Advanced Settings**: Email preferences, filtering, sync options
- **Appearance**: Dark mode toggle

#### Usage Instructions

##### Nostr Settings

1. **Private Key Management**:
   - Enter your nsec/npriv key or generate a new one
   - Toggle visibility, copy, or show QR code
   - Public key (npub) is automatically derived

2. **Encryption Algorithm**:
   - Select "NIP-44 (Recommended)" or "NIP-04 (Legacy)"
   - NIP-44 is the modern, secure standard

3. **Generate Keypair**:
   - Click "Generate New Keypair" to create a new keypair
   - Save your private key securely!

##### Email Settings

1. **Email Provider**:
   - Select your provider (Gmail, Outlook, Yahoo, or Custom)
   - Provider-specific settings are auto-filled

2. **Email Configuration**:
   - Enter email address and password/app password
   - Configure SMTP host and port (default: 587)
   - Configure IMAP host and port (default: 993)
   - Enable TLS/SSL (recommended)

3. **Test Connection**:
   - Click "Test Email Connection" to verify settings

##### Relay Settings

1. **Add Relay**:
   - Enter relay URL (e.g., `wss://relay.damus.io`)
   - Click "Add" to add relay
   - Relays are used for DMs and profile updates

2. **Manage Relays**:
   - Enable/disable relays by toggling switches
   - Remove relays you no longer need

##### Advanced Settings

1. **Inbox Filter**:
   - Choose "Nostr Emails Only" or "All Emails"

2. **Email Preferences**:
   - **Send Matching DM**: Automatically send DM when emailing Nostr contacts
   - **Require Signatures**: Only accept emails with valid signatures
   - **Hide Undecryptable Emails**: Hide emails that can't be decrypted
   - **Automatically Encrypt**: Encrypt all outgoing emails
   - **Automatically Sign**: Sign all outgoing emails
   - **Hide Unverified**: Hide messages without verified signatures

3. **Sync Settings**:
   - **Sync Cutoff**: How far back to sync emails (default: 365 days)
   - **Emails Per Page**: Number of emails per page (default: 50)

##### Appearance

1. **Dark Mode**:
   - Toggle dark mode on/off
   - Preference is saved and persists across sessions

#### Configuration Tips

- **Gmail Setup**:
  1. Generate an app password: https://support.google.com/accounts/answer/185833
  2. Remove spaces from the app password
  3. Enable IMAP in Gmail settings
  4. Use SMTP: `smtp.gmail.com:587`, IMAP: `imap.gmail.com:993`

- **Security**:
  - Never share your private key (nsec/npriv)
  - Use app passwords instead of your main email password
  - Enable TLS/SSL for all email connections
  - Use NIP-44 encryption (not NIP-04)

- **Relays**:
  - Add multiple relays for redundancy
  - Use reliable, well-maintained relays
  - Test relay connections regularly

## Architecture

### Project Structure

```
tauri-app/
├── frontend/              # Frontend assets
│   ├── index.html        # Main HTML file
│   ├── js/               # JavaScript modules
│   │   ├── app.js        # Main application logic
│   │   ├── app-state.js  # State management
│   │   ├── email-service.js    # Email functionality
│   │   ├── dm-service.js       # Direct messages
│   │   ├── contacts-service.js # Contact management
│   │   ├── tauri-service.js    # Backend communication
│   │   └── ...
│   └── styles/          # CSS files
│       ├── variables.css
│       ├── email.css
│       ├── contacts.css
│       └── ...
├── backend/              # Rust backend
│   ├── src/
│   │   ├── main.rs       # Main entry point
│   │   ├── lib.rs        # Library exports
│   │   ├── email.rs      # Email operations
│   │   ├── nostr.rs      # Nostr protocol
│   │   ├── crypto.rs     # Encryption/decryption
│   │   ├── database.rs   # Database operations
│   │   └── ...
│   └── Cargo.toml       # Rust dependencies
└── tauri.conf.json       # Tauri configuration
```

### Data Flow

1. **Frontend** (JavaScript) handles UI and user interactions
2. **Tauri Service** bridges frontend and backend via Tauri commands
3. **Backend** (Rust) handles:
   - Email operations (SMTP/IMAP)
   - Nostr operations (DMs, profiles, relays)
   - Encryption/decryption (NIP-44/NIP-04)
   - Database operations (SQLite)
4. **Database** stores:
   - Contacts and profiles
   - Email messages
   - DM conversations
   - Settings and preferences

### Key Technologies

- **Tauri**: Cross-platform framework wrapping Rust backend with web frontend
- **nostr-sdk**: Rust library for Nostr protocol implementation
- **lettre**: Rust email library for SMTP/IMAP
- **rusqlite**: SQLite database for local storage
- **NIP-44**: Modern encryption standard for Nostr messages

## Development

### Building for Production

```bash
cd tauri-app
cargo tauri build
```

The built application will be in `backend/src-tauri/target/release/`

### Running Frontend in Browser (Development Mode)

You can run the frontend as a static site in your browser while connecting to the backend via HTTP:

```bash
# On macOS/Linux:
./run-browser-dev.sh

# On Windows:
run-browser-dev.bat

# Or manually:
# Terminal 1: Start HTTP server
cd backend
cargo run --bin http-server --release

# Terminal 2: Serve frontend
cd frontend
python3 -m http.server 8080
# Then open http://127.0.0.1:8080 in your browser
```

The HTTP server runs on `http://127.0.0.1:1420` and the frontend is served on `http://127.0.0.1:8080`. The frontend automatically detects browser mode and uses HTTP instead of Tauri APIs.

### Development Workflow

1. **Make Changes**: Edit frontend (HTML/CSS/JS) or backend (Rust) code
2. **Hot Reload**: Frontend changes reload automatically in dev mode
3. **Rebuild Backend**: Rust changes require rebuilding (`cargo tauri dev` handles this)
4. **Test**: Use browser mode for faster iteration, Tauri mode for full testing

## NIP-44 Encryption

Nostr-mail uses **NIP-44** as the default encryption standard for all encrypted communications.

### What is NIP-44?

NIP-44 is a modern encryption standard for Nostr that provides:
- **Better Security**: Improved cryptographic properties compared to NIP-04
- **Versioning**: Supports multiple encryption versions for future improvements
- **Standardization**: Widely adopted across Nostr clients

### NIP-44 vs NIP-04

| Feature | NIP-44 | NIP-04 |
|---------|--------|--------|
| Security | Modern, secure | Legacy, known issues |
| Versioning | Supported | Not supported |
| Adoption | Growing | Legacy |
| Recommendation | ✓ Use | ✗ Avoid |

### Encryption Details

- **Algorithm**: NIP-44 encryption using shared secret derived from keypairs
- **Default**: NIP-44 is the default for all new installations
- **Backward Compatibility**: NIP-04 is supported for reading legacy messages
- **Attachments**: Hybrid encryption (AES-256 for files, NIP-44 for keys)

### Security Notes

- NIP-44 provides better security than NIP-04
- All new messages use NIP-44 by default
- Legacy NIP-04 messages can still be decrypted
- Consider migrating old conversations to NIP-44

For more information, see the [NIP-44 specification](https://github.com/nostr-protocol/nips/blob/master/44.md).

## Legacy Documentation

For documentation on the legacy Docker/Python setup, see [LEGACY.md](LEGACY.md).

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## License

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
