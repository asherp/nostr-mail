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

The built application will be in `backend/target/release/` (installers under `backend/target/release/bundle/`).

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
5. Once the GitHub Release assets are live, publish the public site by merging
   `master` into `docs` (see below).

### Why the ordering matters

Two workflows react to a release:

- **`build.yml`** triggers on the tag push. It builds Windows/Linux/macOS installers on three runners, then its `release` job attaches them to a GitHub Release named after the tag. End-to-end: ~15–30 minutes.
- **`mkdocs-deploy.yml`** triggers on pushes to the **`docs`** branch. It reads `tauri.conf.json`, substitutes `{{VERSION_TAG}}` / `{{VERSION}}` placeholders in the landing page, and deploys the landing page + docs to GitHub Pages (`gh-pages`). End-to-end: ~3–5 minutes.

Because the deploy is gated on the `docs` branch rather than `master`, bumping the
version and tagging on `master` does **not** change the public website. The site —
including the version number and download links — only updates when you merge into
`docs`. So after a release, wait until the GitHub Release assets exist, then merge
`master → docs`. This deliberately closes the window where the landing page could
point at download URLs for a release that is still compiling (which would 404).

### Publishing docs (the `docs` branch)

The public site is published from the long-lived `docs` branch; `master` is the
authoring source of truth.

- **To publish:** merge `master → docs`. The push to `docs` triggers the deploy.
- **Documentation-only changes** may originate on `docs` (the site updates
  immediately) and then be merged back to `master`. App and version changes
  originate on `master` and reach the site when you merge to `docs`.
- **Sync the two branches with ordinary merge commits only — never squash or
  rebase them.** Squashing or rebasing rewrites SHAs and makes later merges in the
  opposite direction see phantom conflicts, causing the branches to drift.
- The version shown on the site is always whatever `tauri.conf.json` holds **on the
  `docs` branch**, so keep the version bump off `docs` until the release is live.

### Manual trigger

Both workflows also support `workflow_dispatch` from the Actions tab, useful for
re-deploying or testing build changes without cutting a release.

## Code Organization

### Frontend

- **Modular Architecture**: Each major feature has its own service module
- **State Management**: Centralized state in `app-state.js`
- **DOM Management**: Centralized DOM utilities in `dom-manager.js`
- **Service Layer**: Backend communication abstracted in `tauri-service.js`

### Backend

- **Command Handlers**: Tauri commands are defined in `lib.rs` and registered via `tauri::generate_handler!`. `main.rs` is a thin shim that calls `lib::run`.
- **Business Logic**: Feature-specific logic in separate modules (`email.rs`, `nostr.rs`, `crypto.rs`, etc.)
- **Database Layer**: SQLite operations in `database.rs`
- **Key Storage**: OS-keychain vault in `keychain.rs`
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
