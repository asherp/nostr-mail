/**
 * Database Service
 * Handles all database operations through Tauri commands
 */
class DatabaseService {
    /**
     * Initialize the database
     */
    static async initDatabase() {
        try {
            await window.__TAURI__.core.invoke('init_database');
            console.log('Database initialized successfully');
            return true;
        } catch (error) {
            console.error('Failed to initialize database:', error);
            return false;
        }
    }

    // Contact operations
    static async saveContact(contact) {
        try {
            const id = await window.__TAURI__.core.invoke('db_save_contact', { contact });
            console.log('Contact saved with ID:', id);
            return id;
        } catch (error) {
            console.error('Failed to save contact:', error);
            throw error;
        }
    }

    static async getContact(pubkey) {
        try {
            const contact = await window.__TAURI__.core.invoke('db_get_contact', { pubkey });
            return contact;
        } catch (error) {
            console.error('Failed to get contact:', error);
            return null;
        }
    }

    static async getAllContacts() {
        try {
            const contacts = await window.__TAURI__.core.invoke('db_get_all_contacts');
            return contacts;
        } catch (error) {
            console.error('Failed to get all contacts:', error);
            return [];
        }
    }

    static async deleteContact(pubkey) {
        try {
            await window.__TAURI__.core.invoke('db_delete_contact', { pubkey });
            console.log('Contact deleted successfully');
            return true;
        } catch (error) {
            console.error('Failed to delete contact:', error);
            return false;
        }
    }

    // Email operations
    static async saveEmail(email) {
        try {
            const id = await window.__TAURI__.core.invoke('db_save_email', { email });
            console.log('Email saved with ID:', id);
            return id;
        } catch (error) {
            console.error('Failed to save email:', error);
            throw error;
        }
    }

    static async getEmail(messageId) {
        try {
            const email = await window.__TAURI__.core.invoke('db_get_email', { messageId });
            return email;
        } catch (error) {
            console.error('Failed to get email:', error);
            return null;
        }
    }

    static async getEmails(limit = 50, offset = 0, nostrOnly = null) {
        try {
            const emails = await window.__TAURI__.core.invoke('db_get_emails', { 
                limit, 
                offset, 
                nostrOnly 
            });
            return emails;
        } catch (error) {
            console.error('Failed to get emails:', error);
            return [];
        }
    }

    // Direct message operations
    static async saveDirectMessage(dm) {
        try {
            const id = await window.__TAURI__.core.invoke('db_save_dm', { dm });
            console.log('Direct message saved with ID:', id);
            return id;
        } catch (error) {
            console.error('Failed to save direct message:', error);
            throw error;
        }
    }

    static async getDirectMessagesForConversation(userPubkey, contactPubkey) {
        try {
            const messages = await window.__TAURI__.core.invoke('db_get_dms_for_conversation', {
                userPubkey,
                contactPubkey
            });
            return messages;
        } catch (error) {
            console.error('Failed to get direct messages:', error);
            return [];
        }
    }

    // Settings operations
    static async saveSetting(key, value) {
        try {
            await window.__TAURI__.core.invoke('db_save_setting', { key, value });
            console.log('Setting saved successfully');
            return true;
        } catch (error) {
            console.error('Failed to save setting:', error);
            return false;
        }
    }

    static async getSetting(key) {
        try {
            const value = await window.__TAURI__.core.invoke('db_get_setting', { key });
            return value;
        } catch (error) {
            console.error('Failed to get setting:', error);
            return null;
        }
    }

    static async getAllSettings() {
        try {
            const settings = await window.__TAURI__.core.invoke('db_get_all_settings');
            return settings;
        } catch (error) {
            console.error('Failed to get all settings:', error);
            return {};
        }
    }

    // Utility operations
    static async getDatabaseSize() {
        try {
            const size = await window.__TAURI__.core.invoke('db_get_database_size');
            return size;
        } catch (error) {
            console.error('Failed to get database size:', error);
            return 0;
        }
    }

    static async clearAllData() {
        try {
            await window.__TAURI__.core.invoke('db_clear_all_data');
            console.log('All data cleared successfully');
            return true;
        } catch (error) {
            console.error('Failed to clear all data:', error);
            return false;
        }
    }

    // Helper methods for data conversion
    static convertContactToDbFormat(contact) {
        return {
            id: contact.id,
            pubkey: contact.pubkey,
            name: contact.name || null,
            email: contact.email || (contact.fields && contact.fields.email) || null,
            picture_url: contact.picture_url || contact.picture || (contact.fields && contact.fields.picture) || null,
            picture_data_url: contact.picture_data_url || null,
            about: contact.about || (contact.fields && contact.fields.about) || null,
            created_at: contact.created_at || new Date().toISOString(),
            updated_at: new Date().toISOString()
        };
    }

    static convertEmailToDbFormat(email) {
        return {
            id: email.id,
            message_id: email.message_id || email.id,
            from_address: email.from,
            to_address: email.to,
            subject: email.subject,
            body: email.body,
            body_plain: email.bodyPlain || null,
            body_html: email.bodyHtml || null,
            received_at: email.receivedAt || new Date().toISOString(),
            is_nostr_encrypted: email.isNostrEncrypted || false,
            nostr_pubkey: email.nostrPubkey || null,
            created_at: new Date().toISOString()
        };
    }

    static convertDirectMessageToDbFormat(dm) {
        return {
            id: dm.id,
            event_id: dm.eventId,
            sender_pubkey: dm.senderPubkey,
            recipient_pubkey: dm.recipientPubkey,
            content: dm.content,
            decrypted_content: dm.decryptedContent,
            created_at: dm.createdAt || new Date().toISOString(),
            received_at: dm.receivedAt || new Date().toISOString()
        };
    }

    // Migration helpers
    static async migrateFromLocalStorage() {
        try {
            console.log('Starting migration from localStorage to database...');
            
            // Initialize database first
            await this.initDatabase();
            
            // Migrate contacts
            const contacts = JSON.parse(localStorage.getItem('contacts') || '[]');
            for (const contact of contacts) {
                const dbContact = this.convertContactToDbFormat(contact);
                await this.saveContact(dbContact);
            }
            console.log(`Migrated ${contacts.length} contacts`);
            
            // Migrate settings
            const settings = JSON.parse(localStorage.getItem('settings') || '{}');
            for (const [key, value] of Object.entries(settings)) {
                await this.saveSetting(key, JSON.stringify(value));
            }
            console.log(`Migrated ${Object.keys(settings).length} settings`);
            
            // Clear localStorage after successful migration
            localStorage.removeItem('contacts');
            localStorage.removeItem('settings');
            
            console.log('Migration completed successfully');
            return true;
        } catch (error) {
            console.error('Migration failed:', error);
            return false;
        }
    }
} 
window.DatabaseService = DatabaseService; 