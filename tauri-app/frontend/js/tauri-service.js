// Tauri Service
// Handles all communication with the Rust backend via Tauri commands

const { invoke } = window.__TAURI__.core;

export class TauriService {
    // Helper function to safely call Tauri commands
    static async invoke(command, args = {}) {
        try {
            return await invoke(command, args);
        } catch (error) {
            console.error(`Tauri command failed: ${command}`, error);
            throw error;
        }
    }

    // Test Tauri availability
    static testTauriAvailability() {
        console.log('=== Tauri API Test ===');
        console.log('window.__TAURI__:', window.__TAURI__);
        console.log('window.__TAURI__?.invoke:', window.__TAURI__?.invoke);
        console.log('window.invoke:', window.invoke);
        console.log('window.tauri:', window.tauri);
        console.log('window.__TAURI__?.tauri:', window.__TAURI__?.tauri);
        
        if (window.__TAURI__) {
            console.log('window.__TAURI__ keys:', Object.keys(window.__TAURI__));
            console.log('window.__TAURI__ type:', typeof window.__TAURI__);
            console.log('window.__TAURI__.invoke type:', typeof window.__TAURI__.invoke);
            
            if (typeof window.__TAURI__.invoke === 'function') {
                console.log('Attempting to test invoke with a simple command...');
                window.__TAURI__.invoke('validate_private_key', { privateKey: 'nsec1test' })
                    .then(result => {
                        console.log('Invoke test successful:', result);
                    })
                    .catch(error => {
                        console.error('Invoke test failed:', error);
                    });
            }
        }
        
        console.log('User agent:', navigator.userAgent);
        console.log('=== End Tauri API Test ===');
    }

    // Keypair management
    static async generateKeypair() {
        return await this.invoke('generate_keypair');
    }

    static async validatePrivateKey(privateKey) {
        return await this.invoke('validate_private_key', { privateKey });
    }

    static async validatePublicKey(publicKey) {
        return await this.invoke('validate_public_key', { publicKey });
    }

    static async getPublicKeyFromPrivate(privateKey) {
        return await this.invoke('get_public_key_from_private', { privateKey });
    }

    // Encryption operations
    static async encryptNip04Message(privateKey, publicKey, message) {
        return await this.invoke('encrypt_nip04_message', {
            privateKey,
            publicKey,
            message
        });
    }

    // Nostr operations
    static async sendDirectMessage(privateKey, recipientPubkey, message, relays) {
        return await this.invoke('send_direct_message', {
            privateKey,
            recipientPubkey,
            message,
            relays
        });
    }

    static async fetchDirectMessages(privateKey, relays) {
        return await this.invoke('fetch_direct_messages', {
            privateKey,
            relays
        });
    }

    static async fetchConversations(privateKey, relays) {
        return await this.invoke('fetch_conversations', {
            privateKey,
            relays
        });
    }

    static async fetchConversationMessages(privateKey, contactPubkey, relays) {
        return await this.invoke('fetch_conversation_messages', {
            privateKey,
            contactPubkey,
            relays
        });
    }

    static async fetchProfile(pubkey, relays) {
        return await this.invoke('fetch_profile', {
            pubkey,
            relays
        });
    }

    static async fetchFollowingProfiles(privateKey, relays) {
        return await this.invoke('fetch_following_profiles', {
            privateKey,
            relays
        });
    }

    static async fetchProfiles(pubkeys, relays) {
        return await this.invoke('fetch_profiles', {
            pubkeys,
            relays
        });
    }

    static async publishNostrEvent(privateKey, content, kind, tags, relays) {
        return await this.invoke('publish_nostr_event', {
            privateKey,
            content,
            kind,
            tags,
            relays
        });
    }

    static async followUser(privateKey, pubkeyToFollow, relays) {
        return await this.invoke('follow_user', {
            privateKey,
            pubkeyToFollow,
            relays
        });
    }

    static async updateProfile(privateKey, fields, relays) {
        return await this.invoke('update_profile', {
            privateKey,
            fields,
            relays
        });
    }

    static async checkMessageConfirmation(eventId, relays) {
        return await this.invoke('check_message_confirmation', {
            eventId,
            relays
        });
    }

    // Email operations
    static async sendEmail(emailConfig, toAddress, subject, body) {
        return await this.invoke('send_email', {
            emailConfig,
            toAddress,
            subject,
            body
        });
    }

    static async fetchEmails(emailConfig, limit, searchQuery) {
        return await this.invoke('fetch_emails', {
            emailConfig,
            limit,
            searchQuery
        });
    }

    static async testImapConnection(emailConfig) {
        return await this.invoke('test_imap_connection', { emailConfig });
    }

    static async testSmtpConnection(emailConfig) {
        return await this.invoke('test_smtp_connection', { emailConfig });
    }

    // Image operations
    static async fetchImage(url) {
        return await this.invoke('fetch_image', { url });
    }

    static async fetchMultipleImages(urls) {
        return await this.invoke('fetch_multiple_images', { urls });
    }

    static async cacheProfileImage(pubkey, dataUrl) {
        return await this.invoke('cache_profile_image', {
            pubkey,
            dataUrl
        });
    }

    static async getCachedProfileImage(pubkey) {
        return await this.invoke('get_cached_profile_image', { pubkey });
    }

    // Storage operations
    static async getContacts() {
        return await this.invoke('get_contacts');
    }

    static async setContacts(contacts) {
        return await this.invoke('set_contacts', { contacts });
    }

    static async saveContact(contact) {
        return await this.invoke('save_contact', { contact });
    }

    static async getContact(pubkey) {
        return await this.invoke('get_contact', { pubkey });
    }

    static async updateContactPictureDataUrl(pubkey, pictureDataUrl) {
        return await this.invoke('update_contact_picture_data_url', {
            pubkey,
            pictureDataUrl
        });
    }

    static async getConversations() {
        return await this.invoke('get_conversations');
    }

    static async setConversations(conversations) {
        return await this.invoke('set_conversations', { conversations });
    }

    // Relay operations
    static async getRelays() {
        return await this.invoke('get_relays');
    }

    static async setRelays(relays) {
        return await this.invoke('set_relays', { relays });
    }

    // QR code generation
    static async generateQrCode(data, size = 200) {
        return await this.invoke('generate_qr_code', { data, size });
    }
} 