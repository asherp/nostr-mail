# Release Notes - v1.0.2-beta

## Overview

We're excited to announce the beta release of **nostr-mail**, a modern cross-platform email encryption tool that bridges Nostr and email protocols. This release brings end-to-end encrypted email communication using secp256k1 key pairs and Nostr's decentralized social key registry.

## What's New in v1.0.2-beta

### 🚀 Performance Improvements
- **Faster Contact Loading**: Significantly improved performance when loading contacts from your Nostr follow list
- **Immediate Message Loading**: Sent and received messages now load immediately on startup

### 🔧 Bug Fixes & Improvements
- **Fixed Startup Workflow**: Improved application startup and initialization process
- **Code Quality**: Fixed compiler warnings and improved code stability
- **UI Enhancements**: Added new modal styles and QR scanner styling improvements

### 🏗️ Technical Changes
- Major backend refactoring for better maintainability
- Enhanced database functionality with improved query handling
- Frontend service improvements for better reliability
- Removed deprecated state management code

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
- **Direct Messages**: Send and receive encrypted DMs via Nostr protocol
- **Relay Management**: Configure and manage multiple Nostr relays
- **Contact Discovery**: Automatically load contacts from your Nostr follow list
- **Profile Caching**: Offline access to cached profile data and images

### 👥 Contact Management
- **Contact List**: View all Nostr contacts in an organized interface
- **Profile Details**: View complete contact profiles with metadata
- **Quick Actions**: Send emails, DMs, or copy public keys directly from contacts
- **Search**: Quickly find contacts by name

### 🎨 User Interface
- **Modern Design**: Clean, responsive interface with gradient accents
- **Dark Mode**: Toggle between light and dark themes
- **Tabbed Interface**: Organized sections for Compose, Inbox, Sent, Drafts, DMs, Contacts, Profile, and Settings
- **Cross-Platform**: Works on Windows, macOS, Linux, and Android

## Technical Highlights

- **Encryption**: NIP-44 encryption standard (with NIP-04 backward compatibility)
- **Architecture**: Built with Tauri (Rust backend + web frontend)
- **Storage**: SQLite database for local caching and storage
- **Performance**: Vanilla JavaScript frontend for maximum performance

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
- Improved relay connection management
- Enhanced attachment handling
- Additional email provider presets

---

**License**: Apache License 2.0
