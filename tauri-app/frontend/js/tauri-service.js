// Tauri command parameter naming:
// Tauri automatically converts camelCase keys in JS to snake_case for Rust command parameters.
// For example, passing { userEmail: ... } from JS will be received as user_email in Rust.
// You can use camelCase in JS and it will map to the expected snake_case Rust parameter.
// See: https://tauri.app/v1/guides/features/command/#naming-conventions
// Tauri Service
// Handles all communication with the Rust backend via Tauri commands or HTTP

const TauriService = {
    // Detect if running in Tauri or browser
    isTauriAvailable: function() {
        return typeof window !== 'undefined' && window.__TAURI__ && window.__TAURI__.core;
    },
    
    // HTTP API base URL (for browser mode)
    httpBaseUrl: 'http://127.0.0.1:1420',
    
    invoke: async function(command, args = {}) {
        // Use Tauri if available, otherwise use HTTP
        if (this.isTauriAvailable()) {
            try {
                return await window.__TAURI__.core.invoke(command, args);
            } catch (error) {
                console.error(`Tauri command failed: ${command}`, error);
                throw error;
            }
        } else {
            // Use HTTP API
            try {
                const response = await fetch(`${this.httpBaseUrl}/invoke`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({
                        command: command,
                        args: args,
                    }),
                });
                
                if (!response.ok) {
                    throw new Error(`HTTP error! status: ${response.status}`);
                }
                
                const result = await response.json();
                
                if (result.success) {
                    return result.data;
                } else {
                    throw new Error(result.error || 'Unknown error');
                }
            } catch (error) {
                console.error(`HTTP command failed: ${command}`, error);
                throw error;
            }
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
    getDefaultPrivateKeyFromConfig: async function() {
        return await this.invoke('get_default_private_key_from_config');
    },
    signData: async function(privateKey, data) {
        return await this.invoke('sign_data', { privateKey, data });
    },
    verifySignature: async function(publicKey, signature, data) {
        return await this.invoke('verify_signature', { publicKey, signature, data });
    },
    recheckEmailSignature: async function(messageId) {
        return await this.invoke('recheck_email_signature', { messageId });
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
    fetchProfilePersistent: async function(pubkey) {
        return await this.invoke('fetch_profile_persistent', { pubkey });
    },
    decodeNostrIdentifier: async function(identifier) {
        return await this.invoke('decode_nostr_identifier', { identifier });
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
    followUser: async function(privateKey, pubkeyToFollow) {
        return await this.invoke('follow_user', { privateKey, pubkeyToFollow });
    },
    publishFollowList: async function(privateKey, userPubkey, relays) {
        return await this.invoke('publish_follow_list', { privateKey, userPubkey, relays });
    },
    updateProfile: async function(privateKey, fields, relays) {
        return await this.invoke('update_profile', { privateKey, fields, relays });
    },
    // Update profile using persistent client (more efficient)
    updateProfilePersistent: async function(privateKey, fields) {
        return await this.invoke('update_profile_persistent', { privateKey, fields });
    },
    checkMessageConfirmation: async function(eventId, relays) {
        return await this.invoke('check_message_confirmation', { eventId, relays });
    },
    sendEmail: async function(emailConfig, toAddress, subject, body, nostrNpub = null, messageId = null, attachments = null) {
        const args = { emailConfig, toAddress, subject, body };
        if (nostrNpub) {
            args.nostrNpub = nostrNpub;
        }
        if (messageId) {
            args.messageId = messageId;
        }
        if (attachments) {
            args.attachments = attachments;
        }
        return await this.invoke('send_email', args);
    },
    constructEmailHeaders: async function(emailConfig, toAddress, subject, body, nostrNpub = null, messageId = null, attachments = null) {
        const args = { emailConfig, toAddress, subject, body };
        if (nostrNpub) {
            args.nostrNpub = nostrNpub;
        }
        if (messageId) {
            args.messageId = messageId;
        }
        if (attachments) {
            args.attachments = attachments;
        }
        return await this.invoke('construct_email_headers', args);
    },
    fetchEmails: async function(emailConfig, limit, searchQuery, onlyNostr = true, requireSignature = null) {
        return await this.invoke('fetch_emails', { emailConfig, limit, searchQuery, onlyNostr, requireSignature });
    },
    testImapConnection: async function(emailConfig) {
        return await this.invoke('test_imap_connection', { emailConfig });
    },
    testSmtpConnection: async function(emailConfig) {
        return await this.invoke('test_smtp_connection', { emailConfig });
    },
    listImapFolders: async function(emailConfig) {
        return await this.invoke('list_imap_folders', { emailConfig });
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
    getCachedProfileImage: async function(pubkey, pictureUrl) {
        return await this.invoke('get_cached_profile_image', { pubkey, picture_url: pictureUrl || null });
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
    getConversations: async function(userPubkey, privateKey) {
        return await this.invoke('db_get_conversations_with_decrypted_last_message', { 
            userPubkey, 
            privateKey 
        });
    },
    getConversationsWithoutDecryption: async function(userPubkey) {
        return await this.invoke('db_get_conversations', { userPubkey });
    },
    saveConversation: async function(conversation) {
        return await this.invoke('db_save_conversation', { conversation });
    },
    updateConversationMetadata: async function(userPubkey, contactPubkey, lastMessageEventId, lastTimestamp, messageCount) {
        return await this.invoke('db_update_conversation_metadata', {
            userPubkey,
            contactPubkey,
            lastMessageEventId,
            lastTimestamp,
            messageCount
        });
    },
    deleteConversation: async function(userPubkey, contactPubkey) {
        return await this.invoke('db_delete_conversation', { userPubkey, contactPubkey });
    },
    clearConversations: async function(userPubkey) {
        return await this.invoke('db_clear_conversations', { userPubkey });
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
    syncNostrEmails: async function(folder = null) {
        const settings = window.appState?.getSettings();
        const keypair = window.appState?.getKeypair();
        
        if (!settings || !keypair) {
            throw new Error('Settings or keypair not available');
        }
        
        // Ensure use_tls is explicitly set - default to true if not set
        // Most modern email servers require TLS, and the backend enforces it
        const useTls = settings.use_tls !== undefined && settings.use_tls !== null 
            ? settings.use_tls 
            : true; // Default to true for security
        
        const emailConfig = {
            email_address: settings.email_address,
            password: settings.password,
            smtp_host: settings.smtp_host,
            smtp_port: settings.smtp_port,
            imap_host: settings.imap_host,
            imap_port: settings.imap_port,
            use_tls: useTls,
            private_key: keypair.private_key
        };
        
        return await this.invoke('sync_nostr_emails', { config: emailConfig, folder });
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
    searchEmails: async function(searchQuery, userEmail = null, privateKey = null, limit = null, offset = null) {
        return await this.invoke('db_search_emails', { searchQuery, userEmail, privateKey, limit, offset });
    },
    searchSentEmails: async function(searchQuery, userEmail = null, privateKey = null, limit = null, offset = null) {
        return await this.invoke('db_search_sent_emails', { searchQuery, userEmail, privateKey, limit, offset });
    },
    getDbSentEmails: async function(limit = 50, offset = 0, userEmail = null) {
        return await this.invoke('db_get_sent_emails', { limit, offset, userEmail: userEmail });
    },
    getDbEmail: async function(messageId) {
        return await this.invoke('db_get_email', { messageId: messageId });
    },
    decryptDmContent: async function(privateKey, senderPubkey, encryptedContent) {
        return await this.invoke('decrypt_dm_content', { privateKey, senderPubkey, encryptedContent });
    },
    filterNewContacts: async function(userPubkey, pubkeys) {
        return await this.invoke('db_filter_new_contacts', { userPubkey, pubkeys });
    },
    
    // Attachment functions
    getAttachmentsForEmail: async function(emailId) {
        return await this.invoke('db_get_attachments_for_email', { emailId: parseInt(emailId) });
    },
    getAttachment: async function(attachmentId) {
        return await this.invoke('db_get_attachment', { attachmentId: parseInt(attachmentId) });
    },
    saveAttachment: async function(attachment) {
        return await this.invoke('db_save_attachment', { attachment });
    },
    deleteAttachment: async function(attachmentId) {
        return await this.invoke('db_delete_attachment', { attachmentId });
    },
    saveAttachmentToDisk: async function(filename, data, mimeType) {
        return await this.invoke('save_attachment_to_disk', { filename, data, mimeType });
    },
    saveAttachmentsAsZip: async function(zipFilename, attachments) {
        return await this.invoke('save_attachments_as_zip', { zipFilename, attachments });
    },
    findEmailsByMessageId: async function(messageId) {
        return await this.invoke('db_find_emails_by_message_id', { messageId });
    },
    fetchFollowingPubkeys: async function(pubkey, relays) {
        return await this.invoke('fetch_nostr_following_pubkeys', { pubkey, relays });
    },
    // Fetch following pubkeys using persistent client (more efficient)
    fetchFollowingPubkeysPersistent: async function(pubkey) {
        return await this.invoke('fetch_following_pubkeys_persistent', { pubkey });
    },
    // Fetch single profile using persistent client (more efficient)
    fetchProfilePersistent: async function(pubkey) {
        return await this.invoke('fetch_profile_persistent', { pubkey });
    },
    decodeNostrIdentifier: async function(identifier) {
        return await this.invoke('decode_nostr_identifier', { identifier });
    },
    // Fetch profiles using persistent client (more efficient)
    fetchProfilesPersistent: async function(pubkeys) {
        return await this.invoke('fetch_profiles_persistent', { pubkeys });
    },
    // Draft operations
    saveDraft: async function(draft) {
        return await this.invoke('db_save_draft', { draft });
    },

    getDrafts: async function(limit = 50, offset = 0, userEmail = null) {
        return await this.invoke('db_get_drafts', { limit, offset, userEmail: userEmail });
    },

    deleteDraft: async function(messageId) {
        return await this.invoke('db_delete_draft', { messageId });
    },
    
    deleteSentEmail: async function(messageId, userEmail) {
        return await this.invoke('db_delete_sent_email', { messageId, userEmail });
    },

    markAsRead: async function(messageId) {
        return await this.invoke('db_mark_as_read', { messageId });
    },

    // Live Event Subscription System
    startLiveEventSubscription: async function(privateKey) {
        return await this.invoke('start_live_event_subscription', { privateKey });
    },

    stopLiveEventSubscription: async function() {
        return await this.invoke('stop_live_event_subscription');
    },

    getLiveSubscriptionStatus: async function() {
        return await this.invoke('get_live_subscription_status');
    },
    
    // Settings with pubkey association
    dbSaveSetting: async function(pubkey, key, value) {
        return await this.invoke('db_save_setting', { pubkey, key, value });
    },
    
    dbGetSetting: async function(pubkey, key) {
        return await this.invoke('db_get_setting', { pubkey, key });
    },
    
    dbGetAllSettings: async function(pubkey, privateKey) {
        return await this.invoke('db_get_all_settings', { pubkey, privateKey });
    },
    
    dbSaveSettingsBatch: async function(pubkey, settings, privateKey) {
        return await this.invoke('db_save_settings_batch', { pubkey, settings, privateKey });
    },
    
    // Update email recipient pubkey
    updateEmailRecipientPubkey: async function(messageId, recipientPubkey) {
        return await this.invoke('db_update_email_recipient_pubkey', { messageId, recipientPubkey });
    },
    
    updateEmailRecipientPubkeyById: async function(id, recipientPubkey) {
        return await this.invoke('db_update_email_recipient_pubkey_by_id', { id, recipientPubkey });
    }
};
window.TauriService = TauriService; 