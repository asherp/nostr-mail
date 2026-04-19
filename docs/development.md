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

## Releasing

The single source of truth for the app version is `tauri-app/backend/tauri.conf.json` → `version`. Both the CI build (`.github/workflows/build.yml`) and the landing page template (`.github/workflows/mkdocs-deploy.yml`) read from it.

### Version format

Use plain semver: `1.0.4`, `1.0.5`, etc. **Do not** add pre-release suffixes like `-beta` or `-rc1` — the Windows MSI bundler rejects non-numeric pre-release identifiers. Pre-release status is instead marked on the GitHub release itself via the `prerelease: true` flag in `build.yml`'s release job. Flip it to `false` when shipping a stable version.

### Release steps

1. Bump `version` in `tauri-app/backend/tauri.conf.json`.
2. Commit the bump on `master`.
3. Create an annotated tag matching the version with a `v` prefix:
   ```bash
   git tag -a v1.0.5 -m "Release v1.0.5"
   ```
4. Push the **tag first**, then the branch:
   ```bash
   git push origin v1.0.5
   # wait for the build workflow's release job to publish assets
   git push origin master
   ```

### Why the ordering matters

Two workflows react to a release:

- **`build.yml`** triggers on the tag push. It builds Windows/Linux/macOS installers on three runners, then its `release` job attaches them to a GitHub Release named after the tag. End-to-end: ~15–30 minutes.
- **`mkdocs-deploy.yml`** triggers on any push to `master`. It reads `tauri.conf.json`, substitutes `{{VERSION_TAG}}` / `{{VERSION}}` placeholders in the landing page, and deploys to GitHub Pages. End-to-end: ~3–5 minutes.

If you push the branch before the tag (or both at once), the landing page will go live pointing at download URLs for a release that doesn't exist yet, because `build.yml` is still compiling. Users hitting the site during that window get 404s on the download buttons.

Pushing the tag first, waiting for the GitHub Release to appear, then pushing the branch closes the window. If you forget and push both together, the landing page self-heals once `build.yml` finishes and the release assets become available — no re-deploy needed.

### Manual trigger

Both workflows also support `workflow_dispatch` from the Actions tab, useful for testing build changes without cutting a release.

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
