#!/bin/bash
# Script to run the backend HTTP server and serve the frontend for browser development

set -e

echo "🚀 Starting NostrMail Browser Development Mode"
echo "================================================"

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
BACKEND_DIR="$SCRIPT_DIR/backend"
FRONTEND_DIR="$SCRIPT_DIR/frontend"

# Check if we're in the right directory
if [ ! -d "$BACKEND_DIR" ] || [ ! -d "$FRONTEND_DIR" ]; then
    echo "❌ Error: This script must be run from the tauri-app directory"
    exit 1
fi

# Build the HTTP server if needed
echo ""
echo "📦 Building HTTP server..."
cd "$BACKEND_DIR"
cargo build --bin http-server --release 2>&1 | grep -E "(Compiling|Finished|error)" || true

# Start the HTTP server in the background
echo ""
echo "🌐 Starting HTTP server on http://127.0.0.1:1420..."
cd "$BACKEND_DIR"
cargo run --bin http-server --release &
HTTP_SERVER_PID=$!

# Wait a moment for the server to start
sleep 2

# Check if the server started successfully
if ! kill -0 $HTTP_SERVER_PID 2>/dev/null; then
    echo "❌ Error: HTTP server failed to start"
    exit 1
fi

echo "✅ HTTP server started (PID: $HTTP_SERVER_PID)"

# Start a simple HTTP server for the frontend
echo ""
echo "📂 Serving frontend on http://127.0.0.1:8080..."
echo "   Open http://127.0.0.1:8080 in your browser"
echo ""
echo "Press Ctrl+C to stop both servers"

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "🛑 Stopping servers..."
    kill $HTTP_SERVER_PID 2>/dev/null || true
    kill $FRONTEND_SERVER_PID 2>/dev/null || true
    exit 0
}

trap cleanup SIGINT SIGTERM

# Start frontend server (Python's http.server)
cd "$FRONTEND_DIR"
python3 -m http.server 8080 &
FRONTEND_SERVER_PID=$!

# Wait for both processes
wait
