#!/usr/bin/env python3
"""
Inspect the encryption format of specific DMs to diagnose the NIP-04 vs NIP-44 issue.
"""

import sqlite3
import os
from pathlib import Path
from datetime import datetime
import base64
import re

# Event IDs of the messages that failed to decrypt (from the conversation)
FAILED_DM_EVENT_IDS = [
    "1d359a54571f3e4d117664ce69142fcc1c5374a8ba199558897a2cfe093a1d93",  # DM #1 - "Sending from amethyst"
    "519f57788489e9ec420dc528fb85127f2da5b1969f2889abfc6ebcef9c4db645",  # DM #2 - "It's on the zapstore"
]

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

def detect_encryption_format(content):
    """Detect if content is NIP-04 or NIP-44 format."""
    if not content:
        return "UNKNOWN", {}
    
    info = {}
    
    # Check for NIP-04 format: base64?iv=base64
    nip04_pattern = r'^[A-Za-z0-9+/=]+\?iv=[A-Za-z0-9+/=]+$'
    if re.match(nip04_pattern, content):
        info['format'] = 'NIP-04'
        parts = content.split('?iv=')
        if len(parts) == 2:
            info['encrypted_data'] = parts[0]
            info['iv'] = parts[1]
            try:
                info['encrypted_data_length'] = len(base64.b64decode(parts[0]))
                info['iv_length'] = len(base64.b64decode(parts[1]))
            except:
                pass
        return "NIP-04", info
    
    # Check for NIP-44 format: versioned format
    # NIP-44 format starts with a version byte (usually 0x02 for v2)
    # It's base64 encoded and when decoded, the first byte indicates version
    try:
        decoded = base64.b64decode(content)
        if len(decoded) > 0:
            version_byte = decoded[0]
            # NIP-44 v2 uses version 2 (0x02)
            if version_byte == 2:
                info['format'] = 'NIP-44'
                info['version'] = version_byte
                info['total_length'] = len(decoded)
                return "NIP-44", info
            # NIP-44 v1 uses version 1 (0x01)
            elif version_byte == 1:
                info['format'] = 'NIP-44'
                info['version'] = version_byte
                info['total_length'] = len(decoded)
                return "NIP-44", info
    except:
        pass
    
    # Check if it's just base64 (could be either, but likely NIP-44 if no ?iv=)
    base64_pattern = r'^[A-Za-z0-9+/=]+$'
    if re.match(base64_pattern, content):
        try:
            decoded = base64.b64decode(content)
            if len(decoded) > 0:
                version_byte = decoded[0]
                if version_byte in [1, 2]:
                    info['format'] = 'NIP-44'
                    info['version'] = version_byte
                    info['total_length'] = len(decoded)
                    return "NIP-44", info
                else:
                    info['format'] = 'UNKNOWN (base64 but not NIP-44 versioned)'
                    info['first_byte'] = version_byte
                    info['total_length'] = len(decoded)
                    return "UNKNOWN", info
        except:
            pass
    
    return "UNKNOWN", {'raw_content_preview': content[:100]}

def inspect_dm(conn, event_id):
    """Inspect a specific DM by event ID."""
    cursor = conn.cursor()
    cursor.execute("""
        SELECT 
            id, event_id, sender_pubkey, recipient_pubkey, content,
            created_at, received_at
        FROM direct_messages
        WHERE event_id = ?
    """, (event_id,))
    
    row = cursor.fetchone()
    if not row:
        return None
    
    return {
        'id': row[0],
        'event_id': row[1],
        'sender_pubkey': row[2],
        'recipient_pubkey': row[3],
        'content': row[4],
        'created_at': row[5],
        'received_at': row[6],
    }

def main():
    print("=" * 80)
    print("INSPECTING DM ENCRYPTION FORMAT")
    print("=" * 80)
    print()
    
    # Find database
    try:
        db_path = find_database()
    except FileNotFoundError as e:
        print(f"Error: {e}")
        return 1
    
    # Connect to database
    try:
        conn = sqlite3.connect(db_path)
        conn.row_factory = sqlite3.Row
    except Exception as e:
        print(f"Error connecting to database: {e}")
        return 1
    
    try:
        print(f"Inspecting {len(FAILED_DM_EVENT_IDS)} failed DM(s):\n")
        
        for i, event_id in enumerate(FAILED_DM_EVENT_IDS, 1):
            print(f"{'=' * 80}")
            print(f"DM #{i} - Event ID: {event_id}")
            print(f"{'=' * 80}")
            
            dm = inspect_dm(conn, event_id)
            if not dm:
                print(f"  ❌ DM not found in database!")
                print()
                continue
            
            print(f"  Database ID: {dm['id']}")
            print(f"  Sender: {dm['sender_pubkey']}")
            print(f"  Recipient: {dm['recipient_pubkey']}")
            print(f"  Created: {dm['created_at']}")
            print(f"  Received: {dm['received_at']}")
            print()
            
            content = dm['content']
            print(f"  Content Length: {len(content)} characters")
            print(f"  Content Preview: {content[:150]}...")
            print()
            
            # Detect encryption format
            format_type, info = detect_encryption_format(content)
            print(f"  🔍 Detected Format: {format_type}")
            print(f"  Format Details:")
            for key, value in info.items():
                print(f"    - {key}: {value}")
            print()
            
            # Additional analysis
            print(f"  📊 Additional Analysis:")
            print(f"    - Contains '?iv=': {'Yes' if '?iv=' in content else 'No'}")
            print(f"    - Is base64-like: {bool(re.match(r'^[A-Za-z0-9+/=]+$', content))}")
            
            # Try to decode and inspect first bytes
            try:
                decoded = base64.b64decode(content)
                print(f"    - Decoded length: {len(decoded)} bytes")
                if len(decoded) > 0:
                    print(f"    - First byte (hex): 0x{decoded[0]:02x}")
                    print(f"    - First byte (decimal): {decoded[0]}")
                    if len(decoded) > 1:
                        print(f"    - First 16 bytes (hex): {decoded[:16].hex()}")
            except Exception as e:
                print(f"    - Could not decode as base64: {e}")
            
            print()
        
        # Also check all recent DMs to compare
        print(f"{'=' * 80}")
        print("COMPARING WITH OTHER RECENT DMs")
        print(f"{'=' * 80}")
        print()
        
        cursor = conn.cursor()
        cursor.execute("""
            SELECT event_id, content, created_at
            FROM direct_messages
            WHERE recipient_pubkey = 'npub15lmjekxgcl8337n0gnqnrcqatwyv9arhywjkvfh0x0tfjrn2nu2sgz2796'
            ORDER BY created_at DESC
            LIMIT 10
        """)
        
        all_dms = cursor.fetchall()
        print(f"Found {len(all_dms)} recent DM(s) to compare:\n")
        
        for i, row in enumerate(all_dms, 1):
            event_id = row[0]
            content = row[1]
            created_at = row[2]
            
            format_type, info = detect_encryption_format(content)
            is_failed = event_id in FAILED_DM_EVENT_IDS
            
            status = "❌ FAILED" if is_failed else "✅ OK"
            print(f"  {status} DM #{i} ({format_type})")
            print(f"    Event ID: {event_id[:16]}...")
            print(f"    Created: {created_at}")
            print(f"    Content length: {len(content)}")
            if 'version' in info:
                print(f"    NIP-44 version: {info['version']}")
            print()
        
    except Exception as e:
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
        return 1
    finally:
        conn.close()
    
    return 0

if __name__ == "__main__":
    exit(main())
