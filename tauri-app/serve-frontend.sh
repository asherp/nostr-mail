#!/bin/bash

# Serve the Tauri frontend as a static site
# This allows you to view the frontend in a browser for development/testing

FRONTEND_DIR="frontend"
PORT=${1:-8000}

# Check if frontend directory exists
if [ ! -d "$FRONTEND_DIR" ]; then
    echo "Error: Frontend directory '$FRONTEND_DIR' not found"
    echo "Make sure you're running this from the tauri-app directory"
    exit 1
fi

echo "🚀 Serving NostrMail frontend..."
echo "📁 Directory: $(pwd)/$FRONTEND_DIR"
echo "🌐 URL: http://localhost:$PORT"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

# Try Python 3 first, then Python 2, then suggest alternatives
if command -v python3 &> /dev/null; then
    cd "$FRONTEND_DIR" && python3 -m http.server "$PORT"
elif command -v python &> /dev/null; then
    cd "$FRONTEND_DIR" && python -m SimpleHTTPServer "$PORT"
else
    echo "Python not found. Alternative options:"
    echo ""
    echo "1. Install Python (recommended)"
    echo "2. Use Node.js: npx http-server frontend -p $PORT"
    echo "3. Use PHP: php -S localhost:$PORT -t frontend"
    echo "4. Use Ruby: cd frontend && ruby -run -e httpd . -p $PORT"
    exit 1
fi
