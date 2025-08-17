// Tauri command parameter naming:
// Tauri automatically converts camelCase keys in JS to snake_case for Rust command parameters.
// For example, passing { userEmail: ... } from JS will be received as user_email in Rust.
// You can use camelCase in JS and it will map to the expected snake_case Rust parameter.
// See: https://tauri.app/v1/guides/features/command/#naming-conventions
// Tauri Service
// Handles all communication with the Rust backend via Tauri commands

const TauriService = {
    invoke: async function(command, args = {}) {
        try {
            return await window.__TAURI__.core.invoke(command, args);
        } catch (error) {
            console.error(`Tauri command failed: ${command}`, error);
            throw error;
        }
    },
    testTauriAvailability: function() {
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
    },
    generateKeypair: async function() {
        return await this.invoke('generate_keypair');
    },
    validatePrivateKey: async function(privateKey) {
        return await this.invoke('validate_private_key', { privateKey });
    },
    validatePublicKey: async function(publicKey) {
        return await this.invoke('validate_public_key', { publicKey });
    },
    getPublicKeyFromPrivate: async function(privateKey) {
        return await this.invoke('get_public_key_from_private', { privateKey });
    },
    encryptNip44Message: async function(privateKey, publicKey, message) {
        return await this.invoke('encrypt_nip04_message', { privateKey, publicKey, message });
    },
    encryptNip04MessageLegacy: async function(privateKey, publicKey, message) {
        return await this.invoke('encrypt_nip04_message_legacy', { privateKey, publicKey, message });
    },
    
    encryptMessageWithAlgorithm: async function(privateKey, publicKey, message, algorithm) {
        return await this.invoke('encrypt_message_with_algorithm', { privateKey, publicKey, message, algorithm });
    },
    sendDirectMessage: async function(privateKey, recipientPubkey, message, relays) {
        // Determine if the message is already encrypted
        const isEncrypted = Utils.isLikelyEncryptedContent(message);
        
        // Get encryption algorithm from settings
        const settings = window.appState?.getSettings();
        const encryptionAlgorithm = settings?.encryption_algorithm || 'nip44';
        
        // Create the request with explicit content type
        const request = {
            sender_private_key: privateKey,
            recipient_pubkey: recipientPubkey,
            content: isEncrypted ? 
                { Encrypted: message } : 
                { Plaintext: message },
            relays: relays,
            encryption_algorithm: encryptionAlgorithm
        };
        
        return await this.invoke('send_direct_message', { request });
    },
    
    sendEncryptedDirectMessage: async function(privateKey, recipientPubkey, encryptedMessage, relays) {
        // Get encryption algorithm from settings
        const settings = window.appState?.getSettings();
        const encryptionAlgorithm = settings?.encryption_algorithm || 'nip44';
        
        // Explicitly send already-encrypted content
        const request = {
            sender_private_key: privateKey,
            recipient_pubkey: recipientPubkey,
            content: { Encrypted: encryptedMessage },
            relays: relays,
            encryption_algorithm: encryptionAlgorithm
        };
        
        return await this.invoke('send_direct_message', { request });
    },
    fetchDirectMessages: async function(privateKey, relays) {
        return await this.invoke('fetch_direct_messages', { privateKey, relays });
    },
    fetchConversations: async function(privateKey, relays) {
        return await this.invoke('fetch_conversations', { privateKey, relays });
    },
    fetchConversationMessages: async function(privateKey, contactPubkey, relays) {
        return await this.invoke('fetch_conversation_messages', { privateKey, contactPubkey, relays });
    },
    fetchProfile: async function(pubkey, relays) {
        return await this.invoke('fetch_profile', { pubkey, relays });
    },
    fetchFollowingProfiles: async function(privateKey, relays) {
        return await this.invoke('fetch_following_profiles', { privateKey, relays });
    },
    fetchProfiles: async function(pubkeys, relays) {
        return await this.invoke('fetch_profiles', { pubkeys, relays });
    },
    publishNostrEvent: async function(privateKey, content, kind, tags, relays) {
        return await this.invoke('publish_nostr_event', { privateKey, content, kind, tags, relays });
    },
    followUser: async function(privateKey, pubkeyToFollow, relays) {
        return await this.invoke('follow_user', { privateKey, pubkeyToFollow, relays });
    },
    updateProfile: async function(privateKey, fields, relays) {
        return await this.invoke('update_profile', { privateKey, fields, relays });
    },
    checkMessageConfirmation: async function(eventId, relays) {
        return await this.invoke('check_message_confirmation', { eventId, relays });
    },
    sendEmail: async function(emailConfig, toAddress, subject, body, nostrNpub = null, messageId = null) {
        const args = { emailConfig, toAddress, subject, body };
        if (nostrNpub) {
            args.nostrNpub = nostrNpub;
        }
        if (messageId) {
            args.messageId = messageId;
        }
        return await this.invoke('send_email', args);
    },
    constructEmailHeaders: async function(emailConfig, toAddress, subject, body, nostrNpub = null, messageId = null) {
        const args = { emailConfig, toAddress, subject, body };
        if (nostrNpub) {
            args.nostrNpub = nostrNpub;
        }
        if (messageId) {
            args.messageId = messageId;
        }
        return await this.invoke('construct_email_headers', args);
    },
    fetchEmails: async function(emailConfig, limit, searchQuery, onlyNostr = true) {
        return await this.invoke('fetch_emails', { emailConfig, limit, searchQuery, onlyNostr });
    },
    testImapConnection: async function(emailConfig) {
        return await this.invoke('test_imap_connection', { emailConfig });
    },
    testSmtpConnection: async function(emailConfig) {
        return await this.invoke('test_smtp_connection', { emailConfig });
    },
    fetchImage: async function(url) {
        return await this.invoke('fetch_image', { url });
    },
    fetchMultipleImages: async function(urls) {
        return await this.invoke('fetch_multiple_images', { urls });
    },
    cacheProfileImage: async function(pubkey, dataUrl) {
        return await this.invoke('cache_profile_image', { pubkey, dataUrl });
    },
    getCachedProfileImage: async function(pubkey) {
        return await this.invoke('get_cached_profile_image', { pubkey });
    },
    getContacts: async function() {
        return await this.invoke('get_contacts');
    },
    setContacts: async function(contacts) {
        return await this.invoke('set_contacts', { contacts });
    },
    saveContact: async function(contact) {
        return await this.invoke('save_contact', { contact });
    },
    getContact: async function(pubkey) {
        return await this.invoke('get_contact', { pubkey });
    },
    updateContactPictureDataUrl: async function(pubkey, pictureDataUrl) {
        return await this.invoke('update_contact_picture_data_url', { pubkey, pictureDataUrl });
    },
    getConversations: async function() {
        return await this.invoke('get_conversations');
    },
    setConversations: async function(conversations) {
        return await this.invoke('set_conversations', { conversations });
    },
    getRelays: async function() {
        return await this.invoke('get_relays');
    },
    setRelays: async function(relays) {
        return await this.invoke('set_relays', { relays });
    },
    updateSingleRelay: async function(relayUrl, isActive) {
        return await this.invoke('update_single_relay', { relayUrl, isActive });
    },
    syncRelayStates: async function() {
        return await this.invoke('sync_relay_states');
    },
    getDbRelays: async function() {
        return await this.invoke('db_get_all_relays');
    },
    getRelayStatus: async function() {
        return await this.invoke('get_relay_status');
    },
    initPersistentNostrClient: async function(privateKey) {
        return await this.invoke('init_persistent_nostr_client', { privateKey });
    },
    disconnectNostrClient: async function() {
        return await this.invoke('disconnect_nostr_client');
    },
    getNostrClientStatus: async function() {
        return await this.invoke('get_nostr_client_status');
    },
    generateQrCode: async function(data) {
        return await this.invoke('generate_qr_code', { data });
    },
    fetchNostrEmailsLast24h: async function(emailConfig) {
        return await this.invoke('fetch_nostr_emails_last_24h', { emailConfig });
    },
    fetchNostrEmailsSmart: async function(emailConfig) {
        return await this.invoke('fetch_nostr_emails_smart', { emailConfig });
    },
    initDatabase: async function() {
        return await this.invoke('init_database');
    },
    syncNostrEmails: async function() {
        const settings = window.appState?.getSettings();
        const keypair = window.appState?.getKeypair();
        
        if (!settings || !keypair) {
            throw new Error('Settings or keypair not available');
        }
        
        const emailConfig = {
            email_address: settings.email_address,
            password: settings.password,
            smtp_host: settings.smtp_host,
            smtp_port: settings.smtp_port,
            imap_host: settings.imap_host,
            imap_port: settings.imap_port,
            use_tls: settings.use_tls,
            private_key: keypair.private_key
        };
        
        return await this.invoke('sync_nostr_emails', { config: emailConfig });
    },

    syncSentEmails: async function() {
        const settings = window.appState?.getSettings();
        const keypair = window.appState?.getKeypair();
        
        if (!settings || !keypair) {
            throw new Error('Settings or keypair not available');
        }
        
        const emailConfig = {
            email_address: settings.email_address,
            password: settings.password,
            smtp_host: settings.smtp_host,
            smtp_port: settings.smtp_port,
            imap_host: settings.imap_host,
            imap_port: settings.imap_port,
            use_tls: settings.use_tls,
            private_key: keypair.private_key
        };
        
        return await this.invoke('sync_sent_emails', { config: emailConfig });
    },

    syncAllEmails: async function() {
        const settings = window.appState?.getSettings();
        const keypair = window.appState?.getKeypair();
        
        if (!settings || !keypair) {
            throw new Error('Settings or keypair not available');
        }
        
        const emailConfig = {
            email_address: settings.email_address,
            password: settings.password,
            smtp_host: settings.smtp_host,
            smtp_port: settings.smtp_port,
            imap_host: settings.imap_host,
            imap_port: settings.imap_port,
            use_tls: settings.use_tls,
            private_key: keypair.private_key
        };
        
        return await this.invoke('sync_all_emails', { config: emailConfig });
    },
    /**
     * Tauri automatically converts camelCase keys in JS to snake_case for Rust command parameters.
     * For example, passing { userEmail: ... } from JS will be received as user_email in Rust.
     * You can use camelCase in JS and it will map to the expected snake_case Rust parameter.
     * See: https://tauri.app/v1/guides/features/command/#naming-conventions
     */
    getDbEmails: async function(limit = 50, offset = 0, nostrOnly = true, userEmail = null) {
        return await this.invoke('db_get_emails', { limit, offset, nostrOnly, userEmail: userEmail });
    },
    getDbSentEmails: async function(limit = 50, offset = 0, userEmail = null) {
        return await this.invoke('db_get_sent_emails', { limit, offset, userEmail: userEmail });
    },
    decryptDmContent: async function(privateKey, senderPubkey, encryptedContent) {
        return await this.invoke('decrypt_dm_content', { privateKey, senderPubkey, encryptedContent });
    },
    filterNewContacts: async function(pubkeys) {
        return await this.invoke('db_filter_new_contacts', { pubkeys });
    },
    findEmailsByMessageId: async function(messageId) {
        return await this.invoke('db_find_emails_by_message_id', { messageId });
    },
    fetchFollowingPubkeys: async function(pubkey, relays) {
        return await this.invoke('fetch_nostr_following_pubkeys', { pubkey, relays });
    },
    // Draft operations
    saveDraft: async function(draft) {
        return await this.invoke('db_save_draft', { draft });
    },

    getDrafts: async function(userEmail) {
        return await this.invoke('db_get_drafts', { userEmail });
    },

    deleteDraft: async function(messageId) {
        return await this.invoke('db_delete_draft', { messageId });
    },

    markAsRead: async function(messageId) {
        return await this.invoke('db_mark_as_read', { messageId });
    }
};
window.TauriService = TauriService; 