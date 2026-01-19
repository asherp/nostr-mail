use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub pubkey: String,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub picture: Option<String>,
    pub picture_data_url: Option<String>,
    pub about: Option<String>,
    pub email: Option<String>,
    pub cached_at: DateTime<Utc>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub pubkey: String,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub picture: Option<String>,
    pub about: Option<String>,
    pub email: Option<String>,
    pub picture_data_url: Option<String>,
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_use_tls: bool,
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub imap_username: Option<String>,
    pub imap_password: Option<String>,
    pub imap_use_tls: bool,
    pub nostr_private_key: Option<String>,
    pub dark_mode: bool,
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDraft {
    pub id: String,
    pub to_address: String,
    pub subject: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageData {
    pub contacts: HashMap<String, Contact>,
    pub user_profile: Option<UserProfile>,
    pub settings: Option<AppSettings>,
    pub email_drafts: HashMap<String, EmailDraft>,
    pub relays: Vec<Relay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relay {
    pub url: String,
    pub is_active: bool,
}

pub struct Storage {
    _data_dir: PathBuf,
    data_file: PathBuf,
}

impl Storage {
    pub fn new() -> Result<Self> {
        let data_dir = Self::get_data_dir()?;
        let data_file = data_dir.join("nostr_mail_data.json");
        
        // Create data directory if it doesn't exist
        if !data_dir.exists() {
            fs::create_dir_all(&data_dir)?;
        }
        
        Ok(Self { _data_dir: data_dir, data_file })
    }
    
    fn get_data_dir() -> Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            let app_data = std::env::var("APPDATA")?;
            Ok(PathBuf::from(app_data).join("NostrMail"))
        }
        
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME")?;
            Ok(PathBuf::from(home).join("Library/Application Support/NostrMail"))
        }
        
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var("HOME")?;
            Ok(PathBuf::from(home).join(".config/nostr-mail"))
        }

        #[cfg(target_os = "android")]
        {
            let base = std::env::var("HOME").unwrap_or_else(|_| "/data/data".to_string());
            Ok(PathBuf::from(base).join("nostr-mail"))
        }
    }
    
    pub fn load_data(&self) -> Result<StorageData> {
        if !self.data_file.exists() {
            return Ok(StorageData {
                contacts: HashMap::new(),
                user_profile: None,
                settings: None,
                email_drafts: HashMap::new(),
                relays: Self::load_default_relays_from_file(),
            });
        }
        
        let content = fs::read_to_string(&self.data_file)?;
        // Use a temporary struct that includes conversations for backward compatibility
        #[derive(Deserialize)]
        struct StorageDataWithConversations {
            contacts: HashMap<String, Contact>,
            #[serde(default)]
            #[allow(dead_code)]
            conversations: HashMap<String, serde_json::Value>, // Ignore old conversations
            user_profile: Option<UserProfile>,
            settings: Option<AppSettings>,
            email_drafts: HashMap<String, EmailDraft>,
            relays: Vec<Relay>,
        }
        
        let temp_data: StorageDataWithConversations = serde_json::from_str(&content)?;
        Ok(StorageData {
            contacts: temp_data.contacts,
            user_profile: temp_data.user_profile,
            settings: temp_data.settings,
            email_drafts: temp_data.email_drafts,
            relays: temp_data.relays,
        })
    }
    
    pub fn save_data(&self, data: &StorageData) -> Result<()> {
        println!("[STORAGE] save_data: Serializing data to JSON...");
        let content = serde_json::to_string_pretty(data)?;
        println!("[STORAGE] save_data: JSON serialized ({} bytes), writing to file...", content.len());
        fs::write(&self.data_file, content)?;
        println!("[STORAGE] save_data: File written successfully");
        Ok(())
    }
    
    // Contact operations
    pub fn save_contacts(&self, contacts: Vec<Contact>) -> Result<()> {
        let mut data = self.load_data()?;
        data.contacts.clear();
        
        for contact in contacts {
            data.contacts.insert(contact.pubkey.clone(), contact);
        }
        
        self.save_data(&data)
    }
    
    pub fn get_contacts(&self) -> Result<Vec<Contact>> {
        let data = self.load_data()?;
        Ok(data.contacts.values().cloned().collect())
    }
    
    pub fn clear_contacts(&self) -> Result<()> {
        let mut data = self.load_data()?;
        data.contacts.clear();
        self.save_data(&data)
    }
    
    // Individual contact operations for efficiency
    pub fn save_contact(&self, contact: Contact) -> Result<()> {
        let mut data = self.load_data()?;
        data.contacts.insert(contact.pubkey.clone(), contact);
        self.save_data(&data)
    }
    
    pub fn get_contact(&self, pubkey: &str) -> Result<Option<Contact>> {
        let data = self.load_data()?;
        Ok(data.contacts.get(pubkey).cloned())
    }
    
    pub fn update_contact_picture_data_url(&self, pubkey: &str, picture_data_url: String) -> Result<()> {
        // Only store if valid image data URL
        let is_valid = picture_data_url.starts_with("data:image") && picture_data_url != "data:application/octet-stream;base64," && !picture_data_url.trim().is_empty();
        if !is_valid {
            println!("[STORAGE] Not storing invalid picture_data_url for {}", pubkey);
            return Ok(()); // No-op
        }
        let mut data = self.load_data()?;
        if let Some(contact) = data.contacts.get_mut(pubkey) {
            contact.picture_data_url = Some(picture_data_url);
            contact.cached_at = chrono::Utc::now();
            self.save_data(&data)
        } else {
            Err(anyhow::anyhow!("Contact not found: {}", pubkey))
        }
    }
    
    // Profile operations
    pub fn save_user_profile(&self, profile: UserProfile) -> Result<()> {
        let mut data = self.load_data()?;
        data.user_profile = Some(profile);
        self.save_data(&data)
    }
    
    pub fn get_user_profile(&self) -> Result<Option<UserProfile>> {
        let data = self.load_data()?;
        Ok(data.user_profile)
    }
    
    pub fn clear_user_profile(&self) -> Result<()> {
        let mut data = self.load_data()?;
        data.user_profile = None;
        self.save_data(&data)
    }
    
    // Settings operations
    pub fn save_settings(&self, settings: AppSettings) -> Result<()> {
        let mut data = self.load_data()?;
        data.settings = Some(settings);
        self.save_data(&data)
    }
    
    pub fn get_settings(&self) -> Result<Option<AppSettings>> {
        let data = self.load_data()?;
        Ok(data.settings)
    }
    
    // Email draft operations
    pub fn save_email_draft(&self, draft: EmailDraft) -> Result<()> {
        let mut data = self.load_data()?;
        data.email_drafts.insert(draft.id.clone(), draft);
        self.save_data(&data)
    }
    
    pub fn get_email_draft(&self, id: &str) -> Result<Option<EmailDraft>> {
        let data = self.load_data()?;
        Ok(data.email_drafts.get(id).cloned())
    }
    
    pub fn clear_email_draft(&self, id: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.email_drafts.remove(id);
        self.save_data(&data)
    }
    
    // Relay operations
    pub fn save_relays(&self, relays: Vec<Relay>) -> Result<()> {
        let mut data = self.load_data()?;
        data.relays = relays;
        self.save_data(&data)
    }
    
    pub fn get_relays(&self) -> Result<Vec<Relay>> {
        let data = self.load_data()?;
        Ok(data.relays)
    }
    
    /// Load default relays from JSON file, with fallback to hardcoded defaults
    fn load_default_relays_from_file() -> Vec<Relay> {
        // Try to load from file in backend directory
        let json_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("nostr-mail-config.json");
        
        if let Ok(json_content) = fs::read_to_string(&json_path) {
            println!("[STORAGE] Loading default relays from: {:?}", json_path);
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&json_content) {
                if let Some(relays_array) = config.get("relays").and_then(|r| r.as_array()) {
                    let mut loaded_relays = Vec::new();
                    for relay_obj in relays_array {
                        if let (Some(url), is_active) = (
                            relay_obj.get("url").and_then(|u| u.as_str()),
                            relay_obj.get("is_active").and_then(|a| a.as_bool()).unwrap_or(true)
                        ) {
                            loaded_relays.push(Relay {
                                url: url.to_string(),
                                is_active,
                            });
                        }
                    }
                    if !loaded_relays.is_empty() {
                        println!("[STORAGE] Loaded {} relay(s) from JSON file", loaded_relays.len());
                        return loaded_relays;
                    }
                }
            }
            println!("[STORAGE] Failed to parse JSON file, using hardcoded defaults");
        } else {
            println!("[STORAGE] Default relays file not found at {:?}, using hardcoded defaults", json_path);
        }
        
        // Fallback to hardcoded defaults
        vec![
            Relay {
                url: "wss://relay.damus.io".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://nos.lol".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://relay.nostr.pub".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://nostr.rocks".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://nostr.mom".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://nostr.wine".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://cache1.primal.net".to_string(),
                is_active: true,
            },
            Relay {
                url: "wss://relay.nostr.band".to_string(),
                is_active: true,
            },
        ]
    }
    
    // Utility methods
    pub fn clear_all_data(&self) -> Result<()> {
        if self.data_file.exists() {
            fs::remove_file(&self.data_file)?;
        }
        Ok(())
    }
    
    pub fn get_data_size(&self) -> Result<u64> {
        if self.data_file.exists() {
            let metadata = fs::metadata(&self.data_file)?;
            Ok(metadata.len())
        } else {
            Ok(0)
        }
    }
} 