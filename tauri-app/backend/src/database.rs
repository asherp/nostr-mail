use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserSettings {
    pub id: Option<i64>,
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
                message_id TEXT UNIQUE NOT NULL,
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
                created_at DATETIME NOT NULL
            )",
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

        // User settings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT UNIQUE NOT NULL,
                value TEXT NOT NULL,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )",
            [],
        )?;

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

        // Create indexes for better performance
        conn.execute("CREATE INDEX IF NOT EXISTS idx_contacts_pubkey ON contacts(pubkey)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_message_id ON emails(message_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_emails_received_at ON emails(received_at)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_event_id ON direct_messages(event_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dms_created_at ON direct_messages(created_at)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_settings_key ON user_settings(key)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_relays_url ON relays(url)", [])?;

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
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        if let Some(id) = email.id {
            conn.execute(
                "UPDATE emails SET 
                    message_id = ?, from_address = ?, to_address = ?, subject = ?, 
                    body = ?, body_plain = ?, body_html = ?, received_at = ?, 
                    is_nostr_encrypted = ?, nostr_pubkey = ?, raw_headers = ?
                WHERE id = ?",
                params![
                    email.message_id, email.from_address, email.to_address, email.subject,
                    email.body, email.body_plain, email.body_html, email.received_at,
                    email.is_nostr_encrypted, email.nostr_pubkey, email.raw_headers, id
                ],
            )?;
            Ok(id)
        } else {
            let _id = conn.execute(
                "INSERT INTO emails (message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    email.message_id, email.from_address, email.to_address, email.subject,
                    email.body, email.body_plain, email.body_html, email.received_at,
                    email.is_nostr_encrypted, email.nostr_pubkey, email.raw_headers, now
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn get_email(&self, message_id: &str) -> Result<Option<Email>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, created_at
             FROM emails WHERE message_id = ?"
        )?;
        
        let mut rows = stmt.query(params![message_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Email {
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
                created_at: row.get(12)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_emails(&self, limit: Option<i64>, offset: Option<i64>, nostr_only: Option<bool>) -> Result<Vec<Email>> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        
        let mut query = String::from(
            "SELECT id, message_id, from_address, to_address, subject, body, body_plain, body_html, received_at, is_nostr_encrypted, nostr_pubkey, raw_headers, created_at
             FROM emails"
        );
        
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        if let Some(nostr_only) = nostr_only {
            query.push_str(" WHERE is_nostr_encrypted = ?");
            params.push(Box::new(nostr_only));
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
                created_at: row.get(12)?,
            })
        })?;
        
        rows.collect()
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

    // Direct message operations
    pub fn save_dm(&self, dm: &DirectMessage) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        
        if let Some(id) = dm.id {
            conn.execute(
                "UPDATE direct_messages SET 
                    event_id = ?, sender_pubkey = ?, recipient_pubkey = ?, content = ?, 
                    created_at = ?, received_at = ?
                WHERE id = ?",
                params![
                    dm.event_id, dm.sender_pubkey, dm.recipient_pubkey, dm.content,
                    dm.created_at, dm.received_at, id
                ],
            )?;
            Ok(id)
        } else {
            let _id = conn.execute(
                "INSERT INTO direct_messages (event_id, sender_pubkey, recipient_pubkey, content, created_at, received_at)
                VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    dm.event_id, dm.sender_pubkey, dm.recipient_pubkey, dm.content,
                    dm.created_at, dm.received_at
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
            conn.execute(
                "INSERT INTO direct_messages (event_id, sender_pubkey, recipient_pubkey, content, created_at, received_at)
                VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    dm.event_id, dm.sender_pubkey, dm.recipient_pubkey, dm.content,
                    dm.created_at, dm.received_at
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

    // Settings operations
    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        
        conn.execute(
            "INSERT OR REPLACE INTO user_settings (key, value, created_at, updated_at)
             VALUES (?, ?, ?, ?)",
            params![key, value, now, now],
        )?;
        
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM user_settings WHERE key = ?")?;
        
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_settings(&self) -> Result<HashMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM user_settings")?;
        
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        
        rows.collect()
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
        println!("[RUST] Updating nostr_pubkey for id {} to {}", id, nostr_pubkey);
        let rows = conn.execute(
            "UPDATE emails SET nostr_pubkey = ?1 WHERE id = ?2",
            params![nostr_pubkey, id],
        )?;
        println!("[RUST] Rows affected by nostr_pubkey update: {}", rows);
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

    pub fn update_contact_picture_data_url(&self, pubkey: &str, picture_data_url: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now();
        conn.execute(
            "UPDATE contacts SET picture_data_url = ?, updated_at = ? WHERE pubkey = ?",
            rusqlite::params![picture_data_url, now, pubkey],
        )?;
        Ok(())
    }
} 