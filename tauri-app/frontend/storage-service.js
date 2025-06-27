// Storage Service - Handles all backend storage operations
// This replaces localStorage usage with backend storage

class StorageService {
    constructor() {
        this.initialized = false;
    }

    async initialize() {
        if (this.initialized) return;
        this.initialized = true;
    }

    // Contact operations
    async saveContacts(contacts) {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_save_contacts', { contacts });
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to save contacts:', error);
            return false;
        }
    }

    async getContacts() {
        await this.initialize();
        try {
            return await window.__TAURI__.invoke('storage_get_contacts');
        } catch (error) {
            console.error('[STORAGE] Failed to get contacts:', error);
            return [];
        }
    }

    async clearContacts() {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_clear_contacts');
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to clear contacts:', error);
            return false;
        }
    }

    // Conversation operations
    async saveConversations(conversations) {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_save_conversations', { conversations });
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to save conversations:', error);
            return false;
        }
    }

    async getConversations() {
        await this.initialize();
        try {
            return await window.__TAURI__.invoke('storage_get_conversations');
        } catch (error) {
            console.error('[STORAGE] Failed to get conversations:', error);
            return [];
        }
    }

    async clearConversations() {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_clear_conversations');
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to clear conversations:', error);
            return false;
        }
    }

    // User profile operations
    async saveUserProfile(profile) {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_save_user_profile', { profile });
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to save user profile:', error);
            return false;
        }
    }

    async getUserProfile() {
        await this.initialize();
        try {
            return await window.__TAURI__.invoke('storage_get_user_profile');
        } catch (error) {
            console.error('[STORAGE] Failed to get user profile:', error);
            return null;
        }
    }

    async clearUserProfile() {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_clear_user_profile');
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to clear user profile:', error);
            return false;
        }
    }

    // Settings operations
    async saveSettings(settings) {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_save_settings', { settings });
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to save settings:', error);
            return false;
        }
    }

    async getSettings() {
        await this.initialize();
        try {
            return await window.__TAURI__.invoke('storage_get_settings');
        } catch (error) {
            console.error('[STORAGE] Failed to get settings:', error);
            return null;
        }
    }

    // Email draft operations
    async saveEmailDraft(draft) {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_save_email_draft', { draft });
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to save email draft:', error);
            return false;
        }
    }

    async getEmailDraft(id) {
        await this.initialize();
        try {
            return await window.__TAURI__.invoke('storage_get_email_draft', { id });
        } catch (error) {
            console.error('[STORAGE] Failed to get email draft:', error);
            return null;
        }
    }

    async clearEmailDraft(id) {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_clear_email_draft', { id });
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to clear email draft:', error);
            return false;
        }
    }

    // Relay operations
    async saveRelays(relays) {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_save_relays', { relays });
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to save relays:', error);
            return false;
        }
    }

    async getRelays() {
        await this.initialize();
        try {
            return await window.__TAURI__.invoke('storage_get_relays');
        } catch (error) {
            console.error('[STORAGE] Failed to get relays:', error);
            return [];
        }
    }

    // Utility operations
    async clearAllData() {
        await this.initialize();
        try {
            await window.__TAURI__.invoke('storage_clear_all_data');
            return true;
        } catch (error) {
            console.error('[STORAGE] Failed to clear all data:', error);
            return false;
        }
    }

    async getDataSize() {
        await this.initialize();
        try {
            return await window.__TAURI__.invoke('storage_get_data_size');
        } catch (error) {
            console.error('[STORAGE] Failed to get data size:', error);
            return 0;
        }
    }

    // Helper methods for data conversion
    convertToContact(profileData) {
        return {
            pubkey: profileData.pubkey,
            name: profileData.name || null,
            display_name: profileData.display_name || null,
            picture: profileData.picture || null,
            about: profileData.about || null,
            email: profileData.email || null,
            cached_at: new Date().toISOString()
        };
    }

    convertToConversation(conversationData) {
        return {
            contact_pubkey: conversationData.contact_pubkey,
            contact_name: conversationData.contact_name || null,
            last_message: conversationData.last_message || '',
            last_timestamp: conversationData.last_timestamp || Date.now(),
            message_count: conversationData.message_count || 0,
            messages: (conversationData.messages || []).map(msg => ({
                id: msg.id || crypto.randomUUID(),
                sender_pubkey: msg.sender_pubkey,
                receiver_pubkey: msg.receiver_pubkey,
                content: msg.content || '',
                timestamp: msg.timestamp || Date.now(),
                is_sent: msg.is_sent || false
            })),
            cached_at: new Date().toISOString()
        };
    }

    convertToUserProfile(profileData, pictureDataUrl = null) {
        return {
            pubkey: profileData.pubkey || '',
            name: profileData.name || null,
            display_name: profileData.display_name || null,
            picture: profileData.picture || null,
            about: profileData.about || null,
            email: profileData.email || null,
            picture_data_url: pictureDataUrl,
            cached_at: new Date().toISOString()
        };
    }

    convertToAppSettings(settingsData, keypairData = null, darkMode = false) {
        return {
            smtp_host: settingsData.smtp_host || null,
            smtp_port: settingsData.smtp_port || null,
            smtp_username: settingsData.smtp_username || null,
            smtp_password: settingsData.smtp_password || null,
            smtp_use_tls: settingsData.smtp_use_tls || false,
            imap_host: settingsData.imap_host || null,
            imap_port: settingsData.imap_port || null,
            imap_username: settingsData.imap_username || null,
            imap_password: settingsData.imap_password || null,
            imap_use_tls: settingsData.imap_use_tls || false,
            nostr_private_key: keypairData?.private_key || null,
            dark_mode: darkMode,
            cached_at: new Date().toISOString()
        };
    }

    convertToEmailDraft(draftData) {
        return {
            id: draftData.id || crypto.randomUUID(),
            to_address: draftData.to_address || '',
            subject: draftData.subject || '',
            body: draftData.body || '',
            created_at: draftData.created_at || new Date().toISOString(),
            updated_at: new Date().toISOString()
        };
    }
}

// Export for use in main.js
window.StorageService = StorageService; 