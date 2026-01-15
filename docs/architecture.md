# Architecture

## Project Structure

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

## Data Flow

**1. Frontend** (JavaScript) handles UI and user interactions

**2. Tauri Service** bridges frontend and backend via Tauri commands

**3. Backend** (Rust) handles:

   - Email operations (SMTP/IMAP)
   - Nostr operations (DMs, profiles, relays)
   - Encryption/decryption (NIP-44/NIP-04)
   - Database operations (SQLite)

**4. Database** stores:

   - Contacts and profiles
   - Email messages
   - DM conversations
   - Settings and preferences

## Key Technologies

- **Tauri**: Cross-platform framework wrapping Rust backend with web frontend
- **nostr-sdk**: Rust library for Nostr protocol implementation
- **lettre**: Rust email library for SMTP/IMAP
- **rusqlite**: SQLite database for local storage
- **NIP-44**: Modern encryption standard for Nostr messages

## Frontend Architecture

### Module Structure

- **app-state.js**: Centralized state management
- **dom-manager.js**: DOM element management utilities
- **tauri-service.js**: Backend communication layer
- **email-service.js**: Email-specific functionality
- **dm-service.js**: Direct message functionality
- **contacts-service.js**: Contact management
- **notification-service.js**: User feedback system
- **utils.js**: Common utility functions

### State Management

The application uses a centralized state management system (`appState`) that tracks:

- Contacts
- Direct messages
- Email messages
- Settings
- Keypair
- Relays
- Selection state

## Backend Architecture

### Core Modules

- **main.rs**: Application entry point and Tauri command definitions
- **lib.rs**: Library exports and shared utilities
- **email.rs**: SMTP sending and IMAP fetching
- **nostr.rs**: Nostr protocol operations (DMs, profiles, relays)
- **crypto.rs**: Encryption/decryption operations
- **database.rs**: SQLite database operations
- **types.rs**: Shared type definitions

### Database Schema

The SQLite database stores:

- Contacts (profiles, metadata)
- Email messages (encrypted content, metadata)
- DM conversations (encrypted messages, metadata)
- Settings (per-pubkey configuration)
- Relay configurations

## Security Considerations

- The current private is stored in frontend localStorage (not in database)
- Entering a new private key effectively logs out of the previous profile
- Encryption happens in Rust backend
- Database is local and encrypted at rest (OS-level)
- All network communications use TLS/SSL
- NIP-44 encryption provides modern security guarantees
- App passwords are encrypted at rest via the user's private key

The workflow for switching accounts is being simplified. Currently, the app will not keep store a previous private key when switching accounts, but the user data (private contacts, settings) will be stored with the public key.