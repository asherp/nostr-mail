#!/bin/bash
# Test IMAP connection to mock-email server
# Usage: ./test_imap.sh

HOST="localhost"
PORT="1143"
USER="alice@example.com"
PASS="password"

echo "Connecting to IMAP server at $HOST:$PORT..."
echo ""

# Connect and send IMAP commands
{
    # Wait for greeting
    sleep 0.1
    
    # LOGIN
    echo "a001 LOGIN $USER $PASS"
    sleep 0.1
    
    # LIST mailboxes
    echo "a002 LIST \"\" \"*\""
    sleep 0.1
    
    # SELECT Sent mailbox
    echo "a003 SELECT Sent"
    sleep 0.1
    
    # SEARCH for all emails
    echo "a004 SEARCH ALL"
    sleep 0.1
    
    # FETCH email 1 with RFC822
    echo "a005 FETCH 1 RFC822"
    sleep 0.2
    
    # LOGOUT
    echo "a006 LOGOUT"
    sleep 0.1
} | nc $HOST $PORT
