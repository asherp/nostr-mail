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

## Getting Started

Ready to get started? Check out the [Installation Guide](installation.md) and [Quick Start Guide](quick-start.md).

## Documentation

### User Guides
- [Installation](installation.md) - Set up nostr-mail on your system
- [Quick Start](quick-start.md) - Get up and running quickly
- [Compose Page](pages/compose.md) - Create and send encrypted emails
- [Inbox Page](pages/inbox.md) - View and manage received emails
- [Sent Page](pages/sent.md) - View sent email history
- [Drafts Page](pages/drafts.md) - Manage email drafts
- [Direct Messages](pages/direct-messages.md) - Send and receive Nostr DMs
- [Contacts Page](pages/contacts.md) - Manage your Nostr contacts
- [Profile Page](pages/profile.md) - Manage your Nostr profile
- [Settings Page](pages/settings.md) - Configure all application settings

### Technical Documentation
- [Architecture](architecture.md) - Project structure and data flow
- [Development](development.md) - Building and development workflow
- [NIP-44 Encryption](nip44.md) - Encryption standard details

### Legacy Documentation
- [Legacy Docker Setup](legacy.md) - Old Docker/Python setup documentation
