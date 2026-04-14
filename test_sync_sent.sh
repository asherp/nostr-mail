#!/bin/bash
# Test script for sync_sent_emails command
# This requires the app to be running and accessible via HTTP API

set -e

HTTP_BASE_URL="${HTTP_BASE_URL:-http://localhost:1420}"

echo "=========================================="
echo "Testing sync_sent_emails command"
echo "=========================================="
echo ""

# Check if HTTP server is running
if ! curl -s "${HTTP_BASE_URL}/health" > /dev/null 2>&1; then
    echo "❌ HTTP server is NOT running on ${HTTP_BASE_URL}"
    echo ""
    echo "To start the HTTP server, run:"
    echo "  cd tauri-app/backend"
    echo "  cargo run --bin http-server --release"
    echo ""
    echo "Or use the browser dev script:"
    echo "  cd tauri-app"
    echo "  ./run-browser-dev.sh"
    echo ""
    echo "The server will start on ${HTTP_BASE_URL}"
    exit 1
fi

echo "✅ HTTP server is running"
echo ""

# You'll need to provide email config - adjust these values
EMAIL_CONFIG='{
  "email_address": "example@email.com",
  "password": "password",
  "smtp_host": "localhost",
  "smtp_port": 2525,
  "imap_host": "localhost",
  "imap_port": 1143,
  "use_tls": false,
  "private_key": null
}'

echo "Testing sync_sent_emails..."
echo "Config: ${EMAIL_CONFIG}"
echo ""

RESPONSE=$(curl -s -X POST "${HTTP_BASE_URL}/invoke" \
  -H "Content-Type: application/json" \
  -d "{
    \"command\": \"sync_sent_emails\",
    \"args\": {
      \"config\": ${EMAIL_CONFIG}
    }
  }")

echo "Response:"
echo "${RESPONSE}" | jq '.' 2>/dev/null || echo "${RESPONSE}"

if echo "${RESPONSE}" | grep -q '"success":true'; then
    echo ""
    echo "✅ Sync successful!"
    COUNT=$(echo "${RESPONSE}" | jq -r '.data' 2>/dev/null || echo "unknown")
    echo "   Synced ${COUNT} new emails"
else
    echo ""
    echo "❌ Sync failed!"
    ERROR=$(echo "${RESPONSE}" | jq -r '.error' 2>/dev/null || echo "${RESPONSE}")
    echo "   Error: ${ERROR}"
fi

echo ""
echo "=========================================="
