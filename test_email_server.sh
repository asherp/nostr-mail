#!/bin/bash
# Comprehensive test script for mock email server

set -e

SMTP_PORT=2525
IMAP_PORT=1143
HOST=127.0.0.1

echo "=========================================="
echo "Mock Email Server Test Script"
echo "=========================================="
echo ""

# Check if server is running
echo "Checking if server is running..."
if ! nc -z $HOST $SMTP_PORT 2>/dev/null; then
    echo "❌ SMTP server is NOT running on port $SMTP_PORT"
    echo "   Please start it with: cd mock-email && cargo run -- --smtp-port $SMTP_PORT --imap-port $IMAP_PORT"
    exit 1
fi

if ! nc -z $HOST $IMAP_PORT 2>/dev/null; then
    echo "❌ IMAP server is NOT running on port $IMAP_PORT"
    echo "   Please start it with: cd mock-email && cargo run -- --smtp-port $SMTP_PORT --imap-port $IMAP_PORT"
    exit 1
fi

echo "✅ Both servers are running!"
echo ""

# Test SMTP
echo "=========================================="
echo "Testing SMTP (Sending Email)"
echo "=========================================="
echo ""

{
  echo "EHLO localhost"
  sleep 0.3
  echo "MAIL FROM:<test-sender@example.com>"
  sleep 0.3
  echo "RCPT TO:<test-recipient@example.com>"
  sleep 0.3
  echo "DATA"
  sleep 0.3
  echo "From: test-sender@example.com"
  echo "To: test-recipient@example.com"
  echo "Subject: Test Email - $(date)"
  echo "Date: $(date -R)"
  echo ""
  echo "This is a test email sent via SMTP."
  echo "Sent at: $(date)"
  echo "."
  sleep 0.3
  echo "QUIT"
} | nc -v $HOST $SMTP_PORT

echo ""
echo "✅ SMTP test completed!"
echo ""

# Wait a moment for email to be stored
sleep 1

# Test IMAP
echo "=========================================="
echo "Testing IMAP (Fetching Emails)"
echo "=========================================="
echo ""

{
  echo "a001 CAPABILITY"
  sleep 0.3
  echo "a002 LOGIN test@example.com password"
  sleep 0.3
  echo "a003 LIST \"\" \"*\""
  sleep 0.3
  echo "a004 SELECT INBOX"
  sleep 0.3
  echo "a005 STATUS INBOX (MESSAGES)"
  sleep 0.3
  echo "a006 SEARCH ALL"
  sleep 0.3
  echo "a007 FETCH 1 (ENVELOPE)"
  sleep 0.3
  echo "a008 FETCH 1 BODY[TEXT]"
  sleep 0.3
  echo "a009 LOGOUT"
} | nc -v $HOST $IMAP_PORT

echo ""
echo "✅ IMAP test completed!"
echo ""
echo "=========================================="
echo "All tests completed!"
echo "=========================================="
