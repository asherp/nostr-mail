# Development

## Development Workflow

1. **Make Changes**: Edit frontend (HTML/CSS/JS) or backend (Rust) code
2. **Hot Reload**: Frontend changes reload automatically in dev mode
3. **Rebuild Backend**: Rust changes require rebuilding (`cargo tauri dev` handles this)
4. **Test**: Use browser mode for faster iteration, Tauri mode for full testing

## Running in Development Mode

```bash
cd tauri-app
cargo tauri dev
```

This will:
- Build the Rust backend
- Start the Tauri application
- Enable hot reload for frontend changes
- Show console output for debugging

## Running Frontend in Browser (Development Mode)

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

## Building for Production

```bash
cd tauri-app
cargo tauri build
```

The built application will be in `backend/src-tauri/target/release/`

### Build Targets

- **Windows**: `.exe` installer
- **macOS**: `.app` bundle and `.dmg` installer
- **Linux**: `.deb` and `.AppImage` packages
- **Android**: `.apk` file

## Code Organization

### Frontend

- **Modular Architecture**: Each major feature has its own service module
- **State Management**: Centralized state in `app-state.js`
- **DOM Management**: Centralized DOM utilities in `dom-manager.js`
- **Service Layer**: Backend communication abstracted in `tauri-service.js`

### Backend

- **Command Handlers**: Tauri commands defined in `main.rs`
- **Business Logic**: Feature-specific logic in separate modules (`email.rs`, `nostr.rs`, etc.)
- **Database Layer**: SQLite operations in `database.rs`
- **Type Safety**: Shared types in `types.rs`

## Debugging

### Frontend Debugging

- Use browser DevTools when running in browser mode
- Console logs are available in Tauri dev mode
- Check `tauri-service.js` for backend communication issues

### Backend Debugging

- Rust compiler errors show in terminal
- Use `println!` macros for debugging (visible in terminal)
- Check Tauri console output for command execution

### Database Debugging

- Database file location: OS-specific app data directory
- Use SQLite browser tools to inspect database
- Database schema defined in `database.rs`

## Testing

### Manual Testing

- Test each page functionality
- Test email sending/receiving
- Test DM sending/receiving
- Test contact loading
- Test settings persistence

### Integration Testing

- Test email + Nostr integration
- Test encryption/decryption
- Test database operations
- Test relay connectivity

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## Code Style

### JavaScript

- Use ES6+ features
- Follow module pattern (services, utilities)
- Use meaningful variable names
- Add comments for complex logic

### Rust

- Follow Rust naming conventions
- Use `Result` types for error handling
- Add documentation comments
- Keep functions focused and small
