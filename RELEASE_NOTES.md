# Release Notes - v1.0.4-beta

## Overview

We're excited to announce the beta release of **nostr-mail**, a modern cross-platform email encryption tool that bridges Nostr and email protocols. This release brings end-to-end encrypted email communication using secp256k1 key pairs and Nostr's decentralized social key registry.

## What's New in v1.0.4-beta

### 🧪 Mock Nostr Relay Server
- **Development Tool**: Complete mock relay server for local testing and development
- **Full Protocol Support**: Implements all core Nostr protocol messages (REQ, EVENT, CLOSE, EOSE, OK, AUTH)
- **Event Generation**: Generate test events with real secp256k1 keypairs and proper signatures
- **Multiple Relays**: Run multiple relay instances simultaneously for testing
- **Preload Events**: Load test events from JSON files for reproducible testing
- **Signature Verification**: Uses the same nostr-sdk library as the main app for consistency

### 👥 Private/Public Contact Management
- **Privacy Toggle**: Mark contacts as public or private
- **Organized UI**: Separate collapsible sections for public and private contacts
- **Visual Distinction**: Clear visual indicators for contact privacy status
- **Persistent Settings**: Privacy preferences are saved and persist across app restarts
- **Batch Operations**: Efficiently update privacy status for multiple contacts

### 📱 Android Connection Improvements
- **Startup Fixes**: Resolved Android connection startup issues
- **Auto-Authentication**: Automatic handling of relay authentication requirements
- **Better Error Messages**: Improved error handling and user feedback for connection issues
- **Connection Probing**: Smart detection of relay authentication requirements
- **State Synchronization**: Improved relay connection state management

### 💬 Conversation Sync
- **DM Synchronization**: Automatic synchronization of direct messages from relays
- **Conversation Tracking**: Better tracking and management of conversations
- **Live Updates**: Improved integration with relay connections for real-time updates
- **Event Ordering**: Proper handling of event ordering and deduplication

### 🔍 QR Code Enhancements
- **Multi-Format Support**: Decode various Nostr identifier formats (`nostr:`, `npub1`, `nprofile1`, hex)
- **Contact Form Integration**: QR codes automatically pre-populate the contact form
- **Better Error Handling**: Clear error messages for invalid or unsupported QR codes
- **NIP-21 Support**: Full support for `nostr:` URI scheme (NIP-21 standard)

### 🔧 Bug Fixes & Improvements
- **Contact Loading**: Improved error handling and state preservation in contact loading
- **Search Functionality**: Enhanced contact search with better filtering
- **UI Polish**: Improved styling for contact sections and privacy toggles
- **Code Quality**: Better error handling throughout the application

## Key Features

### 📧 Email Functionality
- **Send & Receive**: Full SMTP/IMAP support for sending and receiving emails
- **End-to-End Encryption**: Encrypt email content using NIP-44 (default) or NIP-04 encryption
- **Email Signing**: Sign emails with your Nostr private key for authentication
- **Attachments**: Support for encrypted file attachments using hybrid encryption (AES-256)
- **Draft Management**: Save and manage email drafts locally
- **Email Filtering**: Filter inbox to show only Nostr-encrypted emails or all emails

### 🔐 Nostr Integration
- **Key Management**: Generate or import Nostr keypairs (nsec/npriv format)
- **Profile Management**: Create and update Nostr profiles with metadata
- **Direct Messages**: Send and receive encrypted DMs via Nostr protocol with conversation sync
- **Relay Management**: Configure and manage multiple Nostr relays with auto-authentication
- **Contact Discovery**: Automatically load contacts from your Nostr follow list
- **Profile Caching**: Offline access to cached profile data and images
- **Mock Relay**: Local testing relay server for development and testing

### 👥 Contact Management
- **Contact List**: View all Nostr contacts in an organized interface
- **Privacy Control**: Mark contacts as public or private with visual separation
- **Profile Details**: View complete contact profiles with metadata
- **Quick Actions**: Send emails, DMs, or copy public keys directly from contacts
- **Search**: Quickly find contacts by name, pubkey, or email
- **QR Code Integration**: Add contacts by scanning QR codes with multi-format support

### 🎨 User Interface
- **Modern Design**: Clean, responsive interface with gradient accents
- **Dark Mode**: Toggle between light and dark themes
- **Tabbed Interface**: Organized sections for Compose, Inbox, Sent, Drafts, DMs, Contacts, Profile, and Settings
- **Cross-Platform**: Works on Windows, macOS, Linux, and Android

## Technical Highlights

- **Encryption**: NIP-44 encryption standard (with NIP-04 backward compatibility)
- **Architecture**: Built with Tauri (Rust backend + web frontend)
- **Storage**: SQLite database for local caching and storage with privacy support
- **Performance**: Vanilla JavaScript frontend for maximum performance
- **Mock Relay**: Rust-based testing server using tokio and nostr-sdk
- **Relay Authentication**: Automatic handling of AUTH challenges and responses
- **Identifier Decoding**: Multi-format Nostr identifier parser supporting all common formats

## Platform Support

- ✅ Windows
- ✅ macOS
- ✅ Linux
- ✅ Android

## Getting Started

1. Generate or import a Nostr keypair in Settings
2. Configure your email provider (SMTP/IMAP)
3. Add Nostr relays
4. Load contacts from your Nostr follow list
5. Start sending encrypted emails!

## Beta Status

This is a beta release. While core functionality is stable, some features may still be refined based on user feedback. Please report any issues or suggestions.

## Security Notes

- Always verify recipient public keys when sending encrypted emails
- Use app passwords instead of your main email password
- Enable TLS/SSL for all email connections
- Keep your private key (nsec/npriv) secure and never share it

## What's Next

Future releases will include:
- CC/BCC support with multi-recipient encryption
- Enhanced attachment handling
- Additional email provider presets
- More comprehensive mock relay features
- Advanced contact management features
- MOAR PRIVACY!

---

**License**: Apache License 2.0
