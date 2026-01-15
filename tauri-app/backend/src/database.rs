use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use sha2::{Sha256, Digest};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Contact {
    pub id: Option<i64>,
    pub pubkey: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub picture_url: Option<String>,
    pub picture_data_url: Option<String>,
    pub about: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Email {
    pub id: Option<i64>,
    pub message_id: String,
    pub from_address: String,
    pub to_address: String,
    pub subject: String,
    pub body: String,
    pub body_plain: Option<String>,
    pub body_html: Option<String>,
    pub received_at: DateTime<Utc>,
    pub is_nostr_encrypted: bool,
    pub sender_pubkey: Option<String>,
    pub recipient_pubkey: Option<String>,
    pub raw_headers: Option<String>,
    pub is_draft: bool,
    pub is_read: bool,
    pub updated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub signature_valid: Option<bool>,
    pub transport_auth_verified: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DirectMessage {
    pub id: Option<i64>,
    pub event_id: String,
    pub sender_pubkey: String,
    pub recipient_pubkey: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserSettings {
    pub id: Option<i64>,
    pub pubkey: String,
    pub key: String,
    pub value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DbRelay {
    pub id: Option<i64>,
    pub url: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attachment {
    pub id: Option<i64>,
    pub email_id: i64,
    pub filename: String,
    pub content_type: String,
    pub data: String, // Base64 encoded data
    pub size: usize,
    pub is_encrypted: bool,
    pub encryption_method: Option<String>,
    pub algorithm: Option<String>,
    pub original_filename: Option<String>,
    pub original_type: Option<String>,
    pub original_size: Option<usize>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        // Enable WAL mode for better concurrency
        conn.pragma_update(None, "journal_mode", &"WAL")?;
        let db = Database { conn: Arc::new(Mutex::new(conn)) };
        db.init_tables()?;
        db.seed_default_relays()?;
        Ok(db)
    }
    
    /// Seed default relays if the relays table is empty
    fn seed_default_relays(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Check if relays table is empty
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM relays", [], |row| row.get(0))?;
        
        if count == 0 {
            println!("[DB] Seeding default relays...");
            let now = Utc::now();
            let default_relays = vec![
                ("wss://nostr-pub.wellorder.net", true),
                ("wss://nostr.mom", true),
                ("wss://purplepage.es", true),
                ("wss://relay.damus.io", true),
                ("wss://relay.nostr.band", true),
                ("wss://relay.primal.net", true),
                ("wss://relay.weloveit.info", true),
            ];
            
            for (url, is_active) in default_relays {
                conn.execute(
                    "INSERT OR IGNORE INTO relays (url, is_active, created_at, updated_at) VALUES (?, ?, ?, ?)",
                    params![url, is_active, now, now],
                )?;
                println!("[DB] Seeded relay: {} (active: {})", url, is_active);
            }
            println!("[DB] Default relays seeded successfully");
        } else {
            println!("[DB] Relays table already contains {} relay(s), skipping seed", count);
        }
        
        Ok(())
    }

    /// Compute SHA256 hash of encrypted content for fast lookups
    fn compute_content_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Contacts table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS contacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pubkey TEXT UNIQUE NOT NULL,
                name TEXT,
                email TEXT,
                picture_url TEXT,
                picture_data_url TEXT,
                about TEXT,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )",
            [],
        )?;

        // User contacts junction table - tracks which users follow which contacts
        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_contacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_pubkey TEXT NOT NULL,
                contact_pubkey TEXT NOT NULL,
                is_public BOOLEAN NOT NULL DEFAULT 1,
                created_at DATETIME NOT NULL,
                UNIQUE(user_pubkey, contact_pubkey),
                FOREIGN KEY (contact_pubkey) REFERENCES contacts(pubkey) ON DELETE CASCADE
            )",
            [],
        )?;

        // Emails table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS emails (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id TEXT NOT NULL UNIQUE,
                from_address TEXT NOT NULL,
                to_address TEXT NOT NULL,
                subject TEXT NOT NULL,
                body TEXT NOT NULL,
                body_plain TEXT,
                body_html TEXT,
                received_at DATETIME NOT NULL,
                is_nostr_encrypted BOOLEAN NOT NULL DEFAULT 0,
                sender_pubkey TEXT,
                recipient_pubkey TEXT,
                raw_headers TEXT,
                is_draft BOOLEAN NOT NULL DEFAULT 0,
                is_read BOOLEAN NOT NULL DEFAULT 0,
                updated_at DATETIME,
                created_at DATETIME NOT NULL,
                signature_valid BOOLEAN
            )",
            [],
        )?;
        
        
        // Add UNIQUE constraint to message_id if it doesn't exist (for existing databases)
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_emails_message_id_unique ON emails(message_id)",
            [],
        )?;

        // Direct messages table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS direct_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT UNIQUE NOT NULL,
                sender_pubkey TEXT NOT NULL,
                recipient_pubkey TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at DATETIME NOT NULL,
                received_at DATETIME NOT NULL
            )",
            [],
        )?;

        // User settings table - create with pubkey support
        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pubkey TEXT NOT NULL DEFAULT '',
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL,
                UNIQUE(pubkey, key)
            )",
            [],
        )?;
        
        // Migrate existing settings: add pubkey column if it doesn't exist
        // Check if pubkey column exists by querying table_info
        let has_pubkey = match conn.prepare("PRAGMA table_info(user_settings)") {
            Ok(mut check_stmt) => {
                let columns: Result<Vec<String>, _> = check_stmt.query_map([], |row| {
                    Ok(row.get::<_, String>(1)?) // column name
                })?.collect();
                columns.map(|cols| cols.contains(&"pubkey".to_string())).unwrap_or(false)
            },
            Err(_) => {
                // Table doesn't exist yet, no migration needed (will be created with new schema)
                false
            }
        };
        
        if !has_pubkey {
            // Add pubkey column for existing databases
            if let Err(e) = conn.execute(
                "ALTER TABLE user_settings ADD COLUMN pubkey TEXT NOT NULL DEFAULT ''",
                [],
            ) {
                println!("[DB] Warning: Could not add pubkey column (may already exist): {}", e);
            } else {
                // Update existing rows to have empty string pubkey (legacy settings)
                conn.execute(
                    "UPDATE user_settings SET pubkey = '' WHERE pubkey IS NULL OR pubkey = ''",
                    [],
                )?;
            }
        }

        // Relays table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS relays (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT UNIQUE NOT NULL,
                is_active BOOLEAN NOT NULL,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )",
            [],
        )?;

        // Attachments table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS attachments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email_id INTEGER NOT NULL,
                filename TEXT NOT NULL,
                content_type TEXT NOT NULL,
                data TEXT NOT NULL,
                size INTEGER NOT NULL,
                is_encrypted BOOLEAN NOT NULL DEFAULT 0,
                encryption_method TEXT,
                algorithm TEXT,
                original_filename TEXT,
                original_type TEXT,
                original_size INTEGER,
                created_at DATETIME NOT NULL,
                FOREIGN KEY (email_id) REFERENCES emails (id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Migrate: Add hash columns to emails and direct_messages tables if they don't exist
        Self::migrate_add_hash_columns(&conn)?;

        // Migrate: Add user_contacts junction table if it doesn't exist
        Self::migrate_add_user_contacts_table(&conn)?;
        
        // Migrate: Convert nostr_pubkey to sender_pubkey and recipient_pubkey
        Self::migrate_email_pubkey_columns(&conn)?;

        // Migrate: Add signature_valid column to emails table if it doesn't exist
        Self::migrate_add_signature_valid_column(&conn)?;

        // Migrate: Add transport_auth_verified column to emails table if it doesn't exist
        Self::migrate_add_transport_auth_column(&conn)?;

        // Migrate: Remove duplicate settings entries
        Self::migrate_remove_duplicate_settings(&conn)?;

        // Create indexes for better performance
        conn.execute("CREATE INDEX IF NOT EXISTS idx_contacts_pubkey ON contacts(pubkey)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_user_contacts_user_pubkey ON user_contacts(user_pubkey)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_user_contacts_contact_pubkey ON user_contacts(contact_pubkey)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_user_contacts_user_contact ON user_contacts(user_pubkey, contact_pubkey)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_message_id ON emails(message_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_received_at ON emails(received_at)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_subject_hash ON emails(subject_hash)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_event_id ON direct_messages(event_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_created_at ON direct_messages(created_at)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_content_hash ON direct_messages(content_hash)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_settings_key ON user_settings(key)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_settings_pubkey ON user_settings(pubkey)", [])?;
        // Drop old non-unique index if it exists (we're replacing it with a unique one)
        let _ = conn.execute("DROP INDEX IF EXISTS idx_settings_pubkey_key", []);
        // Create UNIQUE index to ensure (pubkey, key) uniqueness
        // Note: migrate_remove_duplicate_settings is called before this in the migration section
        conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_settings_pubkey_key_unique ON user_settings(pubkey, key)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_relays_url ON relays(url)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_attachments_email_id ON attachments(email_id)", [])?;

        Ok(())
    }

    /// Migration: Add hash columns to emails and direct_messages tables
    fn migrate_add_hash_columns(conn: &Connection) -> Result<()> {
        // Check if subject_hash column exists in emails table
        let has_subject_hash = {
            let mut stmt = conn.prepare("PRAGMA table_info(emails)")?;
            let columns: Result<Vec<String>, _> = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(1)?) // column name
            })?.collect();
            columns.map(|cols| cols.contains(&"subject_hash".to_string())).unwrap_or(false)
        };

        if !has_subject_hash {
            println!("[DB] Adding subject_hash column to emails table");
            conn.execute(
                "ALTER TABLE emails ADD COLUMN subject_hash TEXT",
                [],
            )?;
            
            // Backfill hash for existing encrypted emails
            println!("[DB] Backfilling subject_hash for existing encrypted emails");
            let mut stmt = conn.prepare("SELECT id, subject FROM emails WHERE is_nostr_encrypted = 1 AND (subject_hash IS NULL OR subject_hash = '')")?;
            let rows: Result<Vec<(i64, String)>, _> = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?.collect();
            
            if let Ok(rows) = rows {
                let count = rows.len();
                for (id, subject) in rows {
                    let hash = Self::compute_content_hash(&subject);
                    conn.execute(
                        "UPDATE emails SET subject_hash = ? WHERE id = ?",
                        params![hash, id],
                    )?;
                }
                println!("[DB] Backfilled {} email hashes", count);
            }
        }

        // Check if content_hash column exists in direct_messages table
        let has_content_hash = {
            let mut stmt = conn.prepare("PRAGMA table_info(direct_messages)")?;
            let columns: Result<Vec<String>, _> = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(1)?) // column name
            })?.collect();
            columns.map(|cols| cols.contains(&"content_hash".to_string())).unwrap_or(false)
        };

        if !has_content_hash {
            println!("[DB] Adding content_hash column to direct_messages table");
            conn.execute(
                "ALTER TABLE direct_messages ADD COLUMN content_hash TEXT",
                [],
            )?;
            
            // Backfill hash for existing DMs
            println!("[DB] Backfilling content_hash for existing direct messages");
            let mut stmt = conn.prepare("SELECT id, content FROM direct_messages WHERE content_hash IS NULL OR content_hash = ''")?;
            let rows: Result<Vec<(i64, String)>, _> = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?.collect();
            
            if let Ok(rows) = rows {
                let count = rows.len();
                for (id, content) in rows {
                    let hash = Self::compute_content_hash(&content);
                    conn.execute(
                        "UPDATE direct_messages SET content_hash = ? WHERE id = ?",
                        params![hash, id],
                    )?;
                }
                println!("[DB] Backfilled {} DM hashes", count);
            }
        }

        Ok(())
    }

    /// Migration: Add signature_valid column to emails table if it doesn't exist
    fn migrate_add_signature_valid_column(conn: &Connection) -> Result<()> {
        // Check if signature_valid column exists in emails table
        let has_signature_valid = {
            let mut stmt = conn.prepare("PRAGMA table_info(emails)")?;
            let columns: Result<Vec<String>, _> = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(1)?) // column name
            })?.collect();
            columns.map(|cols| cols.contains(&"signature_valid".to_string())).unwrap_or(false)
        };

        if !has_signature_valid {
            println!("[DB] Adding signature_valid column to emails table");
            conn.execute(
                "ALTER TABLE emails ADD COLUMN signature_valid BOOLEAN",
                [],
            )?;
            println!("[DB] Migration complete: signature_valid column added to emails");
        }

        Ok(())
    }

    /// Migration: Add transport_auth_verified column to emails table if it doesn't exist
    fn migrate_add_transport_auth_column(conn: &Connection) -> Result<()> {
        let has_transport_auth = {
            let mut stmt = conn.prepare("PRAGMA table_info(emails)")?;
            let columns: Result<Vec<String>, _> = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(1)?) // column name
            })?.collect();
            columns.map(|cols| cols.contains(&"transport_auth_verified".to_string())).unwrap_or(false)
        };

        if !has_transport_auth {
            println!("[DB] Adding transport_auth_verified column to emails table");
            conn.execute(
                "ALTER TABLE emails ADD COLUMN transport_auth_verified BOOLEAN",
                [],
            )?;
            println!("[DB] Migration complete: transport_auth_verified column added to emails");
        }

        Ok(())
    }

    /// Migration: Remove duplicate settings entries, keeping the most recent one for each (pubkey, key) pair
    fn migrate_remove_duplicate_settings(conn: &Connection) -> Result<()> {
        println!("[DB] Checking for duplicate settings entries");
        
        // Check if there are any duplicates
        let duplicate_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM (
                SELECT pubkey, key, COUNT(*) as cnt 
                FROM user_settings 
                GROUP BY pubkey, key 
                HAVING cnt > 1
            )",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        if duplicate_count > 0 {
            println!("[DB] Found {} duplicate settings entries, removing duplicates...", duplicate_count);
            
            // Delete duplicates, keeping only the most recent entry (highest id) for each (pubkey, key) pair
            let deleted = conn.execute(
                "DELETE FROM user_settings 
                WHERE id NOT IN (
                    SELECT MAX(id) 
                    FROM user_settings 
                    GROUP BY pubkey, key
                )",
                [],
            )?;
            
            println!("[DB] Removed {} duplicate settings entries", deleted);
        } else {
            println!("[DB] No duplicate settings entries found");
        }

        Ok(())
    }

    /// Migration: Add user_contacts junction table if it doesn't exist
    fn migrate_add_user_contacts_table(conn: &Connection) -> Result<()> {
        // Check if user_contacts table exists
        let table_exists = {
            let mut stmt = conn.prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='user_contacts'"
            )?;
            let mut rows = stmt.query([])?;
            rows.next()?.is_some()
        };

        if !table_exists {
            println!("[DB] Creating user_contacts junction table");
            conn.execute(
                "CREATE TABLE user_contacts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    user_pubkey TEXT NOT NULL,
                    contact_pubkey TEXT NOT NULL,
                    is_public BOOLEAN NOT NULL DEFAULT 1,
                    created_at DATETIME NOT NULL,
                    UNIQUE(user_pubkey, contact_pubkey),
                    FOREIGN KEY (contact_pubkey) REFERENCES contacts(pubkey) ON DELETE CASCADE
                )",
                [],
            )?;

            // Create indexes
            conn.execute("CREATE INDEX IF NOT EXISTS idx_user_contacts_user_pubkey ON user_contacts(user_pubkey)", [])?;
            conn.execute("CREATE INDEX IF NOT EXISTS idx_user_contacts_contact_pubkey ON user_contacts(contact_pubkey)", [])?;
            conn.execute("CREATE INDEX IF NOT EXISTS idx_user_contacts_user_contact ON user_contacts(user_pubkey, contact_pubkey)", [])?;

            // For existing databases, migrate all existing contacts to user_contacts
            // We'll create entries for all contacts with an empty user_pubkey as a fallback
            // This ensures existing contacts are still visible until users are properly associated
            println!("[DB] Note: Existing contacts will need to be associated with users via user_contacts table");
        } else {
            // Migration: Add is_public column if it doesn't exist
            let has_is_public = {
                let mut stmt = conn.prepare("PRAGMA table_info(user_contacts)")?;
                let columns: Result<Vec<String>, _> = stmt.query_map([], |row| {
                    Ok(row.get::<_, String>(1)?) // column name
                })?.collect();
                columns.map(|cols| cols.contains(&"is_public".to_string())).unwrap_or(false)
            };

            if !has_is_public {
                println!("[DB] Adding is_public column to user_contacts table");
                conn.execute(
                    "ALTER TABLE user_contacts ADD COLUMN is_public BOOLEAN NOT NULL DEFAULT 1",
                    [],
                )?;
                println!("[DB] Migration complete: is_public column added to user_contacts");
            }
        }

        Ok(())
    }

    /// Migration: Convert nostr_pubkey to sender_pubkey and recipient_pubkey
    fn migrate_email_pubkey_columns(conn: &Connection) -> Result<()> {
        // Check if sender_pubkey column exists
        let has_sender_pubkey = {
            let mut stmt = conn.prepare("PRAGMA table_info(emails)")?;
            let columns: Result<Vec<String>, _> = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(1)?) // column name
            })?.collect();
            columns.map(|cols| cols.contains(&"sender_pubkey".to_string())).unwrap_or(false)
        };

        if !has_sender_pubkey {
            println!("[DB] Migrating email pubkey columns: adding sender_pubkey and recipient_pubkey");
            
            // Add new columns
            conn.execute(
                "ALTER TABLE emails ADD COLUMN sender_pubkey TEXT",
                [],
            )?;
            conn.execute(
                "ALTER TABLE emails ADD COLUMN recipient_pubkey TEXT",
                [],
            )?;
            
            // Migrate existing nostr_pubkey data to sender_pubkey
            // For inbox emails (is_draft = 0), nostr_pubkey is the sender's pubkey
            // For sent emails, we'd need to determine recipient_pubkey from contacts
            // For now, migrate all nostr_pubkey values to sender_pubkey
            println!("[DB] Migrating existing nostr_pubkey values to sender_pubkey");
            conn.execute(
                "UPDATE emails SET sender_pubkey = nostr_pubkey WHERE nostr_pubkey IS NOT NULL AND nostr_pubkey != ''",
                [],
            )?;
            
            // Drop the old nostr_pubkey column (SQLite doesn't support DROP COLUMN directly)
            // Instead, we'll create a new table and copy data
            println!("[DB] Removing old nostr_pubkey column");
            conn.execute(
                "CREATE TABLE emails_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    message_id TEXT NOT NULL UNIQUE,
                    from_address TEXT NOT NULL,
                    to_address TEXT NOT NULL,
                    subject TEXT NOT NULL,
                    body TEXT NOT NULL,
                    body_plain TEXT,
                    body_html TEXT,
                    received_at DATETIME NOT NULL,
                    is_nostr_encrypted BOOLEAN NOT NULL DEFAULT 0,
                    sender_pubkey TEXT,
                    recipient_pubkey TEXT,
                    raw_headers TEXT,
                    is_draft BOOLEAN NOT NULL DEFAULT 0,
                    is_read BOOLEAN NOT NULL DEFAULT 0,
                    updated_at DATETIME,
                    created_at DATETIME NOT NULL,
                    subject_hash TEXT,
                    signature_valid BOOLEAN
                )",
                [],
            )?;
            
            // Copy data from old table to new table
            // Note: signature_valid will be NULL for existing emails, which is fine
            conn.execute(
                "INSERT INTO emails_new SELECT 
                    id, message_id, from_address, to_address, subject, body, body_plain, body_html,
                    received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers,
                    is_draft, is_read, updated_at, created_at, subject_hash, NULL
                FROM emails",
                [],
            )?;
            
            // Drop old table and rename new table
            conn.execute("DROP TABLE emails", [])?;
            conn.execute("ALTER TABLE emails_new RENAME TO emails", [])?;
            
            // Recreate indexes
            conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_emails_message_id_unique ON emails(message_id)", [])?;
            conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_message_id ON emails(message_id)", [])?;
            conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_received_at ON emails(received_at)", [])?;
            
            println!("[DB] Migration complete: email pubkey columns migrated");
        }

        Ok(())
    }

    // Contact operations
    pub fn save_contact(&self, contact: &Contact) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        conn.execute(
            "INSERT INTO contacts (pubkey, name, email, picture_url, picture_data_url, about, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(pubkey) DO UPDATE SET
                name = excluded.name,
                email = excluded.email,
                picture_url = excluded.picture_url,
                picture_data_url = excluded.picture_data_url,
                about = excluded.about,
                updated_at = excluded.updated_at",
            params![
                contact.pubkey, contact.name, contact.email, contact.picture_url,
                contact.picture_data_url, contact.about, now, now
            ],
        )?;
        // Return the rowid of the upserted/updated contact
        let mut stmt = conn.prepare("SELECT id FROM contacts WHERE pubkey = ?")?;
        let mut rows = stmt.query(params![contact.pubkey])?;
        if let Some(row) = rows.next()? {
            Ok(row.get(0)?)
        } else {
            Err(rusqlite::Error::QueryReturnedNoRows)
        }
    }

    /// Add a user-contact relationship (user follows contact)
    pub fn add_user_contact(&self, user_pubkey: &str, contact_pubkey: &str, is_public: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        conn.execute(
            "INSERT OR REPLACE INTO user_contacts (user_pubkey, contact_pubkey, is_public, created_at)
            VALUES (?, ?, ?, ?)",
            params![user_pubkey, contact_pubkey, is_public, now],
        )?;
        Ok(())
    }

    /// Remove a user-contact relationship (user unfollows contact)
    pub fn remove_user_contact(&self, user_pubkey: &str, contact_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM user_contacts WHERE user_pubkey = ? AND contact_pubkey = ?",
            params![user_pubkey, contact_pubkey],
        )?;
        Ok(())
    }

    /// Check if a user follows a contact
    pub fn user_follows_contact(&self, user_pubkey: &str, contact_pubkey: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT 1 FROM user_contacts WHERE user_pubkey = ? AND contact_pubkey = ? LIMIT 1"
        )?;
        let mut rows = stmt.query(params![user_pubkey, contact_pubkey])?;
        Ok(rows.next()?.is_some())
    }

    /// Get all public contact pubkeys for a user
    pub fn get_public_contact_pubkeys(&self, user_pubkey: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT contact_pubkey FROM user_contacts WHERE user_pubkey = ? AND is_public = 1"
        )?;
        let rows = stmt.query_map(params![user_pubkey], |row| {
            Ok(row.get::<_, String>(0)?)
        })?;
        rows.collect()
    }

    /// Update is_public status for a user-contact relationship
    pub fn update_user_contact_public_status(&self, user_pubkey: &str, contact_pubkey: &str, is_public: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE user_contacts SET is_public = ? WHERE user_pubkey = ? AND contact_pubkey = ?",
            params![is_public, user_pubkey, contact_pubkey],
        )?;
        Ok(())
    }

    pub fn get_contact(&self, pubkey: &str) -> Result<Option<Contact>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, pubkey, name, email, picture_url, picture_data_url, about, created_at, updated_at
             FROM contacts WHERE pubkey = ?"
        )?;
        
        let mut rows = stmt.query(params![pubkey])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Contact {
                id: Some(row.get(0)?),
                pubkey: row.get(1)?,
                name: row.get(2)?,
                email: row.get(3)?,
                picture_url: row.get(4)?,
                picture_data_url: row.get(5)?,
                about: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                is_public: None, // Not available without user context
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_contacts(&self, user_pubkey: &str) -> Result<Vec<Contact>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.pubkey, c.name, c.email, c.picture_url, c.picture_data_url, c.about, c.created_at, c.updated_at, uc.is_public
             FROM contacts c
             INNER JOIN user_contacts uc ON c.pubkey = uc.contact_pubkey
             WHERE uc.user_pubkey = ?
             ORDER BY c.name COLLATE NOCASE"
        )?;
        
        let rows = stmt.query_map(params![user_pubkey], |row| {
            Ok(Contact {
                id: Some(row.get(0)?),
                pubkey: row.get(1)?,
                name: row.get(2)?,
                email: row.get(3)?,
                picture_url: row.get(4)?,
                picture_data_url: row.get(5)?,
                about: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                is_public: Some(row.get(9)?),
            })
        })?;
        
        rows.collect()
    }

    pub fn delete_contact(&self, pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM contacts WHERE pubkey = ?", params![pubkey])?;
        Ok(())
    }

    // Email operations
    pub fn save_email(&self, email: &Email) -> Result<i64> {
        println!("[DB] save_email: Starting save for message_id={}, has_id={}", 
            email.message_id, email.id.is_some());
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        // Compute hash for encrypted emails
        let subject_hash = if email.is_nostr_encrypted {
            Some(Self::compute_content_hash(&email.subject))
        } else {
            None
        };

        if let Some(id) = email.id {
            println!("[DB] save_email: Updating existing email with id={}", id);
            conn.execute(
                "UPDATE emails SET 
                    message_id = ?, from_address = ?, to_address = ?, subject = ?, 
                    body = ?, body_plain = ?, body_html = ?, received_at = ?, 
                    is_nostr_encrypted = ?, sender_pubkey = ?, recipient_pubkey = ?, raw_headers = ?, is_draft = ?, is_read = ?, updated_at = ?,
                    subject_hash = ?, signature_valid = ?, transport_auth_verified = ?
                WHERE id = ?",
                params![
                    email.message_id, email.from_address, email.to_address, email.subject,
                    email.body, email.body_plain, email.body_html, email.received_at,
                    email.is_nostr_encrypted, email.sender_pubkey, email.recipient_pubkey, email.raw_headers, email.is_draft, email.is_read, now,
                    subject_hash, email.signature_valid, email.transport_auth_verified, id
                ],
            )?;
            println!("[DB] save_email: Successfully updated email id={}", id);
            Ok(id)
        } else {
            println!("[DB] save_email: Checking if email with message_id {} already exists", email.message_id);
            // Check if email with this message_id already exists (normalized comparison)
            // Use inline check to avoid deadlock (don't call get_email which would try to acquire lock again)
            let normalized_id = Self::normalize_message_id(&email.message_id);
            let existing_id: Option<i64> = {
                let mut stmt = conn.prepare(
                    "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at
                     FROM emails
                     WHERE TRIM(REPLACE(REPLACE(message_id, '<', ''), '>', '')) = ?
                     ORDER BY received_at DESC
                     LIMIT 1"
                )?;
                
                let mut rows = stmt.query(params![normalized_id])?;
                
                if let Some(row) = rows.next()? {
                    Some(row.get(0)?)
                } else {
                    None
                }
            };
            
            if let Some(existing_id) = existing_id {
                // Email already exists, update it instead of creating duplicate
                println!("[DB] Email with message_id {} already exists (id: {:?}), updating instead of creating duplicate", email.message_id, existing_id);
                drop(conn); // Release lock before recursive call
                return self.save_email(&Email {
                    id: Some(existing_id),
                    ..email.clone()
                });
            }
            
            // Email is new, insert it
            println!("[DB] save_email: Email is new, inserting into database");
            println!("[DB] save_email: About to execute INSERT statement");
            match conn.execute(
                "INSERT INTO emails (message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, created_at, subject_hash, signature_valid, transport_auth_verified)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    email.message_id, email.from_address, email.to_address, email.subject,
                    email.body, email.body_plain, email.body_html, email.received_at,
                    email.is_nostr_encrypted, email.sender_pubkey, email.recipient_pubkey, email.raw_headers, email.is_draft, email.is_read, now,
                    subject_hash, email.signature_valid, email.transport_auth_verified
                ],
            ) {
                Ok(rows_affected) => {
                    let new_id = conn.last_insert_rowid();
                    println!("[DB] save_email: Successfully inserted email, rows_affected={}, new_id={}", rows_affected, new_id);
                    Ok(new_id)
                }
                Err(e) => {
                    println!("[DB] save_email: ERROR executing INSERT: {}", e);
                    Err(e)
                }
            }
        }
    }
    
    // Insert email directly without checking for duplicates (faster when we already know it's new)
    pub fn insert_email_direct(&self, email: &Email) -> Result<i64> {
        println!("[DB] insert_email_direct: Inserting email directly without duplicate check, message_id={}", email.message_id);
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        // Compute hash for encrypted emails
        let subject_hash = if email.is_nostr_encrypted {
            Some(Self::compute_content_hash(&email.subject))
        } else {
            None
        };
        
        println!("[DB] insert_email_direct: About to execute INSERT statement");
        match conn.execute(
            "INSERT INTO emails (message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, created_at, subject_hash, signature_valid, transport_auth_verified)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                email.message_id, email.from_address, email.to_address, email.subject,
                email.body, email.body_plain, email.body_html, email.received_at,
                email.is_nostr_encrypted, email.sender_pubkey, email.recipient_pubkey, email.raw_headers, email.is_draft, email.is_read, now,
                subject_hash, email.signature_valid, email.transport_auth_verified
            ],
        ) {
            Ok(rows_affected) => {
                let new_id = conn.last_insert_rowid();
                println!("[DB] insert_email_direct: Successfully inserted email, rows_affected={}, new_id={}", rows_affected, new_id);
                Ok(new_id)
            }
            Err(e) => {
                println!("[DB] insert_email_direct: ERROR executing INSERT: {}", e);
                // If it's a unique constraint violation, the email already exists - fall back to save_email
                if e.to_string().contains("UNIQUE constraint") || e.to_string().contains("unique") {
                    println!("[DB] insert_email_direct: Unique constraint violation, falling back to save_email");
                    drop(conn);
                    return self.save_email(email);
                }
                Err(e)
            }
        }
    }
    
    // Normalize message_id for comparison (remove angle brackets and whitespace)
    fn normalize_message_id(message_id: &str) -> String {
        message_id.trim().trim_start_matches('<').trim_end_matches('>').to_string()
    }

    pub fn get_email(&self, message_id: &str) -> Result<Option<Email>> {
        println!("[DB] get_email: Checking for message_id={}", message_id);
        let conn = self.conn.lock().unwrap();
        println!("[DB] get_email: Acquired database lock");
        // Normalize message_id for comparison
        let normalized_id = Self::normalize_message_id(message_id);
        println!("[DB] get_email: Normalized message_id={}", normalized_id);
        
        // Use SQL query with normalization to find matching email efficiently
        // SQLite's TRIM and REPLACE can handle the normalization
        // We'll try multiple formats: exact match, normalized match, and with/without angle brackets
        let mut stmt = conn.prepare(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at, signature_valid, transport_auth_verified
             FROM emails 
             WHERE TRIM(REPLACE(REPLACE(message_id, '<', ''), '>', '')) = ? 
             ORDER BY received_at DESC"
        )?;
        println!("[DB] get_email: Prepared optimized query with normalization");
        
        let mut rows = stmt.query(params![normalized_id])?;
        
        // Get the first matching email (most recent if multiple)
        if let Some(row) = rows.next()? {
            let email = Email {
                id: Some(row.get(0)?),
                message_id: row.get(1)?,
                from_address: row.get(2)?,
                to_address: row.get(3)?,
                subject: row.get(4)?,
                body: row.get(5)?,
                body_plain: row.get(6)?,
                body_html: row.get(7)?,
                received_at: row.get(8)?,
                is_nostr_encrypted: row.get(9)?,
                sender_pubkey: row.get(10)?,
                recipient_pubkey: row.get(11)?,
                raw_headers: row.get(12)?,
                is_draft: row.get(13)?,
                is_read: row.get(14)?,
                updated_at: row.get(15)?,
                created_at: row.get(16)?,
                signature_valid: row.get(17)?,
                transport_auth_verified: row.get(18)?,
            };
            println!("[DB] get_email: Found matching email, id={:?}", email.id);
            return Ok(Some(email));
        }
        
        println!("[DB] get_email: No matching email found");
        Ok(None)
    }

    pub fn get_emails(&self, limit: Option<i64>, offset: Option<i64>, nostr_only: Option<bool>, user_email: Option<&str>) -> Result<Vec<Email>> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let mut query = String::from(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at, signature_valid, transport_auth_verified FROM emails"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut where_clauses = Vec::new();
        if let Some(nostr_only) = nostr_only {
            where_clauses.push("is_nostr_encrypted = ?");
            params.push(Box::new(nostr_only));
        }
        if let Some(email) = user_email {
            where_clauses.push("LOWER(TRIM(to_address)) = LOWER(TRIM(?))");
            params.push(Box::new(email));
        }
        if !where_clauses.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&where_clauses.join(" AND "));
        }
        query.push_str(" ORDER BY received_at DESC LIMIT ? OFFSET ?");
        params.push(Box::new(limit));
        params.push(Box::new(offset));

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(Email {
                id: Some(row.get(0)?),
                message_id: row.get(1)?,
                from_address: row.get(2)?,
                to_address: row.get(3)?,
                subject: row.get(4)?,
                body: row.get(5)?,
                body_plain: row.get(6)?,
                body_html: row.get(7)?,
                received_at: row.get(8)?,
                is_nostr_encrypted: row.get(9)?,
                sender_pubkey: row.get(10)?,
                recipient_pubkey: row.get(11)?,
                raw_headers: row.get(12)?,
                is_draft: row.get(13)?,
                is_read: row.get(14)?,
                updated_at: row.get(15)?,
                created_at: row.get(16)?,
                signature_valid: row.get(17)?,
                transport_auth_verified: row.get(18)?,
            })
        })?;
        rows.collect()
    }

    // Normalize Gmail address by removing + aliases (e.g., user+alias@gmail.com -> user@gmail.com)
    fn normalize_gmail_address(email: &str) -> String {
        let email_lower = email.trim().to_lowercase();
        if email_lower.contains("@gmail.com") {
            if let Some(at_pos) = email_lower.find('@') {
                let local_part = &email_lower[..at_pos];
                let domain = &email_lower[at_pos..];
                // Remove everything after + if present
                let normalized_local = if let Some(plus_pos) = local_part.find('+') {
                    &local_part[..plus_pos]
                } else {
                    local_part
                };
                return format!("{}{}", normalized_local, domain);
            }
        }
        email_lower
    }

    pub fn get_sent_emails(&self, limit: Option<i64>, offset: Option<i64>, user_email: Option<&str>) -> Result<Vec<Email>> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let mut query = String::from(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at, signature_valid, transport_auth_verified FROM emails"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut where_clauses = Vec::new();
        
        // Always exclude drafts from sent emails
        where_clauses.push("is_draft = 0");
        
        if let Some(email) = user_email {
            // Normalize the user email for Gmail addresses
            let normalized_user_email = Self::normalize_gmail_address(email);
            let user_email_lower = email.trim().to_lowercase();
            
            // For Gmail addresses, always match both the original and normalized versions
            // This handles cases where:
            // - User receives at apembroke+nostr@gmail.com but sends from apembroke@gmail.com
            // - User sends from apembroke+nostr@gmail.com but Gmail stores it as apembroke@gmail.com
            if email.contains("@gmail.com") {
                if normalized_user_email != user_email_lower {
                    // User email has a + alias, match both versions
                    where_clauses.push("(LOWER(TRIM(from_address)) = LOWER(TRIM(?)) OR LOWER(TRIM(from_address)) = LOWER(TRIM(?)))");
                    params.push(Box::new(user_email_lower.clone()));
                    params.push(Box::new(normalized_user_email.clone()));
                } else {
                    // User email is already normalized (no + alias), but still match both to handle from_address with + aliases
                    where_clauses.push("(LOWER(TRIM(from_address)) = LOWER(TRIM(?)) OR LOWER(TRIM(from_address)) = LOWER(TRIM(?)))");
                    params.push(Box::new(user_email_lower.clone()));
                    params.push(Box::new(normalized_user_email.clone()));
                }
            } else {
                // Non-Gmail, just match exactly
                where_clauses.push("LOWER(TRIM(from_address)) = LOWER(TRIM(?))");
                params.push(Box::new(user_email_lower));
            }
        }
        if !where_clauses.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&where_clauses.join(" AND "));
        }
        query.push_str(" ORDER BY received_at DESC LIMIT ? OFFSET ?");
        params.push(Box::new(limit));
        params.push(Box::new(offset));

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(Email {
                id: Some(row.get(0)?),
                message_id: row.get(1)?,
                from_address: row.get(2)?,
                to_address: row.get(3)?,
                subject: row.get(4)?,
                body: row.get(5)?,
                body_plain: row.get(6)?,
                body_html: row.get(7)?,
                received_at: row.get(8)?,
                is_nostr_encrypted: row.get(9)?,
                sender_pubkey: row.get(10)?,
                recipient_pubkey: row.get(11)?,
                raw_headers: row.get(12)?,
                is_draft: row.get(13)?,
                is_read: row.get(14)?,
                updated_at: row.get(15)?,
                created_at: row.get(16)?,
                signature_valid: row.get(17)?,
                transport_auth_verified: row.get(18)?,
            })
        })?;
        let emails: Vec<Email> = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(emails)
    }

    pub fn get_latest_nostr_email_received_at(&self) -> Result<Option<DateTime<Utc>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT received_at FROM emails WHERE is_nostr_encrypted = 1 ORDER BY received_at DESC LIMIT 1"
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_latest_email_received_at(&self) -> Result<Option<DateTime<Utc>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT received_at FROM emails ORDER BY received_at DESC LIMIT 1"
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_latest_sent_email_received_at(&self, user_email: Option<&str>) -> Result<Option<DateTime<Utc>>> {
        let conn = self.conn.lock().unwrap();
        
        if let Some(email) = user_email {
            // Normalize the user email for Gmail addresses
            let normalized_user_email = Self::normalize_gmail_address(email);
            // Properly identify sent emails as those where the user is the sender
            // Match both original and normalized versions for Gmail + aliases
            let mut stmt = if normalized_user_email != email.trim().to_lowercase() {
                conn.prepare(
                    "SELECT received_at FROM emails WHERE is_nostr_encrypted = 1 AND (LOWER(TRIM(from_address)) = LOWER(TRIM(?)) OR LOWER(TRIM(from_address)) = LOWER(TRIM(?))) ORDER BY received_at DESC LIMIT 1"
                )?
            } else {
                conn.prepare(
                    "SELECT received_at FROM emails WHERE is_nostr_encrypted = 1 AND LOWER(TRIM(from_address)) = LOWER(TRIM(?)) ORDER BY received_at DESC LIMIT 1"
                )?
            };
            let mut rows = if normalized_user_email != email.trim().to_lowercase() {
                stmt.query(params![email, normalized_user_email])?
            } else {
                stmt.query(params![email])?
            };
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        } else {
            // Fallback to the old logic if no user email provided
            let mut stmt = conn.prepare(
                "SELECT received_at FROM emails WHERE is_nostr_encrypted = 1 AND from_address = to_address ORDER BY received_at DESC LIMIT 1"
            )?;
            let mut rows = stmt.query([])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        }
    }

    // Direct message operations
    pub fn save_dm(&self, dm: &DirectMessage) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        
        // Compute hash for DM content
        let content_hash = Self::compute_content_hash(&dm.content);
        
        if let Some(id) = dm.id {
            conn.execute(
                "UPDATE direct_messages SET 
                    event_id = ?, sender_pubkey = ?, recipient_pubkey = ?, content = ?, 
                    created_at = ?, received_at = ?, content_hash = ?
                WHERE id = ?",
                params![
                    dm.event_id, dm.sender_pubkey, dm.recipient_pubkey, dm.content,
                    dm.created_at, dm.received_at, content_hash, id
                ],
            )?;
            Ok(id)
        } else {
            let _id = conn.execute(
                "INSERT INTO direct_messages (event_id, sender_pubkey, recipient_pubkey, content, created_at, received_at, content_hash)
                VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    dm.event_id, dm.sender_pubkey, dm.recipient_pubkey, dm.content,
                    dm.created_at, dm.received_at, content_hash
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    /// Save a batch of direct messages, skipping any that already exist by event_id
    pub fn save_dm_batch(&self, dms: &[DirectMessage]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut inserted = 0;
        for dm in dms {
            // Check if this event_id already exists
            let mut stmt = conn.prepare("SELECT 1 FROM direct_messages WHERE event_id = ? LIMIT 1")?;
            let mut rows = stmt.query(params![dm.event_id])?;
            if rows.next()?.is_some() {
                continue; // Skip if already exists
            }
            // Compute hash for DM content
            let content_hash = Self::compute_content_hash(&dm.content);
            conn.execute(
                "INSERT INTO direct_messages (event_id, sender_pubkey, recipient_pubkey, content, created_at, received_at, content_hash)
                VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    dm.event_id, dm.sender_pubkey, dm.recipient_pubkey, dm.content,
                    dm.created_at, dm.received_at, content_hash
                ],
            )?;
            inserted += 1;
        }
        Ok(inserted)
    }

    pub fn get_dms_for_conversation(&self, user_pubkey: &str, contact_pubkey: &str) -> Result<Vec<DirectMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, event_id, sender_pubkey, recipient_pubkey, content, created_at, received_at
             FROM direct_messages 
             WHERE (sender_pubkey = ? AND recipient_pubkey = ?) OR (sender_pubkey = ? AND recipient_pubkey = ?)
             ORDER BY created_at ASC"
        )?;
        
        let rows = stmt.query_map(params![user_pubkey, contact_pubkey, contact_pubkey, user_pubkey], |row| {
            Ok(DirectMessage {
                id: Some(row.get(0)?),
                event_id: row.get(1)?,
                sender_pubkey: row.get(2)?,
                recipient_pubkey: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
                received_at: row.get(6)?,
            })
        })?;
        
        rows.collect()
    }

    // Returns the latest created_at timestamp from direct_messages, or None if no messages exist
    pub fn get_latest_dm_created_at(&self) -> Result<Option<DateTime<Utc>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT created_at FROM direct_messages ORDER BY created_at DESC LIMIT 1"
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_dm_encrypted_content_by_event_id(&self, event_id: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT content FROM direct_messages WHERE event_id = ?"
        )?;
        let mut rows = stmt.query_map(params![event_id], |row| {
            Ok(row.get::<_, String>(0)?)
        })?;
        
        rows.next().ok_or(rusqlite::Error::QueryReturnedNoRows)?
    }

    /// Get DM content hash by event_id for fast matching
    pub fn get_dm_content_hash_by_event_id(&self, event_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT content_hash FROM direct_messages WHERE event_id = ?"
        )?;
        let mut rows = stmt.query_map(params![event_id], |row| {
            Ok(row.get::<_, Option<String>>(0)?)
        })?;
        
        match rows.next() {
            Some(Ok(Some(hash))) => Ok(Some(hash)),
            Some(Ok(None)) => {
                // Hash not set, compute it from content
                let content = self.get_dm_encrypted_content_by_event_id(event_id)?;
                let hash = Self::compute_content_hash(&content);
                // Update the database with the hash
                conn.execute(
                    "UPDATE direct_messages SET content_hash = ? WHERE event_id = ?",
                    params![hash, event_id],
                )?;
                Ok(Some(hash))
            },
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Find email by subject hash (for fast DM-email matching)
    pub fn find_email_by_subject_hash(&self, subject_hash: &str) -> Result<Option<Email>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at, signature_valid, transport_auth_verified
             FROM emails WHERE subject_hash = ? AND is_nostr_encrypted = 1 LIMIT 1"
        )?;
        let mut rows = stmt.query_map(params![subject_hash], |row| {
            Ok(Email {
                id: Some(row.get(0)?),
                message_id: row.get(1)?,
                from_address: row.get(2)?,
                to_address: row.get(3)?,
                subject: row.get(4)?,
                body: row.get(5)?,
                body_plain: row.get(6)?,
                body_html: row.get(7)?,
                received_at: row.get(8)?,
                is_nostr_encrypted: row.get(9)?,
                sender_pubkey: row.get(10)?,
                recipient_pubkey: row.get(11)?,
                raw_headers: row.get(12)?,
                is_draft: row.get(13)?,
                is_read: row.get(14)?,
                updated_at: row.get(15)?,
                created_at: row.get(16)?,
                signature_valid: row.get(17)?,
                transport_auth_verified: row.get(18)?,
            })
        })?;
        
        match rows.next() {
            Some(Ok(email)) => Ok(Some(email)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    // Attachment operations
    pub fn save_attachment(&self, attachment: &Attachment) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        // SECURITY: Never store decrypted attachment metadata (original_filename, original_type, original_size)
        // These fields must always be None - decrypted names are only used for display after manifest decryption
        // This ensures encrypted attachment filenames remain private even if the database is compromised
        if attachment.original_filename.is_some() || attachment.original_type.is_some() || attachment.original_size.is_some() {
            println!("[DB] WARNING: Attempted to save attachment with decrypted metadata - ignoring original_filename/original_type/original_size");
        }
        
        if let Some(id) = attachment.id {
            // Update existing attachment
            // Always set original_filename, original_type, original_size to NULL to prevent storing decrypted data
            conn.execute(
                "UPDATE attachments SET 
                    email_id = ?, filename = ?, content_type = ?, data = ?, size = ?,
                    is_encrypted = ?, encryption_method = ?, algorithm = ?, 
                    original_filename = NULL, original_type = NULL, original_size = NULL
                WHERE id = ?",
                params![
                    attachment.email_id, attachment.filename, attachment.content_type, 
                    attachment.data, attachment.size as i64, attachment.is_encrypted,
                    attachment.encryption_method, attachment.algorithm, id
                ],
            )?;
            Ok(id)
        } else {
            // Insert new attachment
            // Always set original_filename, original_type, original_size to NULL
            conn.execute(
                "INSERT INTO attachments (
                    email_id, filename, content_type, data, size, is_encrypted,
                    encryption_method, algorithm, original_filename, original_type, 
                    original_size, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, ?)",
                params![
                    attachment.email_id, attachment.filename, attachment.content_type,
                    attachment.data, attachment.size as i64, attachment.is_encrypted,
                    attachment.encryption_method, attachment.algorithm, now
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn get_attachments_for_email(&self, email_id: i64) -> Result<Vec<Attachment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, email_id, filename, content_type, data, size, is_encrypted,
                    encryption_method, algorithm, original_filename, original_type, 
                    original_size, created_at
             FROM attachments WHERE email_id = ? ORDER BY created_at"
        )?;
        
        let rows = stmt.query_map(params![email_id], |row| {
            Ok(Attachment {
                id: Some(row.get(0)?),
                email_id: row.get(1)?,
                filename: row.get(2)?,
                content_type: row.get(3)?,
                data: row.get(4)?,
                size: row.get::<_, i64>(5)? as usize,
                is_encrypted: row.get(6)?,
                encryption_method: row.get(7)?,
                algorithm: row.get(8)?,
                original_filename: row.get(9)?,
                original_type: row.get(10)?,
                original_size: row.get::<_, Option<i64>>(11)?.map(|s| s as usize),
                created_at: row.get(12)?,
            })
        })?;
        
        rows.collect()
    }

    pub fn get_attachment(&self, attachment_id: i64) -> Result<Option<Attachment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, email_id, filename, content_type, data, size, is_encrypted,
                    encryption_method, algorithm, original_filename, original_type, 
                    original_size, created_at
             FROM attachments WHERE id = ?"
        )?;
        
        let mut rows = stmt.query(params![attachment_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Attachment {
                id: Some(row.get(0)?),
                email_id: row.get(1)?,
                filename: row.get(2)?,
                content_type: row.get(3)?,
                data: row.get(4)?,
                size: row.get::<_, i64>(5)? as usize,
                is_encrypted: row.get(6)?,
                encryption_method: row.get(7)?,
                algorithm: row.get(8)?,
                original_filename: row.get(9)?,
                original_type: row.get(10)?,
                original_size: row.get::<_, Option<i64>>(11)?.map(|s| s as usize),
                created_at: row.get(12)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_attachment(&self, attachment_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM attachments WHERE id = ?", params![attachment_id])?;
        Ok(())
    }

    pub fn delete_attachments_for_email(&self, email_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM attachments WHERE email_id = ?", params![email_id])?;
        Ok(())
    }

    // Combined operations
    pub fn save_email_with_attachments(&self, email: &Email, attachments: &[crate::types::EmailAttachment]) -> Result<i64> {
        // First save the email
        let email_id = self.save_email(email)?;
        
        // Then save each attachment
        for attachment in attachments {
            let db_attachment = Attachment {
                id: None,
                email_id,
                filename: attachment.filename.clone(),
                content_type: attachment.content_type.clone(),
                data: attachment.data.clone(),
                size: attachment.size,
                is_encrypted: attachment.is_encrypted,
                encryption_method: attachment.encryption_method.clone(),
                algorithm: attachment.algorithm.clone(),
                original_filename: attachment.original_filename.clone(),
                original_type: attachment.original_type.clone(),
                original_size: attachment.original_size,
                created_at: Utc::now(),
            };
            
            self.save_attachment(&db_attachment)?;
        }
        
        Ok(email_id)
    }

    // Settings operations
    pub fn save_setting(&self, pubkey: &str, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        conn.execute(
            "INSERT OR REPLACE INTO user_settings (pubkey, key, value, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
            params![pubkey, key, value, now, now],
        )?;
        
        Ok(())
    }

    pub fn get_setting(&self, pubkey: &str, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM user_settings WHERE pubkey = ? AND key = ?")?;
        
        let mut rows = stmt.query(params![pubkey, key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_settings(&self, pubkey: &str) -> Result<HashMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM user_settings WHERE pubkey = ?")?;
        
        let rows = stmt.query_map(params![pubkey], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        
        rows.collect()
    }
    
    // Delete all settings for a pubkey (useful for cleanup)
    pub fn delete_settings_for_pubkey(&self, pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM user_settings WHERE pubkey = ?", params![pubkey])?;
        Ok(())
    }

    // Relay operations
    pub fn save_relay(&self, relay: &DbRelay) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        if let Some(id) = relay.id {
            conn.execute(
                "UPDATE relays SET url = ?, is_active = ?, updated_at = ? WHERE id = ?",
                params![relay.url, relay.is_active, now, id],
            )?;
            Ok(id)
        } else {
            conn.execute(
                "INSERT INTO relays (url, is_active, created_at, updated_at) VALUES (?, ?, ?, ?)",
                params![relay.url, relay.is_active, now, now],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn get_all_relays(&self) -> Result<Vec<DbRelay>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, is_active, created_at, updated_at FROM relays ORDER BY url"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRelay {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                is_active: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete_relay(&self, url: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM relays WHERE url = ?", params![url])?;
        Ok(())
    }

    /// Returns a list of pubkeys whose contact email matches the given email address (case-insensitive, trimmed)
    pub fn find_pubkeys_by_email(&self, email: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let email_trimmed = email.trim().to_lowercase();
        println!("[RUST] Searching for pubkeys with email: {}", email_trimmed);
        let mut stmt = conn.prepare(
            "SELECT pubkey FROM contacts WHERE LOWER(TRIM(email)) = ?1"
        )?;
        let rows = stmt.query_map(params![email_trimmed], |row| {
            let pubkey: String = row.get(0)?;
            Ok(pubkey)
        })?;
        let mut pubkeys = Vec::new();
        for row in rows {
            pubkeys.push(row?);
        }
        println!("[RUST] Found pubkeys for email '{}': {:?}", email_trimmed, pubkeys);
        Ok(pubkeys)
    }
    
    /// Returns a list of pubkeys whose email_address setting matches the given email address (case-insensitive, trimmed)
    pub fn find_pubkeys_by_email_setting(&self, email: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let email_trimmed = email.trim().to_lowercase();
        println!("[RUST] Searching for pubkeys with email_address setting: {}", email_trimmed);
        let mut stmt = conn.prepare(
            "SELECT DISTINCT pubkey FROM user_settings WHERE key = 'email_address' AND LOWER(TRIM(value)) = ?1"
        )?;
        let rows = stmt.query_map(params![email_trimmed], |row| {
            let pubkey: String = row.get(0)?;
            Ok(pubkey)
        })?;
        let mut pubkeys = Vec::new();
        for row in rows {
            pubkeys.push(row?);
        }
        println!("[RUST] Found pubkeys for email_address setting '{}': {:?}", email_trimmed, pubkeys);
        Ok(pubkeys)
    }
    
    /// Find pubkeys by email, including all DM participants
    /// This searches both contacts table by email and includes all unique pubkeys from DMs
    pub fn find_pubkeys_by_email_including_dms(&self, email: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let email_trimmed = email.trim().to_lowercase();
        println!("[RUST] Searching for pubkeys with email (including all DMs): {}", email_trimmed);
        
        let mut pubkeys = std::collections::HashSet::new();
        
        // 1. Search contacts table by email
        let mut stmt = conn.prepare(
            "SELECT pubkey FROM contacts WHERE LOWER(TRIM(email)) = ?1"
        )?;
        let rows = stmt.query_map(params![email_trimmed], |row| {
            let pubkey: String = row.get(0)?;
            Ok(pubkey)
        })?;
        for row in rows {
            if let Ok(pubkey) = row {
                pubkeys.insert(pubkey);
            }
        }
        
        // 2. Get ALL unique pubkeys from DMs (regardless of email address setting)
        let mut stmt = conn.prepare(
            "SELECT DISTINCT dm_pubkey
             FROM (
               SELECT sender_pubkey as dm_pubkey FROM direct_messages
               UNION
               SELECT recipient_pubkey as dm_pubkey FROM direct_messages
             ) dm_pubkeys
             WHERE dm_pubkey != ''"
        )?;
        let rows = stmt.query_map([], |row| {
            let pubkey: String = row.get(0)?;
            Ok(pubkey)
        })?;
        for row in rows {
            if let Ok(pubkey) = row {
                pubkeys.insert(pubkey);
            }
        }
        
        let result: Vec<String> = pubkeys.into_iter().collect();
        println!("[RUST] Found pubkeys for email '{}' (including all DMs): {:?}", email_trimmed, result);
        Ok(result)
    }

    pub fn update_email_sender_pubkey(&self, message_id: &str, sender_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        println!("[RUST] Updating sender_pubkey for message_id {} to {}", message_id, sender_pubkey);
        let rows = conn.execute(
            "UPDATE emails SET sender_pubkey = ?1 WHERE message_id = ?2",
            params![sender_pubkey, message_id],
        )?;
        println!("[RUST] Rows affected by sender_pubkey update: {}", rows);
        Ok(())
    }

    pub fn update_email_sender_pubkey_by_id(&self, id: i64, sender_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE emails SET sender_pubkey = ? WHERE id = ?",
            params![sender_pubkey, id],
        )?;
        Ok(())
    }
    
    pub fn update_email_recipient_pubkey(&self, message_id: &str, recipient_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        println!("[RUST] Updating recipient_pubkey for message_id {} to {}", message_id, recipient_pubkey);
        let rows = conn.execute(
            "UPDATE emails SET recipient_pubkey = ?1 WHERE message_id = ?2",
            params![recipient_pubkey, message_id],
        )?;
        println!("[RUST] Rows affected by recipient_pubkey update: {}", rows);
        Ok(())
    }

    pub fn update_email_recipient_pubkey_by_id(&self, id: i64, recipient_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE emails SET recipient_pubkey = ? WHERE id = ?",
            params![recipient_pubkey, id],
        )?;
        Ok(())
    }

    pub fn find_emails_by_message_id(&self, message_id: &str) -> Result<Vec<Email>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at, signature_valid
             FROM emails WHERE message_id = ? ORDER BY received_at DESC"
        )?;
        
        let rows = stmt.query_map(params![message_id], |row| {
            Ok(Email {
                id: Some(row.get(0)?),
                message_id: row.get(1)?,
                from_address: row.get(2)?,
                to_address: row.get(3)?,
                subject: row.get(4)?,
                body: row.get(5)?,
                body_plain: row.get(6)?,
                body_html: row.get(7)?,
                received_at: row.get(8)?,
                is_nostr_encrypted: row.get(9)?,
                sender_pubkey: row.get(10)?,
                recipient_pubkey: row.get(11)?,
                raw_headers: row.get(12)?,
                is_draft: row.get(13)?,
                is_read: row.get(14)?,
                updated_at: row.get(15)?,
                created_at: row.get(16)?,
                signature_valid: row.get(17)?,
                transport_auth_verified: row.get(18)?,
            })
        })?;
        
        let mut emails = Vec::new();
        for row in rows {
            emails.push(row?);
        }
        Ok(emails)
    }

    // Draft operations
    pub fn save_draft(&self, draft: &Email) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        if let Some(id) = draft.id {
            conn.execute(
                "UPDATE emails SET 
                    message_id = ?, from_address = ?, to_address = ?, subject = ?, 
                    body = ?, body_plain = ?, body_html = ?, received_at = ?, 
                    is_nostr_encrypted = ?, sender_pubkey = ?, recipient_pubkey = ?, raw_headers = ?, 
                    is_draft = ?, is_read = ?, updated_at = ?
                WHERE id = ?",
                params![
                    draft.message_id, draft.from_address, draft.to_address, draft.subject,
                    draft.body, draft.body_plain, draft.body_html, draft.received_at,
                    draft.is_nostr_encrypted, draft.sender_pubkey, draft.recipient_pubkey, draft.raw_headers,
                    true, draft.is_read, now, id
                ],
            )?;
            Ok(id)
        } else {
            let _id = conn.execute(
                "INSERT INTO emails (message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    draft.message_id, draft.from_address, draft.to_address, draft.subject,
                    draft.body, draft.body_plain, draft.body_html, draft.received_at,
                    draft.is_nostr_encrypted, draft.sender_pubkey, draft.recipient_pubkey, draft.raw_headers,
                    true, draft.is_read, now
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn get_drafts(&self, limit: Option<i64>, offset: Option<i64>, user_email: Option<&str>) -> Result<Vec<Email>> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        
        let mut query = String::from(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at, signature_valid, transport_auth_verified FROM emails WHERE is_draft = 1"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        if let Some(email) = user_email {
            query.push_str(" AND from_address = ?");
            params.push(Box::new(email));
        }
        
        query.push_str(" ORDER BY updated_at DESC, created_at DESC LIMIT ? OFFSET ?");
        params.push(Box::new(limit));
        params.push(Box::new(offset));
        
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(Email {
                id: Some(row.get(0)?),
                message_id: row.get(1)?,
                from_address: row.get(2)?,
                to_address: row.get(3)?,
                subject: row.get(4)?,
                body: row.get(5)?,
                body_plain: row.get(6)?,
                body_html: row.get(7)?,
                received_at: row.get(8)?,
                is_nostr_encrypted: row.get(9)?,
                sender_pubkey: row.get(10)?,
                recipient_pubkey: row.get(11)?,
                raw_headers: row.get(12)?,
                is_draft: row.get(13)?,
                is_read: row.get(14)?,
                updated_at: row.get(15)?,
                created_at: row.get(16)?,
                signature_valid: row.get(17)?,
                transport_auth_verified: row.get(18)?,
            })
        })?;
        
        let mut drafts = Vec::new();
        for row in rows {
            drafts.push(row?);
        }
        Ok(drafts)
    }

    pub fn delete_draft(&self, message_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Delete attachments first (CASCADE should handle this, but explicit is safer)
        if let Ok(Some(email)) = self.get_email(message_id) {
            if let Some(email_id) = email.id {
                self.delete_attachments_for_email(email_id)?;
            }
        }
        conn.execute(
            "DELETE FROM emails WHERE message_id = ? AND is_draft = 1",
            params![message_id],
        )?;
        Ok(())
    }
    
    pub fn delete_sent_email(&self, message_id: &str, user_email: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Delete attachments first
        if let Ok(Some(email)) = self.get_email(message_id) {
            if let Some(email_id) = email.id {
                self.delete_attachments_for_email(email_id)?;
            }
        }
        
        // Delete the email (only if it's a sent email, not a draft)
        let mut query = String::from("DELETE FROM emails WHERE message_id = ? AND is_draft = 0");
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(message_id)];
        
        // Optionally filter by user_email to ensure user can only delete their own sent emails
        if let Some(email) = user_email {
            query.push_str(" AND LOWER(TRIM(from_address)) = LOWER(TRIM(?))");
            params.push(Box::new(email));
        }
        
        conn.execute(&query, rusqlite::params_from_iter(params.iter()))?;
        Ok(())
    }

    pub fn mark_as_read(&self, message_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE emails SET is_read = 1 WHERE message_id = ?",
            params![message_id],
        )?;
        Ok(())
    }

    /// Update signature_valid field for an email
    pub fn update_signature_valid(&self, message_id: &str, signature_valid: Option<bool>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let normalized_id = Self::normalize_message_id(message_id);
        conn.execute(
            "UPDATE emails SET signature_valid = ? WHERE TRIM(REPLACE(REPLACE(message_id, '<', ''), '>', '')) = ?",
            params![signature_valid, normalized_id],
        )?;
        Ok(())
    }

    /// Get all unique pubkeys (npubs) that appear as sender or recipient in direct_messages, including the user's own pubkey
    pub fn get_all_dm_pubkeys(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT sender_pubkey FROM direct_messages UNION SELECT recipient_pubkey FROM direct_messages"
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut set = std::collections::HashSet::new();
        for row in rows {
            let pk: String = row?;
            if !pk.is_empty() {
                set.insert(pk);
            }
        }
        Ok(set.into_iter().collect())
    }

    pub fn get_all_dm_pubkeys_sorted(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey
             FROM (
               SELECT sender_pubkey as pubkey, created_at FROM direct_messages
               UNION ALL
               SELECT recipient_pubkey as pubkey, created_at FROM direct_messages
             )
             WHERE pubkey != ''
             GROUP BY pubkey
             ORDER BY MAX(created_at) DESC"
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut pubkeys = Vec::new();
        for row in rows {
            pubkeys.push(row?);
        }
        Ok(pubkeys)
    }

    // Utility operations
    pub fn get_database_size(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let path = conn.path().ok_or_else(|| rusqlite::Error::InvalidPath(std::path::PathBuf::from("No path")))?;
        
        if let Ok(metadata) = std::fs::metadata(path) {
            Ok(metadata.len())
        } else {
            Ok(0)
        }
    }

    pub fn clear_all_data(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM contacts", [])?;
        conn.execute("DELETE FROM emails", [])?;
        conn.execute("DELETE FROM direct_messages", [])?;
        conn.execute("DELETE FROM user_settings", [])?;
        conn.execute("DELETE FROM relays", [])?;
        Ok(())
    }
    
    pub fn clear_settings_for_pubkey(&self, pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM user_settings WHERE pubkey = ?", params![pubkey])?;
        Ok(())
    }

    pub fn update_contact_picture_data_url(&self, pubkey: &str, picture_data_url: &str) -> rusqlite::Result<()> {
        // Only store if valid image data URL
        let is_valid = picture_data_url.starts_with("data:image") && picture_data_url != "data:application/octet-stream;base64," && !picture_data_url.trim().is_empty();
        if !is_valid {
            println!("[DB] Not storing invalid picture_data_url for {}", pubkey);
            return Ok(()); // No-op
        }
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now();
        conn.execute(
            "UPDATE contacts SET picture_data_url = ?, updated_at = ? WHERE pubkey = ?",
            rusqlite::params![picture_data_url, now, pubkey],
        )?;
        Ok(())
    }
} 