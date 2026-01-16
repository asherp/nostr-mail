# NostrMail

A modern email client built with Tauri that integrates Nostr protocol features for enhanced privacy and decentralized communication.

## Features

### 📧 Email Functionality
- **SMTP Email Sending**: Send emails through configured SMTP servers
- **IMAP Email Fetching**: Fetch and display emails from IMAP servers
- **Email Composition**: Rich email composition with subject, body, and recipient fields
- **Draft Saving**: Save email drafts locally for later completion
- **Email Preview**: View email list with sender, subject, and preview text
- **Email Details**: Full email viewing with formatted content

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
- **Dark Mode Support**: Full dark mode support for all contact interfaces

### 🎨 User Interface
- **Modern Design**: Clean, responsive interface with gradient accents
- **Dark Mode**: Toggle between light and dark themes
- **Tabbed Interface**: Organized sections for Compose, Inbox, DMs, Contacts, Profile, and Settings
- **Responsive Layout**: Works on desktop and mobile devices
- **Loading States**: Visual feedback during data loading and operations
- **Notifications**: Success and error notifications for user actions

### 🔧 Settings & Configuration
- **Email Configuration**: SMTP and IMAP server settings with TLS support
- **Nostr Settings**: Private key management and relay configuration
- **Connection Testing**: Test email and Nostr connections
- **Settings Persistence**: All settings saved locally and restored on startup

## Technical Architecture

### Frontend
- **Vanilla JavaScript**: No framework dependencies for maximum performance
- **HTML5**: Semantic markup with accessibility features
- **CSS3**: Modern styling with CSS Grid, Flexbox, and custom properties
- **Font Awesome**: Icon library for consistent UI elements

### Backend (Tauri)
- **Rust**: High-performance backend for email and Nostr operations
- **Tauri**: Cross-platform desktop app framework
- **Nostr Libraries**: Rust Nostr implementation for protocol features
- **Email Libraries**: SMTP and IMAP support for email functionality

### Data Management
- **Local Storage**: Settings, contacts, and profile data cached locally
- **Progressive Loading**: Images and data loaded progressively for better UX
- **Smart Caching**: Intelligent cache management with expiration and updates

## Getting Started

### Prerequisites
- Rust (latest stable)
- Cargo (comes with Rust)
- Tauri CLI

### Installation
```bash
# Clone the repository
git clone <repository-url>
cd nostr-mail/tauri-app

# Install Tauri CLI (if not already installed)
cargo install tauri-cli

# Start development server
cargo tauri dev
```

### Building for Production
```bash
# Build the application
cargo tauri build

# The built application will be in src-tauri/target/release/
```

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

### Configuration
1. **Email Setup**: Configure your SMTP and IMAP settings in the Settings tab
2. **Nostr Setup**: Generate or import your Nostr private key
3. **Relay Configuration**: Add and configure Nostr relays
4. **Test Connections**: Use the test buttons to verify your configuration

## Usage

### Email
- **Compose**: Write and send emails from the Compose tab
- **Inbox**: View received emails in the Inbox tab
- **Drafts**: Save and resume email composition

### Nostr Features
- **Profile**: Update your Nostr profile with metadata
- **Direct Messages**: Send encrypted DMs to other Nostr users
- **Contacts**: View and interact with your Nostr contacts
- **Relays**: Manage your Nostr relay connections

### Contact Management
- **View Contacts**: See all your Nostr contacts in the Contacts tab
- **Profile Details**: Click any contact to view their full profile
- **Quick Actions**: Send emails, DMs, or copy public keys directly
- **Refresh**: Update your contact list with the refresh button

## Development

### Project Structure
```
tauri-app/
├── frontend/           # Frontend assets
│   ├── index.html     # Main HTML file
│   ├── styles.css     # Styles and themes
│   └── main.js        # Main JavaScript logic
├── src/               # Rust backend
│   ├── main.rs        # Main application logic
│   └── nostr/         # Nostr protocol implementation
└── tauri.conf.json    # Tauri configuration
```

### Key Features Implementation
- **Contact Loading**: Progressive loading with cache management
- **Image Handling**: Automatic image fetching and caching as data URLs
- **Dark Mode**: CSS-based theme switching with localStorage persistence
- **Responsive Design**: Mobile-friendly layouts with CSS Grid and Flexbox
- **Error Handling**: Comprehensive error handling and user feedback

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## License

[Add your license information here]

## Roadmap

- [ ] Email threading and conversation view
- [ ] Advanced Nostr filters and search
- [ ] Contact groups and organization
- [ ] Email templates and signatures
- [ ] Multi-account support
- [ ] Offline mode improvements
- [ ] Advanced security features
