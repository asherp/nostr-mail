#!/usr/bin/env python3
"""
Test script for mock email server using Python's smtplib and imaplib
"""

import smtplib
import imaplib
import socket
import sys
from email.mime.text import MIMEText
from datetime import datetime

SMTP_HOST = "127.0.0.1"
SMTP_PORT = 2525
IMAP_HOST = "127.0.0.1"
IMAP_PORT = 1143

def test_smtp_connection():
    """Test SMTP server connection"""
    print("=" * 50)
    print("Testing SMTP Server")
    print("=" * 50)
    
    try:
        # Check if port is open
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        result = sock.connect_ex((SMTP_HOST, SMTP_PORT))
        sock.close()
        
        if result != 0:
            print(f"❌ Cannot connect to SMTP server at {SMTP_HOST}:{SMTP_PORT}")
            print("   Make sure the server is running!")
            return False
        
        print(f"✅ SMTP server is reachable at {SMTP_HOST}:{SMTP_PORT}")
        
        # Connect to SMTP server
        print("\nConnecting to SMTP server...")
        server = smtplib.SMTP(SMTP_HOST, SMTP_PORT)
        server.set_debuglevel(1)  # Show SMTP conversation
        
        # Send email
        print("\nSending test email...")
        msg = MIMEText("This is a test email sent via Python smtplib.\n\nSent at: " + str(datetime.now()))
        msg['From'] = "test-sender@example.com"
        msg['To'] = "test-recipient@example.com"
        msg['Subject'] = f"Test Email - {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}"
        
        server.sendmail("test-sender@example.com", ["test-recipient@example.com"], msg.as_string())
        server.quit()
        
        print("✅ Email sent successfully!")
        return True
        
    except Exception as e:
        print(f"❌ SMTP test failed: {e}")
        return False

def test_imap_connection():
    """Test IMAP server connection"""
    print("\n" + "=" * 50)
    print("Testing IMAP Server")
    print("=" * 50)
    
    try:
        # Check if port is open
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        result = sock.connect_ex((IMAP_HOST, IMAP_PORT))
        sock.close()
        
        if result != 0:
            print(f"❌ Cannot connect to IMAP server at {IMAP_HOST}:{IMAP_PORT}")
            print("   Make sure the server is running!")
            return False
        
        print(f"✅ IMAP server is reachable at {IMAP_HOST}:{IMAP_PORT}")
        
        # Connect to IMAP server
        print("\nConnecting to IMAP server...")
        server = imaplib.IMAP4(IMAP_HOST, IMAP_PORT)
        server.debug = 4  # Show IMAP conversation
        
        # Login (mock server accepts any credentials)
        print("\nLogging in...")
        server.login("test@example.com", "password")
        print("✅ Login successful!")
        
        # List mailboxes
        print("\nListing mailboxes...")
        status, mailboxes = server.list()
        if status == 'OK':
            print(f"✅ Found {len(mailboxes)} mailboxes:")
            for mailbox in mailboxes:
                print(f"   - {mailbox.decode()}")
        
        # Select INBOX
        print("\nSelecting INBOX...")
        status, messages = server.select("INBOX")
        if status == 'OK':
            num_messages = int(messages[0])
            print(f"✅ INBOX selected. Found {num_messages} message(s)")
        
        # Search for all emails
        print("\nSearching for emails...")
        status, email_ids = server.search(None, "ALL")
        if status == 'OK':
            email_ids = email_ids[0].split()
            print(f"✅ Found {len(email_ids)} email(s)")
            
            # Fetch first email if available
            if email_ids:
                print(f"\nFetching first email (ID: {email_ids[0].decode()})...")
                status, msg_data = server.fetch(email_ids[0], "(RFC822 ENVELOPE)")
                if status == 'OK':
                    print("✅ Email fetched successfully!")
                    print(f"\nEmail data preview:")
                    print("-" * 50)
                    # Print first 500 chars of email
                    email_body = msg_data[0][1]
                    if isinstance(email_body, bytes):
                        preview = email_body[:500].decode('utf-8', errors='ignore')
                        print(preview)
                        if len(email_body) > 500:
                            print("... (truncated)")
        
        # Logout
        server.logout()
        print("\n✅ IMAP test completed successfully!")
        return True
        
    except Exception as e:
        print(f"❌ IMAP test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def main():
    print("\n" + "=" * 50)
    print("Mock Email Server Test Suite")
    print("=" * 50)
    print(f"SMTP: {SMTP_HOST}:{SMTP_PORT}")
    print(f"IMAP: {IMAP_HOST}:{IMAP_PORT}")
    print()
    
    smtp_ok = test_smtp_connection()
    imap_ok = test_imap_connection()
    
    print("\n" + "=" * 50)
    print("Test Summary")
    print("=" * 50)
    print(f"SMTP: {'✅ PASS' if smtp_ok else '❌ FAIL'}")
    print(f"IMAP: {'✅ PASS' if imap_ok else '❌ FAIL'}")
    
    if smtp_ok and imap_ok:
        print("\n🎉 All tests passed!")
        sys.exit(0)
    else:
        print("\n⚠️  Some tests failed. Check the output above for details.")
        sys.exit(1)

if __name__ == "__main__":
    main()
