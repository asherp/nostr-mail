// Main Application
// Coordinates all modules and handles application initialization

// Remove all import/export statements. Ensure all code is in the global scope or attached to window. No ES module syntax. Replace any usage of imported symbols with window equivalents if needed.
// Try to import Tauri APIs if available
let tauriDialog, tauriFs;
try {
    tauriDialog = window.__TAURI__ ? window.__TAURI__.dialog : undefined;
    tauriFs = window.__TAURI__ ? window.__TAURI__.fs : undefined;
} catch (e) {
    tauriDialog = undefined;
    tauriFs = undefined;
}

function NostrMailApp() {
    this.initialized = false;
    this.mobileNavState = 'navbar'; // 'navbar' or 'page'
    this.swipeStartX = null;
    this.swipeStartY = null;
    this.swipeThreshold = 50; // Minimum distance for swipe
}

// Initialize the application
NostrMailApp.prototype.init = async function() {
    // #region agent log
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:24',message:'init() function entry',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
    // #endregion
    console.log('🚀 ========================================');
    console.log('🚀   NostrMail - Starting Application');
    console.log('🚀 ========================================');
    console.log('📧 Email + 🔐 Nostr Integration');
    console.log('🌐 Version: 1.0.0-beta');
    console.log('⏰ Started at:', new Date().toLocaleString());
    console.log('🚀 ========================================');
    // Initialize the database
    try {
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:34',message:'Before TauriService.initDatabase()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        await TauriService.initDatabase();
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:35',message:'After TauriService.initDatabase() - success',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        console.log('Database initialized');
    } catch (e) {
        if (e && e.toString().includes('already initialized')) {
            console.log('Database already initialized, continuing...');
        } else {
            console.error('Failed to initialize database:', e);
            notificationService.showError('Failed to initialize database');
            return;
        }
    }
    try {
        console.log('📋 Loading application settings...');
        // Settings will be loaded after keypair is loaded (in loadKeypair)
        // This ensures we load settings for the correct pubkey

        console.log('🌐 Loading relay configuration from database...');
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:51',message:'Before loadRelaysFromDatabase()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        await this.loadRelaysFromDatabase();
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:51',message:'After loadRelaysFromDatabase()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion

        console.log('🔑 Loading/generating keypair...');
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:54',message:'Before loadKeypair()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        await this.loadKeypair();
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:54',message:'After loadKeypair()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        
        console.log('🔄 Initializing live event subscription...');
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:57',message:'Before initializeLiveEvents()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        await this.initializeLiveEvents();
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:57',message:'After initializeLiveEvents()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        
        console.log('🎯 Setting up event listeners...');
        this.setupEventListeners();
        
        console.log('📱 Initializing mobile navigation...');
        this.initializeMobileNavigation();
        
        console.log('📬 Loading initial data...');
        // Load contacts first so DM contacts can access cached profile photos
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:67',message:'Before contactsService.loadContacts()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        await contactsService.loadContacts();
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:67',message:'After contactsService.loadContacts()',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        
        // Ensure default nostr-mail contact is added for this user
        console.log('📇 Ensuring default nostr-mail contact...');
        await this.ensureDefaultContact();
        
        // NOTE: We do NOT load emails here. Emails are only loaded when the inbox tab is clicked.
        // await emailService.loadEmails(); // <-- Remove or comment out this line so emails are not loaded on startup
        // await dmService.loadDmContacts(); // TODO: add this back in once we have stored DMs in the DB
        
        // Populate Nostr contact dropdown for compose page
        if (window.emailService) {
            window.emailService.populateNostrContactDropdown();

            // Initialize attachment functionality
            window.emailService.initializeAttachmentListeners();

            // Don't restore contact selection on startup - start with empty selection
        }
        
        console.log('✅ ========================================');
        console.log('✅   NostrMail - Successfully Started!');
        console.log('✅ ========================================');
        console.log('🎉 Application is ready for use');
        console.log('📱 UI: Modern email client with Nostr integration');
        console.log('🔐 Features: Email, DMs, Contacts, Profile Management');
        console.log('✅ ========================================');
        
        this.initialized = true;
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:95',message:'init() completed successfully',data:{hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
    } catch (error) {
        console.error('❌ ========================================');
        console.error('❌   NostrMail - Startup Failed!');
        console.error('❌ ========================================');
        console.error('💥 Error during initialization:', error);
        console.error('❌ ========================================');
    }
}

// Load settings from localStorage
NostrMailApp.prototype.loadSettings = async function() {
    try {
        // First try to load from database based on current pubkey
        const keypair = appState.getKeypair();
        if (keypair && keypair.public_key) {
            try {
                const dbSettings = await TauriService.dbGetAllSettings(keypair.public_key, keypair.private_key);
                if (dbSettings && Object.keys(dbSettings).length > 0) {
                    console.log('[APP] Loaded settings from database for pubkey:', keypair.public_key);
                    // Convert database settings back to settings object format
                    const settings = {
                        npriv_key: keypair.private_key,
                        encryption_algorithm: dbSettings.encryption_algorithm || 'nip44',
                        email_address: dbSettings.email_address || '',
                        password: dbSettings.password || '',
                        smtp_host: dbSettings.smtp_host || '',
                        smtp_port: parseInt(dbSettings.smtp_port) || 587,
                        imap_host: dbSettings.imap_host || '',
                        imap_port: parseInt(dbSettings.imap_port) || 993,
                        use_tls: dbSettings.use_tls === 'true',
                        email_filter: dbSettings.email_filter || 'nostr',
                        send_matching_dm: dbSettings.send_matching_dm !== 'false', // Default to true if not set
                        sync_cutoff_days: parseInt(dbSettings.sync_cutoff_days) || 365, // Default to 1 year
                        emails_per_page: parseInt(dbSettings.emails_per_page) || 50, // Default to 50
                        require_signature: dbSettings.require_signature !== 'false', // Default to true if not set
                        hide_undecryptable_emails: dbSettings.hide_undecryptable_emails !== 'false', // Default to true if not set
                        automatically_encrypt: dbSettings.automatically_encrypt !== 'false', // Default to true if not set
                        automatically_sign: dbSettings.automatically_sign !== 'false', // Default to true if not set
                        hide_unsigned_messages: dbSettings.hide_unsigned_messages !== 'false' // Default to true if not set
                    };
                    appState.setSettings(settings);
                    this.populateSettingsForm();
                    this.updateComposeButtons();
                    // Also update localStorage as backup
                    localStorage.setItem('nostr_mail_settings', JSON.stringify(settings));
                    return;
                }
            } catch (error) {
                console.error('[APP] Failed to load settings from database:', error);
                // Fall through to localStorage fallback
            }
        }
        
        // Fallback to localStorage if database load failed or no pubkey
        const stored = localStorage.getItem('nostr_mail_settings');
        if (stored) {
            const settings = JSON.parse(stored);
            appState.setSettings(settings);
            this.populateSettingsForm();
            this.updateComposeButtons();
        }
    } catch (error) {
        console.error('Error loading settings:', error);
    }
}

NostrMailApp.prototype.resetSettingsToDefaults = async function() {
    console.log('[APP] Resetting all settings to defaults');
    
    // Clear keypair from appState
    appState.setKeypair(null);
    localStorage.removeItem('nostr_keypair');
    
    // Define default settings
    const defaultSettings = {
        npriv_key: '',
        encryption_algorithm: 'nip44',
        email_address: '',
        password: '',
        smtp_host: '',
        smtp_port: 587,
        imap_host: '',
        imap_port: 993,
        use_tls: false,
        email_filter: 'nostr',
        send_matching_dm: true,
        sync_cutoff_days: 365,
        emails_per_page: 50,
        require_signature: true
    };
    
    // Set default settings in appState
    appState.setSettings(defaultSettings);
    
    // Clear localStorage settings
    localStorage.removeItem('nostr_mail_settings');
    
    // Populate form with default values
    this.populateSettingsForm();
    
    // Clear public key display
    this.renderProfilePubkey();
    
    // Cleanup live events since there's no keypair
    await this.cleanupLiveEvents();
    
    console.log('[APP] Settings reset to defaults');
};

// Reset settings to defaults for a specific pubkey without clearing the keypair
NostrMailApp.prototype.resetSettingsToDefaultsForPubkey = function(pubkey) {
    console.log('[APP] Resetting settings to defaults for pubkey:', pubkey);
    
    // Get current keypair to preserve it
    const keypair = appState.getKeypair();
    
    // Define default settings
    const defaultSettings = {
        npriv_key: keypair ? keypair.private_key : '',
        encryption_algorithm: 'nip44',
        email_address: '',
        password: '',
        smtp_host: '',
        smtp_port: 587,
        imap_host: '',
        imap_port: 993,
        use_tls: false,
        email_filter: 'nostr',
        send_matching_dm: true,
        sync_cutoff_days: 1825, // Default to 5 years (matching loadSettingsForPubkey)
        emails_per_page: 50,
        require_signature: true,
        hide_undecryptable_emails: true,
        automatically_encrypt: true,
        automatically_sign: true,
        hide_unsigned_messages: true
    };
    
    // Set default settings in appState
    appState.setSettings(defaultSettings);
    
    // Update last loaded pubkey tracker BEFORE populating form
    // This ensures autosave knows which pubkey to save to
    appState.setLastLoadedPubkey(pubkey);
    
    // Populate form with default values
    this.populateSettingsForm();
    
    // Ensure the isPopulatingForm flag is cleared after form population completes
    // Use a slightly longer delay to ensure all form updates are complete
    setTimeout(() => {
        if (this._setPopulatingForm) {
            this._setPopulatingForm(false);
            console.log('[APP] Cleared isPopulatingForm flag after resetting to defaults');
        }
    }, 200);
    
    console.log('[APP] Settings reset to defaults for pubkey:', pubkey);
};

NostrMailApp.prototype.loadSettingsForPubkey = async function(pubkey) {
    try {
        if (!pubkey) {
            console.log('[APP] No pubkey provided, skipping settings load');
            return;
        }
        
        console.log('[APP] Loading settings for pubkey:', pubkey);
        // Get private key for decryption
        const keypair = appState.getKeypair();
        const privateKey = keypair ? keypair.private_key : null;
        console.log('[APP] Using private key for decryption:', privateKey ? privateKey.substring(0, 20) + '...' : 'null');
        
        // #region agent log
        const dbQueryStartTime = Date.now();
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:305',message:'Before TauriService.dbGetAllSettings',data:{hypothesisId:'D'},timestamp:dbQueryStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        const dbSettings = await TauriService.dbGetAllSettings(pubkey, privateKey);
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:305',message:'After TauriService.dbGetAllSettings',data:{duration:Date.now()-dbQueryStartTime,settingsCount:Object.keys(dbSettings||{}).length,hypothesisId:'D'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        console.log('[APP] Loaded settings from database:', Object.keys(dbSettings || {}).length, 'keys');
        
        if (dbSettings && Object.keys(dbSettings).length > 0) {
            // Get current keypair to include private key in settings
            const keypair = appState.getKeypair();
            const settings = {
                npriv_key: keypair ? keypair.private_key : '',
                encryption_algorithm: dbSettings.encryption_algorithm || 'nip44',
                email_address: dbSettings.email_address || '',
                password: dbSettings.password || '',
                smtp_host: dbSettings.smtp_host || '',
                smtp_port: parseInt(dbSettings.smtp_port) || 587,
                imap_host: dbSettings.imap_host || '',
                imap_port: parseInt(dbSettings.imap_port) || 993,
                use_tls: dbSettings.use_tls === 'true',
                email_filter: dbSettings.email_filter || 'nostr',
                send_matching_dm: dbSettings.send_matching_dm !== 'false', // Default to true if not set
                sync_cutoff_days: parseInt(dbSettings.sync_cutoff_days) || 1825, // Default to 5 years
                emails_per_page: parseInt(dbSettings.emails_per_page) || 50, // Default to 50
                require_signature: dbSettings.require_signature !== 'false', // Default to true if not set
                hide_undecryptable_emails: dbSettings.hide_undecryptable_emails !== 'false', // Default to true if not set
                automatically_encrypt: dbSettings.automatically_encrypt !== 'false', // Default to true if not set
                automatically_sign: dbSettings.automatically_sign !== 'false', // Default to true if not set
                hide_unsigned_messages: dbSettings.hide_unsigned_messages !== 'false' // Default to true if not set
            };
            
            appState.setSettings(settings);
            this.populateSettingsForm();
            this.updateComposeButtons();
            // Update localStorage as backup
            localStorage.setItem('nostr_mail_settings', JSON.stringify(settings));
            console.log('[APP] Settings loaded for pubkey:', pubkey);
            
            // Update last loaded pubkey tracker
            appState.setLastLoadedPubkey(pubkey);
            
            // Show toast notification that settings were loaded
            notificationService.showSuccess('Settings loaded');
        } else {
            console.log('[APP] No settings found in database for pubkey:', pubkey);
            // Try to load from localStorage as fallback
            const stored = localStorage.getItem('nostr_mail_settings');
            if (stored) {
                try {
                    const settings = JSON.parse(stored);
                    // Only use localStorage settings if they match the current pubkey's keypair
                    // (localStorage might have settings from a different keypair)
                    const currentKeypair = appState.getKeypair();
                    if (currentKeypair && settings.npriv_key === currentKeypair.private_key) {
                        appState.setSettings(settings);
                        this.populateSettingsForm();
                        console.log('[APP] Loaded settings from localStorage for pubkey:', pubkey);
                    } else {
                        // localStorage has settings for a different keypair, reset to defaults
                        console.log('[APP] localStorage settings are for a different keypair, resetting to defaults');
                        this.resetSettingsToDefaultsForPubkey(pubkey);
                    }
                } catch (e) {
                    console.error('[APP] Failed to parse localStorage settings:', e);
                    // Reset to defaults if localStorage parse fails
                    this.resetSettingsToDefaultsForPubkey(pubkey);
                }
            } else {
                // No settings found in DB or localStorage, reset to defaults
                console.log('[APP] No settings found anywhere, resetting to defaults for pubkey:', pubkey);
                this.resetSettingsToDefaultsForPubkey(pubkey);
            }
        }
    } catch (error) {
        console.error('[APP] Error loading settings for pubkey:', error);
        // Fallback to localStorage
        const stored = localStorage.getItem('nostr_mail_settings');
        if (stored) {
            try {
                const settings = JSON.parse(stored);
                const currentKeypair = appState.getKeypair();
                if (currentKeypair && settings.npriv_key === currentKeypair.private_key) {
                    appState.setSettings(settings);
                    this.populateSettingsForm();
                } else {
                    // Reset to defaults if localStorage settings don't match current keypair
                    this.resetSettingsToDefaultsForPubkey(pubkey);
                }
            } catch (e) {
                console.error('[APP] Failed to load from localStorage:', e);
                // Reset to defaults on error
                this.resetSettingsToDefaultsForPubkey(pubkey);
            }
        } else {
            // No localStorage settings, reset to defaults
            this.resetSettingsToDefaultsForPubkey(pubkey);
        }
    }
}

// Load relays from database only
NostrMailApp.prototype.loadRelaysFromDatabase = async function() {
    try {
        const relays = await TauriService.getDbRelays();
        console.log('Loaded relays from DB:', relays); // DEBUG
        appState.setRelays(relays);
        
        // Sync disconnected relays first, then render
        await this.syncDisconnectedRelays();
        
        // Always render (which will show summary by default)
        await this.renderRelays();
    } catch (error) {
        console.error('Failed to load relays from database:', error);
        notificationService.showError('Could not load relays from database.');
        
        // Show empty summary on error
    }
}

// Load keypair
NostrMailApp.prototype.loadKeypair = async function() {
    // #region agent log
    const loadKeypairStartTime = Date.now();
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:422',message:'loadKeypair entry',data:{hypothesisId:'A'},timestamp:loadKeypairStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
    // #endregion
    try {
        // #region agent log
        const localStorageStartTime = Date.now();
        // #endregion
        const stored = localStorage.getItem('nostr_keypair');
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:424',message:'After localStorage.getItem',data:{hasStored:!!stored,hypothesisId:'A'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        let keypair;
        if (stored) {
            // #region agent log
            const parseStartTime = Date.now();
            // #endregion
            keypair = JSON.parse(stored);
            // #region agent log
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:427',message:'After JSON.parse stored keypair',data:{hypothesisId:'A'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
            // #endregion
            appState.setKeypair(keypair);
        } else {
            // #region agent log
            const generateStartTime = Date.now();
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:430',message:'Before TauriService.generateKeypair',data:{hypothesisId:'B'},timestamp:generateStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
            // #endregion
            keypair = await TauriService.generateKeypair();
            // #region agent log
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:430',message:'After TauriService.generateKeypair',data:{duration:Date.now()-generateStartTime,hypothesisId:'B'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
            // #endregion
            appState.setKeypair(keypair);
            localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
        }
        console.log('Keypair loaded:', appState.getKeypair().public_key.substring(0, 20) + '...');
        this.renderProfilePubkey();
        
        // Populate private key field in settings form if it's empty
        // This ensures the user sees they're already logged in when the app starts
        const nprivKeyInput = domManager.get('nprivKey');
        if (nprivKeyInput && (!nprivKeyInput.value || nprivKeyInput.value.trim() === '')) {
            if (keypair && keypair.private_key) {
                domManager.setValue('nprivKey', keypair.private_key);
                console.log('[APP] Populated private key field from cached keypair on startup');
                // Update public key display to show the user is logged in
                // #region agent log
                const updateDisplayStartTime = Date.now();
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:445',message:'Before updatePublicKeyDisplay',data:{hypothesisId:'C'},timestamp:updateDisplayStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
                // #endregion
                await this.updatePublicKeyDisplay();
                // #region agent log
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:445',message:'After updatePublicKeyDisplay',data:{duration:Date.now()-updateDisplayStartTime,hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
                // #endregion
            }
        }
        
        // Load settings for this pubkey
        if (keypair && keypair.public_key) {
            // #region agent log
            const loadSettingsStartTime = Date.now();
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:451',message:'Before loadSettingsForPubkey',data:{pubkey:keypair.public_key.substring(0,20)+'...',hypothesisId:'D'},timestamp:loadSettingsStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
            // #endregion
            await this.loadSettingsForPubkey(keypair.public_key);
            // #region agent log
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:451',message:'After loadSettingsForPubkey',data:{duration:Date.now()-loadSettingsStartTime,hypothesisId:'D'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
            // #endregion
        }
        
        // Initialize persistent Nostr client with the loaded keypair
        // #region agent log
        const initClientStartTime = Date.now();
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:455',message:'Before initializeNostrClient',data:{hypothesisId:'E'},timestamp:initClientStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        await this.initializeNostrClient();
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:455',message:'After initializeNostrClient',data:{duration:Date.now()-initClientStartTime,hypothesisId:'E'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:422',message:'loadKeypair completed',data:{totalDuration:Date.now()-loadKeypairStartTime,hypothesisId:'A'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
    } catch (error) {
        console.error('Failed to load keypair:', error);
        notificationService.showError('Failed to load encryption keys');
    }
}

// Initialize the persistent Nostr client
NostrMailApp.prototype.initializeNostrClient = async function() {
    // #region agent log
    const initNostrClientStartTime = Date.now();
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:463',message:'initializeNostrClient entry',data:{hypothesisId:'E'},timestamp:initNostrClientStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
    // #endregion
    try {
        const keypair = appState.getKeypair();
        if (!keypair || !keypair.private_key) {
            console.warn('[APP] No keypair available for Nostr client initialization');
            return;
        }
        
        console.log('[APP] Initializing persistent Nostr client...');
        // #region agent log
        const tauriInitStartTime = Date.now();
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:472',message:'Before TauriService.initPersistentNostrClient',data:{hypothesisId:'E'},timestamp:tauriInitStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        await TauriService.initPersistentNostrClient(keypair.private_key);
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:472',message:'After TauriService.initPersistentNostrClient',data:{duration:Date.now()-tauriInitStartTime,hypothesisId:'E'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        console.log('[APP] ✅ Nostr client initialized successfully');
        
        // Update relay status display after client initialization
        if (appState.getRelays().length > 0) {
            setTimeout(async () => {
                await this.renderRelays();
            }, 1000); // Give the client time to connect
        }
    } catch (error) {
        console.error('[APP] Failed to initialize Nostr client:', error);
        notificationService.showError('Failed to initialize Nostr client: ' + error);
    }
}

// Ensure the default nostr-mail contact is added for this user
NostrMailApp.prototype.ensureDefaultContact = async function() {
    const DEFAULT_CONTACT_PUBKEY = 'npub1agg7melcvmue7vwj2wqdh2666454xv5vsdu7n2tmkv73fl0a0tsq7asr9j';
    
    // Prevent concurrent execution
    if (this._ensuringDefaultContact) {
        return;
    }
    
    this._ensuringDefaultContact = true;
    
    try {
        const keypair = appState.getKeypair();
        if (!keypair || !keypair.public_key) {
            console.warn('[APP] No keypair available, skipping default contact check');
            return;
        }
        
        const userPubkey = keypair.public_key;
        console.log('[APP] Checking for default nostr-mail contact for user:', userPubkey.substring(0, 20) + '...');
        
        // Check if the default contact already exists for this user
        const userContacts = await window.DatabaseService.getAllContacts(userPubkey);
        const hasDefaultContact = userContacts.some(contact => contact.pubkey === DEFAULT_CONTACT_PUBKEY);
        
        if (hasDefaultContact) {
            console.log('[APP] Default nostr-mail contact already exists for this user');
            return;
        }
        
        console.log('[APP] Default nostr-mail contact not found, creating contact immediately...');
        
        // Create contact immediately with default data (don't wait for profile fetch)
        // This ensures the contact appears instantly in the UI
        const now = new Date().toISOString();
        const contact = {
            pubkey: DEFAULT_CONTACT_PUBKEY,
            name: 'nostr-mail', // Default name, will be updated when profile is fetched
            email: null,
            picture: '',
            fields: {},
            picture_data_url: null,
            picture_loaded: false,
            picture_loading: false,
            is_public: false, // Private contact - not in public follow list
            created_at: now,
            updated_at: now
        };
        
        // Convert to database format and save as private contact immediately
        const dbContact = window.DatabaseService.convertContactToDbFormat(contact);
        await window.DatabaseService.saveContact(dbContact, userPubkey, false); // isPublic = false
        
        // Reload contacts from database immediately so user sees the contact right away
        if (window.contactsService) {
            await window.contactsService.loadContacts();
        }
        
        // Fetch profile asynchronously in the background and update the contact when ready
        // This doesn't block the UI - user sees the contact immediately
        window.TauriService.fetchProfilePersistent(DEFAULT_CONTACT_PUBKEY).then(async (profileResult) => {
            if (profileResult) {
                console.log('[APP] Successfully fetched default nostr-mail profile (async update)');
                
                // Extract profile information
                const profile = profileResult;
                const profileName = profile.fields?.name || profile.fields?.display_name || 'nostr-mail';
                const email = profile.fields?.email || null;
                
                // Update the contact with fetched profile data
                const updatedContact = {
                    pubkey: DEFAULT_CONTACT_PUBKEY,
                    name: profileName,
                    email: email,
                    picture: profile.fields?.picture || '',
                    fields: profile.fields || {},
                    picture_data_url: null,
                    picture_loaded: false,
                    picture_loading: false,
                    is_public: false,
                    created_at: contact.created_at,
                    updated_at: new Date().toISOString()
                };
                
                // Update in database
                const updatedDbContact = window.DatabaseService.convertContactToDbFormat(updatedContact);
                await window.DatabaseService.saveContact(updatedDbContact, userPubkey, false);
                
                // Reload contacts to show updated profile data
                if (window.contactsService) {
                    await window.contactsService.loadContacts();
                }
                
                // Refresh compose dropdown
                if (window.emailService) {
                    window.emailService.populateNostrContactDropdown();
                }
            } else {
                console.warn('[APP] Could not fetch default nostr-mail profile (async), contact created with default name');
            }
        }).catch((error) => {
            console.error('[APP] Error fetching default nostr-mail profile (async):', error);
        });
        
        // Refresh compose dropdown to include new contact
        if (window.emailService) {
            window.emailService.populateNostrContactDropdown();
        }
        
        console.log('[APP] ✅ Default nostr-mail contact added privately for user');
    } catch (error) {
        // Don't block startup if this fails - just log the error
        console.error('[APP] Failed to ensure default contact:', error);
    } finally {
        this._ensuringDefaultContact = false;
    }
}

// Setup event listeners
NostrMailApp.prototype.setupEventListeners = function() {
    console.log('Setting up event listeners...');
    
    try {
        // Navigation
        const navItems = domManager.get('navItems');
        if (navItems && navItems.length > 0) {
            navItems.forEach(item => {
                item.addEventListener('click', () => this.switchTab(item.dataset.tab));
            });
        }
        
        // Modal
        const modalClose = domManager.get('modalClose');
        if (modalClose) {
            modalClose.addEventListener('click', () => this.hideModal());
        }
        const modalOverlay = domManager.get('modalOverlay');
        if (modalOverlay) {
            modalOverlay.addEventListener('click', (e) => {
                if (e.target === modalOverlay) this.hideModal();
            });
        }
        
        // Compose form
        const sendBtn = domManager.get('sendBtn');
        if (sendBtn) {
            console.log('[JS] Setting up send button event listener');
            sendBtn.addEventListener('click', () => window.emailService?.sendEmail());
        }
        const saveDraftBtn = domManager.get('saveDraftBtn');
        if (saveDraftBtn) {
            saveDraftBtn.addEventListener('click', () => window.emailService?.saveDraft());
        }
        const encryptBtn = domManager.get('encryptBtn');
        if (encryptBtn) {
            console.log('[JS] Setting up encrypt button event listener');
            encryptBtn.dataset.encrypted = 'false';
            // Update DM checkbox visibility on initialization
            if (window.emailService) window.emailService.updateDmCheckboxVisibility();
            encryptBtn.addEventListener('click', async function handleEncryptClick() {
                const iconSpan = encryptBtn.querySelector('.encrypt-btn-icon i');
                const labelSpan = encryptBtn.querySelector('.encrypt-btn-label');
                const isEncrypted = encryptBtn.dataset.encrypted === 'true';
                const subjectInput = domManager.get('subject');
                const messageBodyInput = domManager.get('messageBody');
                if (!isEncrypted) {
                    // Encrypt mode
                    console.log('[JS] Encrypt button clicked');
                    const subjectValue = domManager.getValue('subject') || '';
                    const messageBodyValue = domManager.getValue('messageBody') || '';
                    
                    // Always require both subject and body
                    if (!subjectValue || !messageBodyValue) {
                        notificationService.showError('Both subject and message body must be filled to encrypt.');
                        return;
                    }
                    const didEncrypt = await window.emailService.encryptEmailFields();
                    if (didEncrypt) {
                        if (iconSpan) iconSpan.className = 'fas fa-unlock';
                        if (labelSpan) labelSpan.textContent = 'Decrypt';
                        encryptBtn.dataset.encrypted = 'true';
                        // Disable editing
                        if (subjectInput) subjectInput.disabled = true;
                        if (messageBodyInput) messageBodyInput.disabled = true;
                        // Update DM checkbox visibility
                        if (window.emailService) window.emailService.updateDmCheckboxVisibility();
                        // Clear signature when encrypting (body state changed)
                        window.emailService.clearSignature();
                    }
                } else {
                    // Decrypt mode
                    console.log('[JS] Decrypt button clicked');
                    // Get keys and contact
                    const privkey = appState.getKeypair().private_key;
                    const pubkey = window.emailService?.selectedNostrContact?.pubkey;
                    const armoredBody = domManager.getValue('messageBody') || '';
                    const encryptedSubject = domManager.getValue('subject') || '';
                    // Regex for both NIP-04 and NIP-44 armored messages
                    const match = armoredBody.match(/-----BEGIN NOSTR NIP-(04|44) ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-(04|44) ENCRYPTED MESSAGE-----/);
                    let decryptedAny = false;
                    if (privkey && pubkey) {
                        // Decrypt subject if it looks encrypted
                        if (window.Utils && window.Utils.isLikelyEncryptedContent(encryptedSubject)) {
                            try {
                                const decryptedSubject = await TauriService.decryptDmContent(privkey, pubkey, encryptedSubject);
                                domManager.setValue('subject', decryptedSubject);
                                notificationService.showSuccess('Subject decrypted');
                                decryptedAny = true;
                            } catch (err) {
                                notificationService.showError('Failed to decrypt subject: ' + err);
                            }
                        }
                        // Decrypt body if armored
                        if (match) {
                            try {
                                // Use the new manifest-aware decryption function
                                await window.emailService.decryptBodyContent();
                                decryptedAny = true;
                            } catch (err) {
                                // Fallback to legacy decryption
                                const encryptedContent = match[2].replace(/\s+/g, '');
                                try {
                                    const decrypted = await TauriService.decryptDmContent(privkey, pubkey, encryptedContent);
                                    domManager.setValue('messageBody', decrypted);
                                    notificationService.showSuccess('Body decrypted');
                                    decryptedAny = true;
                                    // Clear signature when decrypting (body state changed)
                                    if (window.emailService) window.emailService.clearSignature();
                                } catch (legacyErr) {
                                    notificationService.showError('Failed to decrypt body: ' + legacyErr);
                                }
                            }
                        }
                        if (!decryptedAny) {
                            notificationService.showError('No encrypted message found in subject or body');
                        }
                    } else {
                        notificationService.showError('No encrypted message found or missing keys');
                    }
                    
                    // Also decrypt any encrypted attachments
                    try {
                        await window.emailService.decryptAllAttachments();
                    } catch (error) {
                        console.error('Failed to decrypt attachments:', error);
                    }
                    if (iconSpan) iconSpan.className = 'fas fa-lock';
                    if (labelSpan) labelSpan.textContent = 'Encrypt';
                    encryptBtn.dataset.encrypted = 'false';
                    // Re-enable editing
                    if (subjectInput) subjectInput.disabled = false;
                    if (messageBodyInput) messageBodyInput.disabled = false;
                    // Update DM checkbox visibility
                    if (window.emailService) window.emailService.updateDmCheckboxVisibility();
                    // Clear signature when decrypting (body state changed)
                    if (decryptedAny) {
                        window.emailService.clearSignature();
                    }
                }
            });
        } else {
            console.error('[JS] Encrypt button not found in DOM');
        }
        
        // Sign button
        const signBtn = domManager.get('signBtn');
        if (signBtn) {
            console.log('[JS] Setting up sign button event listener');
            signBtn.dataset.signed = 'false';
            signBtn.addEventListener('click', async function handleSignClick() {
                const iconSpan = signBtn.querySelector('.sign-btn-icon i');
                const labelSpan = signBtn.querySelector('.sign-btn-label');
                const isSigned = signBtn.dataset.signed === 'true';
                
                if (!isSigned) {
                    // Sign mode
                    console.log('[JS] Sign button clicked');
                    const subjectValue = domManager.getValue('subject') || '';
                    const messageBodyValue = domManager.getValue('messageBody') || '';
                    
                    // Require both subject and body to sign
                    if (!subjectValue || !messageBodyValue) {
                        notificationService.showError('Both subject and message body must be filled to sign.');
                        return;
                    }
                    
                    const keypair = appState.getKeypair();
                    if (!keypair || !keypair.private_key) {
                        notificationService.showError('No private key found. Please set up your keypair first.');
                        return;
                    }
                    
                    try {
                        // Sign the email body in whatever state it's in (encrypted or decrypted)
                        const contentToSign = messageBodyValue; // Sign the current body state
                        const signature = await TauriService.signData(keypair.private_key, contentToSign);
                        
                        console.log('[JS] Email signed successfully, signature:', signature.substring(0, 32) + '...');
                        
                        // Store signature for later use
                        signBtn.dataset.signature = signature;
                        signBtn.dataset.signed = 'true';
                        
                        if (iconSpan) iconSpan.className = 'fas fa-check-circle';
                        if (labelSpan) labelSpan.textContent = 'Signed';
                        signBtn.classList.add('signed');
                        
                        notificationService.showSuccess('Email signed successfully. Signature will be added when sending.');
                    } catch (error) {
                        console.error('[JS] Failed to sign email:', error);
                        notificationService.showError('Failed to sign email: ' + error);
                    }
                } else {
                    // Unsigned mode
                    console.log('[JS] Unsign button clicked');
                    signBtn.dataset.signed = 'false';
                    delete signBtn.dataset.signature;
                    
                    if (iconSpan) iconSpan.className = 'fas fa-pen';
                    if (labelSpan) labelSpan.textContent = 'Sign';
                    signBtn.classList.remove('signed');
                    
                    notificationService.showInfo('Email signature removed.');
                }
            });
        } else {
            console.error('[JS] Sign button not found in DOM');
        }
        
        // Preview headers button
        const previewHeadersBtn = domManager.get('previewHeadersBtn');
        if (previewHeadersBtn) {
            console.log('[JS] Setting up preview headers button event listener');
            previewHeadersBtn.addEventListener('click', () => window.emailService?.previewEmailHeaders());
        } else {
            console.error('[JS] Preview headers button not found in DOM');
        }
        
        // Nostr contact dropdown
        const nostrContactSelect = domManager.get('nostrContactSelect');
        if (nostrContactSelect) {
            console.log('[JS] Setting up Nostr contact dropdown event listener');
            nostrContactSelect.addEventListener('change', () => window.emailService?.handleNostrContactSelection());
        }
        
        // Inbox
        const refreshInbox = domManager.get('refreshInbox');
        if (refreshInbox) {
            refreshInbox.addEventListener('click', async () => {
                // Clear search input
                domManager.clear('emailSearch');
                // Show loading state immediately
                domManager.disable('refreshInbox');
                domManager.setHTML('refreshInbox', '<span class="loading"></span> Loading...');
                // Sync and load all emails (no search filter)
                try {
                    await window.emailService.syncInboxEmails();
                    await window.emailService.loadEmails();
                    notificationService.showSuccess('Inbox synced successfully');
                } catch (error) {
                    console.error('[JS] Error syncing inbox:', error);
                    notificationService.showError('Failed to sync inbox: ' + error.message);
                    // Restore button state on error
                    domManager.enable('refreshInbox');
                    domManager.setHTML('refreshInbox', '<i class="fas fa-sync"></i> Refresh');
                }
            });
        }
        
        // Back to inbox button
        const backToInboxBtn = document.getElementById('back-to-inbox');
        if (backToInboxBtn) {
            backToInboxBtn.addEventListener('click', () => window.emailService?.showEmailList());
        }
        
        // Email Search
        const emailSearch = domManager.get('emailSearch');
        if (emailSearch) {
            emailSearch.addEventListener('input', () => window.emailService?.filterEmails());
        }
        
        // Sent Email Search
        const sentSearch = domManager.get('sentSearch');
        if (sentSearch) {
            sentSearch.addEventListener('input', () => window.emailService?.filterSentEmails());
        }
        
        // DM elements
        const newDmBtn = domManager.get('newDmBtn');
        if (newDmBtn) {
            newDmBtn.addEventListener('click', () => this.showNewDmCompose());
        }
        
        const refreshDm = domManager.get('refreshDm');
        if (refreshDm) {
            refreshDm.addEventListener('click', () => window.dmService?.refreshDmConversations());
        }
        
        const dmSearch = domManager.get('dmSearch');
        if (dmSearch) {
            dmSearch.addEventListener('input', () => dmService.filterDmContacts());
        }
        
        const dmSearchToggle = domManager.get('dmSearchToggle');
        if (dmSearchToggle) {
            dmSearchToggle.addEventListener('click', () => window.dmService?.toggleDmSearch());
        }
        
        // Contacts elements
        const addContactBtn = domManager.get('addContactBtn');
        if (addContactBtn) {
            addContactBtn.addEventListener('click', () => contactsService.showAddContactModal());
        }
        
        const refreshContactsBtn = domManager.get('refreshContactsBtn');
        if (refreshContactsBtn) {
            refreshContactsBtn.addEventListener('click', async () => {
                await contactsService.refreshContacts();
                await contactsService.refreshSelectedContactProfile();
            });
        }
        
        const contactsSearch = domManager.get('contactsSearch');
        if (contactsSearch) {
            contactsSearch.addEventListener('input', () => contactsService.filterContacts());
        }
        
        const contactsSearchToggle = domManager.get('contactsSearchToggle');
        if (contactsSearchToggle) {
            contactsSearchToggle.addEventListener('click', () => contactsService.toggleContactsSearch());
        }
        
        // Wire up contacts back button
        const contactsBackButton = document.getElementById('contacts-back-btn');
        if (contactsBackButton) {
            contactsBackButton.addEventListener('click', () => {
                contactsService.handleBackToContactsList();
            });
        }
        
        // Profile refresh button
        const refreshProfile = domManager.get('refreshProfile');
        if (refreshProfile) {
            refreshProfile.addEventListener('click', () => this.refreshProfile());
        }
        
        // Settings
        // Auto-save setup - removed save button, settings auto-save on change
        this.setupAutoSaveSettings();
        const testConnectionBtn = domManager.get('testConnectionBtn');
        if (testConnectionBtn) {
            testConnectionBtn.addEventListener('click', () => this.testConnection());
        }
        const testEmailConnectionBtn = domManager.get('testEmailConnectionBtn');
        if (testEmailConnectionBtn) {
            testEmailConnectionBtn.addEventListener('click', () => window.emailService?.testEmailConnection());
        }
        // Add this block to wire up the add relay button
        const addRelayBtn = domManager.get('addRelayBtn');
        if (addRelayBtn) {
            addRelayBtn.addEventListener('click', () => this.addRelay());
        }
        
        // Email provider selection
        const emailProvider = domManager.get('emailProvider');
        if (emailProvider) {
            emailProvider.addEventListener('change', () => window.emailService?.handleEmailProviderChange());
        }
        // Toggle private key visibility (eye button)
        const toggleNprivVisibilityBtn = domManager.get('toggleNprivVisibilityBtn');
        const nprivKeyInput = domManager.get('nprivKey');
        if (toggleNprivVisibilityBtn && nprivKeyInput) {
            toggleNprivVisibilityBtn.addEventListener('click', () => {
                const icon = toggleNprivVisibilityBtn.querySelector('i');
                if (nprivKeyInput.type === 'password') {
                    nprivKeyInput.type = 'text';
                    toggleNprivVisibilityBtn.title = 'Hide private key';
                    if (icon) {
                        icon.classList.remove('fa-eye');
                        icon.classList.add('fa-eye-slash');
                    }
                } else {
                    nprivKeyInput.type = 'password';
                    toggleNprivVisibilityBtn.title = 'Show private key';
                    if (icon) {
                        icon.classList.remove('fa-eye-slash');
                        icon.classList.add('fa-eye');
                    }
                }
            });
        }
        // Copy private key to clipboard (copy button)
        const copyNprivBtn = domManager.get('copyNprivBtn');
        if (copyNprivBtn && nprivKeyInput) {
            copyNprivBtn.addEventListener('click', () => {
                const value = nprivKeyInput.value;
                if (!value) return;
                navigator.clipboard.writeText(value)
                    .then(() => notificationService.showSuccess('Private key copied to clipboard'))
                    .catch(() => notificationService.showError('Failed to copy private key'));
            });
        }
        
        // Toggle email password visibility (eye button)
        const toggleEmailPasswordVisibilityBtn = domManager.get('toggleEmailPasswordVisibilityBtn');
        const emailPasswordInput = domManager.get('emailPassword');
        if (toggleEmailPasswordVisibilityBtn && emailPasswordInput) {
            toggleEmailPasswordVisibilityBtn.addEventListener('click', () => {
                const icon = toggleEmailPasswordVisibilityBtn.querySelector('i');
                if (emailPasswordInput.type === 'password') {
                    emailPasswordInput.type = 'text';
                    toggleEmailPasswordVisibilityBtn.title = 'Hide password';
                    if (icon) {
                        icon.classList.remove('fa-eye');
                        icon.classList.add('fa-eye-slash');
                    }
                } else {
                    emailPasswordInput.type = 'password';
                    toggleEmailPasswordVisibilityBtn.title = 'Show password';
                    if (icon) {
                        icon.classList.remove('fa-eye-slash');
                        icon.classList.add('fa-eye');
                    }
                }
            });
        }
        
        // Copy email password to clipboard (copy button)
        const copyEmailPasswordBtn = domManager.get('copyEmailPasswordBtn');
        if (copyEmailPasswordBtn && emailPasswordInput) {
            copyEmailPasswordBtn.addEventListener('click', () => {
                const value = emailPasswordInput.value;
                if (!value) return;
                navigator.clipboard.writeText(value)
                    .then(() => notificationService.showSuccess('Password copied to clipboard'))
                    .catch(() => notificationService.showError('Failed to copy password'));
            });
        }
        
        // Instantly update npub as npriv changes
        // Also reset settings to defaults if private key is cleared or invalid
        if (nprivKeyInput) {
            // Use a debounce to avoid resetting while user is still typing
            let validationTimeout = null;
            let loadSettingsTimeout = null;
            nprivKeyInput.addEventListener('input', async () => {
                await this.updatePublicKeyDisplay();
                
                // Clear any pending validation
                if (validationTimeout) {
                    clearTimeout(validationTimeout);
                }
                
                // Clear any pending settings load
                if (loadSettingsTimeout) {
                    clearTimeout(loadSettingsTimeout);
                }
                
                const nprivKey = nprivKeyInput.value.trim();
                
                // Check if private key field is empty/cleared
                if (!nprivKey) {
                    console.log('[APP] Private key cleared, resetting settings to defaults');
                    await this.resetSettingsToDefaults();
                    appState.setLastLoadedPubkey(null);
                    return;
                }
                
                // Check if it looks like a private key but might be invalid
                // Only check if it starts with npriv/nsec (looks like a key)
                if (nprivKey.startsWith('npriv1') || nprivKey.startsWith('nsec1')) {
                    // Debounce validation - wait 1 second after user stops typing
                    validationTimeout = setTimeout(async () => {
                        // Re-read the current value from the input field to avoid race conditions
                        // (user might have changed it since the timeout was set)
                        const currentNprivKey = nprivKeyInput.value.trim();
                        
                        // If the value changed, don't process (user is still typing or changed it)
                        if (currentNprivKey !== nprivKey) {
                            console.log('[APP] Private key changed during debounce, skipping validation');
                            return;
                        }
                        
                        try {
                            const isValid = await TauriService.validatePrivateKey(currentNprivKey);
                            if (!isValid) {
                                console.log('[APP] Invalid private key detected, resetting settings to defaults');
                                await this.resetSettingsToDefaults();
                                appState.setLastLoadedPubkey(null);
                            } else {
                                // Valid key - check if pubkey changed and load settings if needed
                                try {
                                    // Re-read public key from display element (in case it changed)
                                    const publicKey = domManager.getValue('publicKeyDisplay')?.trim();
                                    const lastLoadedPubkey = appState.getLastLoadedPubkey();
                                    
                                    // Check if this is a different pubkey than what was last loaded
                                    // Also verify it's a valid npub (starts with npub1) and not an error message
                                    if (publicKey && publicKey.startsWith('npub1') && publicKey !== lastLoadedPubkey) {
                                        console.log('[APP] Pubkey changed, loading settings for:', publicKey.substring(0, 20) + '...');
                                        
                                        // Double-check the private key hasn't changed again
                                        const finalNprivKey = nprivKeyInput.value.trim();
                                        if (finalNprivKey !== currentNprivKey) {
                                            console.log('[APP] Private key changed during settings load, aborting');
                                            return;
                                        }
                                        
                                        // Immediately set flag to prevent auto-save from triggering
                                        if (this._setPopulatingForm) {
                                            this._setPopulatingForm(true);
                                        }
                                        
                                        // Update keypair in appState BEFORE loading settings (so it has correct private key for decryption)
                                        const keypair = { private_key: finalNprivKey, public_key: publicKey };
                                        appState.setKeypair(keypair);
                                        localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
                                        
                                        // Clear contacts from appState since they belong to the old user
                                        appState.setContacts([]);
                                        // Immediately clear the contacts UI if visible
                                        if (window.contactsService) {
                                            window.contactsService.renderContacts();
                                        }
                                        
                                        // Load settings for the new pubkey
                                        await this.loadSettingsForPubkey(publicKey);
                                        
                                        // Note: loadSettingsForPubkey already sets lastLoadedPubkey, so we don't need to set it here
                                    }
                                } catch (error) {
                                    console.error('[APP] Error checking/loading settings for pubkey:', error);
                                }
                            }
                        } catch (error) {
                            console.error('[APP] Error validating private key:', error);
                        }
                    }, 1000); // Wait 1 second after user stops typing
                }
            });
        }
        
        // Dark mode toggle
        const darkToggle = document.getElementById('dark-mode-toggle');
        if (darkToggle) {
            darkToggle.addEventListener('click', () => this.toggleDarkMode());
        }
        
        // Profile form
        const updateProfileBtn = document.getElementById('update-profile-btn');
        if (updateProfileBtn) {
            updateProfileBtn.addEventListener('click', (e) => {
                e.preventDefault();
                this.updateProfile();
            });
        }
        
        const addProfileFieldBtn = document.getElementById('add-profile-field-btn');
        if (addProfileFieldBtn) {
            addProfileFieldBtn.addEventListener('click', () => this.addProfileField());
        }
        
        const profileFieldsForm = document.getElementById('profile-fields-form');
        if (profileFieldsForm) {
            profileFieldsForm.addEventListener('submit', (e) => {
                e.preventDefault();
                this.updateProfile();
            });
        }
        
        // Contacts: Export npubs
        const exportNpubsBtn = document.getElementById('export-npubs-btn');
        if (exportNpubsBtn) {
            exportNpubsBtn.addEventListener('click', async () => {
                const contacts = appState.getContacts();
                if (!contacts || contacts.length === 0) {
                    notificationService.showError('No contacts to export.');
                    return;
                }
                const npubs = contacts.map(c => c.pubkey).join('\n');
                const filename = 'nostr-contacts.txt';

                // Tauri-native save dialog and file write
                if (window.__TAURI__ && window.__TAURI__.dialog && window.__TAURI__.fs) {
                    try {
                        const { save } = window.__TAURI__.dialog;
                        const { writeTextFile } = window.__TAURI__.fs;
                        const selectedPath = await save({
                            defaultPath: filename,
                            filters: [
                                { name: 'Text Files', extensions: ['txt'] },
                            ],
                        });
                        if (selectedPath) {
                            await writeTextFile({ path: selectedPath, contents: npubs });
                            notificationService.showSuccess(`Exported all contact npubs as a file!\nSaved to: ${selectedPath}`);
                        } else {
                            notificationService.showError('Export cancelled.');
                        }
                        return;
                    } catch (err) {
                        notificationService.showError('Failed to save file: ' + err.message);
                        return;
                    }
                }

                // Try File System Access API
                if (window.showSaveFilePicker) {
                    try {
                        const handle = await window.showSaveFilePicker({
                            suggestedName: filename,
                            types: [
                                {
                                    description: 'Text Files',
                                    accept: { 'text/plain': ['.txt'] },
                                },
                            ],
                        });
                        const writable = await handle.createWritable();
                        await writable.write(npubs);
                        await writable.close();
                        notificationService.showSuccess('Exported all contact npubs as a file!');
                        return;
                    } catch (err) {
                        if (err.name !== 'AbortError') {
                            notificationService.showError('Failed to save file: ' + err.message);
                        }
                        return;
                    }
                }
                // Fallback: download as file
                Utils.downloadAsFile(npubs, filename);
                notificationService.showSuccess('Exported all contact npubs as a file!');
            });
        }
        
        // Copy public key to clipboard (copy button)
        const copyPubkeyBtn = domManager.get('copyPubkeyBtn');
        const publicKeyDisplayInput = domManager.get('publicKeyDisplay');
        if (copyPubkeyBtn && publicKeyDisplayInput) {
            copyPubkeyBtn.addEventListener('click', () => {
                const value = publicKeyDisplayInput.value;
                if (!value) return;
                navigator.clipboard.writeText(value)
                    .then(() => notificationService.showSuccess('Public key copied to clipboard'))
                    .catch(() => notificationService.showError('Failed to copy public key'));
            });
        }
        
        // Generate new keypair button
        const generateKeyBtn = domManager.get('generateKeyBtn');
        if (generateKeyBtn) {
            generateKeyBtn.addEventListener('click', async () => {
                try {
                    const keypair = await TauriService.generateKeypair();
                    appState.setKeypair(keypair);
                    localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
                    domManager.setValue('nprivKey', keypair.private_key);
                    await app.updatePublicKeyDisplay();
                    
                    // Load settings for the new pubkey
                    console.log('[APP] New keypair generated, loading settings for pubkey:', keypair.public_key);
                    await app.loadSettingsForPubkey(keypair.public_key);
                    
                    // Restart live events with new keypair
                    console.log('[LiveEvents] New keypair generated, restarting live events');
                    await app.cleanupLiveEvents();
                    await app.initializeLiveEvents();
                    
                    // Reinitialize the persistent Nostr client with the new keypair
                    console.log('[APP] New keypair generated, reinitializing persistent Nostr client');
                    await app.initializeNostrClient();
                    
                    // Clear contacts from appState since they belong to the old user
                    appState.setContacts([]);
                    // Immediately clear the contacts UI if visible
                    if (window.contactsService) {
                        window.contactsService.renderContacts();
                    }
                    
                    // Reload the currently active page to reflect the new keypair
                    await app.reloadActivePage();
                    
                    // Ensure default nostr-mail contact is added for the new user
                    await app.ensureDefaultContact();
                    
                    notificationService.showSuccess('New keypair generated!');
                } catch (error) {
                    notificationService.showError('Failed to generate keypair: ' + error);
                }
            });
        }
        
        // Sent
        const refreshSent = domManager.get('refreshSent');
        if (refreshSent) {
            refreshSent.addEventListener('click', async () => {
                // Clear search input
                domManager.clear('sentSearch');
                // Save original button state
                const originalRefreshBtnHTML = refreshSent.innerHTML;
                
                // Show loading state
                refreshSent.disabled = true;
                refreshSent.innerHTML = '<span class="loading"></span> Syncing...';
                
                try {
                    await window.emailService.syncSentEmails();
                    // loadSentEmails() will manage the button state from here
                    await window.emailService.loadSentEmails();
                    notificationService.showSuccess('Sent emails synced successfully');
                } catch (error) {
                    console.error('[JS] Error syncing sent emails:', error);
                    notificationService.showError('Failed to sync sent emails: ' + error.message);
                    // Restore button state on error (loadSentEmails won't run if sync fails)
                    refreshSent.disabled = false;
                    refreshSent.innerHTML = originalRefreshBtnHTML;
                }
            });
        }
        const backToSentBtn = document.getElementById('back-to-sent');
        if (backToSentBtn) {
            backToSentBtn.addEventListener('click', () => window.emailService?.showSentList());
        }
        
        // Drafts event listeners
        const refreshDrafts = domManager.get('refreshDrafts');
        if (refreshDrafts) {
            refreshDrafts.addEventListener('click', async () => {
                await window.emailService.loadDrafts();
            });
        }
        
        const backToDraftsBtn = domManager.get('backToDrafts');
        if (backToDraftsBtn) {
            backToDraftsBtn.addEventListener('click', () => window.emailService?.showDraftsList());
        }
        
        console.log('Event listeners set up successfully');
    } catch (error) {
        console.error('Error setting up event listeners:', error);
    }
}

// Live Event Subscription System
NostrMailApp.prototype.initializeLiveEvents = async function() {
    if (!appState.hasKeypair()) {
        console.log('[LiveEvents] No keypair available, skipping live event subscription');
        this.updateLiveEventsStatus('inactive', 'No keypair');
        return;
    }
    
    const privateKey = appState.getKeypair().private_key;
    
    try {
        // Start unified subscription for DMs and profile updates
        await TauriService.startLiveEventSubscription(privateKey);
        
        // Set up event listeners for live events
        await this.setupLiveEventListeners();
        
        this.liveEventsActive = true;
        this.updateLiveEventsStatus('active', 'Connected');
        console.log('[LiveEvents] Live event subscription initialized successfully');
        
    } catch (error) {
        console.error('[LiveEvents] Failed to initialize live events:', error);
        this.updateLiveEventsStatus('error', 'Connection failed');
        // Don't throw - app should continue working with polling fallback
    }
};

NostrMailApp.prototype.setupLiveEventListeners = async function() {
    try {
        console.log('[LiveEvents] Setting up event listeners...');
        
        // Listen for live direct messages
        this.dmUnlisten = await window.__TAURI__.event.listen('dm-received', (event) => {
            console.log('[LiveEvents] *** DM EVENT LISTENER TRIGGERED ***');
            console.log('[LiveEvents] Raw event:', event);
            console.log('[LiveEvents] Event payload:', event.payload);
            this.handleLiveDM(event.payload);
        });
        
        // Listen for live profile updates
        this.profileUnlisten = await window.__TAURI__.event.listen('profile-updated', (event) => {
            console.log('[LiveEvents] *** PROFILE EVENT LISTENER TRIGGERED ***');
            console.log('[LiveEvents] Raw event:', event);
            console.log('[LiveEvents] Event payload:', event.payload);
            this.handleLiveProfileUpdate(event.payload);
        });
        
        console.log('[LiveEvents] Event listeners set up successfully');
        console.log('[LiveEvents] DM listener:', this.dmUnlisten ? 'ACTIVE' : 'FAILED');
        console.log('[LiveEvents] Profile listener:', this.profileUnlisten ? 'ACTIVE' : 'FAILED');
        
    } catch (error) {
        console.error('[LiveEvents] Failed to set up event listeners:', error);
    }
};

NostrMailApp.prototype.handleLiveDM = function(dmData) {
    try {
        console.log('[LiveEvents] *** LIVE DM RECEIVED ***');
        console.log('[LiveEvents] DM Data:', dmData);
        console.log('[LiveEvents] Event payload:', JSON.stringify(dmData, null, 2));
        
        // Start performance timing
        const startTime = performance.now();
        console.log('[LiveEvents] Starting UI refresh at', startTime);
        
        // Run refreshes in parallel for better performance
        const refreshPromises = [];
        
        // Always refresh DM conversations to show new message immediately
        if (window.dmService) {
            console.log('[LiveEvents] Starting DM conversations refresh');
            const contactsPromise = window.dmService.loadDmContacts().catch(error => {
                console.error('[LiveEvents] Failed to refresh DM conversations:', error);
            });
            refreshPromises.push(contactsPromise);
        }
        
        // If currently viewing messages tab, also refresh the active conversation immediately
        if (document.querySelector('.tab-content#dm.active')) {
            // Check if we're viewing a conversation with this sender
            const currentContact = window.appState.getSelectedDmContact();
            if (currentContact && 
                (currentContact.pubkey === dmData.sender_pubkey || currentContact.pubkey === dmData.recipient_pubkey)) {
                console.log('[LiveEvents] Starting active conversation refresh');
                if (window.dmService) {
                    const messagesPromise = window.dmService.loadDmMessages(currentContact.pubkey).catch(error => {
                        console.error('[LiveEvents] Failed to refresh conversation messages:', error);
                    });
                    refreshPromises.push(messagesPromise);
                }
            }
        }
        
        // Wait for all refreshes to complete and log timing
        Promise.all(refreshPromises).then(() => {
            const endTime = performance.now();
            console.log(`[LiveEvents] UI refresh completed in ${(endTime - startTime).toFixed(2)}ms`);
        }).catch(error => {
            const endTime = performance.now();
            console.error(`[LiveEvents] UI refresh failed after ${(endTime - startTime).toFixed(2)}ms:`, error);
        });
        
        // Show notification for new message immediately (don't wait for UI refresh)
        const senderShort = dmData.sender_pubkey.slice(0, 8) + '...';
        notificationService.showInfo(`New message from ${senderShort}`);
        
        // Try to add message to UI immediately if possible (experimental)
        this.tryDirectMessageInsertion(dmData);
        
        // Update unread count or other UI indicators
        // TODO: Implement unread count system
        
        console.log('[LiveEvents] Live DM processed successfully');
        
    } catch (error) {
        console.error('[LiveEvents] Error handling live DM:', error);
    }
};

NostrMailApp.prototype.handleLiveProfileUpdate = function(profileData) {
    try {
        // Only update if it's for the current user and we're on profile tab
        const currentPubkey = appState.getKeypair()?.public_key;
        const isCurrentUser = profileData.pubkey === currentPubkey;
        const isOnProfileTab = document.querySelector('.tab-content#profile.active');
        
        if (isCurrentUser && isOnProfileTab) {
            console.log('[LiveEvents] Updating profile UI for live update');
            
            // Compare with current profile to detect actual changes
            const currentFields = this.editableProfileFields || {};
            const newFields = profileData.fields || {};
            
            // Check if any fields actually changed
            let hasChanges = false;
            const fieldsToCheck = ['name', 'display_name', 'about', 'picture', 'email', 'website', 'banner', 'nip05'];
            
            // Check if any field changed
            for (const field of fieldsToCheck) {
                const currentValue = currentFields[field] || '';
                const newValue = newFields[field] || '';
                if (String(currentValue).trim() !== String(newValue).trim()) {
                    hasChanges = true;
                    console.log(`[LiveEvents] Profile field '${field}' changed: '${currentValue}' -> '${newValue}'`);
                    break;
                }
            }
            
            // Also check if any new fields were added or removed
            const currentKeys = new Set(Object.keys(currentFields));
            const newKeys = new Set(Object.keys(newFields));
            if (currentKeys.size !== newKeys.size) {
                hasChanges = true;
                console.log('[LiveEvents] Profile fields count changed');
            } else {
                // Check if any field values differ (including fields not in fieldsToCheck)
                for (const key of newKeys) {
                    const currentValue = currentFields[key] || '';
                    const newValue = newFields[key] || '';
                    if (String(currentValue).trim() !== String(newValue).trim()) {
                        hasChanges = true;
                        console.log(`[LiveEvents] Profile field '${key}' changed: '${currentValue}' -> '${newValue}'`);
                        break;
                    }
                }
            }
            
            // Update profile UI with new data
            const updatedProfile = {
                pubkey: profileData.pubkey,
                fields: profileData.fields,
                created_at: profileData.created_at,
                raw_content: profileData.raw_content
            };
            
            this.renderProfileFromObject(updatedProfile);
            
            // Update localStorage cache
            this.updateProfileCache(updatedProfile);
            
            // Only show notification if something actually changed
            if (hasChanges) {
                notificationService.showInfo('Profile updated from another device');
            } else {
                console.log('[LiveEvents] Profile update received but no changes detected');
            }
        }
        
        // Update contact profile if this person is in contacts
        if (profileData.fields) {
            contactsService.updateContactProfile(profileData.pubkey, profileData.fields);
        }
        
        console.log('[LiveEvents] Live profile update processed successfully');
        
    } catch (error) {
        console.error('[LiveEvents] Error handling live profile update:', error);
    }
};

NostrMailApp.prototype.updateProfileCache = function(profileData) {
    try {
        const pubkey = profileData.pubkey;
        if (pubkey) {
            let profileDict = {};
            const cached = localStorage.getItem('nostr_mail_profiles');
            if (cached) {
                profileDict = JSON.parse(cached);
            }
            profileDict[pubkey] = profileData;
            localStorage.setItem('nostr_mail_profiles', JSON.stringify(profileDict));
            console.log('[LiveEvents] Profile cache updated for', pubkey);
        }
    } catch (error) {
        console.error('[LiveEvents] Error updating profile cache:', error);
    }
};

NostrMailApp.prototype.cleanupLiveEvents = async function() {
    if (this.liveEventsActive) {
        try {
            // Stop the backend subscription
            await TauriService.stopLiveEventSubscription();
            
            // Clean up event listeners
            if (this.dmUnlisten) {
                this.dmUnlisten();
                this.dmUnlisten = null;
            }
            
            if (this.profileUnlisten) {
                this.profileUnlisten();
                this.profileUnlisten = null;
            }
            
            this.liveEventsActive = false;
            this.updateLiveEventsStatus('inactive', 'Disconnected');
            console.log('[LiveEvents] Live events cleaned up successfully');
            
        } catch (error) {
            console.error('[LiveEvents] Error cleaning up live events:', error);
        }
    }
};

NostrMailApp.prototype.updateLiveEventsStatus = function(status, text) {
    try {
        const indicator = domManager.get('liveEventsIndicator');
        const textElement = domManager.get('liveEventsText');
        
        if (indicator && textElement) {
            // Remove existing status classes
            indicator.classList.remove('active', 'inactive', 'error');
            
            // Add new status class
            indicator.classList.add(status);
            
            // Update text
            textElement.textContent = text;
            
            console.log(`[LiveEvents] Status updated: ${status} - ${text}`);
        }
    } catch (error) {
        console.error('[LiveEvents] Error updating status indicator:', error);
    }
};

// Debug method to check live events status
NostrMailApp.prototype.debugLiveEvents = function() {
    console.log('=== LIVE EVENTS DEBUG INFO ===');
    console.log('Live events active:', this.liveEventsActive);
    console.log('Has keypair:', appState.hasKeypair());
    console.log('Current pubkey:', appState.getKeypair()?.public_key);
    console.log('DM listener active:', !!this.dmUnlisten);
    console.log('Profile listener active:', !!this.profileUnlisten);
    
    const indicator = domManager.get('liveEventsIndicator');
    const textElement = domManager.get('liveEventsText');
    console.log('Status indicator element:', !!indicator);
    console.log('Status text element:', !!textElement);
    if (indicator) {
        console.log('Current status classes:', indicator.className);
    }
    if (textElement) {
        console.log('Current status text:', textElement.textContent);
    }
    
    // Test backend connection
    TauriService.getLiveSubscriptionStatus().then(status => {
        console.log('Backend subscription status:', status);
    }).catch(error => {
        console.error('Failed to get backend status:', error);
    });
    
    console.log('=== END DEBUG INFO ===');
};

// Experimental: Try to insert message directly into UI for instant updates
NostrMailApp.prototype.tryDirectMessageInsertion = function(dmData) {
    try {
        console.log('[LiveEvents] Attempting direct message insertion');
        
        // Only try if we're viewing the messages tab
        if (!document.querySelector('.tab-content#dm.active')) {
            console.log('[LiveEvents] Not on messages tab, skipping direct insertion');
            return;
        }
        
        // Check if we're viewing a conversation with this sender/recipient
        const currentContact = window.appState.getSelectedDmContact();
        if (!currentContact) {
            console.log('[LiveEvents] No active conversation, skipping direct insertion');
            return;
        }
        
        const isRelevantMessage = currentContact.pubkey === dmData.sender_pubkey || 
                                 currentContact.pubkey === dmData.recipient_pubkey;
        
        if (!isRelevantMessage) {
            console.log('[LiveEvents] Message not for current conversation, skipping direct insertion');
            return;
        }
        
        // Find the messages container
        const messagesContainer = document.querySelector('#dm-messages');
        if (!messagesContainer) {
            console.log('[LiveEvents] Messages container not found, skipping direct insertion');
            return;
        }
        
        // Check if this message already exists (prevent duplicates)
        const existingMessage = messagesContainer.querySelector(`[data-event-id="${dmData.event_id}"]`);
        if (existingMessage) {
            console.log('[LiveEvents] Message already exists in UI, skipping direct insertion');
            return;
        }
        
        console.log('[LiveEvents] Messages container found, proceeding with insertion');
        
        // Create message element matching the actual DM service structure
        const messageDiv = document.createElement('div');
        messageDiv.className = 'message';
        messageDiv.setAttribute('data-event-id', dmData.event_id);
        
        // Determine if this is an incoming or outgoing message
        const currentUserPubkey = window.appState.getKeypair()?.public_key;
        const isOutgoing = dmData.sender_pubkey === currentUserPubkey;
        
        if (isOutgoing) {
            messageDiv.classList.add('outgoing');
        } else {
            messageDiv.classList.add('incoming');
        }
        
        // Format timestamp to match existing messages
        const messageDate = new Date(dmData.created_at * 1000);
        const now = new Date();
        const isToday = messageDate.toDateString() === now.toDateString();
        const dateTimeDisplay = isToday 
            ? messageDate.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
            : messageDate.toLocaleString([], { 
                month: 'short', 
                day: 'numeric', 
                hour: '2-digit', 
                minute: '2-digit' 
            });
        
        // Use the same structure as dm-service.js
        messageDiv.innerHTML = `
            <div class="message-content">
                <div class="message-text">${window.Utils?.escapeHtml(dmData.content) || dmData.content || '[Encrypted message]'}</div>
                <div class="message-meta">
                    <div class="message-time">${dateTimeDisplay}</div>
                    <span class="message-status live-message" title="Live message">⚡</span>
                </div>
            </div>
        `;
        
        // Add to messages container
        messagesContainer.appendChild(messageDiv);
        console.log('[LiveEvents] Message element appended to container');
        
        // Scroll to bottom
        messagesContainer.scrollTop = messagesContainer.scrollHeight;
        console.log('[LiveEvents] Scrolled to bottom');
        
        console.log('[LiveEvents] Message inserted directly into UI successfully');
        console.log('[LiveEvents] Message content preview:', dmData.content?.substring(0, 50) + '...');
        
    } catch (error) {
        console.error('[LiveEvents] Error in direct message insertion:', error);
        // Fail silently - the regular refresh will handle it
    }
};

// Reload the currently active page/tab
NostrMailApp.prototype.reloadActivePage = async function() {
    try {
        // Check which tab is currently active and reload it
        if (document.querySelector('.tab-content#inbox.active')) {
            console.log('[APP] Reloading inbox after keypair change');
            if (window.emailService) {
                await window.emailService.loadEmails();
            }
        } else if (document.querySelector('.tab-content#sent.active')) {
            console.log('[APP] Reloading sent emails after keypair change');
            if (window.emailService) {
                await window.emailService.loadSentEmails();
            }
        } else if (document.querySelector('.tab-content#drafts.active')) {
            console.log('[APP] Reloading drafts after keypair change');
            if (window.emailService) {
                await window.emailService.loadDrafts();
            }
        } else if (document.querySelector('.tab-content#contacts.active')) {
            console.log('[APP] Reloading contacts after keypair change');
            if (contactsService) {
                await contactsService.loadContacts();
            }
        } else if (document.querySelector('.tab-content#dm.active')) {
            console.log('[APP] Reloading DM contacts after keypair change');
            if (window.dmService) {
                await window.dmService.loadDmContacts();
            }
        } else if (document.querySelector('.tab-content#profile.active')) {
            console.log('[APP] Reloading profile after keypair change');
            this.loadProfile();
        }
        // Note: compose and settings tabs don't need reloading
    } catch (error) {
        console.error('[APP] Error reloading active page:', error);
    }
};

// Check email settings when switching to compose tab
NostrMailApp.prototype.checkEmailSettingsForCompose = function() {
    const hasEmailSettings = appState.hasEmailSettingsConfigured();
    const warningBanner = document.getElementById('compose-email-settings-warning');
    const headerSettingsLink = document.getElementById('compose-header-settings-link');
    const sendBtn = domManager.get('sendBtn');
    
    // Helper function to navigate to settings
    const goToSettings = () => {
        const settingsTab = document.querySelector('[data-tab="settings"]');
        if (settingsTab) {
            settingsTab.click();
        }
    };
    
    if (!hasEmailSettings) {
        // Add link in tab header if it doesn't exist
        if (!headerSettingsLink) {
            const composeTab = document.getElementById('compose');
            if (composeTab) {
                const tabHeader = composeTab.querySelector('.tab-header');
                if (tabHeader) {
                    // Create or get actions container
                    let actionsContainer = tabHeader.querySelector('.compose-actions');
                    if (!actionsContainer) {
                        actionsContainer = document.createElement('div');
                        actionsContainer.className = 'compose-actions';
                        tabHeader.appendChild(actionsContainer);
                    }
                    
                    const link = document.createElement('a');
                    link.id = 'compose-header-settings-link';
                    link.href = '#';
                    link.className = 'btn btn-primary';
                    link.style.cssText = 'text-decoration: none;';
                    link.innerHTML = '<i class="fas fa-cog"></i> Configure Email Settings';
                    link.addEventListener('click', (e) => {
                        e.preventDefault();
                        goToSettings();
                    });
                    actionsContainer.appendChild(link);
                }
            }
        } else {
            // Show existing header link
            headerSettingsLink.style.display = 'inline-block';
        }
        
        // Show warning banner if it doesn't exist
        if (!warningBanner) {
            const composeTab = document.getElementById('compose');
            if (composeTab) {
                const tabHeader = composeTab.querySelector('.tab-header');
                if (tabHeader) {
                    const banner = document.createElement('div');
                    banner.id = 'compose-email-settings-warning';
                    banner.className = 'alert alert-warning';
                    banner.style.cssText = 'margin: 16px; padding: 12px; border-radius: 4px; background-color: #fff3cd; border: 1px solid #ffc107; color: #856404;';
                    banner.innerHTML = `
                        <strong><i class="fas fa-exclamation-triangle"></i> Email Settings Not Configured</strong>
                        <p style="margin: 8px 0 0 0;">Please configure your email settings before sending emails. <a href="#" id="compose-banner-settings-link" style="color: #856404; text-decoration: underline; font-weight: bold;">Configure settings now</a></p>
                    `;
                    tabHeader.insertAdjacentElement('afterend', banner);
                    
                    // Add click handler for banner link
                    const bannerLink = document.getElementById('compose-banner-settings-link');
                    if (bannerLink) {
                        bannerLink.addEventListener('click', (e) => {
                            e.preventDefault();
                            goToSettings();
                        });
                    }
                }
            }
        } else {
            // Show existing banner
            warningBanner.style.display = 'block';
        }
        
        // Disable send button
        if (sendBtn) {
            sendBtn.disabled = true;
            sendBtn.title = 'Please configure email settings before sending';
            sendBtn.style.opacity = '0.6';
            sendBtn.style.cursor = 'not-allowed';
        }
    } else {
        // Hide header link if settings are configured
        if (headerSettingsLink) {
            headerSettingsLink.style.display = 'none';
        }
        
        // Hide warning banner if settings are configured
        if (warningBanner) {
            warningBanner.style.display = 'none';
        }
        
        // Enable send button
        if (sendBtn) {
            sendBtn.disabled = false;
            sendBtn.title = '';
            sendBtn.style.opacity = '1';
            sendBtn.style.cursor = 'pointer';
        }
    }
}

// Update compose page buttons based on settings
NostrMailApp.prototype.updateComposeButtons = function() {
    const settings = appState.getSettings();
    if (!settings) return;
    
    const encryptBtn = domManager.get('encryptBtn');
    const signBtn = domManager.get('signBtn');
    const sendBtn = domManager.get('sendBtn');
    
    if (!sendBtn) return;
    
    // Get settings (default to true if not set)
    const autoEncrypt = settings.automatically_encrypt !== false;
    const autoSign = settings.automatically_sign !== false;
    
    // Hide/show encrypt button
    if (encryptBtn) {
        encryptBtn.style.display = autoEncrypt ? 'none' : 'inline-block';
    }
    
    // Hide/show sign button
    if (signBtn) {
        signBtn.style.display = autoSign ? 'none' : 'inline-block';
    }
    
    // Update send button icons
    let icons = [];
    let tooltipParts = [];
    
    // Add encrypt icon if auto encrypt is enabled
    if (autoEncrypt) {
        icons.push('<i class="fas fa-lock"></i>');
        tooltipParts.push('encrypt');
    }
    
    // Add sign icon if auto sign is enabled
    if (autoSign) {
        icons.push('<i class="fas fa-pen"></i>');
        tooltipParts.push('sign');
    }
    
    // Always include the paper plane icon last
    icons.push('<i class="fas fa-paper-plane"></i>');
    tooltipParts.push('send');
    
    // Build tooltip text
    let tooltipText = 'Will ';
    if (tooltipParts.length === 1) {
        tooltipText += tooltipParts[0];
    } else if (tooltipParts.length === 2) {
        tooltipText += tooltipParts[0] + ' and ' + tooltipParts[1];
    } else {
        tooltipText += tooltipParts.slice(0, -1).join(', ') + ', and ' + tooltipParts[tooltipParts.length - 1];
    }
    tooltipText += ' email';
    
    // Update the button content (preserve the button element and event listeners)
    sendBtn.innerHTML = icons.join(' ') + ' Send';
    sendBtn.title = tooltipText;
}

// Tab switching
// Check if device is mobile portrait mode
NostrMailApp.prototype.isMobilePortrait = function() {
    if (typeof window === 'undefined') return false;
    
    // Check width and orientation
    const isPortrait = window.matchMedia('(max-width: 480px) and (orientation: portrait)').matches;
    return isPortrait;
}

// Debug function to check landscape button styles
NostrMailApp.prototype.debugLandscapeButtons = function() {
    // #region agent log
    const windowWidth = window.innerWidth;
    const windowHeight = window.innerHeight;
    const orientation = window.innerWidth > window.innerHeight ? 'landscape' : 'portrait';
    const landscapeMediaQuery = window.matchMedia('(max-width: 480px) and (orientation: landscape)');
    const portraitMediaQuery = window.matchMedia('(max-width: 480px) and (orientation: portrait)');
    const mobileMediaQuery = window.matchMedia('(max-width: 768px)');
    
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:1965',message:'Debug landscape buttons - window dimensions',data:{windowWidth,windowHeight,orientation,landscapeMatches:landscapeMediaQuery.matches,portraitMatches:portraitMediaQuery.matches,mobileMatches:mobileMediaQuery.matches,hypothesisId:'A'},timestamp:Date.now(),sessionId:'debug-session',runId:'debug'})}).catch(()=>{});
    // #endregion
    
    // Check for buttons in tab headers
    const tabHeaders = document.querySelectorAll('.tab-header');
    // #region agent log
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:1975',message:'Debug landscape buttons - tab headers found',data:{tabHeaderCount:tabHeaders.length,hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'debug'})}).catch(()=>{});
    // #endregion
    
    tabHeaders.forEach((header, idx) => {
        const buttons = header.querySelectorAll('.btn');
        const contactsActions = header.querySelector('.contacts-actions');
        const composeActions = header.querySelector('.compose-actions');
        const profileActions = header.querySelector('.profile-actions');
        
        // #region agent log
        const buttonData = Array.from(buttons).map((btn, btnIdx) => {
            const computedStyle = window.getComputedStyle(btn);
            return {
                index: btnIdx,
                className: btn.className,
                width: computedStyle.width,
                height: computedStyle.height,
                padding: computedStyle.padding,
                fontSize: computedStyle.fontSize,
                display: computedStyle.display,
                hasInlineStyle: btn.hasAttribute('style'),
                inlineStyle: btn.getAttribute('style') || null,
                textContent: btn.textContent.trim().substring(0, 30)
            };
        });
        
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:1990',message:'Debug landscape buttons - header button styles',data:{headerIndex:idx,buttonCount:buttons.length,buttons:buttonData,hasContactsActions:!!contactsActions,hasComposeActions:!!composeActions,hasProfileActions:!!profileActions,hypothesisId:'B'},timestamp:Date.now(),sessionId:'debug-session',runId:'debug'})}).catch(()=>{});
        // #endregion
        
        // Check for CSS rule matches
        if (contactsActions) {
            const computedStyle = window.getComputedStyle(contactsActions);
            // #region agent log
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:2000',message:'Debug landscape buttons - contacts-actions computed styles',data:{gap:computedStyle.gap,flexWrap:computedStyle.flexWrap,display:computedStyle.display,hypothesisId:'D'},timestamp:Date.now(),sessionId:'debug-session',runId:'debug'})}).catch(()=>{});
            // #endregion
        }
    });
    
    // Check if landscape media query CSS is loaded
    const styleSheets = Array.from(document.styleSheets);
    let responsiveCssFound = false;
    styleSheets.forEach((sheet, idx) => {
        try {
            if (sheet.href && sheet.href.includes('responsive.css')) {
                responsiveCssFound = true;
                // #region agent log
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:2010',message:'Debug landscape buttons - responsive.css found',data:{sheetIndex:idx,href:sheet.href,hypothesisId:'E'},timestamp:Date.now(),sessionId:'debug-session',runId:'debug'})}).catch(()=>{});
                // #endregion
            }
        } catch (e) {
            // Cross-origin stylesheets may throw errors
        }
    });
    
    // #region agent log
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:2020',message:'Debug landscape buttons - summary',data:{responsiveCssFound,hypothesisId:'A'},timestamp:Date.now(),sessionId:'debug-session',runId:'debug'})}).catch(()=>{});
    // #endregion
}

// Initialize mobile navigation
NostrMailApp.prototype.initializeMobileNavigation = function() {
    // Set initial state
    if (this.isMobilePortrait()) {
        this.showNavbar();
    }
    
    // Listen for orientation/resize changes
    window.addEventListener('resize', () => {
        if (this.isMobilePortrait()) {
            // Ensure navbar mode if switching to portrait
            if (this.mobileNavState === 'page') {
                // Don't force navbar, but update state
                this.updateMobileNavState();
            }
        } else {
            // Exit mobile nav mode on larger screens
            const appContainer = document.querySelector('.app-container');
            if (appContainer) {
                appContainer.classList.remove('navbar-mode');
            }
            // Show all back buttons as hidden
            const backButtons = document.querySelectorAll('.back-to-nav-btn');
            backButtons.forEach(btn => btn.style.display = 'none');
        }
    });
    
    // Listen for orientation changes
    window.addEventListener('orientationchange', () => {
        setTimeout(() => {
            // #region agent log
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:1994',message:'Orientation change detected',data:{hypothesisId:'A'},timestamp:Date.now(),sessionId:'debug-session',runId:'orientation-change'})}).catch(()=>{});
            // #endregion
            if (this.isMobilePortrait()) {
                this.updateMobileNavState();
            }
            // Debug button styles after orientation change
            this.debugLandscapeButtons();
        }, 100);
    });
    
    // Also listen for resize events to catch landscape mode
    window.addEventListener('resize', () => {
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:2005',message:'Resize event detected',data:{width:window.innerWidth,height:window.innerHeight,hypothesisId:'A'},timestamp:Date.now(),sessionId:'debug-session',runId:'resize'})}).catch(()=>{});
        // #endregion
        setTimeout(() => {
            this.debugLandscapeButtons();
        }, 100);
    });
    
    // Setup swipe gesture detection
    this.setupSwipeGestures();
    
    // Setup back button handlers
    this.setupBackButtons();
}

// Update mobile navigation state based on current view
NostrMailApp.prototype.updateMobileNavState = function() {
    const appContainer = document.querySelector('.app-container');
    if (!appContainer) return;
    
    if (appContainer.classList.contains('navbar-mode')) {
        this.mobileNavState = 'navbar';
    } else {
        this.mobileNavState = 'page';
    }
}

// Show navbar (main page)
NostrMailApp.prototype.showNavbar = function() {
    if (!this.isMobilePortrait()) return;
    
    const appContainer = document.querySelector('.app-container');
    if (!appContainer) return;
    
    appContainer.classList.add('navbar-mode');
    this.mobileNavState = 'navbar';
    
    // Hide all back buttons
    const backButtons = document.querySelectorAll('.back-to-nav-btn');
    backButtons.forEach(btn => btn.style.display = 'none');
    
    // Remove active state from all nav items
    const navItems = domManager.get('navItems');
    if (navItems) {
        navItems.forEach(item => item.classList.remove('active'));
    }
    
    // Hide all tab contents
    const tabContents = domManager.get('tabContents');
    if (tabContents) {
        tabContents.forEach(tab => tab.classList.remove('active'));
    }
}

// Show a specific page
NostrMailApp.prototype.showPage = function(tabName) {
    if (!this.isMobilePortrait()) return;
    
    const appContainer = document.querySelector('.app-container');
    if (!appContainer) return;
    
    // Remove navbar mode
    appContainer.classList.remove('navbar-mode');
    this.mobileNavState = 'page';
    
    // Show the selected tab
    const tabContents = domManager.get('tabContents');
    if (tabContents) {
        tabContents.forEach(tab => {
            tab.classList.remove('active');
        });
    }
    
    const newTab = document.getElementById(tabName);
    if (newTab) {
        newTab.classList.add('active');
    }
    
    // Show back button for this tab
    const backButton = newTab?.querySelector('.back-to-nav-btn');
    if (backButton) {
        backButton.style.display = 'flex';
    }
    
    // Update nav item active state
    const navItems = domManager.get('navItems');
    if (navItems) {
        navItems.forEach(item => {
            item.classList.remove('active');
            if (item.dataset.tab === tabName) {
                item.classList.add('active');
            }
        });
    }
}

// Setup swipe gesture detection
NostrMailApp.prototype.setupSwipeGestures = function() {
    const mainContent = document.querySelector('.main-content');
    if (!mainContent) return;
    
    mainContent.addEventListener('touchstart', (e) => {
        if (!this.isMobilePortrait() || this.mobileNavState !== 'page') return;
        
        const touch = e.touches[0];
        this.swipeStartX = touch.clientX;
        this.swipeStartY = touch.clientY;
    }, { passive: true });
    
    mainContent.addEventListener('touchmove', (e) => {
        if (!this.isMobilePortrait() || this.mobileNavState !== 'page' || !this.swipeStartX) return;
        
        const touch = e.touches[0];
        const deltaX = touch.clientX - this.swipeStartX;
        const deltaY = touch.clientY - this.swipeStartY;
        
        // Only handle horizontal swipes (swipe right)
        if (Math.abs(deltaX) > Math.abs(deltaY) && deltaX > 0) {
            // Swiping right - allow it
            e.preventDefault();
        }
    }, { passive: false });
    
    mainContent.addEventListener('touchend', (e) => {
        if (!this.isMobilePortrait() || this.mobileNavState !== 'page' || !this.swipeStartX) {
            this.swipeStartX = null;
            this.swipeStartY = null;
            return;
        }
        
        const touch = e.changedTouches[0];
        const deltaX = touch.clientX - this.swipeStartX;
        const deltaY = touch.clientY - this.swipeStartY;
        
        // Check if it's a right swipe (positive deltaX) and significant enough
        if (deltaX > this.swipeThreshold && Math.abs(deltaX) > Math.abs(deltaY)) {
            // Swipe right detected - return to navbar
            this.showNavbar();
        }
        
        this.swipeStartX = null;
        this.swipeStartY = null;
    }, { passive: true });
}

// Setup back button handlers
NostrMailApp.prototype.setupBackButtons = function() {
    const backButtons = document.querySelectorAll('.back-to-nav-btn');
    backButtons.forEach(btn => {
        // Other tabs use default behavior (show navbar)
        btn.addEventListener('click', () => {
            this.showNavbar();
        });
    });
    
    // Setup profile back button (uses dm-back-to-list-btn class for styling)
    const profileBackButton = document.getElementById('profile-back-to-contacts-btn');
    if (profileBackButton) {
        profileBackButton.addEventListener('click', () => {
            // Clear viewing profile pubkey to return to own profile next time
            appState.clearViewingProfilePubkey();
            this.switchTab('contacts');
        });
    }
}

NostrMailApp.prototype.switchTab = function(tabName) {
    try {
        // Prevent switching if a sync/load operation is in progress
        const refreshSent = domManager.get('refreshSent');
        if (refreshSent && refreshSent.disabled) {
            console.log('[JS] Tab switch blocked: sent emails are currently loading');
            return; // Don't switch tabs while loading
        }
        
        // Handle mobile portrait navigation differently
        if (this.isMobilePortrait()) {
            // If in navbar mode, show the selected page
            if (this.mobileNavState === 'navbar') {
                this.showPage(tabName);
            } else {
                // Already on a page, switch to new page
                this.showPage(tabName);
            }
        } else {
            // Desktop/tablet mode - use standard tab switching
            const tabContents = domManager.get('tabContents');
            if (tabContents) {
                tabContents.forEach(tab => {
                    tab.classList.remove('active');
                });
            }
            
            const newTab = document.getElementById(tabName);
            if (newTab) {
                newTab.classList.add('active');
            }
            
            const navItems = domManager.get('navItems');
            if (navItems) {
                navItems.forEach(item => {
                    item.classList.remove('active');
                    if (item.dataset.tab === tabName) {
                        item.classList.add('active');
                    }
                });
            }
        }
    } catch (error) {
        console.error('[JS] Error switching tabs:', error);
    }

    if (tabName === 'profile') {
        // Check if we're viewing another user's profile
        const viewingPubkey = appState.getViewingProfilePubkey();
        if (viewingPubkey) {
            // Load the profile we're viewing
            this.loadProfile(viewingPubkey);
        } else {
            // Load own profile and ensure viewing pubkey is cleared
            appState.clearViewingProfilePubkey();
            this.loadProfile();
        }
    }
    if (tabName === 'settings') {
        this.loadRelaysFromDatabase();
        this.initializeSettingsAccordion();
    }
    if (tabName === 'contacts') {
        // Always reload contacts when switching to contacts tab to ensure they match the current user
        // This handles the case where a new keypair was generated but contacts tab wasn't active
        contactsService.loadContacts();
        // Set up lazy image loading when contacts tab is actually opened
        // Images will load as contacts scroll into view (IntersectionObserver)
        setTimeout(() => {
            contactsService.setupLazyImageLoading();
        }, 100);
        
        // Check if there's a selected contact - if so, show detail view; otherwise show list view
        const selectedContact = window.appState.getSelectedContact();
        if (selectedContact) {
            // Show detail view for selected contact
            contactsService.showContactDetailView(selectedContact);
        } else {
            // Show list view
            contactsService.showContactsListView();
        }
    }
    if (tabName === 'dm') {
        // Only load DM contacts if they haven't been loaded yet
        if (window.dmService) {
            if (!appState.getDmContacts() || appState.getDmContacts().length === 0) {
                window.dmService.loadDmContacts();
            } else {
                // Just render the existing DM contacts
                window.dmService.renderDmContacts();
            }
        }
    }
    if (tabName === 'inbox') {
        if (window.emailService) {
            window.emailService.loadEmails();
        }
    }
    if (tabName === 'sent') {
        if (window.emailService) {
            window.emailService.loadSentEmails();
        }
    }
    if (tabName === 'drafts') {
        if (window.emailService) {
            window.emailService.loadDrafts();
        }
    }
    if (tabName === 'compose') {
        // Check if email settings are configured
        this.checkEmailSettingsForCompose();
        
        if (window.emailService) {
            // Clear current draft state when switching to compose (unless we're loading a draft)
            if (!window.emailService.currentDraftId) {
                window.emailService.clearCurrentDraft();
            }
            // Try to restore contact selection when switching to compose
            window.emailService.restoreContactSelection();
        }
        // Update compose buttons based on settings
        this.updateComposeButtons();
    }
}

// Modal functions
NostrMailApp.prototype.showModal = function(title, content) {
    try {
        const modalTitle = domManager.get('modalTitle');
        const modalBody = domManager.get('modalBody');
        const modalOverlay = domManager.get('modalOverlay');
        
        if (modalTitle) modalTitle.textContent = title;
        if (modalBody) modalBody.innerHTML = content;
        if (modalOverlay) modalOverlay.classList.remove('hidden');
    } catch (error) {
        console.error('Error showing modal:', error);
    }
}

NostrMailApp.prototype.hideModal = function() {
    try {
        const modalOverlay = domManager.get('modalOverlay');
        if (modalOverlay) modalOverlay.classList.add('hidden');
    } catch (error) {
        console.error('Error hiding modal:', error);
    }
}

// Show new DM compose
NostrMailApp.prototype.showNewDmCompose = function() {
    // This would open a modal to compose a new DM
    // For now, just show a placeholder
    notificationService.showInfo('New DM functionality coming soon');
}

// Settings management
NostrMailApp.prototype.saveSettings = async function(showNotification = false) {
    try {
        // Get current keypair - REQUIRED for saving
        const currentKeypair = appState.getKeypair();
        const currentPublicKey = currentKeypair ? currentKeypair.public_key : null;
        
        // Validate npriv key if provided
        const nprivKey = domManager.getValue('nprivKey')?.trim() || '';
        if (nprivKey && !nprivKey.startsWith('npriv1') && !nprivKey.startsWith('nsec1')) {
            this.showSettingsStatus('error', 'Invalid Nostr private key format. Should start with "npriv1" or "nsec1"');
            // Reset settings to defaults when invalid key format detected
            await this.resetSettingsToDefaults();
            // Toast notification is shown in showSettingsStatus for better visibility
            return false;
        }
        
        // Validate email address format if provided
        const emailAddress = domManager.getValue('emailAddress')?.trim() || '';
        if (emailAddress) {
            const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
            if (!emailRegex.test(emailAddress)) {
                this.showSettingsStatus('error', 'Invalid email address format. Please enter a valid email address (e.g., user@example.com)');
                // Highlight the email field
                const emailInput = domManager.get('emailAddress');
                if (emailInput) {
                    emailInput.classList.add('invalid');
                    emailInput.style.borderColor = '#dc3545';
                    emailInput.focus();
                    const errorMessage = document.getElementById('email-address-error');
                    if (errorMessage) {
                        errorMessage.textContent = 'Please enter a valid email address (e.g., user@example.com)';
                        errorMessage.style.display = 'block';
                    }
                }
                return false;
            }
        }
        
        // Ensure we have a valid keypair before saving settings
        // Settings must be saved under the public key, so each keypair has its own settings
        let publicKey = null;
        let isNewKeypair = false;
        
        // If a private key is provided in the form, update appState.keypair
        if (nprivKey && (nprivKey.startsWith('npriv1') || nprivKey.startsWith('nsec1'))) {
            const isValid = await TauriService.validatePrivateKey(nprivKey);
            if (!isValid) {
                this.showSettingsStatus('error', 'Invalid private key');
                // Reset settings to defaults when invalid key detected
                await this.resetSettingsToDefaults();
                // Toast notification is shown in showSettingsStatus for better visibility
                return false;
            }
            publicKey = await TauriService.getPublicKeyFromPrivate(nprivKey);
            const keypair = { private_key: nprivKey, public_key: publicKey };
            
            // Check if this is a different keypair by comparing public keys
            // (public key uniquely identifies the keypair)
            // Store the current public key BEFORE updating appState to ensure accurate comparison
            isNewKeypair = !currentPublicKey || currentPublicKey !== publicKey;
            
            console.log('[APP] Keypair change check:', {
                currentPublicKey: currentPublicKey ? currentPublicKey.substring(0, 20) + '...' : 'null',
                newPublicKey: publicKey.substring(0, 20) + '...',
                isNewKeypair: isNewKeypair
            });
            
            // If keypair changed, update appState FIRST so loadSettingsForPubkey can use the correct private key for decryption
            if (isNewKeypair) {
                console.log('[APP] Keypair changed, updating appState before loading settings');
                appState.setKeypair(keypair);
                localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
                this.renderProfilePubkey();
                
                console.log('[APP] Keypair changed, loading settings for new pubkey:', publicKey);
                await this.loadSettingsForPubkey(publicKey);
                // Note: loadSettingsForPubkey already sets lastLoadedPubkey, so we don't need to set it here
                
                console.log('[LiveEvents] Keypair changed, restarting live events');
                await this.cleanupLiveEvents();
                await this.initializeLiveEvents();
                
                // Reinitialize the persistent Nostr client with the new keypair
                console.log('[APP] Keypair changed, reinitializing persistent Nostr client');
                await this.initializeNostrClient();
                
                // Clear contacts from appState since they belong to the old user
                appState.setContacts([]);
                // Immediately clear the contacts UI if visible
                if (window.contactsService) {
                    window.contactsService.renderContacts();
                }
                
                // Reload the currently active page to reflect the new keypair
                await this.reloadActivePage();
                
                // After loading settings for new keypair, check if settings were actually loaded
                const loadedSettings = appState.getSettings();
                if (loadedSettings && Object.keys(loadedSettings).length > 0) {
                    console.log('[APP] Settings loaded for new keypair, keys:', Object.keys(loadedSettings));
                    // Settings were loaded and form was populated, so we can continue to save them
                    // (this allows user to see the loaded settings and optionally modify before saving)
                } else {
                    console.log('[APP] No settings found for new keypair, will use defaults');
                }
            } else {
                // Same keypair, just update appState
                appState.setKeypair(keypair);
                localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
                this.renderProfilePubkey();
                
                // If on profile tab, reload profile
                if (document.querySelector('.tab-content#profile.active')) {
                    this.loadProfile();
                }
            }
        } else {
            // No private key provided in form, use current keypair's public key
            if (!currentPublicKey) {
                this.showSettingsStatus('error', 'No keypair found. Please enter a private key.');
                return false;
            }
            publicKey = currentPublicKey;
        }
        
        // Build settings object from form values (or use loaded settings if keypair changed)
        // If keypair changed and settings were loaded, use loaded settings from appState
        // Otherwise, use form values
        let settings;
        if (isNewKeypair) {
            // Use loaded settings from appState (which were just loaded for the new keypair)
            // After loadSettingsForPubkey, the form should be populated with loaded values
            // So we can safely use form values which now reflect the loaded settings
            const loadedSettings = appState.getSettings();
            console.log('[APP] Building settings object for new keypair, loadedSettings keys:', Object.keys(loadedSettings || {}));
            
            // Use loaded settings if available, otherwise use form values (which should be populated by loadSettingsForPubkey)
            // Prioritize loadedSettings but fall back to form values as they should match after populateSettingsForm()
            settings = {
                npriv_key: nprivKey,
                encryption_algorithm: (loadedSettings && loadedSettings.encryption_algorithm) ? loadedSettings.encryption_algorithm : (domManager.getValue('encryptionAlgorithm') || 'nip44'),
                email_address: (loadedSettings && loadedSettings.email_address) ? loadedSettings.email_address : (domManager.getValue('emailAddress') || ''),
                password: (loadedSettings && loadedSettings.password) ? loadedSettings.password : (domManager.getValue('emailPassword') || ''),
                smtp_host: (loadedSettings && loadedSettings.smtp_host) ? loadedSettings.smtp_host : (domManager.getValue('smtpHost') || ''),
                smtp_port: (loadedSettings && loadedSettings.smtp_port) ? loadedSettings.smtp_port : (parseInt(domManager.getValue('smtpPort')) || 587),
                imap_host: (loadedSettings && loadedSettings.imap_host) ? loadedSettings.imap_host : (domManager.getValue('imapHost') || ''),
                imap_port: (loadedSettings && loadedSettings.imap_port) ? loadedSettings.imap_port : (parseInt(domManager.getValue('imapPort')) || 993),
                use_tls: (loadedSettings && loadedSettings.use_tls !== undefined) ? loadedSettings.use_tls : (domManager.get('use-tls')?.checked || false),
                email_filter: (loadedSettings && loadedSettings.email_filter) ? loadedSettings.email_filter : (domManager.getValue('emailFilterPreference') || 'nostr'),
                send_matching_dm: (loadedSettings && loadedSettings.send_matching_dm !== undefined) ? loadedSettings.send_matching_dm : (domManager.get('send-matching-dm-preference')?.checked !== false),
                sync_cutoff_days: (loadedSettings && loadedSettings.sync_cutoff_days) ? loadedSettings.sync_cutoff_days : (parseInt(domManager.getValue('syncCutoffDays')) || 365),
                emails_per_page: (loadedSettings && loadedSettings.emails_per_page) ? loadedSettings.emails_per_page : (parseInt(domManager.getValue('emailsPerPage')) || 50),
                require_signature: (loadedSettings && loadedSettings.require_signature !== undefined) ? loadedSettings.require_signature : (domManager.get('require-signature-preference')?.checked !== false),
                hide_undecryptable_emails: (loadedSettings && loadedSettings.hide_undecryptable_emails !== undefined) ? loadedSettings.hide_undecryptable_emails : (domManager.get('hide-undecryptable-emails-preference')?.checked !== false),
                automatically_encrypt: (loadedSettings && loadedSettings.automatically_encrypt !== undefined) ? loadedSettings.automatically_encrypt : (domManager.get('automatically-encrypt-preference')?.checked !== false),
                automatically_sign: (loadedSettings && loadedSettings.automatically_sign !== undefined) ? loadedSettings.automatically_sign : (domManager.get('automatically-sign-preference')?.checked !== false),
                hide_unsigned_messages: (loadedSettings && loadedSettings.hide_unsigned_messages !== undefined) ? loadedSettings.hide_unsigned_messages : (domManager.get('hide-unsigned-messages-preference')?.checked !== false)
            };
        } else {
            // Use form values (normal case)
            // Ensure auto-sign is enabled if auto-encrypt is enabled
            const autoEncryptPref = domManager.get('automatically-encrypt-preference');
            const autoSignPref = domManager.get('automatically-sign-preference');
            const autoEncryptEnabled = autoEncryptPref?.checked !== false; // Default to true
            
            // If auto-encrypt is enabled, also enable auto-sign
            if (autoEncryptEnabled && autoSignPref && !autoSignPref.checked) {
                autoSignPref.checked = true;
                console.log('[JS] Auto-encrypt is enabled, enabling auto-sign as well');
            }
            
            settings = {
                npriv_key: nprivKey || currentKeypair.private_key,
                encryption_algorithm: domManager.getValue('encryptionAlgorithm') || 'nip44',
                email_address: domManager.getValue('emailAddress') || '',
                password: domManager.getValue('emailPassword') || '',
                smtp_host: domManager.getValue('smtpHost') || '',
                smtp_port: parseInt(domManager.getValue('smtpPort')) || 587,
                imap_host: domManager.getValue('imapHost') || '',
                imap_port: parseInt(domManager.getValue('imapPort')) || 993,
                use_tls: domManager.get('use-tls')?.checked || false,
                email_filter: domManager.getValue('emailFilterPreference') || 'nostr',
                send_matching_dm: domManager.get('send-matching-dm-preference')?.checked !== false, // Default to true
                sync_cutoff_days: parseInt(domManager.getValue('syncCutoffDays')) || 365, // Default to 1 year
                emails_per_page: parseInt(domManager.getValue('emailsPerPage')) || 50, // Default to 50
                require_signature: domManager.get('require-signature-preference')?.checked !== false, // Default to true
                hide_undecryptable_emails: domManager.get('hide-undecryptable-emails-preference')?.checked !== false, // Default to true
                automatically_encrypt: autoEncryptEnabled,
                automatically_sign: autoSignPref?.checked !== false, // Default to true (will be true if auto-encrypt is enabled)
                hide_unsigned_messages: domManager.get('hide-unsigned-messages-preference')?.checked !== false // Default to true
            };
        }
        
        // Keep localStorage as backup
        localStorage.setItem('nostr_mail_settings', JSON.stringify(settings));
        appState.setSettings(settings);
        appState.setNprivKey(settings.npriv_key);
        
        // Save settings to database with pubkey association (REQUIRED)
        // Settings are saved under the public key, so each keypair has its own email settings
        // This ensures that when a different private key is used, there will be different email settings
        try {
            // Ensure publicKey is valid (should always be set by this point)
            if (!publicKey) {
                console.error('[APP] No public key available for saving settings');
                this.showSettingsStatus('error', 'No public key available. Please enter a private key.');
                return false;
            }
            
            // Convert settings object to key-value pairs for database storage
            const settingsMap = new Map();
            settingsMap.set('encryption_algorithm', settings.encryption_algorithm);
            settingsMap.set('email_address', settings.email_address);
            settingsMap.set('password', settings.password);
            settingsMap.set('smtp_host', settings.smtp_host);
            settingsMap.set('smtp_port', settings.smtp_port.toString());
            settingsMap.set('imap_host', settings.imap_host);
            settingsMap.set('imap_port', settings.imap_port.toString());
            settingsMap.set('use_tls', settings.use_tls.toString());
            settingsMap.set('email_filter', settings.email_filter);
            settingsMap.set('send_matching_dm', settings.send_matching_dm.toString());
            settingsMap.set('sync_cutoff_days', settings.sync_cutoff_days.toString());
            settingsMap.set('emails_per_page', settings.emails_per_page.toString());
            settingsMap.set('require_signature', settings.require_signature.toString());
            settingsMap.set('hide_undecryptable_emails', (settings.hide_undecryptable_emails || false).toString());
            settingsMap.set('automatically_encrypt', (settings.automatically_encrypt !== undefined ? settings.automatically_encrypt : true).toString());
            settingsMap.set('automatically_sign', (settings.automatically_sign !== undefined ? settings.automatically_sign : true).toString());
            settingsMap.set('hide_unsigned_messages', (settings.hide_unsigned_messages !== undefined ? settings.hide_unsigned_messages : true).toString());
            
            const settingsObj = Object.fromEntries(settingsMap);
            // Get private key for encryption
            const currentKeypair = appState.getKeypair();
            const privateKeyForEncryption = currentKeypair ? currentKeypair.private_key : null;
            await TauriService.dbSaveSettingsBatch(publicKey, settingsObj, privateKeyForEncryption);
            console.log('[APP] Settings saved to database for pubkey:', publicKey);
            
            // Update last loaded pubkey tracker after successful save
            appState.setLastLoadedPubkey(publicKey);
            
            // Update compose page warning if on compose tab
            if (document.querySelector('.tab-content#compose.active')) {
                this.checkEmailSettingsForCompose();
            }
            
            this.showSettingsStatus('success', 'Settings saved automatically');
            // Toast notification is shown in showSettingsStatus for better visibility
        } catch (error) {
            console.error('[APP] Failed to save settings to database:', error);
            this.showSettingsStatus('error', 'Failed to save settings');
            // Toast notification is shown in showSettingsStatus for better visibility
            return false;
        }
        
        await this.saveRelays();
        this.saveRelaysToLocalStorage();
        
        // Update compose buttons based on new settings
        this.updateComposeButtons();
        
        return true;
    } catch (error) {
        console.error('Error saving settings:', error);
        this.showSettingsStatus('error', 'Error saving settings');
        // Toast notification is shown in showSettingsStatus for better visibility
        return false;
    }
}

// Show settings status message (toast notification only)
NostrMailApp.prototype.showSettingsStatus = function(type, message) {
    // Show toast notification for visibility
    if (type === 'success') {
        notificationService.showSuccess(message, 2000);
    } else if (type === 'error') {
        notificationService.showError(message, 4000);
    } else if (type === 'warning') {
        notificationService.showWarning(message, 4000);
    } else {
        notificationService.showInfo(message, 3000);
    }
}

// Setup auto-save for settings fields
NostrMailApp.prototype.setupAutoSaveSettings = function() {
    // Track if we're currently populating the form to avoid auto-save loops
    let isPopulatingForm = false;
    
    // Debounce function to prevent excessive saves
    let saveTimeout = null;
    const debouncedSave = () => {
        // Don't auto-save if we're populating the form
        if (isPopulatingForm) {
            return;
        }
        
        if (saveTimeout) {
            clearTimeout(saveTimeout);
        }
        saveTimeout = setTimeout(() => {
            this.saveSettings(false); // Don't show notification for auto-save
        }, 1000); // Wait 1 second after last change
    };
    
    // List of all settings fields that should trigger auto-save
    const settingsFields = [
        'nprivKey',
        'encryptionAlgorithm',
        'emailAddress',
        'emailPassword',
        'smtpHost',
        'smtpPort',
        'imapHost',
        'imapPort',
        'use-tls',
        'emailFilterPreference',
        'send-matching-dm-preference',
        'require-signature-preference',
        'hide-undecryptable-emails-preference',
        'automatically-encrypt-preference',
        'automatically-sign-preference',
        'hide-unsigned-messages-preference',
        'syncCutoffDays',
        'emailsPerPage'
    ];
    
    settingsFields.forEach(fieldId => {
        const field = domManager.get(fieldId);
        if (field) {
            // Handle different input types
            if (field.type === 'checkbox') {
                field.addEventListener('change', debouncedSave);
            } else {
                field.addEventListener('input', debouncedSave);
                field.addEventListener('change', debouncedSave);
            }
        }
    });
    
    // Special handling: When auto-encrypt is enabled, also enable auto-sign
    const autoEncryptPref = domManager.get('automatically-encrypt-preference');
    const autoSignPref = domManager.get('automatically-sign-preference');
    
    if (autoEncryptPref && autoSignPref) {
        autoEncryptPref.addEventListener('change', (e) => {
            // When auto-encrypt is enabled, also enable auto-sign
            if (e.target.checked) {
                autoSignPref.checked = true;
                console.log('[JS] Auto-encrypt enabled, also enabling auto-sign');
                // Trigger save after enabling auto-sign
                debouncedSave();
            }
            // Note: We don't disable auto-sign when auto-encrypt is disabled
            // User can still have auto-sign enabled independently
        });
    }
    
    // Also listen for email provider changes (which may auto-fill other fields)
    const emailProvider = domManager.get('emailProvider');
    if (emailProvider) {
        emailProvider.addEventListener('change', () => {
            // Wait a bit for auto-fill to complete, then save
            setTimeout(debouncedSave, 500);
        });
    }
    
    // Add email validation
    this.setupEmailValidation();
    
    // Store flag for populateSettingsForm to use
    this._isPopulatingForm = () => isPopulatingForm;
    this._setPopulatingForm = (value) => { isPopulatingForm = value; };
}

// Setup email address validation
NostrMailApp.prototype.setupEmailValidation = function() {
    const emailInput = domManager.get('emailAddress');
    if (!emailInput) return;
    
    // Email validation regex pattern
    const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
    
    // Create error message element
    let errorMessage = document.getElementById('email-address-error');
    if (!errorMessage) {
        errorMessage = document.createElement('div');
        errorMessage.id = 'email-address-error';
        errorMessage.className = 'form-error';
        errorMessage.style.cssText = 'color: #dc3545; font-size: 0.875rem; margin-top: 4px; display: none;';
        emailInput.parentNode.appendChild(errorMessage);
    }
    
    // Validation function
    const validateEmail = () => {
        const email = emailInput.value.trim();
        
        // Clear previous validation state
        emailInput.classList.remove('invalid', 'valid');
        errorMessage.style.display = 'none';
        
        // Skip validation if field is empty (required validation happens on save)
        if (!email) {
            return true; // Allow empty for now, will be validated on save
        }
        
        // Validate email format
        if (!emailRegex.test(email)) {
            emailInput.classList.add('invalid');
            errorMessage.textContent = 'Please enter a valid email address (e.g., user@example.com)';
            errorMessage.style.display = 'block';
            emailInput.style.borderColor = '#dc3545';
            return false;
        } else {
            emailInput.classList.add('valid');
            errorMessage.style.display = 'none';
            emailInput.style.borderColor = '#28a745';
            return true;
        }
    };
    
    // Validate on input (real-time)
    emailInput.addEventListener('input', () => {
        // Only validate if field has content
        if (emailInput.value.trim()) {
            validateEmail();
        } else {
            // Clear validation state when empty
            emailInput.classList.remove('invalid', 'valid');
            errorMessage.style.display = 'none';
            emailInput.style.borderColor = '';
        }
    });
    
    // Validate on blur (when user leaves field)
    emailInput.addEventListener('blur', validateEmail);
    
    // Validate on form submission
    const settingsForm = emailInput.closest('form') || document.querySelector('.settings-section');
    if (settingsForm) {
        // We'll validate in saveSettings function
    }
}

NostrMailApp.prototype.populateSettingsForm = async function() {
    console.log('[QR] populateSettingsForm called');
    const settings = appState.getSettings();
    if (!settings) return;
    
    try {
        // Set flag to prevent auto-save during form population
        if (this._setPopulatingForm) {
            this._setPopulatingForm(true);
        }
        
        // Populate private key field from cached keypair if available and field is empty
        // This ensures the user sees they're already logged in when the app starts
        const nprivKeyInput = domManager.get('nprivKey');
        const currentNprivValue = nprivKeyInput?.value?.trim() || '';
        if (!currentNprivValue) {
            const keypair = appState.getKeypair();
            if (keypair && keypair.private_key) {
                domManager.setValue('nprivKey', keypair.private_key);
                console.log('[APP] Populated private key field from cached keypair');
            }
        }
        
        domManager.setValue('encryptionAlgorithm', settings.encryption_algorithm || 'nip44');
        domManager.setValue('emailAddress', settings.email_address || '');
        domManager.setValue('emailPassword', settings.password || '');
        domManager.setValue('smtpHost', settings.smtp_host || '');
        domManager.setValue('smtpPort', settings.smtp_port || '');
        domManager.setValue('imapHost', settings.imap_host || '');
        domManager.setValue('imapPort', settings.imap_port || '');
        domManager.get('use-tls').checked = settings.use_tls || false;
        domManager.setValue('emailFilterPreference', settings.email_filter || 'nostr');
        domManager.setValue('syncCutoffDays', settings.sync_cutoff_days || 365);
        domManager.setValue('emailsPerPage', settings.emails_per_page || 50);
        
        // Set send matching DM preference (default to true if not set)
        const sendMatchingDmPref = domManager.get('send-matching-dm-preference');
        if (sendMatchingDmPref) {
            sendMatchingDmPref.checked = settings.send_matching_dm !== false;
        }
        
        // Set require signature preference (default to true if not set)
        const requireSignaturePref = domManager.get('require-signature-preference');
        if (requireSignaturePref) {
            requireSignaturePref.checked = settings.require_signature !== false;
        }
        
        // Set hide undecryptable emails preference (default to true if not set)
        const hideUndecryptablePref = domManager.get('hide-undecryptable-emails-preference');
        if (hideUndecryptablePref) {
            hideUndecryptablePref.checked = settings.hide_undecryptable_emails !== false;
        }
        
        // Set automatically encrypt preference (default to true if not set)
        const automaticallyEncryptPref = domManager.get('automatically-encrypt-preference');
        let autoEncryptEnabled = false;
        if (automaticallyEncryptPref) {
            autoEncryptEnabled = settings.automatically_encrypt !== false;
            automaticallyEncryptPref.checked = autoEncryptEnabled;
        }
        
        // Set automatically sign preference (default to true if not set)
        // If auto-encrypt is enabled, also enable auto-sign
        const automaticallySignPref = domManager.get('automatically-sign-preference');
        if (automaticallySignPref) {
            const autoSignEnabled = settings.automatically_sign !== false;
            // Enable auto-sign if auto-encrypt is enabled OR if it was already enabled
            automaticallySignPref.checked = autoEncryptEnabled || autoSignEnabled;
            
            if (autoEncryptEnabled && !autoSignEnabled) {
                console.log('[JS] Auto-encrypt is enabled, enabling auto-sign as well');
            }
        }
        
        // Set hide unsigned messages preference (default to true if not set)
        const hideUnsignedMessagesPref = domManager.get('hide-unsigned-messages-preference');
        if (hideUnsignedMessagesPref) {
            hideUnsignedMessagesPref.checked = settings.hide_unsigned_messages !== false;
        }
        
        // Update compose buttons based on settings
        this.updateComposeButtons();
        
        // Detect and set the email provider based on saved settings
        const emailProvider = domManager.get('emailProvider');
        if (emailProvider) {
            const provider = Utils.detectEmailProvider(settings);
            emailProvider.value = provider;
        }
        
        // Update public key display if npriv is available
        await this.updatePublicKeyDisplay();
        this.setupQrCodeEventListeners();
        
        // Clear flag after a short delay to allow any pending events to settle
        setTimeout(() => {
            if (this._setPopulatingForm) {
                this._setPopulatingForm(false);
            }
        }, 100);
    } catch (error) {
        console.error('Error populating settings form:', error);
    }
}

NostrMailApp.prototype.setupQrCodeEventListeners = function() {
    console.log('[QR] setupQrCodeEventListeners called');
    const nprivKeyInput = domManager.get('nprivKey');
    const publicKeyDisplayInput = domManager.get('publicKeyDisplay');
    let qrNprivBtn = domManager.get('qrNprivBtn');
    let qrNpubBtn = domManager.get('qrNpubBtn');

    if (qrNprivBtn && nprivKeyInput) {
        const newQrNprivBtn = qrNprivBtn.cloneNode(true);
        qrNprivBtn.parentNode.replaceChild(newQrNprivBtn, qrNprivBtn);
        newQrNprivBtn.addEventListener('click', async () => {
            const value = nprivKeyInput.value;
            console.log('[QR] Private key QR button clicked. Value:', value);
            if (!value) return;
            try {
                const dataUrl = await TauriService.generateQrCode(value);
                showQrModal('Private Key QR Code', dataUrl, value);
            } catch (err) {
                notificationService.showError('Failed to generate QR code');
            }
        });
    }
    qrNprivBtn = domManager.get('qrNprivBtn'); // update reference in case replaced
    
    // Setup scan QR code button for private key
    const scanQrNprivBtn = domManager.get('scanQrNprivBtn');
    if (scanQrNprivBtn && nprivKeyInput) {
        const newScanQrNprivBtn = scanQrNprivBtn.cloneNode(true);
        scanQrNprivBtn.parentNode.replaceChild(newScanQrNprivBtn, scanQrNprivBtn);
        newScanQrNprivBtn.addEventListener('click', () => {
            console.log('[QR] Scan private key QR button clicked');
            this.scanPrivateKeyQRCode();
        });
    }
    
    if (qrNpubBtn && publicKeyDisplayInput) {
        const newQrNpubBtn = qrNpubBtn.cloneNode(true);
        qrNpubBtn.parentNode.replaceChild(newQrNpubBtn, qrNpubBtn);
        newQrNpubBtn.addEventListener('click', async () => {
            const value = publicKeyDisplayInput.value;
            console.log('[QR] Public key QR button clicked. Value:', value);
            if (!value) return;
            try {
                const dataUrl = await TauriService.generateQrCode(value);
                showQrModal('Public Key QR Code', dataUrl, value);
            } catch (err) {
                notificationService.showError('Failed to generate QR code');
            }
        });
    }
    function showQrModal(title, dataUrl, value) {
        const modalContent = `
            <div style="text-align:center;">
                <img src="${dataUrl}" alt="QR Code" style="max-width:220px;max-height:220px;" />
                <p style="word-break:break-all;margin-top:10px;">${value}</p>
            </div>
        `;
        window.app.showModal(title, modalContent);
    }
}

// Scan QR code for private key
NostrMailApp.prototype.scanPrivateKeyQRCode = async function() {
    let html5QrCode = null;
    let scannerStarted = false;
    let isCleanedUp = false;
    
    const modalContent = `
        <div id="qr-scanner-container" style="text-align:center;">
            <p style="margin-bottom:15px;">Point your camera at the QR code</p>
            <div id="qr-reader" style="width:100%;max-width:400px;margin:0 auto;"></div>
            <div id="qr-scanner-error" style="display:none;color:#dc3545;margin-top:15px;"></div>
            <div style="margin-top:20px;">
                <button id="cancel-qr-scan-btn" class="btn btn-secondary">
                    <i class="fas fa-times"></i> Cancel
                </button>
            </div>
        </div>
    `;
    
    window.app.showModal('Scan Private Key QR Code', modalContent);
    
    const cleanup = () => {
        if (isCleanedUp) return;
        isCleanedUp = true;
        
        if (html5QrCode && scannerStarted) {
            try {
                html5QrCode.stop().then(() => {
                    html5QrCode.clear();
                }).catch(err => {
                    console.error('[QR] Error stopping scanner:', err);
                });
            } catch (err) {
                console.error('[QR] Error during cleanup:', err);
            }
        }
        html5QrCode = null;
        scannerStarted = false;
    };
    
    // Setup cancel button
    const cancelBtn = document.getElementById('cancel-qr-scan-btn');
    if (cancelBtn) {
        cancelBtn.addEventListener('click', () => {
            cleanup();
            window.app.hideModal();
        });
    }
    
    // Check if Html5Qrcode is available
    if (typeof Html5Qrcode === 'undefined') {
        const errorDiv = document.getElementById('qr-scanner-error');
        if (errorDiv) {
            errorDiv.style.display = 'block';
            errorDiv.textContent = 'QR scanner library not loaded. Please refresh the page.';
        }
        return;
    }
    
    // Check camera availability
    if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
        const errorDiv = document.getElementById('qr-scanner-error');
        if (errorDiv) {
            errorDiv.style.display = 'block';
            errorDiv.textContent = 'Camera access is not available in this browser.';
        }
        return;
    }
    
    try {
        const qrReader = document.getElementById('qr-reader');
        if (!qrReader) {
            console.error('[QR] QR reader element not found');
            return;
        }
        
        html5QrCode = new Html5Qrcode('qr-reader');
        
        // Start scanning
        await html5QrCode.start(
            { facingMode: 'environment' }, // Use back camera on mobile
            {
                fps: 10,
                qrbox: { width: 250, height: 250 }
            },
            (decodedText, decodedResult) => {
                console.log('[QR] QR code scanned:', decodedText);
                
                // Validate the scanned text is a private key
                const scannedKey = decodedText.trim();
                if (!scannedKey.startsWith('npriv1') && !scannedKey.startsWith('nsec1')) {
                    const errorDiv = document.getElementById('qr-scanner-error');
                    if (errorDiv) {
                        errorDiv.style.display = 'block';
                        errorDiv.textContent = 'Invalid QR code: Not a valid private key format (should start with npriv1 or nsec1)';
                    }
                    return;
                }
                
                // Populate the private key input field
                const nprivKeyInput = domManager.get('nprivKey');
                if (nprivKeyInput) {
                    nprivKeyInput.value = scannedKey;
                    // Trigger input event to update public key display
                    nprivKeyInput.dispatchEvent(new Event('input', { bubbles: true }));
                    notificationService.showSuccess('Private key scanned successfully');
                }
                
                // Cleanup and close modal
                cleanup();
                setTimeout(() => {
                    window.app.hideModal();
                }, 100);
            },
            (errorMessage) => {
                // Ignore scanning errors (they're frequent during scanning)
                // Only log if it's not a common "not found" error
                if (!errorMessage.includes('No QR code found')) {
                    console.log('[QR] Scanning:', errorMessage);
                }
            }
        );
        
        scannerStarted = true;
        
    } catch (error) {
        console.error('[QR] Error starting scanner:', error);
        const errorDiv = document.getElementById('qr-scanner-error');
        if (errorDiv) {
            errorDiv.style.display = 'block';
            if (error.name === 'NotAllowedError' || error.name === 'PermissionDeniedError') {
                errorDiv.textContent = 'Camera permission denied. Please allow camera access and try again.';
            } else if (error.name === 'NotFoundError' || error.name === 'DevicesNotFoundError') {
                errorDiv.textContent = 'No camera found on this device.';
            } else {
                errorDiv.textContent = `Camera error: ${error.message || 'Unknown error'}`;
            }
        }
        notificationService.showError('Failed to start camera');
        cleanup();
    }
    
    // Cleanup when modal is closed (if user clicks outside or ESC)
    const modalOverlay = domManager.get('modalOverlay');
    if (modalOverlay) {
        const observer = new MutationObserver((mutations) => {
            mutations.forEach((mutation) => {
                if (mutation.type === 'attributes' && mutation.attributeName === 'class') {
                    if (modalOverlay.classList.contains('hidden')) {
                        cleanup();
                        observer.disconnect();
                    }
                }
            });
        });
        observer.observe(modalOverlay, { attributes: true });
    }
}

// Test connection
NostrMailApp.prototype.testConnection = async function() {
    if (!appState.hasSettings()) {
        notificationService.showError('Please save your settings first');
        return;
    }
    
    try {
        domManager.disable('testConnectionBtn');
        domManager.setHTML('testConnectionBtn', '<span class="loading"></span> Testing...');
        
        // Try to load emails as a connection test
        await window.emailService.loadEmails();
        notificationService.showSuccess('Connection test successful');
        
    } catch (error) {
        console.error('Connection test failed:', error);
        notificationService.showError('Connection test failed: ' + error);
    } finally {
        domManager.enable('testConnectionBtn');
        domManager.setHTML('testConnectionBtn', '<i class="fas fa-test-tube"></i> Test Connection');
    }
}

// Relay management
NostrMailApp.prototype.saveRelays = async function() {
    try {
        await TauriService.setRelays(appState.getRelays());
    } catch (error) {
        console.error('Failed to save relays:', error);
        notificationService.showError('Could not save relays to backend.');
    }
}

NostrMailApp.prototype.saveRelaysToLocalStorage = function() {
    localStorage.setItem('nostr_mail_relays', JSON.stringify(appState.getRelays()));
}

NostrMailApp.prototype.renderRelays = async function() {
    const relaysList = domManager.get('relaysList');
    if (!relaysList) return;
    
    // Get relay connection statuses from backend
    let relayStatuses = [];
    try {
        relayStatuses = await TauriService.getRelayStatus();
        console.log('Relay statuses from backend:', relayStatuses);
    } catch (error) {
        console.error('Failed to get relay statuses:', error);
    }
    
    // Always render the full list (no edit mode toggle needed)
    // Clear only the relay items, not the add-relay-section
    const existingRelayItems = relaysList.querySelectorAll('.relay-item');
    existingRelayItems.forEach(item => item.remove());
    
    // Sort relays by updated_at descending
    const relays = [...appState.getRelays()].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at));
    
    // Get the add-relay-section to insert relays before it
    const addRelaySection = relaysList.querySelector('.add-relay-section');
    
    relays.forEach((relay) => {
        // Find matching status from backend
        const status = relayStatuses.find(s => s.url === relay.url);
        const connectionStatus = status?.status || 'Disconnected';
        const statusClass = this.getRelayStatusClass(connectionStatus);
        const statusIcon = this.getRelayStatusIcon(connectionStatus);
        const statusText = this.getRelayStatusText(connectionStatus);
        
        const relayItem = document.createElement('div');
        relayItem.className = 'relay-item';
        // Add retry button for disconnected relays that are active
        const showRetryButton = relay.is_active && connectionStatus === 'Disconnected';
        const retryButtonHtml = showRetryButton ? 
            `<button class="btn btn-secondary btn-small retry-btn" data-relay-id="${relay.id}" data-relay-url="${relay.url}" title="Retry connection">
                <i class="fas fa-redo"></i>
            </button>` : '';
            
        relayItem.innerHTML = `
            <div class="relay-item-info">
                <span class="relay-item-url">${relay.url}</span>
                <div class="relay-status ${statusClass}">
                    <i class="fas ${statusIcon}"></i>
                    <span class="relay-status-text">${statusText}</span>
                </div>
            </div>
            <div class="relay-item-actions">
                ${retryButtonHtml}
                <label class="toggle-switch">
                    <input type="checkbox" ${relay.is_active ? 'checked' : ''} data-relay-id="${relay.id}" data-relay-url="${relay.url}">
                </label>
                <button class="btn btn-danger btn-small" data-relay-id="${relay.id}" data-relay-url="${relay.url}">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;
        
        // Insert before the add-relay-section
        if (addRelaySection) {
            relaysList.insertBefore(relayItem, addRelaySection);
        } else {
            relaysList.appendChild(relayItem);
        }
    });
    // Add event listeners after rendering
    relaysList.querySelectorAll('input[type="checkbox"]').forEach(toggle => {
        toggle.addEventListener('change', (e) => this.toggleRelayById(e.target.dataset.relayId, e.target.dataset.relayUrl));
    });
    relaysList.querySelectorAll('.btn-danger').forEach(button => {
        button.addEventListener('click', (e) => this.removeRelayById(e.currentTarget.dataset.relayId, e.currentTarget.dataset.relayUrl));
    });
    relaysList.querySelectorAll('.retry-btn').forEach(button => {
        button.addEventListener('click', (e) => this.retryRelayConnection(e.currentTarget.dataset.relayId, e.currentTarget.dataset.relayUrl));
    });
}

// Helper methods for relay status display
NostrMailApp.prototype.getRelayStatusClass = function(connectionStatus) {
    switch (connectionStatus) {
        case 'Connected': return 'status-connected';
        case 'Disconnected': return 'status-disconnected';
        case 'Disabled': return 'status-disabled';
        case 'Connecting': return 'status-connecting';
        case 'Disconnecting': return 'status-disconnecting';
        default: return 'status-unknown';
    }
}

NostrMailApp.prototype.getRelayStatusIcon = function(connectionStatus) {
    switch (connectionStatus) {
        case 'Connected': return 'fa-circle';
        case 'Disconnected': return 'fa-circle';
        case 'Disabled': return 'fa-circle';
        case 'Connecting': return 'fa-spinner fa-spin';
        case 'Disconnecting': return 'fa-spinner fa-spin';
        default: return 'fa-question-circle';
    }
}

NostrMailApp.prototype.getRelayStatusText = function(connectionStatus) {
    switch (connectionStatus) {
        case 'Connected': return 'Connected';
        case 'Disconnected': return 'Connection Failed';
        case 'Disabled': return 'Disabled';
        case 'Connecting': return 'Connecting...';
        case 'Disconnecting': return 'Disconnecting...';
        default: return 'Unknown';
    }
}

// Update status for a single relay without full re-render
NostrMailApp.prototype.updateSingleRelayStatus = function(relayUrl, connectionStatus) {
    const statusElement = document.querySelector(`input[data-relay-url="${relayUrl}"]`)?.closest('.relay-item')?.querySelector('.relay-status');
    if (statusElement) {
        const statusClass = this.getRelayStatusClass(connectionStatus);
        const statusIcon = this.getRelayStatusIcon(connectionStatus);
        const statusText = this.getRelayStatusText(connectionStatus);
        
        // Update the status element
        statusElement.className = `relay-status ${statusClass}`;
        statusElement.innerHTML = `
            <i class="fas ${statusIcon}"></i>
            <span class="relay-status-text">${statusText}</span>
        `;
    }
}

// Sync relay states and auto-disable disconnected ones
NostrMailApp.prototype.syncDisconnectedRelays = async function() {
    try {
        const updatedRelays = await TauriService.syncRelayStates();
        if (updatedRelays.length > 0) {
            console.log('[APP] Auto-disabled disconnected relays:', updatedRelays);
            
            // Update the UI for each disabled relay
            for (const relayUrl of updatedRelays) {
                // Update toggle switch to OFF
                const toggleElement = document.querySelector(`input[data-relay-url="${relayUrl}"]`);
                if (toggleElement) {
                    toggleElement.checked = false;
                }
                
                // Update status to Disabled
                this.updateSingleRelayStatus(relayUrl, 'Disabled');
                
                // Update local state
                const relay = appState.getRelays().find(r => r.url === relayUrl);
                if (relay) {
                    relay.is_active = false;
                    relay.updated_at = new Date().toISOString();
                }
            }
            
            // Show notification to user
            if (updatedRelays.length === 1) {
                notificationService.showWarning(`Relay ${updatedRelays[0]} was automatically disabled (disconnected)`);
            } else {
                notificationService.showWarning(`${updatedRelays.length} disconnected relays were automatically disabled`);
            }
        }
    } catch (error) {
        console.error('Failed to sync disconnected relays:', error);
    }
}

NostrMailApp.prototype.addRelay = async function() {
    const url = domManager.getValue('newRelayUrl')?.trim();
    if (url && (url.startsWith('ws://') || url.startsWith('wss://'))) {
        const relays = appState.getRelays();
        if (!relays.some(r => r.url === url)) {
            try {
                await TauriService.invoke('db_save_relay', {
                    relay: {
                        id: null,
                        url,
                        is_active: true,
                        created_at: new Date().toISOString(),
                        updated_at: new Date().toISOString()
                    }
                });
                domManager.clear('newRelayUrl');
                await this.loadRelaysFromDatabase();
                
                // Add the new relay to the existing client without disconnecting others
                try {
                    await TauriService.updateSingleRelay(url, true);
                    console.log(`[APP] Added new relay: ${url}`);
                    
                    // Verify connection status after a short delay
                    setTimeout(async () => {
                        try {
                            const relayStatuses = await TauriService.getRelayStatus();
                            const status = relayStatuses.find(s => s.url === url);
                            if (status) {
                                this.updateSingleRelayStatus(url, status.status);
                                if (status.status === 'Connected') {
                                    notificationService.showSuccess(`✅ Successfully connected to ${url}`);
                                } else if (status.status === 'Disconnected') {
                                    notificationService.showWarning(`⚠️ Relay added but connection pending: ${url}`);
                                }
                            }
                        } catch (error) {
                            console.error('Failed to verify relay status:', error);
                        }
                    }, 2000);
                } catch (relayError) {
                    console.warn(`[APP] Failed to add relay to client: ${relayError}`);
                    notificationService.showWarning(`Relay saved but connection failed: ${relayError}`);
                }
            } catch (error) {
                notificationService.showError('Failed to add relay: ' + error);
            }
        } else {
            notificationService.showError('Relay already exists.');
        }
    } else {
        notificationService.showError('Invalid relay URL. Must start with ws:// or wss://');
    }
}

NostrMailApp.prototype.toggleRelayById = async function(relayId, relayUrl) {
    const relay = appState.getRelays().find(r => String(r.id) === String(relayId) || r.url === relayUrl);
    if (relay) {
        try {
            const newActiveState = !relay.is_active;
            
            // Show immediate feedback with connecting/disconnecting status
            const intermediateStatus = newActiveState ? 'Connecting' : 'Disconnecting';
            this.updateSingleRelayStatus(relayUrl, intermediateStatus);
            
            // Update the database first
            await TauriService.invoke('db_save_relay', {
                relay: {
                    id: relay.id || null,
                    url: relay.url,
                    is_active: newActiveState,
                    created_at: relay.created_at,
                    updated_at: new Date().toISOString()
                }
            });
            
            // Update the backend Nostr client connection immediately
            try {
                await TauriService.updateSingleRelay(relayUrl, newActiveState);
                console.log(`[APP] ${newActiveState ? 'Connecting to' : 'Disconnecting from'} relay: ${relayUrl}`);
            } catch (relayError) {
                // Log the error but don't fail the entire operation
                console.warn(`[APP] Relay connection update had issues: ${relayError}`);
                // Show error status
                this.updateSingleRelayStatus(relayUrl, 'Disconnected');
                notificationService.showWarning(`Relay connection issue: ${relayError}`);
                return;
            }
            
            // Update the local state
            relay.is_active = newActiveState;
            relay.updated_at = new Date().toISOString();
            
            // Update just the toggle switch state without re-rendering everything
            const toggleElement = document.querySelector(`input[data-relay-url="${relayUrl}"]`);
            if (toggleElement) {
                toggleElement.checked = newActiveState;
            }
            
            // Show success feedback
            const successMessage = newActiveState ? 
                `Attempting to connect to relay: ${relayUrl}` : 
                `Disconnected from relay: ${relayUrl}`;
            notificationService.showInfo(successMessage);
            
            // Verify actual status after connection attempts
            setTimeout(async () => {
                try {
                    const relayStatuses = await TauriService.getRelayStatus();
                    const status = relayStatuses.find(s => s.url === relayUrl);
                    if (status) {
                        this.updateSingleRelayStatus(relayUrl, status.status);
                        
                        // Show final connection result
                        if (newActiveState && status.status === 'Connected') {
                            notificationService.showSuccess(`✅ Successfully connected to ${relayUrl}`);
                        } else if (newActiveState && status.status === 'Disconnected') {
                            notificationService.showError(`❌ Failed to connect to ${relayUrl}`);
                        }
                    } else {
                        // Fallback to expected status
                        const expectedStatus = newActiveState ? 'Connected' : 'Disabled';
                        this.updateSingleRelayStatus(relayUrl, expectedStatus);
                    }
                } catch (error) {
                    console.error('Failed to verify relay status:', error);
                    // Fallback to expected status
                    const expectedStatus = newActiveState ? 'Connected' : 'Disabled';
                    this.updateSingleRelayStatus(relayUrl, expectedStatus);
                }
            }, 2000); // Increased to 2 seconds for better connection verification
            
        } catch (error) {
            console.error('Failed to toggle relay:', error);
            notificationService.showError('Failed to update relay: ' + error);
            // Reset status on error
            this.updateSingleRelayStatus(relayUrl, 'Disconnected');
        }
    }
}

NostrMailApp.prototype.removeRelayById = async function(relayId, relayUrl) {
    const relay = appState.getRelays().find(r => String(r.id) === String(relayId) || r.url === relayUrl);
    if (relay) {
        try {
            // Store scroll position before update
            const relaysList = domManager.get('relaysList');
            const scrollTop = relaysList ? relaysList.scrollTop : 0;
            
            await TauriService.invoke('db_delete_relay', { url: relay.url });
            await this.loadRelaysFromDatabase();
            
            // Restore scroll position after update (adjust for removed item)
            if (relaysList) {
                relaysList.scrollTop = Math.max(0, scrollTop - 60); // Approximate height of one relay item
            }
        } catch (error) {
            notificationService.showError('Failed to remove relay: ' + error);
        }
    }
}

// Profile management
// Track the last rendered profile pubkey
NostrMailApp.prototype.lastRenderedProfilePubkey = null;

NostrMailApp.prototype.loadProfile = async function(pubkey = null) {
    // Determine which pubkey to load
    const targetPubkey = pubkey || (appState.getKeypair() && appState.getKeypair().public_key);
    const currentUserPubkey = appState.getKeypair() && appState.getKeypair().public_key;
    const isViewingOwnProfile = !pubkey || (targetPubkey === currentUserPubkey);
    
    // Store viewing mode
    this.isViewingOwnProfile = isViewingOwnProfile;
    this.viewingProfilePubkey = targetPubkey;
    
    // If viewing own profile, clear viewing pubkey from appState
    if (isViewingOwnProfile) {
        appState.clearViewingProfilePubkey();
    } else {
        appState.setViewingProfilePubkey(targetPubkey);
    }
    
    const profileSpinner = document.getElementById('profile-loading-spinner');
    const profileFieldsList = document.getElementById('profile-fields-list');
    const profilePicture = document.getElementById('profile-picture');

    // Only clear UI if switching pubkeys
    if (this.lastRenderedProfilePubkey !== null && this.lastRenderedProfilePubkey !== targetPubkey) {
        console.log('[Profile] Switching pubkey from', this.lastRenderedProfilePubkey, 'to', targetPubkey, '- clearing UI and showing spinner');
        if (profileFieldsList) profileFieldsList.innerHTML = '';
        if (profilePicture) {
            profilePicture.src = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><circle cx="60" cy="60" r="60" fill="%23e0e0e0"/><circle cx="60" cy="50" r="28" fill="%23bdbdbd"/><ellipse cx="60" cy="100" rx="38" ry="20" fill="%23bdbdbd"/></svg>';
            profilePicture.style.display = '';
        }
        if (profileSpinner) profileSpinner.style.display = '';
    }

    // Always try to render cached profile immediately
    let cachedProfile = null;
    let cachedPictureDataUrl = null;
    try {
        const cached = localStorage.getItem('nostr_mail_profiles');
        if (cached && targetPubkey) {
            const profileDict = JSON.parse(cached);
            cachedProfile = profileDict[targetPubkey];
            
            // Get cached picture for this specific pubkey
            if (isViewingOwnProfile) {
                cachedPictureDataUrl = localStorage.getItem('nostr_mail_profile_picture');
            }
            
            // Also check database cache for picture (faster and more reliable)
            if (!cachedPictureDataUrl && targetPubkey) {
                try {
                    // Get picture URL from cached profile if available
                    const cachedPictureUrl = cachedProfile?.fields?.picture || null;
                    cachedPictureDataUrl = await TauriService.getCachedProfileImage(targetPubkey, cachedPictureUrl);
                    if (cachedPictureDataUrl && isViewingOwnProfile) {
                        localStorage.setItem('nostr_mail_profile_picture', cachedPictureDataUrl);
                    }
                } catch (e) {
                    console.warn('[Profile] Error checking database cache for picture:', e);
                }
            }
            
            if (cachedProfile) {
                console.log('[Profile] Rendering cached profile for pubkey', targetPubkey);
                if (profileSpinner) profileSpinner.style.display = 'none';
                this.renderProfileFromObject(cachedProfile, cachedPictureDataUrl, isViewingOwnProfile);
            } else {
                console.log('[Profile] No cached profile found for pubkey', targetPubkey);
            }
        } else {
            console.log('[Profile] No cached profiles in localStorage or no target pubkey');
        }
    } catch (e) {
        console.warn('[Profile] Error loading cached profile:', e);
    }

    if (!targetPubkey) {
        if (profileSpinner) profileSpinner.style.display = 'none';
        console.log('No public key available to fetch profile.');
        this.renderProfilePubkey();
        if (Utils.isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'No public key available.';
            }
        }
        return;
    }

    // Check if we have any relays configured (persistent client will handle connection logic)
    const allRelays = appState.getRelays();
    if (allRelays.length === 0) {
        notificationService.showError('No relays configured to fetch profile from.');
        this.renderProfilePubkey();
        if (Utils.isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'No relays configured.';
            }
        }
        return;
    }

    try {
        // Use persistent client for better performance (reuses existing connections)
        const profile = await TauriService.fetchProfilePersistent(targetPubkey);

        if (profile) {
            // Remove "no profile" message if it exists
            const profileFieldsList = document.getElementById('profile-fields-list');
            if (profileFieldsList) {
                const existingMessage = profileFieldsList.querySelector('.profile-empty-message');
                if (existingMessage) {
                    existingMessage.remove();
                }
            }
            
            // Render profile immediately (fields and avatar URL)
            // This allows the browser to start loading the image while we fetch/cache the data URL
            this.renderProfileFromObject(profile, null, isViewingOwnProfile);
            
            // If there's a picture URL, fetch and cache it in the background
            if (profile.fields && profile.fields.picture) {
                const pictureUrl = profile.fields.picture;
                
                // Fetch and cache in background (don't block rendering)
                (async () => {
                    try {
                        // Check database cache first (fast)
                        // Pass picture URL to validate cache - if URL changed, cache is invalid
                        let dataUrl = await TauriService.getCachedProfileImage(targetPubkey, pictureUrl);
                        
                        // If not in cache, fetch it
                        if (!dataUrl) {
                            dataUrl = await TauriService.fetchImage(pictureUrl);
                            if (dataUrl) {
                                // Cache it for next time
                                await TauriService.cacheProfileImage(targetPubkey, dataUrl);
                            }
                        }
                        
                        // Update avatar with cached/fetched data URL if available
                        if (dataUrl) {
                            if (isViewingOwnProfile) {
                                localStorage.setItem('nostr_mail_profile_picture', dataUrl);
                            }
                            const profilePicture = document.getElementById('profile-picture');
                            if (profilePicture && profilePicture.src !== dataUrl) {
                                // Only update if still showing the same profile (check target pubkey)
                                const currentlyDisplayedPubkey = this.viewingProfilePubkey;
                                if (currentlyDisplayedPubkey === targetPubkey) {
                                    profilePicture.src = dataUrl;
                                    console.log('[Profile] Updated avatar with cached/fetched data URL');
                                } else {
                                    console.log('[Profile] Profile switched, skipping avatar update');
                                }
                            }
                        }
                    } catch (e) {
                        console.warn('[Profile] Failed to fetch/cache profile picture:', e);
                        // Avatar already rendered with URL, so this is fine
                    }
                })();
            } else if (isViewingOwnProfile) {
                localStorage.removeItem('nostr_mail_profile_picture');
            }
            // Cache the profile in localStorage
            if (targetPubkey) {
                let profileDict = {};
                const cached = localStorage.getItem('nostr_mail_profiles');
                if (cached) {
                    profileDict = JSON.parse(cached);
                }
                profileDict[targetPubkey] = profile;
                localStorage.setItem('nostr_mail_profiles', JSON.stringify(profileDict));
            }
            if (profileSpinner) profileSpinner.style.display = 'none';
        } else {
            if (isViewingOwnProfile) {
                console.log('[Profile] No profile found - rendering empty form for user to create profile');
                if (profileSpinner) profileSpinner.style.display = 'none';
                // Show placeholder fields and picture so the user can create a new profile
                const emptyFields = {};
                PROFILE_FIELD_ORDER.forEach(key => { emptyFields[key] = ''; });
                const emptyProfile = {
                    pubkey: targetPubkey,
                    fields: emptyFields
                };
                console.log('[Profile] Rendering empty profile with fields:', Object.keys(emptyFields));
                this.renderProfileFromObject(emptyProfile, null, true);
                
                // Show helpful message when no profile exists
                const profileFieldsList = document.getElementById('profile-fields-list');
                if (profileFieldsList) {
                    // Check if message already exists to avoid duplicates
                    let existingMessage = profileFieldsList.querySelector('.profile-empty-message');
                    if (!existingMessage) {
                        existingMessage = document.createElement('div');
                        existingMessage.className = 'profile-empty-message';
                        existingMessage.style.cssText = 'background: #e3f2fd; border-left: 4px solid #2196f3; padding: 12px; margin-bottom: 16px; border-radius: 4px;';
                        existingMessage.innerHTML = '<strong>No profile found.</strong> Fill in the fields below and click "Update Profile" to create your Nostr profile.';
                        profileFieldsList.insertBefore(existingMessage, profileFieldsList.firstChild);
                    }
                }
            } else {
                // Viewing other user's profile but no profile found
                if (profileSpinner) profileSpinner.style.display = 'none';
                const profileFieldsList = document.getElementById('profile-fields-list');
                if (profileFieldsList) {
                    profileFieldsList.innerHTML = '<div class="text-muted">No profile found for this user.</div>';
                }
            }
        }
        this.renderProfilePubkey(targetPubkey);
        this.updateProfileUI(isViewingOwnProfile, profile);
        if (Utils.isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                if (profile && profile.raw_content) {
                    rawJsonBox.value = profile.raw_content;
                } else if (profile && profile.fields) {
                    rawJsonBox.value = JSON.stringify(profile.fields, null, 2);
                } else {
                    rawJsonBox.value = 'No profile found.';
                }
            }
        }
    } catch (error) {
        if (profileSpinner) profileSpinner.style.display = 'none';
        console.error('Failed to fetch profile:', error);
        notificationService.showError('Could not fetch profile from relays.');
        this.renderProfilePubkey(targetPubkey);
        this.updateProfileUI(isViewingOwnProfile, null);
        if (Utils.isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'Error: ' + error;
            }
        }
    }
    // After rendering a profile (cached or fetched), update the last rendered pubkey
    this.lastRenderedProfilePubkey = targetPubkey;
}

NostrMailApp.prototype.refreshProfile = async function() {
    console.log('[Profile] Refreshing profile from network...');
    
    // Show loading state on refresh button
    const refreshBtn = domManager.get('refreshProfile');
    const originalRefreshBtnHTML = refreshBtn ? refreshBtn.innerHTML : null;
    if (refreshBtn) {
        refreshBtn.disabled = true;
        refreshBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Syncing...';
    }
    
    // Show loading spinner
    const profileSpinner = document.getElementById('profile-loading-spinner');
    if (profileSpinner) {
        profileSpinner.style.display = '';
    }
    
    // Show loading notification
    notificationService.showInfo('Syncing profile from network...');
    
    try {
        const currentPubkey = appState.getKeypair() && appState.getKeypair().public_key;
        if (!currentPubkey) {
            notificationService.showError('No public key available for profile sync');
            // Reset refresh button
            if (refreshBtn && originalRefreshBtnHTML) {
                refreshBtn.disabled = false;
                refreshBtn.innerHTML = originalRefreshBtnHTML;
            }
            if (profileSpinner) profileSpinner.style.display = 'none';
            return;
        }
        
        // Clear cached profile for current pubkey to force fresh fetch
        try {
            const cached = localStorage.getItem('nostr_mail_profiles');
            if (cached) {
                const profileDict = JSON.parse(cached);
                delete profileDict[currentPubkey];
                localStorage.setItem('nostr_mail_profiles', JSON.stringify(profileDict));
            }
            localStorage.removeItem('nostr_mail_profile_picture');
        } catch (e) {
            console.warn('[Profile] Error clearing cache:', e);
        }
        
        // Clear the UI to show we're refreshing
        const profileFieldsList = document.getElementById('profile-fields-list');
        if (profileFieldsList) {
            profileFieldsList.innerHTML = '';
        }
        
        // Force refresh by calling loadProfile (which will fetch from network since cache is cleared)
        const viewingPubkey = appState.getViewingProfilePubkey();
        await this.loadProfile(viewingPubkey || null);
        
        notificationService.showSuccess('Profile synced from network');
    } catch (error) {
        console.error('[Profile] Failed to refresh profile:', error);
        notificationService.showError('Failed to sync profile from network');
    } finally {
        // Reset refresh button
        if (refreshBtn && originalRefreshBtnHTML) {
            refreshBtn.disabled = false;
            refreshBtn.innerHTML = originalRefreshBtnHTML;
        }
        // Spinner will be hidden by loadProfile when it completes
    }
}

NostrMailApp.prototype.renderProfilePubkey = function(pubkey = null) {
    const pubkeyDiv = document.getElementById('profile-pubkey');
    if (!pubkeyDiv) return;
    
    const targetPubkey = pubkey || (appState.hasKeypair() && appState.getKeypair().public_key);
    if (targetPubkey) {
        const isOwnProfile = !pubkey || (pubkey === appState.getKeypair()?.public_key);
        pubkeyDiv.textContent = isOwnProfile 
            ? `Your npub: ${targetPubkey}` 
            : `npub: ${targetPubkey}`;
    } else {
        pubkeyDiv.textContent = '';
    }
}

// Update profile UI based on viewing mode
NostrMailApp.prototype.updateProfileUI = function(isViewingOwnProfile, profile) {
    const updateBtn = document.getElementById('update-profile-btn');
    const profileForm = document.getElementById('profile-fields-form');
    const profileHeader = document.querySelector('#profile .tab-header h2');
    const profileActions = document.querySelector('#profile .profile-actions');
    
    // Show/hide update button
    if (updateBtn) {
        updateBtn.style.display = isViewingOwnProfile ? 'block' : 'none';
    }
    
    // Disable form submission when viewing other users
    if (profileForm) {
        if (isViewingOwnProfile) {
            profileForm.onsubmit = (e) => {
                e.preventDefault();
                this.updateProfile();
            };
        } else {
            profileForm.onsubmit = (e) => {
                e.preventDefault();
                return false;
            };
        }
    }
    
    // Update header title and back button visibility
    const profileBackButton = document.getElementById('profile-back-to-contacts-btn');
    if (profileHeader) {
        if (isViewingOwnProfile) {
            profileHeader.textContent = 'Profile';
            // Hide back button when viewing own profile
            if (profileBackButton) {
                profileBackButton.style.display = 'none';
            }
        } else {
            // Get contact name if available
            const contacts = appState.getContacts() || [];
            const contact = contacts.find(c => c.pubkey === this.viewingProfilePubkey);
            const name = contact?.name || profile?.fields?.name || profile?.fields?.display_name || 'Contact Profile';
            profileHeader.textContent = name;
            // Show back button when viewing contact profile
            if (profileBackButton) {
                profileBackButton.style.display = 'flex';
            }
        }
    }
    
    // Add action buttons when viewing other users
    if (!isViewingOwnProfile && profileActions) {
        // Remove existing action buttons container if it exists
        let actionButtonsContainer = document.getElementById('profile-view-actions');
        if (actionButtonsContainer) {
            actionButtonsContainer.remove();
        }
        
        // Create action buttons container
        actionButtonsContainer = document.createElement('div');
        actionButtonsContainer.id = 'profile-view-actions';
        actionButtonsContainer.style.display = 'flex';
        actionButtonsContainer.style.gap = '10px';
        actionButtonsContainer.style.flexWrap = 'wrap';
        
        const contactPubkey = this.viewingProfilePubkey;
        const contacts = appState.getContacts() || [];
        const contact = contacts.find(c => c.pubkey === contactPubkey);
        const email = contact?.email || profile?.fields?.email;
        
        // Send DM button
        const sendDmBtn = document.createElement('button');
        sendDmBtn.className = 'btn btn-primary';
        sendDmBtn.innerHTML = '<i class="fas fa-comments"></i> Send DM';
        sendDmBtn.onclick = () => {
            if (window.contactsService && typeof window.contactsService.sendDirectMessageToContact === 'function') {
                window.contactsService.sendDirectMessageToContact(contactPubkey);
            }
        };
        actionButtonsContainer.appendChild(sendDmBtn);
        
        // Send Email button (if email exists)
        if (email) {
            const sendEmailBtn = document.createElement('button');
            sendEmailBtn.className = 'btn btn-secondary';
            sendEmailBtn.innerHTML = '<i class="fas fa-envelope"></i> Send Email';
            sendEmailBtn.onclick = () => {
                if (window.contactsService && typeof window.contactsService.sendEmailToContact === 'function') {
                    window.contactsService.sendEmailToContact(email);
                }
            };
            actionButtonsContainer.appendChild(sendEmailBtn);
        }
        
        // Copy Public Key button
        const copyPubkeyBtn = document.createElement('button');
        copyPubkeyBtn.className = 'btn btn-secondary';
        copyPubkeyBtn.innerHTML = '<i class="fas fa-copy"></i> Copy Public Key';
        copyPubkeyBtn.onclick = () => {
            if (window.contactsService && typeof window.contactsService.copyContactPubkey === 'function') {
                window.contactsService.copyContactPubkey(contactPubkey);
            }
        };
        actionButtonsContainer.appendChild(copyPubkeyBtn);
        
        // Insert after form actions
        const formActions = document.querySelector('#profile .form-actions');
        if (formActions) {
            formActions.appendChild(actionButtonsContainer);
        } else {
            // If form-actions doesn't exist, append to profile-form
            const profileForm = document.querySelector('#profile .profile-form');
            if (profileForm) {
                profileForm.appendChild(actionButtonsContainer);
            }
        }
    } else if (isViewingOwnProfile) {
        // Remove action buttons container when viewing own profile
        const actionButtonsContainer = document.getElementById('profile-view-actions');
        if (actionButtonsContainer) {
            actionButtonsContainer.remove();
        }
    }
}

// Store the current editable fields in memory
NostrMailApp.prototype.editableProfileFields = {};

NostrMailApp.prototype.renderProfileFromObject = function(profile, cachedPictureDataUrl, isViewingOwnProfile = true) {
    // Build editable fields from profile.fields, always include email
    // Ensure all fields from the profile are included, even if not in standard order
    this.editableProfileFields = { ...(profile && profile.fields ? profile.fields : {}) };
    
    // When viewing own profile, always include all standard fields so they can be filled in
    if (isViewingOwnProfile) {
        PROFILE_FIELD_ORDER.forEach(key => {
            if (!(key in this.editableProfileFields)) {
                this.editableProfileFields[key] = '';
            }
        });
    }
    
    this.renderProfileFieldsList(this.editableProfileFields, isViewingOwnProfile);
    // Show warning if profile email and settings email differ (only for own profile)
    if (isViewingOwnProfile) {
        this.renderProfileEmailWarning();
    }

    // Show profile picture if present, otherwise show a placeholder
    const profilePicture = document.getElementById('profile-picture');
    if (profilePicture) {
        // Helper: placeholder SVG
        const placeholderSVG = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><circle cx="60" cy="60" r="60" fill="%23e0e0e0"/><circle cx="60" cy="50" r="28" fill="%23bdbdbd"/><ellipse cx="60" cy="100" rx="38" ry="20" fill="%23bdbdbd"/></svg>';

        // Set image source with fallback logic and log what is used
        let src = '';
        if (cachedPictureDataUrl && typeof cachedPictureDataUrl === 'string' && cachedPictureDataUrl.startsWith('data:image')) {
            src = cachedPictureDataUrl;
            console.log('[Profile] Using cached profile picture data URL');
        } else if (this.editableProfileFields.picture && typeof this.editableProfileFields.picture === 'string' && this.editableProfileFields.picture.trim() !== '') {
            src = this.editableProfileFields.picture.trim();
            console.log('[Profile] Using profile.fields.picture URL:', src);
        } else {
            src = placeholderSVG;
            console.log('[Profile] Using placeholder profile picture');
        }
        profilePicture.src = src;
        profilePicture.style.display = '';

        // Always set an error handler to fallback to placeholder and log error
        profilePicture.onerror = function() {
            console.warn('[Profile] Failed to load profile picture, falling back to placeholder. Tried src:', src);
            profilePicture.src = placeholderSVG;
            profilePicture.style.display = '';
        };
    }

    // Live preview: update profile picture as user types/pastes a new URL
    const pictureInput = document.getElementById('profile-field-picture');
    if (pictureInput && profilePicture) {
        pictureInput.addEventListener('input', function() {
            const url = pictureInput.value.trim();
            if (url) {
                profilePicture.src = url;
                profilePicture.style.display = '';
            } else {
                profilePicture.src = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><circle cx="60" cy="60" r="60" fill="%23e0e0e0"/><circle cx="60" cy="50" r="28" fill="%23bdbdbd"/><ellipse cx="60" cy="100" rx="38" ry="20" fill="%23bdbdbd"/></svg>';
                profilePicture.style.display = '';
            }
        });
    }
}

// Top-level constant for profile field order
const PROFILE_FIELD_ORDER = [
    'name',
    'display_name',
    'email',
    'about',
    'picture',
    'banner',
    'website',
    'location',
    'lud16',
    'lud06',
    'nip05',
];

NostrMailApp.prototype.renderProfileFieldsList = function(fields, isViewingOwnProfile = true) {
    const listDiv = document.getElementById('profile-fields-list');
    if (!listDiv) return;
    
    listDiv.innerHTML = '';
    
    if (!fields || Object.keys(fields).length === 0) {
        listDiv.innerHTML = '<div class="text-muted">No fields found.</div>';
        return;
    }

    // Use the top-level constant for field order
    // When viewing own profile, always show all standard fields (even if empty)
    // When viewing others, only show fields that have values
    for (const key of PROFILE_FIELD_ORDER) {
        if (isViewingOwnProfile || fields.hasOwnProperty(key)) {
            // For own profile, always render (will show empty inputs)
            // For others, only render if field exists
            this._renderProfileFieldItem(listDiv, key, fields[key] || '', isViewingOwnProfile);
        }
    }

    // Render custom fields (not in PROFILE_FIELD_ORDER), sorted alphabetically
    const customKeys = Object.keys(fields)
        .filter(key => !PROFILE_FIELD_ORDER.includes(key))
        .sort();
    for (const key of customKeys) {
        this._renderProfileFieldItem(listDiv, key, fields[key], isViewingOwnProfile);
    }

    // Add real-time warning update for email field (only for own profile)
    if (isViewingOwnProfile) {
        const emailInput = document.getElementById('profile-field-email');
        if (emailInput) {
            emailInput.addEventListener('input', () => {
                this.editableProfileFields.email = emailInput.value;
                this.renderProfileEmailWarning();
            });
        }
    }
}

// Helper to render a single field item
NostrMailApp.prototype._renderProfileFieldItem = function(listDiv, key, value, isViewingOwnProfile = true) {
    const fieldDiv = document.createElement('div');
    fieldDiv.className = 'form-group profile-field-item';
    const label = document.createElement('label');
    label.textContent = key.charAt(0).toUpperCase() + key.slice(1).replace(/_/g, ' ') + ':';
    label.setAttribute('for', `profile-field-${key}`);
    
    if (isViewingOwnProfile) {
        // Editable mode - create input/textarea
        let input;
        if (key === 'about') {
            input = document.createElement('textarea');
            input.rows = 3;
        } else if (key === 'picture' || key === 'banner' || key === 'website') {
            input = document.createElement('input');
            input.type = 'url';
            if (key === 'picture') {
                input.placeholder = 'https://example.com/avatar.png';
            } else if (key === 'banner') {
                input.placeholder = 'https://example.com/banner.jpg';
            } else if (key === 'website') {
                input.placeholder = 'https://example.com';
            }
        } else if (typeof value === 'string' && value.length > 60) {
            input = document.createElement('textarea');
            input.rows = 3;
        } else {
            input = document.createElement('input');
            input.type = key === 'email' ? 'email' : 'text';
        }
        input.id = `profile-field-${key}`;
        input.value = value ?? '';
        input.dataset.key = key;
        input.className = 'profile-field-input';
        input.addEventListener('input', (e) => {
            this.editableProfileFields[key] = e.target.value;
        });
        fieldDiv.appendChild(label);
        fieldDiv.appendChild(input);
    } else {
        // Read-only mode - create display div
        const valueDiv = document.createElement('div');
        valueDiv.className = 'profile-field-value';
        valueDiv.id = `profile-field-${key}`;
        if (key === 'email' && value) {
            const emailLink = document.createElement('a');
            emailLink.href = `mailto:${value}`;
            emailLink.textContent = value;
            valueDiv.appendChild(emailLink);
        } else if ((key === 'picture' || key === 'banner' || key === 'website') && value) {
            // For URLs, show as clickable link
            const urlLink = document.createElement('a');
            urlLink.href = value;
            urlLink.target = '_blank';
            urlLink.rel = 'noopener noreferrer';
            urlLink.textContent = value;
            valueDiv.appendChild(urlLink);
        } else if (value) {
            valueDiv.textContent = value;
        } else {
            valueDiv.textContent = '-';
            valueDiv.style.color = '#999';
        }
        fieldDiv.appendChild(label);
        fieldDiv.appendChild(valueDiv);
    }
    listDiv.appendChild(fieldDiv);
}

// Add new profile field
NostrMailApp.prototype.addProfileField = function() {
    const fieldName = prompt('Enter field name:');
    if (fieldName && fieldName.trim()) {
        const key = fieldName.trim().toLowerCase().replace(/\s+/g, '_');
        this.editableProfileFields[key] = '';
        this.renderProfileFieldsList(this.editableProfileFields);
    }
}

// Update profile
NostrMailApp.prototype.updateProfile = async function() {
    if (!appState.hasKeypair()) {
        notificationService.showError('No keypair available');
        return;
    }

    // Check if we have any relays configured (persistent client will handle connection logic)
    const allRelays = appState.getRelays();
    if (allRelays.length === 0) {
        notificationService.showError('No relays configured to publish profile');
        return;
    }

    try {
        // Show loading state
        const updateBtn = document.getElementById('update-profile-btn');
        if (updateBtn) {
            updateBtn.disabled = true;
            updateBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Updating...';
        }

        // Clean up empty fields
        const cleanedFields = {};
        for (const [key, value] of Object.entries(this.editableProfileFields)) {
            if (value && value.trim() !== '') {
                cleanedFields[key] = value.trim();
            }
        }

        // Update profile on Nostr using persistent client (more efficient)
        await TauriService.updateProfilePersistent(
            appState.getKeypair().private_key,
            cleanedFields
        );

        // Cache the updated profile
        const updatedProfile = {
            pubkey: appState.getKeypair().public_key,
            fields: cleanedFields
        };
        const pubkey = updatedProfile.pubkey;
        let profileDict = {};
        const cached = localStorage.getItem('nostr_mail_profiles');
        if (cached) {
            profileDict = JSON.parse(cached);
        }
        profileDict[pubkey] = updatedProfile;
        localStorage.setItem('nostr_mail_profiles', JSON.stringify(profileDict));

        // Update the editable fields
        this.editableProfileFields = { ...cleanedFields };
        this.renderProfileFieldsList(this.editableProfileFields);

        notificationService.showSuccess('Profile updated successfully');

    } catch (error) {
        console.error('Failed to update profile:', error);
        notificationService.showError('Failed to update profile: ' + error);
    } finally {
        // Re-enable the button
        const updateBtn = document.getElementById('update-profile-btn');
        if (updateBtn) {
            updateBtn.disabled = false;
            updateBtn.innerHTML = '<i class="fas fa-save"></i> Update Profile';
        }
    }
}

NostrMailApp.prototype.updatePublicKeyDisplay = async function() {
    // #region agent log
    const updateDisplayStartTime = Date.now();
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4133',message:'updatePublicKeyDisplay entry',data:{hypothesisId:'C'},timestamp:updateDisplayStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
    // #endregion
    const nprivKey = domManager.getValue('nprivKey')?.trim() || '';
    
    if (!nprivKey) {
        domManager.setValue('publicKeyDisplay', '');
        return;
    }
    
    try {
        // #region agent log
        const validateStartTime = Date.now();
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4142',message:'Before validatePrivateKey',data:{hypothesisId:'C'},timestamp:validateStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        const isValid = await TauriService.validatePrivateKey(nprivKey);
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4142',message:'After validatePrivateKey',data:{isValid:isValid,duration:Date.now()-validateStartTime,hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        
        if (!isValid) {
            domManager.setValue('publicKeyDisplay', 'Invalid private key');
            return;
        }
        
        // #region agent log
        const getPubkeyStartTime = Date.now();
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4149',message:'Before getPublicKeyFromPrivate',data:{hypothesisId:'C'},timestamp:getPubkeyStartTime,sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        const publicKey = await TauriService.getPublicKeyFromPrivate(nprivKey);
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4149',message:'After getPublicKeyFromPrivate',data:{duration:Date.now()-getPubkeyStartTime,hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        domManager.setValue('publicKeyDisplay', publicKey);
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4133',message:'updatePublicKeyDisplay completed',data:{totalDuration:Date.now()-updateDisplayStartTime,hypothesisId:'C'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
        // #endregion
        
    } catch (error) {
        console.error('Failed to get public key:', error);
        domManager.setValue('publicKeyDisplay', 'Error getting public key');
    }
}

// Dark mode management
NostrMailApp.prototype.setDarkMode = function(enabled) {
    document.body.classList.toggle('dark-mode', enabled);
    const icon = document.getElementById('dark-mode-icon');
    if (icon) {
        icon.className = enabled ? 'fas fa-sun' : 'fas fa-moon';
    }
    const text = document.getElementById('dark-mode-text');
    if (text) {
        text.textContent = enabled ? 'Disable Dark Mode' : 'Enable Dark Mode';
    }
    const label = document.getElementById('theme-label');
    if (label) {
        label.textContent = enabled ? 'Dark Mode (Enabled)' : 'Light Mode (Enabled)';
    }
    localStorage.setItem('darkMode', enabled ? '1' : '0');
}

NostrMailApp.prototype.toggleDarkMode = function() {
    const enabled = !document.body.classList.contains('dark-mode');
    this.setDarkMode(enabled);
}

// Add a method to render the email warning
NostrMailApp.prototype.renderProfileEmailWarning = function() {
    const settings = appState.getSettings();
    const profileEmail = this.editableProfileFields.email || '';
    const settingsEmail = settings && settings.email_address ? settings.email_address : '';
    let warningDiv = document.getElementById('profile-email-warning');
    if (!warningDiv) {
        warningDiv = document.createElement('div');
        warningDiv.id = 'profile-email-warning';
        warningDiv.style.color = 'orange';
        warningDiv.style.marginBottom = '8px';
        const form = document.getElementById('profile-fields-form');
        if (form) form.insertBefore(warningDiv, form.firstChild);
    }
    if (profileEmail && settingsEmail && profileEmail !== settingsEmail) {
        warningDiv.innerHTML = `
            <i class=\"fas fa-exclamation-triangle\"></i>
            The email in your profile does not match your settings email.<br>
            <span id=\"sync-profile-email-link\" style=\"color:#ffb300;cursor:pointer;text-decoration:underline;display:inline-block;margin-top:4px;\">
                Click here to copy email from settings
            </span>
        `;
        document.getElementById('sync-profile-email-link').onclick = () => {
            this.editableProfileFields.email = settingsEmail;
            this.renderProfileFieldsList(this.editableProfileFields);
            this.renderProfileEmailWarning();
        };
    } else {
        warningDiv.innerHTML = '';
    }
};

// Create and export the main application instance
window.app = new NostrMailApp();

// Debug function to check Nostr client status
NostrMailApp.prototype.checkNostrClientStatus = async function() {
    try {
        const isConnected = await TauriService.getNostrClientStatus();
        console.log('[APP] Nostr client status:', isConnected ? '✅ Connected' : '❌ Disconnected');
        return isConnected;
    } catch (error) {
        console.error('[APP] Failed to check Nostr client status:', error);
        return false;
    }
}

// Debug function to manually trigger relay sync
NostrMailApp.prototype.manualSyncRelays = async function() {
    console.log('[APP] Manual relay sync triggered...');
    try {
        await this.syncDisconnectedRelays();
        console.log('[APP] Manual relay sync completed');
    } catch (error) {
        console.error('[APP] Manual relay sync failed:', error);
    }
}

// Start periodic relay status updates
NostrMailApp.prototype.startRelayStatusUpdates = function() {
    // Update every 10 seconds as requested
    this.relayStatusInterval = setInterval(async () => {
        try {
            // Check if we're on the settings tab (more reliable check)
            const settingsTab = document.querySelector('.nav-item[data-tab="settings"]');
            const isOnSettingsPage = settingsTab && settingsTab.classList.contains('active');
            const hasRelays = appState.getRelays().length > 0;
            
            // Only log periodically (every 5th check = every 50 seconds) to reduce noise
            if (this.relayStatusUpdateCount === undefined) {
                this.relayStatusUpdateCount = 0;
            }
            this.relayStatusUpdateCount++;
            const shouldLog = this.relayStatusUpdateCount % 5 === 0;
            
            if (shouldLog) {
                console.log(`[APP] Periodic update check: settingsPage=${isOnSettingsPage}, hasRelays=${hasRelays}`);
            }
            
            if (isOnSettingsPage && hasRelays) {
                if (shouldLog) {
                    console.log('[APP] Running periodic relay status update...');
                }
                await this.updateRelayStatusOnly();
            }
        } catch (error) {
            console.error('Error updating relay status:', error);
        }
    }, 10000); // Changed to 10 seconds as requested
}


// Update relay summary display
NostrMailApp.prototype.updateRelaySummary = function(relayStatuses = []) {
    const relaySummary = domManager.get('relaySummary');
    if (!relaySummary) return;
    
    const relays = appState.getRelays();
    const totalRelays = relays.length;
    const activeRelays = relays.filter(r => r.is_active).length;
    
    // Count connected relays from status
    const connectedCount = relayStatuses.filter(status => 
        status.status === 'Connected' && 
        relays.find(r => r.url === status.url && r.is_active)
    ).length;
    
    // Create summary text with visual indicators
    let summaryHtml = '';
    
    if (totalRelays === 0) {
        summaryHtml = '<span style="color: #6c757d;">No relays configured</span>';
    } else if (activeRelays === 0) {
        summaryHtml = `<span style="color: #6c757d;">${totalRelays} relay${totalRelays > 1 ? 's' : ''} (all disabled)</span>`;
    } else {
        // Show connection status with colored dots
        const statusDots = [];
        const disconnectedCount = activeRelays - connectedCount;
        const disabledCount = totalRelays - activeRelays;
        
        // Only show breakdown if there are mixed states
        if (disconnectedCount > 0 || disabledCount > 0) {
            if (connectedCount > 0) {
                statusDots.push(`<span class="relay-status-dot connected"></span>${connectedCount} connected`);
            }
            if (disconnectedCount > 0) {
                statusDots.push(`<span class="relay-status-dot disconnected"></span>${disconnectedCount} failed`);
            }
            if (disabledCount > 0) {
                statusDots.push(`<span class="relay-status-dot disabled"></span>${disabledCount} disabled`);
            }
            summaryHtml = `${connectedCount}/${totalRelays} connected • ${statusDots.join(' • ')}`;
        } else {
            // All relays are connected - just show the simple count
            summaryHtml = `${connectedCount}/${totalRelays} connected`;
        }
    }
    
    relaySummary.innerHTML = summaryHtml;
}

// Retry a failed relay connection
NostrMailApp.prototype.retryRelayConnection = async function(relayId, relayUrl) {
    console.log(`[APP] Retrying connection to relay: ${relayUrl}`);
    
    try {
        // Show connecting status
        this.updateSingleRelayStatus(relayUrl, 'Connecting');
        notificationService.showInfo(`🔄 Retrying connection to ${relayUrl}...`);
        
        // Attempt to reconnect by toggling the relay off and on
        await TauriService.updateSingleRelay(relayUrl, false);
        await new Promise(resolve => setTimeout(resolve, 500)); // Brief pause
        await TauriService.updateSingleRelay(relayUrl, true);
        
        // Wait a bit longer for connection to establish
        setTimeout(async () => {
            try {
                const relayStatuses = await TauriService.getRelayStatus();
                const status = relayStatuses.find(s => s.url === relayUrl);
                
                if (status) {
                    this.updateSingleRelayStatus(relayUrl, status.status);
                    
                    if (status.status === 'Connected') {
                        notificationService.showSuccess(`✅ Successfully reconnected to ${relayUrl}`);
                        // Re-render to remove retry button
                        await this.renderRelays();
                    } else {
                        notificationService.showError(`❌ Retry failed for ${relayUrl}. Check relay URL and network connection.`);
                    }
                } else {
                    this.updateSingleRelayStatus(relayUrl, 'Disconnected');
                    notificationService.showError(`❌ Retry failed for ${relayUrl}`);
                }
            } catch (error) {
                console.error('Failed to verify retry status:', error);
                this.updateSingleRelayStatus(relayUrl, 'Disconnected');
                notificationService.showError(`❌ Retry failed for ${relayUrl}: ${error}`);
            }
        }, 3000); // Give more time for connection to establish
        
    } catch (error) {
        console.error('Failed to retry relay connection:', error);
        this.updateSingleRelayStatus(relayUrl, 'Disconnected');
        notificationService.showError(`Failed to retry connection: ${error}`);
    }
}

// Update only relay status without full re-render (more efficient)
NostrMailApp.prototype.updateRelayStatusOnly = async function() {
    try {
        const relayStatuses = await TauriService.getRelayStatus();
        // Only log if there are status changes or errors (reduce noise)
        
        // Always update the summary
        
        // Update each relay's status in the UI (only if expanded)
        const relaysList = domManager.get('relaysList');
        const isExpanded = relaysList && relaysList.classList.contains('expanded');
        
        if (isExpanded) {
            relayStatuses.forEach(status => {
                this.updateSingleRelayStatus(status.url, status.status);
            });
            
            // Check if we need to re-render to show/hide retry buttons
            const hasDisconnectedActive = relayStatuses.some(status => 
                status.status === 'Disconnected' && 
                appState.getRelays().find(r => r.url === status.url && r.is_active)
            );
            
            // Re-render if there are new disconnected active relays that need retry buttons
            if (hasDisconnectedActive) {
                const currentRetryButtons = document.querySelectorAll('.retry-btn').length;
                const expectedRetryButtons = relayStatuses.filter(status => 
                    status.status === 'Disconnected' && 
                    appState.getRelays().find(r => r.url === status.url && r.is_active)
                ).length;
                
                if (currentRetryButtons !== expectedRetryButtons) {
                    await this.renderRelays();
                }
            }
        }
        
        // Also check for any relays that might need auto-disabling
        await this.syncDisconnectedRelays();
        
    } catch (error) {
        console.error('[APP] Failed to update relay status:', error);
    }
}

// Stop periodic updates
NostrMailApp.prototype.stopRelayStatusUpdates = function() {
    if (this.relayStatusInterval) {
        clearInterval(this.relayStatusInterval);
        this.relayStatusInterval = null;
    }
}

// Initialize Settings Accordion
NostrMailApp.prototype.initializeSettingsAccordion = function() {
    // Prevent duplicate initialization
    if (this.settingsAccordionInitialized) {
        return;
    }
    this.settingsAccordionInitialized = true;
    
    // Load saved collapsed state from localStorage
    const savedState = localStorage.getItem('settingsSectionsState');
    const collapsedSections = savedState ? JSON.parse(savedState) : {};
    
    // Set up click handlers for all settings section headers
    const sectionHeaders = document.querySelectorAll('.settings-section-header');
    sectionHeaders.forEach(header => {
        const section = header.closest('.settings-section');
        const sectionId = section.getAttribute('data-section');
        
        // Restore saved state
        if (collapsedSections[sectionId]) {
            section.classList.add('collapsed');
        }
        
        // Add click handler
        header.addEventListener('click', (e) => {
            // Don't toggle if clicking on buttons inside the header (like relay edit button)
            if (e.target.closest('button')) {
                return;
            }
            
            const isCollapsed = section.classList.contains('collapsed');
            
            if (isCollapsed) {
                section.classList.remove('collapsed');
                collapsedSections[sectionId] = false;
            } else {
                section.classList.add('collapsed');
                collapsedSections[sectionId] = true;
            }
            
            // Save state to localStorage
            localStorage.setItem('settingsSectionsState', JSON.stringify(collapsedSections));
        });
    });
};

// Initialize when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    // #region agent log
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4369',message:'DOMContentLoaded event fired',data:{hypothesisId:'B'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
    // #endregion
    window.domManager = new DOMManager();
    console.log('🌐 DOM loaded - Initializing NostrMail interface...');
    
    // Set initial dark mode from localStorage
    const darkPref = localStorage.getItem('darkMode');
    window.app.setDarkMode(darkPref === '1');
    
    console.log('🎨 Dark mode initialized:', darkPref === '1' ? 'enabled' : 'disabled');
    
    // Initialize the application
    // #region agent log
    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4380',message:'Calling app.init()',data:{hypothesisId:'B'},timestamp:Date.now(),sessionId:'debug-session',runId:'startup'})}).catch(()=>{});
    // #endregion
    window.app.init();
    
    // Debug button styles after initialization - immediate check
    setTimeout(() => {
        // #region agent log
        const checkLandscapeButtons = () => {
            const windowWidth = window.innerWidth;
            const windowHeight = window.innerHeight;
            const screenWidth = window.screen.width;
            const screenHeight = window.screen.height;
            const orientation = window.innerWidth > window.innerHeight ? 'landscape' : 'portrait';
            const landscapeMediaQuery = window.matchMedia('(max-width: 1024px) and (orientation: landscape)');
            const portraitMediaQuery = window.matchMedia('(max-width: 1024px) and (orientation: portrait)');
            const anyLandscapeQuery = window.matchMedia('(orientation: landscape)');
            const any1024Query = window.matchMedia('(max-width: 1024px)');
            
            const debugInfo = {
                windowWidth, 
                windowHeight, 
                screenWidth,
                screenHeight,
                orientation, 
                landscapeMatches: landscapeMediaQuery.matches,
                portraitMatches: portraitMediaQuery.matches,
                anyLandscapeMatches: anyLandscapeQuery.matches,
                any1024Matches: any1024Query.matches
            };
            
            console.log('🔍 EMULATOR Landscape Debug:', debugInfo);
            
            const logData = {...debugInfo, hypothesisId:'A'};
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4648',message:'EMULATOR Landscape check - window dimensions and media query',data:logData,timestamp:Date.now(),sessionId:'debug-session',runId:'emulator-check'})}).catch(()=>{});
            
            const tabHeaders = document.querySelectorAll('.tab-header');
            console.log('🔍 Found tab headers:', tabHeaders.length);
            
            tabHeaders.forEach((header, idx) => {
                const buttons = header.querySelectorAll('.btn');
                const contactsActions = header.querySelector('.contacts-actions');
                
                console.log(`🔍 Header ${idx}:`, {buttonCount: buttons.length, hasContactsActions: !!contactsActions});
                
                if (contactsActions) {
                    const computedStyle = window.getComputedStyle(contactsActions);
                    const buttonStyles = Array.from(buttons).slice(0, 4).map((btn, btnIdx) => {
                        const btnStyle = window.getComputedStyle(btn);
                        const styles = {
                            width: btnStyle.width,
                            height: btnStyle.height,
                            fontSize: btnStyle.fontSize,
                            display: btnStyle.display,
                            flexWrap: btnStyle.flexWrap,
                            gap: btnStyle.gap,
                            padding: btnStyle.padding,
                            textContent: btn.textContent.trim().substring(0, 30),
                            hasBtnText: !!btn.querySelector('.btn-text'),
                            btnTextDisplay: btn.querySelector('.btn-text') ? window.getComputedStyle(btn.querySelector('.btn-text')).display : 'N/A'
                        };
                        console.log(`🔍 Button ${btnIdx}:`, styles);
                        return styles;
                    });
                    
                    const containerStyles = {
                        gap: computedStyle.gap,
                        flexWrap: computedStyle.flexWrap,
                        display: computedStyle.display,
                        width: computedStyle.width,
                        maxWidth: computedStyle.maxWidth,
                        flexShrink: computedStyle.flexShrink
                    };
                    
                    console.log(`🔍 Contacts Actions Container Styles:`, containerStyles);
                    
                    const logData2 = {headerIndex:idx,buttonCount:buttons.length,containerStyles,buttonStyles,hypothesisId:'B'};
                    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'app.js:4680',message:'EMULATOR Landscape check - contacts-actions styles',data:logData2,timestamp:Date.now(),sessionId:'debug-session',runId:'emulator-check'})}).catch(()=>{});
                }
            });
        };
        checkLandscapeButtons();
        window.app.debugLandscapeButtons();
    }, 1000);
    
    // Start relay status updates
    window.app.startRelayStatusUpdates();
}); 