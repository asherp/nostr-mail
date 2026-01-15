# Installation

## Prerequisites

- **Rust** (latest stable version)
- **Cargo** (comes with Rust)
- **Tauri CLI** (install with `cargo install tauri-cli`)
- **System Dependencies**:
  - **macOS**: Xcode Command Line Tools
  - **Linux**: `libwebkit2gtk-4.0-dev`, `build-essential`, `curl`, `wget`, `libssl-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
  - **Windows**: Microsoft Visual Studio C++ Build Tools

## Installation Steps

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

## Building for Production

```bash
cd tauri-app
cargo tauri build
```

The built application will be in `backend/src-tauri/target/release/`

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
