# Nostr Mail - Frontend JavaScript Architecture

This directory contains the modular JavaScript architecture for the Nostr Mail application. The code has been refactored from a monolithic `main.js` file into separate, well-organized modules.

## 📁 Module Structure

### Core Modules

#### `app-state.js`
- **Purpose**: Centralized state management for the entire application
- **Exports**: `AppState` class and `appState` singleton instance
- **Features**:
  - Contact management (add, update, sort)
  - DM management (contacts, messages)
  - Email management
  - Settings management
  - Keypair management
  - Relay management
  - Selection state management

#### `dom-manager.js`
- **Purpose**: DOM element management and manipulation utilities
- **Exports**: `DOMManager` class and `domManager` singleton instance
- **Features**:
  - Centralized element selection with error handling
  - Element value getters/setters
  - Event listener management
  - CSS class manipulation
  - Show/hide utilities
  - Focus and form utilities

#### `tauri-service.js`
- **Purpose**: All communication with the Rust backend via Tauri commands
- **Exports**: `TauriService` class with static methods
- **Features**:
  - Keypair operations (generate, validate, convert)
  - Nostr operations (send DMs, fetch profiles, publish events)
  - Email operations (send, fetch, test connections)
  - Image operations (fetch, cache)
  - Storage operations (contacts, conversations)
  - Relay operations
  - QR code generation

#### `utils.js`
- **Purpose**: Common utility functions used throughout the application
- **Exports**: `Utils` class with static methods
- **Features**:
  - HTML escaping
  - Time formatting
  - Email provider detection
  - Development mode detection
  - Debounce and throttle functions
  - Validation functions
  - Clipboard operations
  - Object manipulation utilities

#### `notification-service.js`
- **Purpose**: User feedback and notification system
- **Exports**: `NotificationService` class and `notificationService` singleton instance
- **Features**:
  - Success, error, warning, and info notifications
  - Loading notifications
  - Confirmation dialogs
  - Auto-dismiss with animations
  - Fallback to console logging

### Feature Modules

#### `email-service.js`
- **Purpose**: Email functionality including sending, fetching, and management
- **Exports**: `EmailService` class and `emailService` singleton instance
- **Features**:
  - Send emails with validation
  - Load and render emails
  - Email search with debouncing
  - Email detail view
  - Draft saving
  - Connection testing
  - Provider auto-configuration

#### `app.js`
- **Purpose**: Main application coordinator and initialization
- **Exports**: `NostrMailApp` class and `app` singleton instance
- **Features**:
  - Application initialization
  - Event listener setup
  - Tab switching
  - Modal management
  - Settings management
  - Relay management
  - Profile management
  - Dark mode management

## 🔄 Migration from Monolithic Structure

The original `main.js` file (4,058 lines) has been broken down into these modules:

| Original Function | New Module | Lines |
|------------------|------------|-------|
| App state management | `app-state.js` | ~120 |
| DOM utilities | `dom-manager.js` | ~200 |
| Tauri communication | `tauri-service.js` | ~180 |
| Utility functions | `utils.js` | ~200 |
| Notifications | `notification-service.js` | ~250 |
| Email functionality | `email-service.js` | ~400 |
| Main app logic | `app.js` | ~500 |
| **Total** | **7 modules** | **~1,850** |

## 🚀 Benefits of Modular Structure

1. **Maintainability**: Each module has a single responsibility
2. **Testability**: Modules can be tested independently
3. **Reusability**: Utilities and services can be reused across features
4. **Readability**: Smaller files are easier to understand
5. **Collaboration**: Multiple developers can work on different modules
6. **Debugging**: Issues can be isolated to specific modules
7. **Performance**: Modules can be loaded on-demand if needed

## 📋 Usage Examples

### Using the App State
```javascript
import { appState } from './app-state.js';

// Add a contact
appState.addContact({
    name: 'John Doe',
    pubkey: 'npub1...',
    email: 'john@example.com'
});

// Get active relays
const activeRelays = appState.getActiveRelays();
```

### Using the DOM Manager
```javascript
import { domManager } from './dom-manager.js';

// Set a form value
domManager.setValue('emailAddress', 'user@example.com');

// Add event listener
domManager.addEventListener('sendBtn', 'click', handleSend);

// Show/hide elements
domManager.hide('loadingSpinner');
domManager.show('successMessage');
```

### Using the Tauri Service
```javascript
import { TauriService } from './tauri-service.js';

// Send a direct message
await TauriService.sendDirectMessage(
    privateKey,
    recipientPubkey,
    message,
    relays
);

// Fetch emails
const emails = await TauriService.fetchEmails(emailConfig, 10, searchQuery);
```

### Using the Notification Service
```javascript
import { notificationService } from './notification-service.js';

// Show success message
notificationService.showSuccess('Email sent successfully!');

// Show error with custom duration
notificationService.showError('Connection failed', 10000);

// Show confirmation dialog
const confirmed = await notificationService.showConfirmation(
    'Are you sure you want to delete this contact?',
    'Confirm Deletion'
);
```

### Using Utilities
```javascript
import { Utils } from './utils.js';

// Escape HTML
const safeHtml = Utils.escapeHtml('<script>alert("xss")</script>');

// Format time
const timeAgo = Utils.formatTimeAgo(new Date());

// Validate email
const isValid = Utils.isValidEmail('user@example.com');

// Copy to clipboard
await Utils.copyToClipboard('Text to copy');
```

## 🔧 Adding New Features

To add new features, follow these patterns:

1. **New Service**: Create a new service module (e.g., `contacts-service.js`)
2. **State Management**: Add relevant state to `app-state.js`
3. **DOM Elements**: Add element references to `dom-manager.js`
4. **Backend Communication**: Add methods to `tauri-service.js`
5. **Utilities**: Add helper functions to `utils.js`
6. **Integration**: Wire everything together in `app.js`

## 🐛 Debugging

Each module includes comprehensive error handling and logging:

- **Console Logging**: Detailed logs for debugging
- **Error Boundaries**: Graceful error handling
- **Fallbacks**: Alternative behavior when operations fail
- **Validation**: Input validation throughout

## 📦 Module Dependencies

```
app.js
├── app-state.js
├── dom-manager.js
├── tauri-service.js
├── notification-service.js
├── email-service.js
└── utils.js

email-service.js
├── app-state.js
├── dom-manager.js
├── tauri-service.js
├── notification-service.js
└── utils.js
```

## 🔄 Future Enhancements

1. **Lazy Loading**: Load modules on-demand for better performance
2. **TypeScript**: Add type safety with TypeScript
3. **Testing**: Add unit tests for each module
4. **State Persistence**: Add state persistence across sessions
5. **Event System**: Add a centralized event system
6. **Plugin Architecture**: Allow for plugin modules

## 📝 Notes

- All modules use ES6 modules (`import`/`export`)
- Singleton pattern is used for services to ensure single instances
- Error handling is consistent across all modules
- Console logging is used for debugging (can be removed in production)
- The modular structure makes it easy to add new features or modify existing ones 