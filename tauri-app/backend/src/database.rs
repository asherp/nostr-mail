use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use sha2::{Sha256, Digest};

// Embed config file at compile time for Android builds
#[cfg(target_os = "android")]
const EMBEDDED_CONFIG: &str = include_str!("../nostr-mail-config.json");

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DbConversation {
    pub id: Option<i64>,
    pub user_pubkey: String,
    pub contact_pubkey: String,
    pub contact_name: Option<String>,
    pub last_message_event_id: String,
    pub last_timestamp: i64,
    pub message_count: i64,
    pub cached_at: DateTime<Utc>,
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
        
        // Determine config path: use NOSTR_MAIL_CONFIG if set, otherwise use platform-specific approach
        let is_test_mode = std::env::var("NOSTR_MAIL_CONFIG").is_ok();
        
        #[cfg(target_os = "android")]
        {
            // On Android, use embedded config content
            if !is_test_mode {
                println!("[DB] Loading config data from embedded config (Android)");
                let config_loaded = db.load_config_data_from_str(EMBEDDED_CONFIG).is_ok();
                if !config_loaded {
                    println!("[DB] Warning: Failed to parse embedded config data");
                } else {
                    println!("[DB] Successfully loaded config data from embedded config");
                }
            } else {
                // Test mode: use environment variable path
                let config_path = std::env::var("NOSTR_MAIL_CONFIG").unwrap();
                println!("[DB] Loading config data from: {} (test mode)", config_path);
                let config_loaded = db.load_config_data(&config_path).is_ok();
                if !config_loaded {
                    println!("[DB] Failed to load test config from {}: file not found or invalid", config_path);
                } else {
                    println!("[DB] Successfully loaded config data from {}", config_path);
                }
            }
        }
        
        #[cfg(not(target_os = "android"))]
        {
            // On desktop, use file path
            let config_path = if is_test_mode {
                std::env::var("NOSTR_MAIL_CONFIG").unwrap()
            } else {
                // Default to nostr-mail-config.json in backend directory
                let default_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("nostr-mail-config.json");
                default_path.to_string_lossy().to_string()
            };
            
            // Load config data - this will handle relay seeding from the config file
            println!("[DB] Loading config data from: {}", config_path);
            let config_loaded = db.load_config_data(&config_path).is_ok();
            
            if !config_loaded {
                if is_test_mode {
                    println!("[DB] Failed to load test config from {}: file not found or invalid", config_path);
                } else {
                    println!("[DB] Warning: Failed to load config from {}: file not found or invalid", config_path);
                }
            } else {
                println!("[DB] Successfully loaded config data from {}", config_path);
            }
        }
        
        // Only seed hardcoded defaults if:
        // 1. Not in test mode (NOSTR_MAIL_CONFIG not set)
        // 2. Relays table is still empty (config file had no relays or failed to load)
        if !is_test_mode {
            db.seed_default_relays()?;
        }
        
        Ok(db)
    }
    
    /// Seed default relays if the relays table is empty
    /// Loads relays from nostr-mail-config.json (shipped with the app)
    fn seed_default_relays(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Check if relays table is empty
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM relays", [], |row| row.get(0))?;
        
        if count == 0 {
            println!("[DB] Seeding default relays from nostr-mail-config.json...");
            let now = Utc::now();
            
            // Load config content - use embedded on Android, file on desktop
            let json_content = Self::get_config_content();
            
            match json_content {
                Ok(content) => {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(relays_array) = config.get("relays").and_then(|r| r.as_array()) {
                            let mut loaded_count = 0;
                            for relay_obj in relays_array {
                                if let (Some(url), is_active) = (
                                    relay_obj.get("url").and_then(|u| u.as_str()),
                                    relay_obj.get("is_active").and_then(|a| a.as_bool()).unwrap_or(true)
                                ) {
                                    conn.execute(
                                        "INSERT OR IGNORE INTO relays (url, is_active, created_at, updated_at) VALUES (?, ?, ?, ?)",
                                        params![url, is_active, now, now],
                                    )?;
                                    println!("[DB] Seeded relay: {} (active: {})", url, is_active);
                                    loaded_count += 1;
                                }
                            }
                            if loaded_count > 0 {
                                println!("[DB] Successfully seeded {} relay(s) from nostr-mail-config.json", loaded_count);
                            } else {
                                println!("[DB] Warning: nostr-mail-config.json contains no valid relays");
                            }
                        } else {
                            println!("[DB] Warning: Invalid relays format in nostr-mail-config.json");
                        }
                    } else {
                        println!("[DB] Warning: Failed to parse nostr-mail-config.json");
                    }
                }
                Err(e) => {
                    println!("[DB] Warning: Could not read nostr-mail-config.json: {} (file should be shipped with the app)", e);
                }
            }
        } else {
            println!("[DB] Relays table already contains {} relay(s), skipping seed", count);
        }
        
        Ok(())
    }
    
    /// Get config file content - embedded on Android, from file on desktop
    fn get_config_content() -> std::result::Result<String, String> {
        #[cfg(target_os = "android")]
        {
            // On Android, use embedded config
            Ok(EMBEDDED_CONFIG.to_string())
        }
        
        #[cfg(not(target_os = "android"))]
        {
            // On desktop, read from file
            let json_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("nostr-mail-config.json");
            std::fs::read_to_string(&json_path)
                .map_err(|e| format!("{}", e))
        }
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

        // Conversations table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_pubkey TEXT NOT NULL,
                contact_pubkey TEXT NOT NULL,
                contact_name TEXT,
                last_message_event_id TEXT NOT NULL,
                last_timestamp INTEGER NOT NULL,
                message_count INTEGER NOT NULL DEFAULT 0,
                cached_at DATETIME NOT NULL,
                UNIQUE(user_pubkey, contact_pubkey)
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
        conn.execute("CREATE INDEX IF NOT EXISTS idx_conversations_user_pubkey ON conversations(user_pubkey, last_timestamp DESC)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_conversations_contact_pubkey ON conversations(contact_pubkey)", [])?;

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

    /// Batch update is_public status for multiple user-contact relationships
    /// This is much faster than calling update_user_contact_public_status multiple times
    pub fn batch_update_user_contact_public_status(&self, user_pubkey: &str, updates: &[(String, bool)]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE user_contacts SET is_public = ? WHERE user_pubkey = ? AND contact_pubkey = ?"
            )?;
            for (contact_pubkey, is_public) in updates {
                stmt.execute(params![is_public, user_pubkey, contact_pubkey])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Batch save contacts and user-contact relationships
    /// This is much faster than calling save_contact and add_user_contact multiple times
    pub fn batch_save_contacts(&self, user_pubkey: &str, contacts: &[Contact], is_public: bool) -> Result<()> {
        if contacts.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let now = Utc::now();
        
        // Batch insert/update contacts
        {
            let mut stmt = tx.prepare(
                "INSERT INTO contacts (pubkey, name, email, picture_url, picture_data_url, about, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(pubkey) DO UPDATE SET
                    name = excluded.name,
                    email = excluded.email,
                    picture_url = excluded.picture_url,
                    picture_data_url = excluded.picture_data_url,
                    about = excluded.about,
                    updated_at = excluded.updated_at"
            )?;
            for contact in contacts {
                stmt.execute(params![
                    contact.pubkey, contact.name, contact.email, contact.picture_url,
                    contact.picture_data_url, contact.about, now, now
                ])?;
            }
        }
        
        // Batch insert/update user-contact relationships
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO user_contacts (user_pubkey, contact_pubkey, is_public, created_at)
                VALUES (?, ?, ?, ?)"
            )?;
            for contact in contacts {
                stmt.execute(params![user_pubkey, contact.pubkey, is_public, now])?;
            }
        }
        
        tx.commit()?;
        Ok(())
    }

    /// Count how many users are following a given contact
    pub fn count_users_following_contact(&self, contact_pubkey: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM user_contacts WHERE contact_pubkey = ?"
        )?;
        let count: i64 = stmt.query_row(params![contact_pubkey], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Remove a user-contact relationship and cleanup contact if no users remain
    /// Returns (success, contact_deleted)
    pub fn remove_user_contact_and_cleanup(&self, user_pubkey: &str, contact_pubkey: &str) -> Result<(bool, bool)> {
        // First, remove the user-contact relationship
        self.remove_user_contact(user_pubkey, contact_pubkey)?;
        
        // Check if any other users are following this contact
        let remaining_count = self.count_users_following_contact(contact_pubkey)?;
        
        // If no users are following the contact, delete it from the contacts table
        let contact_deleted = if remaining_count == 0 {
            self.delete_contact(contact_pubkey)?;
            true
        } else {
            false
        };
        
        Ok((true, contact_deleted))
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
        // Optimized: Exclude picture_data_url from initial query to reduce IPC overhead
        // picture_data_url will be fetched on-demand via getCachedProfileImage
        let mut stmt = conn.prepare(
            "SELECT c.id, c.pubkey, c.name, c.email, c.picture_url, c.about, c.created_at, c.updated_at, uc.is_public
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
                picture_data_url: None, // Not fetched in initial query - will be loaded on-demand
                about: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                is_public: Some(row.get(8)?),
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

    // Normalize Gmail address by removing + aliases and dots (Gmail ignores dots)
    // e.g., user+alias@gmail.com -> user@gmail.com
    // e.g., user.name@gmail.com -> username@gmail.com
    // e.g., user.name+alias@gmail.com -> username@gmail.com
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
                // Remove dots from local part (Gmail ignores them)
                let normalized_local = normalized_local.replace(".", "");
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
            
            // For Gmail addresses, normalize both sides for comparison
            // This handles cases where:
            // - User receives at asherp.nostr+mail@gmail.com but sends from asherpnostr@gmail.com
            // - Gmail treats dots as equivalent, so asherp.nostr@gmail.com = asherpnostr@gmail.com
            if email.contains("@gmail.com") {
                // For Gmail, normalize both sides for comparison
                // Gmail treats dots as equivalent and may strip + aliases when sending
                // Create a version without + alias for comparison (Gmail may strip it)
                let user_email_no_plus = if let Some(plus_pos) = user_email_lower.find('+') {
                    if let Some(at_pos) = user_email_lower.find('@') {
                        format!("{}@{}", &user_email_lower[..plus_pos], &user_email_lower[at_pos+1..])
                    } else {
                        user_email_lower.clone()
                    }
                } else {
                    user_email_lower.clone()
                };
                let normalized_user_email_no_plus = Self::normalize_gmail_address(&user_email_no_plus);
                
                // Compare:
                // 1. Exact match (original user email)
                // 2. Normalized match (both user email and from_address normalized: dots and + removed)
                // 3. Normalized match without + alias (in case Gmail stripped it)
                // SQL normalizes from_address by: removing dots, removing + alias, then comparing
                where_clauses.push(
                    "(LOWER(TRIM(from_address)) = LOWER(TRIM(?)) OR \
                     (REPLACE(SUBSTR(LOWER(TRIM(from_address)), 1, CASE WHEN INSTR(LOWER(TRIM(from_address)), '+') > 0 THEN INSTR(LOWER(TRIM(from_address)), '+') - 1 ELSE INSTR(LOWER(TRIM(from_address)), '@') - 1 END), '.', '') || '@gmail.com') = ? OR \
                     (REPLACE(SUBSTR(LOWER(TRIM(from_address)), 1, CASE WHEN INSTR(LOWER(TRIM(from_address)), '+') > 0 THEN INSTR(LOWER(TRIM(from_address)), '+') - 1 ELSE INSTR(LOWER(TRIM(from_address)), '@') - 1 END), '.', '') || '@gmail.com') = ?)"
                );
                params.push(Box::new(user_email_lower.clone()));
                params.push(Box::new(normalized_user_email.clone()));
                params.push(Box::new(normalized_user_email_no_plus.clone()));
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
        
        let result = if let Some(id) = dm.id {
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
            id
        } else {
            let _id = conn.execute(
                "INSERT INTO direct_messages (event_id, sender_pubkey, recipient_pubkey, content, created_at, received_at, content_hash)
                VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    dm.event_id, dm.sender_pubkey, dm.recipient_pubkey, dm.content,
                    dm.created_at, dm.received_at, content_hash
                ],
            )?;
            conn.last_insert_rowid()
        };
        
        // Update conversation metadata for both sender and recipient
        drop(conn); // Release lock before calling update_conversation_from_messages
        let _ = self.update_conversation_from_messages(&dm.sender_pubkey, &dm.recipient_pubkey);
        let _ = self.update_conversation_from_messages(&dm.recipient_pubkey, &dm.sender_pubkey);
        
        Ok(result)
    }

    /// Save a batch of direct messages, skipping any that already exist by event_id
    pub fn save_dm_batch(&self, dms: &[DirectMessage]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut inserted = 0;
        let mut conversations_to_update = std::collections::HashSet::new();
        
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
            // Track conversations that need updating
            conversations_to_update.insert((dm.sender_pubkey.clone(), dm.recipient_pubkey.clone()));
            conversations_to_update.insert((dm.recipient_pubkey.clone(), dm.sender_pubkey.clone()));
        }
        
        // Update conversation metadata for all affected conversations
        drop(conn); // Release lock before calling update_conversation_from_messages
        for (user_pubkey, contact_pubkey) in conversations_to_update {
            let _ = self.update_conversation_from_messages(&user_pubkey, &contact_pubkey);
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

    // Returns the latest created_at timestamp for a specific conversation, or None if no messages exist
    pub fn get_latest_dm_created_at_for_conversation(
        &self, 
        user_pubkey: &str, 
        contact_pubkey: &str
    ) -> Result<Option<DateTime<Utc>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT created_at FROM direct_messages 
             WHERE (sender_pubkey = ? AND recipient_pubkey = ?) 
                OR (sender_pubkey = ? AND recipient_pubkey = ?)
             ORDER BY created_at DESC LIMIT 1"
        )?;
        let mut rows = stmt.query_map(
            params![user_pubkey, contact_pubkey, contact_pubkey, user_pubkey], 
            |row| row.get::<_, DateTime<Utc>>(0)
        )?;
        
        if let Some(row) = rows.next() {
            Ok(Some(row?))
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
    
    /// Get the default private key from the config file (first profile's private key)
    pub fn get_default_private_key_from_config() -> Result<Option<String>> {
        // Determine config path: use NOSTR_MAIL_CONFIG if set, otherwise use platform-specific approach
        let is_test_mode = std::env::var("NOSTR_MAIL_CONFIG").is_ok();
        
        let config_path = if is_test_mode {
            std::env::var("NOSTR_MAIL_CONFIG").unwrap()
        } else {
            #[cfg(target_os = "android")]
            {
                // On Android, config is embedded, so we can't read it here
                return Ok(None);
            }
            #[cfg(not(target_os = "android"))]
            {
                // On desktop, use default path
                let default_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("nostr-mail-config.json");
                default_path.to_string_lossy().to_string()
            }
        };
        
        // Try to read and parse the config file
        use std::fs;
        let content = match fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => {
                // Config file doesn't exist or can't be read - that's okay
                return Ok(None);
            }
        };
        
        // Parse JSON to extract first profile's private key
        use serde_json;
        #[derive(Deserialize)]
        struct ConfigProfile {
            private_key: String,
        }
        
        #[derive(Deserialize)]
        struct ConfigData {
            profiles: Option<Vec<ConfigProfile>>,
        }
        
        match serde_json::from_str::<ConfigData>(&content) {
            Ok(config_data) => {
                if let Some(profiles) = config_data.profiles {
                    if let Some(first_profile) = profiles.first() {
                        println!("[DB] Found default private key from config file (first profile)");
                        return Ok(Some(first_profile.private_key.clone()));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                println!("[DB] Failed to parse config file for default private key: {}", e);
                Ok(None)
            }
        }
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

    /// Load config data from JSON string (events, profiles, relays)
    fn load_config_data_from_str(&self, content: &str) -> Result<()> {
        self.parse_and_load_config_data(content)
    }
    
    /// Load config data from JSON file (events, profiles, relays)
    /// Requires an absolute path or path relative to current working directory
    fn load_config_data(&self, json_path: &str) -> Result<()> {
        use std::fs;
        let content = fs::read_to_string(json_path)
            .map_err(|e| rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISUSE),
                Some(format!("Failed to read config file {}: {} (use absolute path)", json_path, e))
            ))?;
        
        self.parse_and_load_config_data(&content)
    }
    
    /// Parse and load config data from JSON content
    /// Supports both full format (events, profiles, relays) and simple format (just relays)
    fn parse_and_load_config_data(&self, content: &str) -> Result<()> {
        use serde_json;
        
        // First, try to parse as simple format (just relays)
        if let Ok(simple_config) = serde_json::from_str::<serde_json::Value>(content) {
            if simple_config.get("relays").is_some() && simple_config.get("events").is_none() {
                // Simple format - just load relays
                if let Some(relays_array) = simple_config.get("relays").and_then(|r| r.as_array()) {
                    let conn = self.conn.lock().unwrap();
                    let now = Utc::now();
                    let mut added_count = 0;
                    let mut skipped_count = 0;
                    
                    for relay_obj in relays_array {
                        if let (Some(url), is_active) = (
                            relay_obj.get("url").and_then(|u| u.as_str()),
                            relay_obj.get("is_active").and_then(|a| a.as_bool()).unwrap_or(true)
                        ) {
                            let exists: bool = conn.query_row(
                                "SELECT EXISTS(SELECT 1 FROM relays WHERE url = ?)",
                                params![url],
                                |row| row.get(0)
                            ).unwrap_or(false);
                            
                            if !exists {
                                conn.execute(
                                    "INSERT INTO relays (url, is_active, created_at, updated_at) VALUES (?, ?, ?, ?)",
                                    params![url, is_active, now, now],
                                )?;
                                added_count += 1;
                            } else {
                                skipped_count += 1;
                            }
                        }
                    }
                    println!("[DB] Loaded {} relay(s) from simple config format ({} added, {} skipped)", 
                        added_count + skipped_count, added_count, skipped_count);
                    return Ok(());
                }
            }
        }
        
        // Try full format (events, profiles, relays)
        #[derive(Deserialize)]
        struct ConfigEvent {
            #[allow(dead_code)]
            id: String,
            pubkey: String,
            created_at: i64,
            kind: u16,
            #[allow(dead_code)]
            tags: Vec<Vec<String>>,
            content: String,
            #[allow(dead_code)]
            sig: String,
        }
        
        #[derive(Deserialize)]
        struct ConfigProfile {
            #[allow(dead_code)]
            pubkey: String,
            private_key: String,
            #[allow(dead_code)]
            name: Option<String>,
        }
        
        #[derive(Deserialize)]
        struct ConfigData {
            events: Vec<ConfigEvent>,
            profiles: Option<Vec<ConfigProfile>>,
            relays: Option<Vec<String>>,
        }
        
        let config_data: ConfigData = serde_json::from_str(content)
            .map_err(|e| rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISUSE),
                Some(format!("Failed to parse config JSON: {}", e))
            ))?;
        
        let profile_count = config_data.profiles.as_ref().map(|p: &Vec<ConfigProfile>| p.len()).unwrap_or(0);
        let relay_count = config_data.relays.as_ref().map(|r: &Vec<String>| r.len()).unwrap_or(0);
        println!("[DB] Parsed config data: {} events, {} profiles, {} relays", 
            config_data.events.len(),
            profile_count,
            relay_count);
        
        // Load relays (merge with existing relays)
        if let Some(relays) = &config_data.relays {
            println!("[DB] Merging {} relays from config", relays.len());
            let conn = self.conn.lock().unwrap();
            let now = Utc::now();
            
            // Insert relays that don't already exist
            let mut added_count = 0;
            let mut skipped_count = 0;
            for relay_url in relays {
                // Check if relay already exists
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM relays WHERE url = ?)",
                    params![relay_url],
                    |row| row.get(0)
                ).unwrap_or(false);
                
                if !exists {
                    conn.execute(
                        "INSERT INTO relays (url, is_active, created_at, updated_at) VALUES (?, ?, ?, ?)",
                        params![relay_url, true, now, now],
                    )?;
                    println!("[DB] Added relay: {} (active: true)", relay_url);
                    added_count += 1;
                } else {
                    println!("[DB] Skipped existing relay: {}", relay_url);
                    skipped_count += 1;
                }
            }
            println!("[DB] Merged relays: {} added, {} skipped (already exist)", added_count, skipped_count);
        }
        
        // Load contacts from profile events (kind 0)
        let mut contacts_loaded = 0;
        for event in &config_data.events {
            if event.kind == 0 {
                // Parse profile metadata
                if let Ok(metadata) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&event.content) {
                    let name = metadata.get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let display_name = metadata.get("display_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let email = metadata.get("email")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let picture = metadata.get("picture")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let about = metadata.get("about")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    
                    let contact = Contact {
                        id: None,
                        pubkey: event.pubkey.clone(),
                        name: display_name.or(name),
                        email,
                        picture_url: picture,
                        picture_data_url: None,
                        about,
                        created_at: DateTime::from_timestamp(event.created_at, 0).unwrap_or_else(Utc::now),
                        updated_at: DateTime::from_timestamp(event.created_at, 0).unwrap_or_else(Utc::now),
                        is_public: Some(true),
                    };
                    
                    if let Err(e) = self.save_contact(&contact) {
                        println!("[DB] Failed to save contact {}: {}", event.pubkey, e);
                    } else {
                        contacts_loaded += 1;
                    }
                }
            }
        }
        println!("[DB] Loaded {} contacts from config data", contacts_loaded);
        
        // Note: Direct messages (kind 4) would need to be decrypted with private keys
        // This is more complex and would require the user's private key, so we skip it for now
        // The DMs will be available from the relay when queried
        
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
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, sender_pubkey, recipient_pubkey, raw_headers, is_draft, is_read, updated_at, created_at, signature_valid, transport_auth_verified
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

    /// Check for duplicate message_ids in the database (for debugging)
    pub fn check_duplicate_message_ids(&self) -> Result<Vec<(String, i64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT message_id, COUNT(*) as count 
             FROM emails 
             GROUP BY message_id 
             HAVING COUNT(*) > 1 
             ORDER BY count DESC"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        
        let mut duplicates = Vec::new();
        for row in rows {
            duplicates.push(row?);
        }
        Ok(duplicates)
    }

    /// Get all message_ids for sent emails (for debugging)
    pub fn get_all_sent_message_ids(&self, user_email: Option<&str>) -> Result<Vec<(i64, String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut query = String::from(
            "SELECT id, message_id, from_address, received_at FROM emails WHERE is_draft = 0"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        if let Some(email) = user_email {
            let email_lower = email.to_lowercase();
            let email_normalized = if email_lower.contains("@gmail.com") {
                email_lower.split('+').next().unwrap_or(&email_lower).split('@').next().unwrap_or(&email_lower).to_string() + "@gmail.com"
            } else {
                email_lower
            };
            query.push_str(" AND (LOWER(TRIM(from_address)) = LOWER(TRIM(?)) OR LOWER(TRIM(from_address)) = LOWER(TRIM(?)))");
            params.push(Box::new(email.clone()));
            params.push(Box::new(email_normalized));
        }
        
        query.push_str(" ORDER BY received_at DESC");
        
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
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

    // Conversation operations
    pub fn save_conversation(&self, conv: &DbConversation) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        if let Some(id) = conv.id {
            conn.execute(
                "UPDATE conversations SET 
                    user_pubkey = ?, contact_pubkey = ?, contact_name = ?, 
                    last_message_event_id = ?, last_timestamp = ?, message_count = ?, cached_at = ?
                WHERE id = ?",
                params![
                    conv.user_pubkey, conv.contact_pubkey, conv.contact_name,
                    conv.last_message_event_id, conv.last_timestamp, conv.message_count, conv.cached_at, id
                ],
            )?;
            Ok(id)
        } else {
            conn.execute(
                "INSERT INTO conversations (user_pubkey, contact_pubkey, contact_name, last_message_event_id, last_timestamp, message_count, cached_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(user_pubkey, contact_pubkey) DO UPDATE SET
                    contact_name = excluded.contact_name,
                    last_message_event_id = excluded.last_message_event_id,
                    last_timestamp = excluded.last_timestamp,
                    message_count = excluded.message_count,
                    cached_at = excluded.cached_at",
                params![
                    conv.user_pubkey, conv.contact_pubkey, conv.contact_name,
                    conv.last_message_event_id, conv.last_timestamp, conv.message_count, now
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn get_conversations(&self, user_pubkey: &str) -> Result<Vec<DbConversation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_pubkey, contact_pubkey, contact_name, last_message_event_id, last_timestamp, message_count, cached_at
             FROM conversations 
             WHERE user_pubkey = ?
             ORDER BY last_timestamp DESC"
        )?;
        
        let rows = stmt.query_map(params![user_pubkey], |row| {
            Ok(DbConversation {
                id: Some(row.get(0)?),
                user_pubkey: row.get(1)?,
                contact_pubkey: row.get(2)?,
                contact_name: row.get(3)?,
                last_message_event_id: row.get(4)?,
                last_timestamp: row.get(5)?,
                message_count: row.get(6)?,
                cached_at: row.get(7)?,
            })
        })?;
        
        rows.collect()
    }

    pub fn get_conversation(&self, user_pubkey: &str, contact_pubkey: &str) -> Result<Option<DbConversation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_pubkey, contact_pubkey, contact_name, last_message_event_id, last_timestamp, message_count, cached_at
             FROM conversations 
             WHERE user_pubkey = ? AND contact_pubkey = ?"
        )?;
        
        let mut rows = stmt.query_map(params![user_pubkey, contact_pubkey], |row| {
            Ok(DbConversation {
                id: Some(row.get(0)?),
                user_pubkey: row.get(1)?,
                contact_pubkey: row.get(2)?,
                contact_name: row.get(3)?,
                last_message_event_id: row.get(4)?,
                last_timestamp: row.get(5)?,
                message_count: row.get(6)?,
                cached_at: row.get(7)?,
            })
        })?;
        
        match rows.next() {
            Some(Ok(conv)) => Ok(Some(conv)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn update_conversation_metadata(&self, user_pubkey: &str, contact_pubkey: &str, last_message_event_id: &str, last_timestamp: i64, message_count: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        conn.execute(
            "INSERT INTO conversations (user_pubkey, contact_pubkey, last_message_event_id, last_timestamp, message_count, cached_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_pubkey, contact_pubkey) DO UPDATE SET
                 last_message_event_id = excluded.last_message_event_id,
                 last_timestamp = excluded.last_timestamp,
                 message_count = excluded.message_count,
                 cached_at = excluded.cached_at",
            params![user_pubkey, contact_pubkey, last_message_event_id, last_timestamp, message_count, now],
        )?;
        Ok(())
    }

    pub fn delete_conversation(&self, user_pubkey: &str, contact_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM conversations WHERE user_pubkey = ? AND contact_pubkey = ?",
            params![user_pubkey, contact_pubkey],
        )?;
        Ok(())
    }

    pub fn clear_conversations(&self, user_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM conversations WHERE user_pubkey = ?",
            params![user_pubkey],
        )?;
        Ok(())
    }

    /// Update conversation metadata from messages in direct_messages table
    /// This computes last_message_event_id, last_timestamp, and message_count from actual messages
    pub fn update_conversation_from_messages(&self, user_pubkey: &str, contact_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Get the last message for this conversation
        let mut stmt = conn.prepare(
            "SELECT event_id, created_at 
             FROM direct_messages 
             WHERE (sender_pubkey = ? AND recipient_pubkey = ?) OR (sender_pubkey = ? AND recipient_pubkey = ?)
             ORDER BY created_at DESC LIMIT 1"
        )?;
        
        let last_message: Option<(String, DateTime<Utc>)> = stmt.query_map(
            params![user_pubkey, contact_pubkey, contact_pubkey, user_pubkey],
            |row| Ok((row.get(0)?, row.get(1)?))
        )?.next().transpose()?;
        
        if let Some((event_id, created_at)) = last_message {
            // Get message count
            let mut count_stmt = conn.prepare(
                "SELECT COUNT(*) 
                 FROM direct_messages 
                 WHERE (sender_pubkey = ? AND recipient_pubkey = ?) OR (sender_pubkey = ? AND recipient_pubkey = ?)"
            )?;
            let message_count: i64 = count_stmt.query_row(
                params![user_pubkey, contact_pubkey, contact_pubkey, user_pubkey],
                |row| row.get(0)
            )?;
            
            // Convert DateTime to timestamp (seconds since epoch)
            let last_timestamp = created_at.timestamp();
            
            // Update or insert conversation
            let now = Utc::now();
            conn.execute(
                "INSERT INTO conversations (user_pubkey, contact_pubkey, last_message_event_id, last_timestamp, message_count, cached_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(user_pubkey, contact_pubkey) DO UPDATE SET
                     last_message_event_id = excluded.last_message_event_id,
                     last_timestamp = excluded.last_timestamp,
                     message_count = excluded.message_count,
                     cached_at = excluded.cached_at",
                params![user_pubkey, contact_pubkey, event_id, last_timestamp, message_count, now],
            )?;
        }
        
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: create a temporary database with NOSTR_MAIL_CONFIG pointed at an empty config file.
    /// Returns the Database and the TempDir (must be kept alive for the duration of the test).
    fn create_test_db() -> (Database, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");

        // Write a minimal config so Database::new does not try to load the real one
        let config_path = dir.path().join("test-config.json");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(f, r#"{{"relays":[]}}"#).unwrap();

        // Point the env var at this config
        std::env::set_var("NOSTR_MAIL_CONFIG", config_path.to_str().unwrap());

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).expect("failed to create database");
        (db, dir)
    }

    fn make_contact(pubkey: &str) -> Contact {
        let now = Utc::now();
        Contact {
            id: None,
            pubkey: pubkey.to_string(),
            name: Some("Test User".to_string()),
            email: Some("test@example.com".to_string()),
            picture_url: None,
            picture_data_url: None,
            about: Some("about text".to_string()),
            created_at: now,
            updated_at: now,
            is_public: Some(true),
        }
    }

    fn make_email(message_id: &str) -> Email {
        let now = Utc::now();
        Email {
            id: None,
            message_id: message_id.to_string(),
            from_address: "sender@example.com".to_string(),
            to_address: "recipient@example.com".to_string(),
            subject: "Test Subject".to_string(),
            body: "Test body content".to_string(),
            body_plain: Some("Test body content".to_string()),
            body_html: None,
            received_at: now,
            is_nostr_encrypted: false,
            sender_pubkey: Some("sender_pub".to_string()),
            recipient_pubkey: Some("recipient_pub".to_string()),
            raw_headers: Some("From: sender@example.com".to_string()),
            is_draft: false,
            is_read: false,
            updated_at: None,
            created_at: now,
            signature_valid: None,
            transport_auth_verified: None,
        }
    }

    fn make_dm(event_id: &str, sender: &str, recipient: &str) -> DirectMessage {
        let now = Utc::now();
        DirectMessage {
            id: None,
            event_id: event_id.to_string(),
            sender_pubkey: sender.to_string(),
            recipient_pubkey: recipient.to_string(),
            content: "Hello from DM".to_string(),
            created_at: now,
            received_at: now,
        }
    }

    // =====================
    // Database creation
    // =====================

    #[test]
    fn test_database_new_creates_tables() {
        let (db, _dir) = create_test_db();
        // Verify we can call methods that query all the tables without error
        let contacts = db.get_all_contacts("nonexistent_user").unwrap();
        assert!(contacts.is_empty());

        let relays = db.get_all_relays().unwrap();
        // Should be empty because config has no relays and we set NOSTR_MAIL_CONFIG
        assert!(relays.is_empty());

        let settings = db.get_all_settings("any_pubkey").unwrap();
        assert!(settings.is_empty());
    }

    #[test]
    fn test_database_new_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cfg.json");
        std::fs::write(&config_path, r#"{"relays":[]}"#).unwrap();
        std::env::set_var("NOSTR_MAIL_CONFIG", config_path.to_str().unwrap());

        let db_path = dir.path().join("test.db");
        // Create twice on same path -- should not error
        let _db1 = Database::new(&db_path).unwrap();
        let _db2 = Database::new(&db_path).unwrap();
    }

    // =====================
    // Contact CRUD
    // =====================

    #[test]
    fn test_save_and_get_contact() {
        let (db, _dir) = create_test_db();
        let contact = make_contact("pk_abc123");

        let id = db.save_contact(&contact).unwrap();
        assert!(id > 0);

        let retrieved = db.get_contact("pk_abc123").unwrap();
        assert!(retrieved.is_some());
        let c = retrieved.unwrap();
        assert_eq!(c.pubkey, "pk_abc123");
        assert_eq!(c.name, Some("Test User".to_string()));
        assert_eq!(c.email, Some("test@example.com".to_string()));
        assert_eq!(c.about, Some("about text".to_string()));
    }

    #[test]
    fn test_save_contact_upsert() {
        let (db, _dir) = create_test_db();
        let mut contact = make_contact("pk_upsert");
        db.save_contact(&contact).unwrap();

        // Update name and save again
        contact.name = Some("Updated Name".to_string());
        db.save_contact(&contact).unwrap();

        let retrieved = db.get_contact("pk_upsert").unwrap().unwrap();
        assert_eq!(retrieved.name, Some("Updated Name".to_string()));
    }

    #[test]
    fn test_get_all_contacts_with_user_contacts() {
        let (db, _dir) = create_test_db();
        let c1 = make_contact("pk_1");
        let c2 = make_contact("pk_2");
        db.save_contact(&c1).unwrap();
        db.save_contact(&c2).unwrap();

        // Without user-contact relationships, get_all_contacts returns nothing for any user
        let all = db.get_all_contacts("user_x").unwrap();
        assert_eq!(all.len(), 0);

        // Add user-contact relationships
        db.add_user_contact("user_x", "pk_1", true).unwrap();
        db.add_user_contact("user_x", "pk_2", false).unwrap();

        let all = db.get_all_contacts("user_x").unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_delete_contact() {
        let (db, _dir) = create_test_db();
        let contact = make_contact("pk_delete_me");
        db.save_contact(&contact).unwrap();

        assert!(db.get_contact("pk_delete_me").unwrap().is_some());
        db.delete_contact("pk_delete_me").unwrap();
        assert!(db.get_contact("pk_delete_me").unwrap().is_none());
    }

    #[test]
    fn test_user_follows_contact() {
        let (db, _dir) = create_test_db();
        let contact = make_contact("pk_follow");
        db.save_contact(&contact).unwrap();
        db.add_user_contact("user_a", "pk_follow", true).unwrap();

        assert!(db.user_follows_contact("user_a", "pk_follow").unwrap());
        assert!(!db.user_follows_contact("user_b", "pk_follow").unwrap());
    }

    #[test]
    fn test_remove_user_contact_and_cleanup() {
        let (db, _dir) = create_test_db();
        let contact = make_contact("pk_cleanup");
        db.save_contact(&contact).unwrap();
        db.add_user_contact("user_a", "pk_cleanup", true).unwrap();

        let (success, deleted) = db.remove_user_contact_and_cleanup("user_a", "pk_cleanup").unwrap();
        assert!(success);
        assert!(deleted); // no other users following => contact deleted
        assert!(db.get_contact("pk_cleanup").unwrap().is_none());
    }

    #[test]
    fn test_remove_user_contact_no_cleanup_when_others_follow() {
        let (db, _dir) = create_test_db();
        let contact = make_contact("pk_shared");
        db.save_contact(&contact).unwrap();
        db.add_user_contact("user_a", "pk_shared", true).unwrap();
        db.add_user_contact("user_b", "pk_shared", true).unwrap();

        let (success, deleted) = db.remove_user_contact_and_cleanup("user_a", "pk_shared").unwrap();
        assert!(success);
        assert!(!deleted); // user_b still follows
        assert!(db.get_contact("pk_shared").unwrap().is_some());
    }

    #[test]
    fn test_get_public_contact_pubkeys() {
        let (db, _dir) = create_test_db();
        db.save_contact(&make_contact("pk_pub")).unwrap();
        db.save_contact(&make_contact("pk_priv")).unwrap();
        db.add_user_contact("user_z", "pk_pub", true).unwrap();
        db.add_user_contact("user_z", "pk_priv", false).unwrap();

        let public = db.get_public_contact_pubkeys("user_z").unwrap();
        assert_eq!(public.len(), 1);
        assert_eq!(public[0], "pk_pub");
    }

    #[test]
    fn test_batch_save_contacts() {
        let (db, _dir) = create_test_db();
        let contacts = vec![make_contact("pk_b1"), make_contact("pk_b2"), make_contact("pk_b3")];
        db.batch_save_contacts("user_batch", &contacts, true).unwrap();

        let all = db.get_all_contacts("user_batch").unwrap();
        assert_eq!(all.len(), 3);
    }

    // =====================
    // Email CRUD
    // =====================

    #[test]
    fn test_save_and_get_email() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-001@example.com");

        let id = db.save_email(&email).unwrap();
        assert!(id > 0);

        let retrieved = db.get_email("msg-001@example.com").unwrap();
        assert!(retrieved.is_some());
        let e = retrieved.unwrap();
        assert_eq!(e.message_id, "msg-001@example.com");
        assert_eq!(e.subject, "Test Subject");
        assert_eq!(e.from_address, "sender@example.com");
    }

    #[test]
    fn test_get_email_normalizes_angle_brackets() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-brackets@example.com");
        db.save_email(&email).unwrap();

        // Query with angle brackets should still find it
        let result = db.get_email("<msg-brackets@example.com>").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_save_email_upsert() {
        let (db, _dir) = create_test_db();
        let mut email = make_email("msg-upsert@example.com");
        db.save_email(&email).unwrap();

        email.subject = "Updated Subject".to_string();
        db.save_email(&email).unwrap();

        let retrieved = db.get_email("msg-upsert@example.com").unwrap().unwrap();
        assert_eq!(retrieved.subject, "Updated Subject");
    }

    #[test]
    fn test_get_emails_with_filters() {
        let (db, _dir) = create_test_db();
        let mut e1 = make_email("msg-e1@example.com");
        e1.is_nostr_encrypted = true;
        e1.to_address = "inbox@example.com".to_string();
        db.save_email(&e1).unwrap();

        let mut e2 = make_email("msg-e2@example.com");
        e2.is_nostr_encrypted = false;
        e2.to_address = "inbox@example.com".to_string();
        db.save_email(&e2).unwrap();

        // nostr_only = true
        let nostr_only = db.get_emails(None, None, Some(true), None).unwrap();
        assert_eq!(nostr_only.len(), 1);
        assert_eq!(nostr_only[0].message_id, "msg-e1@example.com");

        // filter by user email
        let by_email = db.get_emails(None, None, None, Some("inbox@example.com")).unwrap();
        assert_eq!(by_email.len(), 2);
    }

    #[test]
    fn test_insert_email_direct() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-direct@example.com");
        let id = db.insert_email_direct(&email).unwrap();
        assert!(id > 0);

        let retrieved = db.get_email("msg-direct@example.com").unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_mark_as_read() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-read@example.com");
        db.save_email(&email).unwrap();

        db.mark_as_read("msg-read@example.com").unwrap();

        let retrieved = db.get_email("msg-read@example.com").unwrap().unwrap();
        assert!(retrieved.is_read);
    }

    // =====================
    // DirectMessage CRUD
    // =====================

    #[test]
    fn test_save_and_get_dm() {
        let (db, _dir) = create_test_db();
        let dm = make_dm("event_001", "sender_pk", "recipient_pk");
        let id = db.save_dm(&dm).unwrap();
        assert!(id > 0);

        let dms = db.get_dms_for_conversation("sender_pk", "recipient_pk").unwrap();
        assert_eq!(dms.len(), 1);
        assert_eq!(dms[0].event_id, "event_001");
        assert_eq!(dms[0].content, "Hello from DM");
    }

    #[test]
    fn test_get_dms_bidirectional() {
        let (db, _dir) = create_test_db();
        db.save_dm(&make_dm("ev_1", "alice", "bob")).unwrap();
        db.save_dm(&make_dm("ev_2", "bob", "alice")).unwrap();

        // Both directions should be returned
        let dms = db.get_dms_for_conversation("alice", "bob").unwrap();
        assert_eq!(dms.len(), 2);
    }

    #[test]
    fn test_save_dm_batch() {
        let (db, _dir) = create_test_db();
        let dms = vec![
            make_dm("batch_ev1", "alice", "bob"),
            make_dm("batch_ev2", "alice", "bob"),
            make_dm("batch_ev3", "bob", "alice"),
        ];
        let inserted = db.save_dm_batch(&dms).unwrap();
        assert_eq!(inserted, 3);

        // Inserting the same batch again should skip all (idempotent)
        let inserted_again = db.save_dm_batch(&dms).unwrap();
        assert_eq!(inserted_again, 0);
    }

    #[test]
    fn test_get_dm_encrypted_content_by_event_id() {
        let (db, _dir) = create_test_db();
        let dm = make_dm("ev_content", "s", "r");
        db.save_dm(&dm).unwrap();

        let content = db.get_dm_encrypted_content_by_event_id("ev_content").unwrap();
        assert_eq!(content, "Hello from DM");
    }

    #[test]
    fn test_get_latest_dm_created_at() {
        let (db, _dir) = create_test_db();
        assert!(db.get_latest_dm_created_at().unwrap().is_none());

        db.save_dm(&make_dm("ev_latest", "a", "b")).unwrap();
        assert!(db.get_latest_dm_created_at().unwrap().is_some());
    }

    // =====================
    // Relay CRUD
    // =====================

    #[test]
    fn test_save_and_get_relays() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        let relay = DbRelay {
            id: None,
            url: "wss://relay.example.com".to_string(),
            is_active: true,
            created_at: now,
            updated_at: now,
        };

        let id = db.save_relay(&relay).unwrap();
        assert!(id > 0);

        let relays = db.get_all_relays().unwrap();
        assert_eq!(relays.len(), 1);
        assert_eq!(relays[0].url, "wss://relay.example.com");
        assert!(relays[0].is_active);
    }

    #[test]
    fn test_delete_relay() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        let relay = DbRelay {
            id: None,
            url: "wss://relay.delete.me".to_string(),
            is_active: true,
            created_at: now,
            updated_at: now,
        };
        db.save_relay(&relay).unwrap();

        db.delete_relay("wss://relay.delete.me").unwrap();
        let relays = db.get_all_relays().unwrap();
        assert!(relays.is_empty());
    }

    #[test]
    fn test_save_relay_update() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        let relay = DbRelay {
            id: None,
            url: "wss://relay.update.me".to_string(),
            is_active: true,
            created_at: now,
            updated_at: now,
        };
        let id = db.save_relay(&relay).unwrap();

        // Update via save with explicit id
        let updated = DbRelay {
            id: Some(id),
            url: "wss://relay.update.me".to_string(),
            is_active: false,
            created_at: now,
            updated_at: now,
        };
        db.save_relay(&updated).unwrap();

        let relays = db.get_all_relays().unwrap();
        assert_eq!(relays.len(), 1);
        assert!(!relays[0].is_active);
    }

    // =====================
    // UserSettings CRUD
    // =====================

    #[test]
    fn test_save_and_get_setting() {
        let (db, _dir) = create_test_db();
        db.save_setting("pubkey_a", "theme", "dark").unwrap();

        let value = db.get_setting("pubkey_a", "theme").unwrap();
        assert_eq!(value, Some("dark".to_string()));
    }

    #[test]
    fn test_get_setting_not_found() {
        let (db, _dir) = create_test_db();
        let value = db.get_setting("nonexistent", "key").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_save_setting_upsert() {
        let (db, _dir) = create_test_db();
        db.save_setting("pk", "key1", "value1").unwrap();
        db.save_setting("pk", "key1", "value2").unwrap();

        let value = db.get_setting("pk", "key1").unwrap();
        assert_eq!(value, Some("value2".to_string()));
    }

    #[test]
    fn test_get_all_settings() {
        let (db, _dir) = create_test_db();
        db.save_setting("pk_all", "a", "1").unwrap();
        db.save_setting("pk_all", "b", "2").unwrap();
        db.save_setting("pk_all", "c", "3").unwrap();

        let settings = db.get_all_settings("pk_all").unwrap();
        assert_eq!(settings.len(), 3);
        assert_eq!(settings.get("a").unwrap(), "1");
        assert_eq!(settings.get("b").unwrap(), "2");
        assert_eq!(settings.get("c").unwrap(), "3");
    }

    #[test]
    fn test_delete_settings_for_pubkey() {
        let (db, _dir) = create_test_db();
        db.save_setting("pk_del", "x", "1").unwrap();
        db.save_setting("pk_del", "y", "2").unwrap();
        db.save_setting("pk_other", "z", "3").unwrap();

        db.delete_settings_for_pubkey("pk_del").unwrap();

        assert!(db.get_all_settings("pk_del").unwrap().is_empty());
        // Other pubkey's settings should be untouched
        assert_eq!(db.get_all_settings("pk_other").unwrap().len(), 1);
    }

    #[test]
    fn test_settings_isolation_between_pubkeys() {
        let (db, _dir) = create_test_db();
        db.save_setting("pk_1", "theme", "dark").unwrap();
        db.save_setting("pk_2", "theme", "light").unwrap();

        assert_eq!(db.get_setting("pk_1", "theme").unwrap(), Some("dark".to_string()));
        assert_eq!(db.get_setting("pk_2", "theme").unwrap(), Some("light".to_string()));
    }

    // =====================
    // Conversation CRUD
    // =====================

    #[test]
    fn test_save_and_get_conversation() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        let conv = DbConversation {
            id: None,
            user_pubkey: "user_pk".to_string(),
            contact_pubkey: "contact_pk".to_string(),
            contact_name: Some("Alice".to_string()),
            last_message_event_id: "ev_last".to_string(),
            last_timestamp: 1700000000,
            message_count: 5,
            cached_at: now,
        };

        let id = db.save_conversation(&conv).unwrap();
        assert!(id > 0);

        let conversations = db.get_conversations("user_pk").unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].contact_pubkey, "contact_pk");
        assert_eq!(conversations[0].contact_name, Some("Alice".to_string()));
        assert_eq!(conversations[0].message_count, 5);
    }

    #[test]
    fn test_save_conversation_upsert() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        let conv = DbConversation {
            id: None,
            user_pubkey: "u".to_string(),
            contact_pubkey: "c".to_string(),
            contact_name: Some("Bob".to_string()),
            last_message_event_id: "ev1".to_string(),
            last_timestamp: 100,
            message_count: 1,
            cached_at: now,
        };
        db.save_conversation(&conv).unwrap();

        // Upsert with new data
        let conv2 = DbConversation {
            id: None,
            user_pubkey: "u".to_string(),
            contact_pubkey: "c".to_string(),
            contact_name: Some("Bob Updated".to_string()),
            last_message_event_id: "ev2".to_string(),
            last_timestamp: 200,
            message_count: 3,
            cached_at: now,
        };
        db.save_conversation(&conv2).unwrap();

        let convs = db.get_conversations("u").unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].last_message_event_id, "ev2");
        assert_eq!(convs[0].message_count, 3);
    }

    #[test]
    fn test_get_conversation_single() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        let conv = DbConversation {
            id: None,
            user_pubkey: "u1".to_string(),
            contact_pubkey: "c1".to_string(),
            contact_name: None,
            last_message_event_id: "ev".to_string(),
            last_timestamp: 50,
            message_count: 2,
            cached_at: now,
        };
        db.save_conversation(&conv).unwrap();

        let result = db.get_conversation("u1", "c1").unwrap();
        assert!(result.is_some());

        let none = db.get_conversation("u1", "c_nonexistent").unwrap();
        assert!(none.is_none());
    }

    #[test]
    fn test_delete_conversation() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        let conv = DbConversation {
            id: None,
            user_pubkey: "u_del".to_string(),
            contact_pubkey: "c_del".to_string(),
            contact_name: None,
            last_message_event_id: "ev".to_string(),
            last_timestamp: 50,
            message_count: 1,
            cached_at: now,
        };
        db.save_conversation(&conv).unwrap();
        db.delete_conversation("u_del", "c_del").unwrap();

        let convs = db.get_conversations("u_del").unwrap();
        assert!(convs.is_empty());
    }

    #[test]
    fn test_clear_conversations() {
        let (db, _dir) = create_test_db();
        let now = Utc::now();
        for i in 0..3 {
            let conv = DbConversation {
                id: None,
                user_pubkey: "u_clear".to_string(),
                contact_pubkey: format!("c_{}", i),
                contact_name: None,
                last_message_event_id: format!("ev_{}", i),
                last_timestamp: 100 + i,
                message_count: 1,
                cached_at: now,
            };
            db.save_conversation(&conv).unwrap();
        }

        db.clear_conversations("u_clear").unwrap();
        let convs = db.get_conversations("u_clear").unwrap();
        assert!(convs.is_empty());
    }

    // =====================
    // Attachment CRUD
    // =====================

    #[test]
    fn test_save_and_get_attachment() {
        let (db, _dir) = create_test_db();
        // First save an email to get an email_id
        let email = make_email("msg-attach@example.com");
        let email_id = db.save_email(&email).unwrap();

        let now = Utc::now();
        let attachment = Attachment {
            id: None,
            email_id,
            filename: "document.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            data: "base64encodeddata".to_string(),
            size: 1024,
            is_encrypted: false,
            encryption_method: None,
            algorithm: None,
            original_filename: None,
            original_type: None,
            original_size: None,
            created_at: now,
        };

        let att_id = db.save_attachment(&attachment).unwrap();
        assert!(att_id > 0);

        let attachments = db.get_attachments_for_email(email_id).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].filename, "document.pdf");
        assert_eq!(attachments[0].size, 1024);
    }

    #[test]
    fn test_delete_attachment() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-att-del@example.com");
        let email_id = db.save_email(&email).unwrap();

        let now = Utc::now();
        let attachment = Attachment {
            id: None,
            email_id,
            filename: "temp.txt".to_string(),
            content_type: "text/plain".to_string(),
            data: "dGVzdA==".to_string(),
            size: 4,
            is_encrypted: false,
            encryption_method: None,
            algorithm: None,
            original_filename: None,
            original_type: None,
            original_size: None,
            created_at: now,
        };
        let att_id = db.save_attachment(&attachment).unwrap();

        db.delete_attachment(att_id).unwrap();
        let attachments = db.get_attachments_for_email(email_id).unwrap();
        assert!(attachments.is_empty());
    }

    #[test]
    fn test_delete_attachments_for_email() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-att-delall@example.com");
        let email_id = db.save_email(&email).unwrap();

        let now = Utc::now();
        for i in 0..3 {
            let attachment = Attachment {
                id: None,
                email_id,
                filename: format!("file{}.txt", i),
                content_type: "text/plain".to_string(),
                data: "data".to_string(),
                size: 4,
                is_encrypted: false,
                encryption_method: None,
                algorithm: None,
                original_filename: None,
                original_type: None,
                original_size: None,
                created_at: now,
            };
            db.save_attachment(&attachment).unwrap();
        }

        assert_eq!(db.get_attachments_for_email(email_id).unwrap().len(), 3);
        db.delete_attachments_for_email(email_id).unwrap();
        assert!(db.get_attachments_for_email(email_id).unwrap().is_empty());
    }

    #[test]
    fn test_attachment_strips_original_metadata() {
        // The save_attachment method should strip original_filename/original_type/original_size
        // for security reasons
        let (db, _dir) = create_test_db();
        let email = make_email("msg-att-meta@example.com");
        let email_id = db.save_email(&email).unwrap();

        let now = Utc::now();
        let attachment = Attachment {
            id: None,
            email_id,
            filename: "encrypted.enc".to_string(),
            content_type: "application/octet-stream".to_string(),
            data: "encrypted_data".to_string(),
            size: 100,
            is_encrypted: true,
            encryption_method: Some("aes-256-gcm".to_string()),
            algorithm: Some("nip44".to_string()),
            original_filename: Some("secret.doc".to_string()), // should be stripped
            original_type: Some("application/msword".to_string()), // should be stripped
            original_size: Some(50), // should be stripped
            created_at: now,
        };
        let att_id = db.save_attachment(&attachment).unwrap();

        let retrieved = db.get_attachment(att_id).unwrap().unwrap();
        // Security: original metadata should NOT be stored
        assert!(retrieved.original_filename.is_none());
        assert!(retrieved.original_type.is_none());
        assert!(retrieved.original_size.is_none());
    }

    // =====================
    // Utility operations
    // =====================

    #[test]
    fn test_clear_all_data() {
        let (db, _dir) = create_test_db();
        db.save_contact(&make_contact("pk_clear")).unwrap();
        db.save_email(&make_email("msg-clear@example.com")).unwrap();
        db.save_setting("pk_clear", "key", "val").unwrap();

        db.clear_all_data().unwrap();

        assert!(db.get_contact("pk_clear").unwrap().is_none());
        assert!(db.get_email("msg-clear@example.com").unwrap().is_none());
        assert!(db.get_all_settings("pk_clear").unwrap().is_empty());
    }

    #[test]
    fn test_compute_content_hash_deterministic() {
        let hash1 = Database::compute_content_hash("hello world");
        let hash2 = Database::compute_content_hash("hello world");
        assert_eq!(hash1, hash2);

        let hash3 = Database::compute_content_hash("different content");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_normalize_message_id() {
        assert_eq!(Database::normalize_message_id("<abc@example.com>"), "abc@example.com");
        assert_eq!(Database::normalize_message_id("abc@example.com"), "abc@example.com");
        assert_eq!(Database::normalize_message_id("  <abc@example.com>  "), "abc@example.com");
    }

    #[test]
    fn test_normalize_gmail_address() {
        assert_eq!(Database::normalize_gmail_address("user@gmail.com"), "user@gmail.com");
        assert_eq!(Database::normalize_gmail_address("user+alias@gmail.com"), "user@gmail.com");
        assert_eq!(Database::normalize_gmail_address("u.s.e.r@gmail.com"), "user@gmail.com");
        assert_eq!(Database::normalize_gmail_address("u.s.e.r+tag@gmail.com"), "user@gmail.com");
        // Non-Gmail should be lowered but not modified
        assert_eq!(Database::normalize_gmail_address("User@example.com"), "user@example.com");
    }

    #[test]
    fn test_find_pubkeys_by_email() {
        let (db, _dir) = create_test_db();
        let mut contact = make_contact("pk_email_search");
        contact.email = Some("findme@example.com".to_string());
        db.save_contact(&contact).unwrap();

        let pubkeys = db.find_pubkeys_by_email("findme@example.com").unwrap();
        assert_eq!(pubkeys.len(), 1);
        assert_eq!(pubkeys[0], "pk_email_search");

        let empty = db.find_pubkeys_by_email("nope@example.com").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_find_pubkeys_by_email_setting() {
        let (db, _dir) = create_test_db();
        db.save_setting("pk_setting_search", "email_address", "mysetting@example.com").unwrap();

        let pubkeys = db.find_pubkeys_by_email_setting("mysetting@example.com").unwrap();
        assert_eq!(pubkeys.len(), 1);
        assert_eq!(pubkeys[0], "pk_setting_search");
    }

    #[test]
    fn test_get_database_size() {
        let (db, _dir) = create_test_db();
        let size = db.get_database_size().unwrap();
        assert!(size > 0);
    }

    #[test]
    fn test_update_conversation_from_messages() {
        let (db, _dir) = create_test_db();
        db.save_dm(&make_dm("ev_conv_1", "alice", "bob")).unwrap();
        db.save_dm(&make_dm("ev_conv_2", "bob", "alice")).unwrap();

        // After saving DMs, conversations should be auto-updated
        let convs_alice = db.get_conversations("alice").unwrap();
        assert!(!convs_alice.is_empty());
        assert_eq!(convs_alice[0].message_count, 2);
    }

    #[test]
    fn test_save_and_get_drafts() {
        let (db, _dir) = create_test_db();
        let mut draft = make_email("draft-001@example.com");
        draft.is_draft = true;
        draft.from_address = "me@example.com".to_string();

        let id = db.save_draft(&draft).unwrap();
        assert!(id > 0);

        let drafts = db.get_drafts(None, None, Some("me@example.com")).unwrap();
        assert_eq!(drafts.len(), 1);
        assert!(drafts[0].is_draft);
        assert_eq!(drafts[0].from_address, "me@example.com");
    }

    #[test]
    fn test_get_drafts_empty() {
        let (db, _dir) = create_test_db();
        let drafts = db.get_drafts(None, None, None).unwrap();
        assert!(drafts.is_empty());
    }

    #[test]
    fn test_save_draft_update() {
        let (db, _dir) = create_test_db();
        let mut draft = make_email("draft-update@example.com");
        draft.is_draft = true;
        draft.from_address = "me@example.com".to_string();
        let id = db.save_draft(&draft).unwrap();

        // Update via save_draft with id
        let mut updated = make_email("draft-update@example.com");
        updated.id = Some(id);
        updated.is_draft = true;
        updated.subject = "Updated Draft Subject".to_string();
        updated.from_address = "me@example.com".to_string();
        db.save_draft(&updated).unwrap();

        let drafts = db.get_drafts(None, None, Some("me@example.com")).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].subject, "Updated Draft Subject");
    }

    #[test]
    fn test_update_signature_valid() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-sig@example.com");
        db.save_email(&email).unwrap();

        db.update_signature_valid("msg-sig@example.com", Some(true)).unwrap();
        let retrieved = db.get_email("msg-sig@example.com").unwrap().unwrap();
        assert_eq!(retrieved.signature_valid, Some(true));

        db.update_signature_valid("msg-sig@example.com", Some(false)).unwrap();
        let retrieved = db.get_email("msg-sig@example.com").unwrap().unwrap();
        assert_eq!(retrieved.signature_valid, Some(false));
    }

    #[test]
    fn test_get_all_dm_pubkeys() {
        let (db, _dir) = create_test_db();
        db.save_dm(&make_dm("ev_pk1", "alice", "bob")).unwrap();
        db.save_dm(&make_dm("ev_pk2", "carol", "alice")).unwrap();

        let pubkeys = db.get_all_dm_pubkeys().unwrap();
        assert!(pubkeys.contains(&"alice".to_string()));
        assert!(pubkeys.contains(&"bob".to_string()));
        assert!(pubkeys.contains(&"carol".to_string()));
        assert_eq!(pubkeys.len(), 3);
    }

    #[test]
    fn test_update_email_pubkeys() {
        let (db, _dir) = create_test_db();
        let email = make_email("msg-pubkey-update@example.com");
        let id = db.save_email(&email).unwrap();

        db.update_email_sender_pubkey("msg-pubkey-update@example.com", "new_sender_pk").unwrap();
        db.update_email_recipient_pubkey("msg-pubkey-update@example.com", "new_recipient_pk").unwrap();

        let retrieved = db.get_email("msg-pubkey-update@example.com").unwrap().unwrap();
        assert_eq!(retrieved.sender_pubkey, Some("new_sender_pk".to_string()));
        assert_eq!(retrieved.recipient_pubkey, Some("new_recipient_pk".to_string()));

        // Also test by_id variants
        db.update_email_sender_pubkey_by_id(id, "id_sender_pk").unwrap();
        db.update_email_recipient_pubkey_by_id(id, "id_recipient_pk").unwrap();
        let retrieved = db.get_email("msg-pubkey-update@example.com").unwrap().unwrap();
        assert_eq!(retrieved.sender_pubkey, Some("id_sender_pk".to_string()));
        assert_eq!(retrieved.recipient_pubkey, Some("id_recipient_pk".to_string()));
    }
}