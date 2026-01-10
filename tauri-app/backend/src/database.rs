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
    pub nostr_pubkey: Option<String>,
    pub raw_headers: Option<String>,
    pub is_draft: bool,
    pub is_read: bool,
    pub updated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
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
        Ok(db)
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
                nostr_pubkey TEXT,
                raw_headers TEXT,
                is_draft BOOLEAN NOT NULL DEFAULT 0,
                is_read BOOLEAN NOT NULL DEFAULT 0,
                updated_at DATETIME,
                created_at DATETIME NOT NULL
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

        // Create indexes for better performance
        conn.execute("CREATE INDEX IF NOT EXISTS idx_contacts_pubkey ON contacts(pubkey)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_message_id ON emails(message_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_received_at ON emails(received_at)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_subject_hash ON emails(subject_hash)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_event_id ON direct_messages(event_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_created_at ON direct_messages(created_at)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_content_hash ON direct_messages(content_hash)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_settings_key ON user_settings(key)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_settings_pubkey ON user_settings(pubkey)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_settings_pubkey_key ON user_settings(pubkey, key)", [])?;
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
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_contacts(&self) -> Result<Vec<Contact>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, pubkey, name, email, picture_url, picture_data_url, about, created_at, updated_at
             FROM contacts ORDER BY name COLLATE NOCASE"
        )?;
        
        let rows = stmt.query_map([], |row| {
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
                    is_nostr_encrypted = ?, nostr_pubkey = ?, raw_headers = ?, is_draft = ?, is_read = ?, updated_at = ?,
                    subject_hash = ?
                WHERE id = ?",
                params![
                    email.message_id, email.from_address, email.to_address, email.subject,
                    email.body, email.body_plain, email.body_html, email.received_at,
                    email.is_nostr_encrypted, email.nostr_pubkey, email.raw_headers, email.is_draft, email.is_read, now,
                    subject_hash, id
                ],
            )?;
            println!("[DB] save_email: Successfully updated email id={}", id);
            Ok(id)
        } else {
            println!("[DB] save_email: Checking if email with message_id {} already exists", email.message_id);
            // Check if email with this message_id already exists (normalized comparison)
            match self.get_email(&email.message_id) {
                Ok(Some(existing_email)) => {
                    // Email already exists, update it instead of creating duplicate
                    println!("[DB] Email with message_id {} already exists (id: {}), updating instead of creating duplicate", email.message_id, existing_email.id.unwrap_or(0));
                    drop(conn); // Release lock before recursive call
                    return self.save_email(&Email {
                        id: existing_email.id,
                        ..email.clone()
                    });
                }
                Ok(None) => {
                    println!("[DB] save_email: Email is new, inserting into database");
                    println!("[DB] save_email: About to execute INSERT statement");
                    match conn.execute(
                        "INSERT INTO emails (message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, created_at, subject_hash)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        params![
                            email.message_id, email.from_address, email.to_address, email.subject,
                            email.body, email.body_plain, email.body_html, email.received_at,
                            email.is_nostr_encrypted, email.nostr_pubkey, email.raw_headers, email.is_draft, email.is_read, now,
                            subject_hash
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
                Err(e) => {
                    println!("[DB] save_email: ERROR checking if email exists: {}", e);
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
            "INSERT INTO emails (message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, created_at, subject_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                email.message_id, email.from_address, email.to_address, email.subject,
                email.body, email.body_plain, email.body_html, email.received_at,
                email.is_nostr_encrypted, email.nostr_pubkey, email.raw_headers, email.is_draft, email.is_read, now,
                subject_hash
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
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, updated_at, created_at
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
                nostr_pubkey: row.get(10)?,
                raw_headers: row.get(11)?,
                is_draft: row.get(12)?,
                is_read: row.get(13)?,
                updated_at: row.get(14)?,
                created_at: row.get(15)?,
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
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, updated_at, created_at FROM emails"
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
                nostr_pubkey: row.get(10)?,
                raw_headers: row.get(11)?,
                is_draft: row.get(12)?,
                is_read: row.get(13)?,
                updated_at: row.get(14)?,
                created_at: row.get(15)?,
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
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, updated_at, created_at FROM emails"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut where_clauses = Vec::new();
        
        // Always exclude drafts from sent emails
        where_clauses.push("is_draft = 0");
        
        if let Some(email) = user_email {
            // Normalize the user email for Gmail addresses
            let normalized_user_email = Self::normalize_gmail_address(email);
            let user_email_lower = email.trim().to_lowercase();
            println!("[DB] get_sent_emails: user_email={}, normalized={}", email, normalized_user_email);
            
            // For Gmail addresses, always match both the original and normalized versions
            // This handles cases where:
            // - User receives at apembroke+nostr@gmail.com but sends from apembroke@gmail.com
            // - User sends from apembroke+nostr@gmail.com but Gmail stores it as apembroke@gmail.com
            if email.contains("@gmail.com") {
                if normalized_user_email != user_email_lower {
                    // User email has a + alias, match both versions
                    println!("[DB] get_sent_emails: Matching both original ({}) and normalized ({}) Gmail addresses", user_email_lower, normalized_user_email);
                    where_clauses.push("(LOWER(TRIM(from_address)) = LOWER(TRIM(?)) OR LOWER(TRIM(from_address)) = LOWER(TRIM(?)))");
                    params.push(Box::new(user_email_lower.clone()));
                    params.push(Box::new(normalized_user_email.clone()));
                } else {
                    // User email is already normalized (no + alias), but still match both to handle from_address with + aliases
                    println!("[DB] get_sent_emails: Matching Gmail address {} and normalized version", user_email_lower);
                    where_clauses.push("(LOWER(TRIM(from_address)) = LOWER(TRIM(?)) OR LOWER(TRIM(from_address)) = LOWER(TRIM(?)))");
                    params.push(Box::new(user_email_lower.clone()));
                    params.push(Box::new(normalized_user_email.clone()));
                }
            } else {
                // Non-Gmail, just match exactly
                where_clauses.push("LOWER(TRIM(from_address)) = LOWER(TRIM(?))");
                params.push(Box::new(user_email_lower));
                println!("[DB] get_sent_emails: Matching non-Gmail email address");
            }
        } else {
            println!("[DB] get_sent_emails: No user_email filter provided, returning all non-draft emails");
        }
        if !where_clauses.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&where_clauses.join(" AND "));
        }
        query.push_str(" ORDER BY received_at DESC LIMIT ? OFFSET ?");
        params.push(Box::new(limit));
        params.push(Box::new(offset));
        
        println!("[DB] get_sent_emails: Executing query: {}", query);

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
                nostr_pubkey: row.get(10)?,
                raw_headers: row.get(11)?,
                is_draft: row.get(12)?,
                is_read: row.get(13)?,
                updated_at: row.get(14)?,
                created_at: row.get(15)?,
            })
        })?;
        let emails: Vec<Email> = rows.collect::<Result<Vec<_>, _>>()?;
        println!("[DB] get_sent_emails: Found {} emails matching query", emails.len());
        for (i, email) in emails.iter().enumerate() {
            println!("[DB] get_sent_emails: Email {}: id={:?}, from={}, subject={}", 
                i + 1, email.id, email.from_address, email.subject);
        }
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
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, updated_at, created_at
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
                nostr_pubkey: row.get(10)?,
                raw_headers: row.get(11)?,
                is_draft: row.get(12)?,
                is_read: row.get(13)?,
                updated_at: row.get(14)?,
                created_at: row.get(15)?,
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
        
        if let Some(id) = attachment.id {
            // Update existing attachment
            conn.execute(
                "UPDATE attachments SET 
                    email_id = ?, filename = ?, content_type = ?, data = ?, size = ?,
                    is_encrypted = ?, encryption_method = ?, algorithm = ?, 
                    original_filename = ?, original_type = ?, original_size = ?
                WHERE id = ?",
                params![
                    attachment.email_id, attachment.filename, attachment.content_type, 
                    attachment.data, attachment.size as i64, attachment.is_encrypted,
                    attachment.encryption_method, attachment.algorithm,
                    attachment.original_filename, attachment.original_type, 
                    attachment.original_size.map(|s| s as i64), id
                ],
            )?;
            Ok(id)
        } else {
            // Insert new attachment
            conn.execute(
                "INSERT INTO attachments (
                    email_id, filename, content_type, data, size, is_encrypted,
                    encryption_method, algorithm, original_filename, original_type, 
                    original_size, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    attachment.email_id, attachment.filename, attachment.content_type,
                    attachment.data, attachment.size as i64, attachment.is_encrypted,
                    attachment.encryption_method, attachment.algorithm,
                    attachment.original_filename, attachment.original_type,
                    attachment.original_size.map(|s| s as i64), now
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

    pub fn update_email_nostr_pubkey(&self, message_id: &str, nostr_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        println!("[RUST] Updating nostr_pubkey for message_id {} to {}", message_id, nostr_pubkey);
        let rows = conn.execute(
            "UPDATE emails SET nostr_pubkey = ?1 WHERE message_id = ?2",
            params![nostr_pubkey, message_id],
        )?;
        println!("[RUST] Rows affected by nostr_pubkey update: {}", rows);
        Ok(())
    }

    pub fn update_email_nostr_pubkey_by_id(&self, id: i64, nostr_pubkey: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE emails SET nostr_pubkey = ? WHERE id = ?",
            params![nostr_pubkey, id],
        )?;
        Ok(())
    }

    pub fn find_emails_by_message_id(&self, message_id: &str) -> Result<Vec<Email>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, updated_at, created_at
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
                nostr_pubkey: row.get(10)?,
                raw_headers: row.get(11)?,
                is_draft: row.get(12)?,
                is_read: row.get(13)?,
                updated_at: row.get(14)?,
                created_at: row.get(15)?,
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
                    is_nostr_encrypted = ?, nostr_pubkey = ?, raw_headers = ?, 
                    is_draft = ?, is_read = ?, updated_at = ?
                WHERE id = ?",
                params![
                    draft.message_id, draft.from_address, draft.to_address, draft.subject,
                    draft.body, draft.body_plain, draft.body_html, draft.received_at,
                    draft.is_nostr_encrypted, draft.nostr_pubkey, draft.raw_headers,
                    true, draft.is_read, now, id
                ],
            )?;
            Ok(id)
        } else {
            let _id = conn.execute(
                "INSERT INTO emails (message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    draft.message_id, draft.from_address, draft.to_address, draft.subject,
                    draft.body, draft.body_plain, draft.body_html, draft.received_at,
                    draft.is_nostr_encrypted, draft.nostr_pubkey, draft.raw_headers,
                    true, draft.is_read, now
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn get_drafts(&self, user_email: Option<&str>) -> Result<Vec<Email>> {
        let conn = self.conn.lock().unwrap();
        let mut query = String::from(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, is_draft, is_read, updated_at, created_at FROM emails WHERE is_draft = 1"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        if let Some(email) = user_email {
            query.push_str(" AND from_address = ?");
            params.push(Box::new(email));
        }
        
        query.push_str(" ORDER BY updated_at DESC, created_at DESC");
        
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
                nostr_pubkey: row.get(10)?,
                raw_headers: row.get(11)?,
                is_draft: row.get(12)?,
                is_read: row.get(13)?,
                updated_at: row.get(14)?,
                created_at: row.get(15)?,
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