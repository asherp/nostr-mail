#!/usr/bin/env python3
"""
Query script to find all messages sent to a specific npub in the nostr-mail database.
"""

import sqlite3
import os
from pathlib import Path
from datetime import datetime
import json

TARGET_NPUB = "npub15lmjekxgcl8337n0gnqnrcqatwyv9arhywjkvfh0x0tfjrn2nu2sgz2796"

def find_database():
    """Find the database file in common locations."""
    possible_paths = [
        # macOS
        Path.home() / "Library" / "Application Support" / "nostr-mail" / "nostr_mail.db",
        # Linux
        Path.home() / ".config" / "nostr-mail" / "nostr_mail.db",
        # Windows
        Path(os.environ.get("APPDATA", "")) / "NostrMail" / "nostr_mail.db" if os.environ.get("APPDATA") else None,
        # Custom path from environment variable
        Path(os.environ.get("NOSTR_MAIL_DB")) if os.environ.get("NOSTR_MAIL_DB") else None,
    ]
    
    # Filter out None values
    possible_paths = [p for p in possible_paths if p is not None]
    
    for db_path in possible_paths:
        if db_path.exists():
            print(f"Found database at: {db_path}")
            return db_path
    
    # If not found, try to find it in the current directory or subdirectories
    current_dir = Path.cwd()
    for db_file in current_dir.rglob("nostr_mail.db"):
        print(f"Found database at: {db_file}")
        return db_file
    
    raise FileNotFoundError(
        f"Could not find nostr_mail.db database. Tried:\n" + 
        "\n".join(f"  - {p}" for p in possible_paths)
    )

def query_emails(conn, npub):
    """Query emails table for messages sent to the specified npub."""
    cursor = conn.cursor()
    cursor.execute("""
        SELECT 
            id, message_id, from_address, to_address, subject, 
            body, body_plain, body_html, received_at, is_nostr_encrypted,
            sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read,
            updated_at, created_at, signature_valid, transport_auth_verified
        FROM emails
        WHERE recipient_pubkey = ?
        ORDER BY received_at DESC
    """, (npub,))
    
    return cursor.fetchall()

def query_direct_messages(conn, npub):
    """Query direct_messages table for messages sent to the specified npub."""
    cursor = conn.cursor()
    cursor.execute("""
        SELECT 
            id, event_id, sender_pubkey, recipient_pubkey, content,
            created_at, received_at
        FROM direct_messages
        WHERE recipient_pubkey = ?
        ORDER BY created_at DESC
    """, (npub,))
    
    return cursor.fetchall()

def format_datetime(dt_str):
    """Format datetime string for display."""
    if dt_str:
        try:
            dt = datetime.fromisoformat(dt_str.replace('Z', '+00:00'))
            return dt.strftime('%Y-%m-%d %H:%M:%S UTC')
        except:
            return str(dt_str)
    return "N/A"

def main():
    print(f"Searching for messages sent to: {TARGET_NPUB}\n")
    print("=" * 80)
    
    # Find database
    try:
        db_path = find_database()
    except FileNotFoundError as e:
        print(f"Error: {e}")
        print("\nYou can specify a custom database path using the NOSTR_MAIL_DB environment variable:")
        print("  export NOSTR_MAIL_DB=/path/to/nostr_mail.db")
        return 1
    
    # Connect to database
    try:
        conn = sqlite3.connect(db_path)
        conn.row_factory = sqlite3.Row  # Enable column access by name
    except Exception as e:
        print(f"Error connecting to database: {e}")
        return 1
    
    try:
        # Query emails
        print("\n📧 EMAILS:")
        print("-" * 80)
        email_rows = query_emails(conn, TARGET_NPUB)
        
        if email_rows:
            print(f"Found {len(email_rows)} email(s):\n")
            for i, row in enumerate(email_rows, 1):
                print(f"Email #{i}:")
                print(f"  ID: {row['id']}")
                print(f"  Message ID: {row['message_id']}")
                print(f"  From: {row['from_address']}")
                print(f"  To: {row['to_address']}")
                print(f"  Subject: {row['subject']}")
                print(f"  Sender Pubkey: {row['sender_pubkey'] or 'N/A'}")
                print(f"  Recipient Pubkey: {row['recipient_pubkey']}")
                print(f"  Received At: {format_datetime(row['received_at'])}")
                print(f"  Created At: {format_datetime(row['created_at'])}")
                print(f"  Is Nostr Encrypted: {bool(row['is_nostr_encrypted'])}")
                print(f"  Is Draft: {bool(row['is_draft'])}")
                print(f"  Is Read: {bool(row['is_read'])}")
                if row['signature_valid'] is not None:
                    print(f"  Signature Valid: {bool(row['signature_valid'])}")
                if row['transport_auth_verified'] is not None:
                    print(f"  Transport Auth Verified: {bool(row['transport_auth_verified'])}")
                print(f"  Body Preview: {row['body'][:100]}..." if len(row['body']) > 100 else f"  Body: {row['body']}")
                print()
        else:
            print("No emails found.\n")
        
        # Query direct messages
        print("\n💬 DIRECT MESSAGES:")
        print("-" * 80)
        dm_rows = query_direct_messages(conn, TARGET_NPUB)
        
        if dm_rows:
            print(f"Found {len(dm_rows)} direct message(s):\n")
            for i, row in enumerate(dm_rows, 1):
                print(f"DM #{i}:")
                print(f"  ID: {row['id']}")
                print(f"  Event ID: {row['event_id']}")
                print(f"  Sender Pubkey: {row['sender_pubkey']}")
                print(f"  Recipient Pubkey: {row['recipient_pubkey']}")
                print(f"  Created At: {format_datetime(row['created_at'])}")
                print(f"  Received At: {format_datetime(row['received_at'])}")
                print(f"  Content Preview: {row['content'][:100]}..." if len(row['content']) > 100 else f"  Content: {row['content']}")
                print()
        else:
            print("No direct messages found.\n")
        
        # Summary
        total = len(email_rows) + len(dm_rows)
        print("=" * 80)
        print(f"\nSUMMARY:")
        print(f"  Total emails: {len(email_rows)}")
        print(f"  Total direct messages: {len(dm_rows)}")
        print(f"  Total messages: {total}")
        
    except Exception as e:
        print(f"Error querying database: {e}")
        import traceback
        traceback.print_exc()
        return 1
    finally:
        conn.close()
    
    return 0

if __name__ == "__main__":
    exit(main())
