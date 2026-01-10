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
}

// Initialize the application
NostrMailApp.prototype.init = async function() {
    console.log('🚀 ========================================');
    console.log('🚀   Nostr Mail - Starting Application');
    console.log('🚀 ========================================');
    console.log('📧 Email + 🔐 Nostr Integration');
    console.log('🌐 Version: 1.0.0');
    console.log('⏰ Started at:', new Date().toLocaleString());
    console.log('🚀 ========================================');
    // Initialize the database
    try {
        await TauriService.initDatabase();
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
        await this.loadRelaysFromDatabase();

        console.log('🔑 Loading/generating keypair...');
        await this.loadKeypair();
        
        console.log('🔄 Initializing live event subscription...');
        await this.initializeLiveEvents();
        
        console.log('🎯 Setting up event listeners...');
        this.setupEventListeners();
        
        console.log('📬 Loading initial data...');
        // Load contacts first so DM contacts can access cached profile photos
        await contactsService.loadContacts();
        // NOTE: We do NOT load emails here. Emails are only loaded when the inbox tab is clicked.
        // await emailService.loadEmails(); // <-- Remove or comment out this line so emails are not loaded on startup
        // await dmService.loadDmContacts(); // TODO: add this back in once we have stored DMs in the DB
        
        // Populate Nostr contact dropdown for compose page
        if (window.emailService) {
            window.emailService.populateNostrContactDropdown();

            // Initialize attachment functionality
            window.emailService.initializeAttachmentListeners();

            // Try to restore saved contact selection
            window.emailService.restoreContactSelection();
        }
        
        console.log('✅ ========================================');
        console.log('✅   Nostr Mail - Successfully Started!');
        console.log('✅ ========================================');
        console.log('🎉 Application is ready for use');
        console.log('📱 UI: Modern email client with Nostr integration');
        console.log('🔐 Features: Email, DMs, Contacts, Profile Management');
        console.log('✅ ========================================');
        
        this.initialized = true;
    } catch (error) {
        console.error('❌ ========================================');
        console.error('❌   Nostr Mail - Startup Failed!');
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
                const dbSettings = await TauriService.dbGetAllSettings(keypair.public_key);
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
                        sync_cutoff_days: parseInt(dbSettings.sync_cutoff_days) || 365 // Default to 1 year
                    };
                    appState.setSettings(settings);
                    this.populateSettingsForm();
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
        }
    } catch (error) {
        console.error('Error loading settings:', error);
    }
}

NostrMailApp.prototype.loadSettingsForPubkey = async function(pubkey) {
    try {
        if (!pubkey) {
            console.log('[APP] No pubkey provided, skipping settings load');
            return;
        }
        
        console.log('[APP] Loading settings for pubkey:', pubkey);
        const dbSettings = await TauriService.dbGetAllSettings(pubkey);
        
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
                sync_cutoff_days: parseInt(dbSettings.sync_cutoff_days) || 1825 // Default to 5 years
            };
            
            appState.setSettings(settings);
            this.populateSettingsForm();
            // Update localStorage as backup
            localStorage.setItem('nostr_mail_settings', JSON.stringify(settings));
            console.log('[APP] Settings loaded for pubkey:', pubkey);
        } else {
            console.log('[APP] No settings found in database for pubkey:', pubkey);
            // Try to load from localStorage as fallback
            const stored = localStorage.getItem('nostr_mail_settings');
            if (stored) {
                const settings = JSON.parse(stored);
                appState.setSettings(settings);
                this.populateSettingsForm();
            }
        }
    } catch (error) {
        console.error('[APP] Error loading settings for pubkey:', error);
        // Fallback to localStorage
        const stored = localStorage.getItem('nostr_mail_settings');
        if (stored) {
            try {
                const settings = JSON.parse(stored);
                appState.setSettings(settings);
                this.populateSettingsForm();
            } catch (e) {
                console.error('[APP] Failed to load from localStorage:', e);
            }
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
        this.updateRelaySummary([]);
    }
}

// Load keypair
NostrMailApp.prototype.loadKeypair = async function() {
    try {
        const stored = localStorage.getItem('nostr_keypair');
        let keypair;
        if (stored) {
            keypair = JSON.parse(stored);
            appState.setKeypair(keypair);
        } else {
            keypair = await TauriService.generateKeypair();
            appState.setKeypair(keypair);
            localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
        }
        console.log('Keypair loaded:', appState.getKeypair().public_key.substring(0, 20) + '...');
        this.renderProfilePubkey();
        
        // Load settings for this pubkey
        if (keypair && keypair.public_key) {
            await this.loadSettingsForPubkey(keypair.public_key);
        }
        
        // Initialize persistent Nostr client with the loaded keypair
        await this.initializeNostrClient();
    } catch (error) {
        console.error('Failed to load keypair:', error);
        notificationService.showError('Failed to load encryption keys');
    }
}

// Initialize the persistent Nostr client
NostrMailApp.prototype.initializeNostrClient = async function() {
    try {
        const keypair = appState.getKeypair();
        if (!keypair || !keypair.private_key) {
            console.warn('[APP] No keypair available for Nostr client initialization');
            return;
        }
        
        console.log('[APP] Initializing persistent Nostr client...');
        await TauriService.initPersistentNostrClient(keypair.private_key);
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
                }
            });
        } else {
            console.error('[JS] Encrypt button not found in DOM');
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
        
        // Relay edit toggle button
        const relayEditToggle = domManager.get('relayEditToggle');
        if (relayEditToggle) {
            relayEditToggle.addEventListener('click', () => this.toggleRelayEdit());
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
        if (nprivKeyInput) {
            nprivKeyInput.addEventListener('input', () => {
                this.updatePublicKeyDisplay();
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
                    
                    // Restart live events with new keypair
                    console.log('[LiveEvents] New keypair generated, restarting live events');
                    await app.cleanupLiveEvents();
                    await app.initializeLiveEvents();
                    
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
            
            // Show notification
            notificationService.showInfo('Profile updated from another device');
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

// Tab switching
NostrMailApp.prototype.switchTab = function(tabName) {
    try {
        // Prevent switching if a sync/load operation is in progress
        const refreshSent = domManager.get('refreshSent');
        if (refreshSent && refreshSent.disabled) {
            console.log('[JS] Tab switch blocked: sent emails are currently loading');
            return; // Don't switch tabs while loading
        }
        
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
    } catch (error) {
        console.error('[JS] Error switching tabs:', error);
    }

    if (tabName === 'profile') {
        this.loadProfile();
    }
    if (tabName === 'settings') {
        this.loadRelaysFromDatabase();
    }
    if (tabName === 'contacts') {
        // Only load contacts if they haven't been loaded yet
        if (!appState.getContacts() || appState.getContacts().length === 0) {
            contactsService.loadContacts();
        } else {
            // Just render the existing contacts
            contactsService.renderContacts();
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
        if (window.emailService) {
            // Clear current draft state when switching to compose (unless we're loading a draft)
            if (!window.emailService.currentDraftId) {
                window.emailService.clearCurrentDraft();
            }
            // Try to restore contact selection when switching to compose
            window.emailService.restoreContactSelection();
        }
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
        if (!currentKeypair || !currentKeypair.public_key) {
            this.showSettingsStatus('warning', 'Please enter a private key to save settings');
            // Toast notification is shown in showSettingsStatus for better visibility
            return false;
        }
        
        // Validate npriv key if provided
        const nprivKey = domManager.getValue('nprivKey')?.trim() || '';
        if (nprivKey && !nprivKey.startsWith('npriv1') && !nprivKey.startsWith('nsec1')) {
            this.showSettingsStatus('error', 'Invalid Nostr private key format. Should start with "npriv1" or "nsec1"');
            // Toast notification is shown in showSettingsStatus for better visibility
            return false;
        }
        
        const settings = {
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
            sync_cutoff_days: parseInt(domManager.getValue('syncCutoffDays')) || 365 // Default to 1 year
        };
        
        // Keep localStorage as backup
        localStorage.setItem('nostr_mail_settings', JSON.stringify(settings));
        appState.setSettings(settings);
        appState.setNprivKey(settings.npriv_key);
        
        // If a private key is provided in the form, update appState.keypair
        let publicKey = currentKeypair.public_key;
        if (nprivKey && (nprivKey.startsWith('npriv1') || nprivKey.startsWith('nsec1'))) {
            const isValid = await TauriService.validatePrivateKey(nprivKey);
            if (!isValid) {
                this.showSettingsStatus('error', 'Invalid private key');
                // Toast notification is shown in showSettingsStatus for better visibility
                return false;
            }
            publicKey = await TauriService.getPublicKeyFromPrivate(nprivKey);
            const keypair = { private_key: nprivKey, public_key: publicKey };
            
            // Check if this is a different keypair
            const isNewKeypair = currentKeypair.private_key !== nprivKey;
            
            appState.setKeypair(keypair);
            localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
            this.renderProfilePubkey();
            
            // If keypair changed, load settings for new pubkey and restart services
            if (isNewKeypair) {
                console.log('[LiveEvents] Keypair changed, loading settings for new pubkey');
                await this.loadSettingsForPubkey(publicKey);
                
                console.log('[LiveEvents] Keypair changed, restarting live events');
                await this.cleanupLiveEvents();
                await this.initializeLiveEvents();
                
                // Reinitialize the persistent Nostr client with the new keypair
                console.log('[APP] Keypair changed, reinitializing persistent Nostr client');
                await this.initializeNostrClient();
            }
            
            // If on profile tab, reload profile
            if (document.querySelector('.tab-content#profile.active')) {
                this.loadProfile();
            }
        }
        
        // Save settings to database with pubkey association (REQUIRED)
        try {
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
            settingsMap.set('sync_cutoff_days', settings.sync_cutoff_days.toString());
            
            const settingsObj = Object.fromEntries(settingsMap);
            await TauriService.dbSaveSettingsBatch(publicKey, settingsObj);
            console.log('[APP] Settings auto-saved to database for pubkey:', publicKey);
            
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
        'send-matching-dm-preference'
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
    
    // Also listen for email provider changes (which may auto-fill other fields)
    const emailProvider = domManager.get('emailProvider');
    if (emailProvider) {
        emailProvider.addEventListener('change', () => {
            // Wait a bit for auto-fill to complete, then save
            setTimeout(debouncedSave, 500);
        });
    }
    
    // Store flag for populateSettingsForm to use
    this._isPopulatingForm = () => isPopulatingForm;
    this._setPopulatingForm = (value) => { isPopulatingForm = value; };
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
        
        domManager.setValue('nprivKey', settings.npriv_key || '');
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
        
        // Set send matching DM preference (default to true if not set)
        const sendMatchingDmPref = domManager.get('send-matching-dm-preference');
        if (sendMatchingDmPref) {
            sendMatchingDmPref.checked = settings.send_matching_dm !== false;
        }
        
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
    
    // Update summary first
    this.updateRelaySummary(relayStatuses);
    
    // Only render the full list if in edit mode
    const isEditing = relaysList.classList.contains('expanded');
    if (!isEditing) {
        return; // Don't render the full list when collapsed
    }
    
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
    console.log('[APP] syncDisconnectedRelays called');
    try {
        const updatedRelays = await TauriService.syncRelayStates();
        console.log('[APP] syncRelayStates returned:', updatedRelays);
        if (updatedRelays.length > 0) {
            console.log('Auto-disabled disconnected relays:', updatedRelays);
            
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

NostrMailApp.prototype.loadProfile = async function() {
    const currentPubkey = appState.getKeypair() && appState.getKeypair().public_key;
    const profileSpinner = document.getElementById('profile-loading-spinner');
    const profileFieldsList = document.getElementById('profile-fields-list');
    const profilePicture = document.getElementById('profile-picture');

    // Only clear UI if switching pubkeys
    if (this.lastRenderedProfilePubkey !== null && this.lastRenderedProfilePubkey !== currentPubkey) {
        console.log('[Profile] Switching pubkey from', this.lastRenderedProfilePubkey, 'to', currentPubkey, '- clearing UI and showing spinner');
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
        if (cached && currentPubkey) {
            const profileDict = JSON.parse(cached);
            cachedProfile = profileDict[currentPubkey];
            cachedPictureDataUrl = localStorage.getItem('nostr_mail_profile_picture');
            if (cachedProfile) {
                console.log('[Profile] Rendering cached profile for pubkey', currentPubkey);
                if (profileSpinner) profileSpinner.style.display = 'none';
                this.renderProfileFromObject(cachedProfile, cachedPictureDataUrl);
            } else {
                console.log('[Profile] No cached profile found for pubkey', currentPubkey);
            }
        } else {
            console.log('[Profile] No cached profiles in localStorage or no current pubkey');
        }
    } catch (e) {
        console.warn('[Profile] Error loading cached profile:', e);
    }

    if (!appState.hasKeypair() || !appState.getKeypair().public_key) {
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
        const profile = await TauriService.fetchProfilePersistent(appState.getKeypair().public_key);

        if (profile) {
            // If there's a new picture URL, fetch and cache the image as a data URL
            if (profile.fields && profile.fields.picture) {
                const pictureUrl = profile.fields.picture;
                try {
                    const dataUrl = await TauriService.fetchImage(pictureUrl);
                    if (dataUrl) {
                        localStorage.setItem('nostr_mail_profile_picture', dataUrl);
                        this.renderProfileFromObject(profile, dataUrl);
                    } else {
                        localStorage.removeItem('nostr_mail_profile_picture');
                        this.renderProfileFromObject(profile, null);
                    }
                } catch (e) {
                    this.renderProfileFromObject(profile, null);
                }
            } else {
                localStorage.removeItem('nostr_mail_profile_picture');
                this.renderProfileFromObject(profile, null);
            }
            // Cache the profile in localStorage
            const pubkey = appState.getKeypair() && appState.getKeypair().public_key;
            if (pubkey) {
                let profileDict = {};
                const cached = localStorage.getItem('nostr_mail_profiles');
                if (cached) {
                    profileDict = JSON.parse(cached);
                }
                profileDict[pubkey] = profile;
                localStorage.setItem('nostr_mail_profiles', JSON.stringify(profileDict));
            }
            if (profileSpinner) profileSpinner.style.display = 'none';
        } else {
            if (profileSpinner) profileSpinner.style.display = 'none';
            // Show placeholder fields and picture so the user can create a new profile
            const emptyFields = {};
            PROFILE_FIELD_ORDER.forEach(key => { emptyFields[key] = ''; });
            const emptyProfile = {
                pubkey: appState.getKeypair().public_key,
                fields: emptyFields
            };
            this.renderProfileFromObject(emptyProfile, null);
        }
        this.renderProfilePubkey();
        if (Utils.isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                if (profile && profile.raw_content) {
                    rawJsonBox.value = profile.raw_content;
                } else if (profile && profile.fields) {
                    rawJsonBox.value = JSON.stringify(profile.fields, null, 2);
                } else {
                    rawJsonBox.value = JSON.stringify(profile, null, 2);
                }
            }
        }
    } catch (error) {
        if (profileSpinner) profileSpinner.style.display = 'none';
        console.error('Failed to fetch profile:', error);
        notificationService.showError('Could not fetch profile from relays.');
        this.renderProfilePubkey();
        if (Utils.isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'Error: ' + error;
            }
        }
    }
    // After rendering a profile (cached or fetched), update the last rendered pubkey
    this.lastRenderedProfilePubkey = currentPubkey;
}

NostrMailApp.prototype.renderProfilePubkey = function() {
    const pubkeyDiv = document.getElementById('profile-pubkey');
    if (pubkeyDiv && appState.hasKeypair() && appState.getKeypair().public_key) {
        pubkeyDiv.textContent = `Your npub: ${appState.getKeypair().public_key}`;
    } else if (pubkeyDiv) {
        pubkeyDiv.textContent = '';
    }
}

// Store the current editable fields in memory
NostrMailApp.prototype.editableProfileFields = {};

NostrMailApp.prototype.renderProfileFromObject = function(profile, cachedPictureDataUrl) {
    // Build editable fields from profile.fields, always include email
    // Ensure all fields from the profile are included, even if not in standard order
    this.editableProfileFields = { ...(profile && profile.fields ? profile.fields : {}) };
    // Optionally, always include email if missing
    if (!('email' in this.editableProfileFields)) {
        this.editableProfileFields.email = '';
    }
    this.renderProfileFieldsList(this.editableProfileFields);
    // Show warning if profile email and settings email differ
    this.renderProfileEmailWarning();

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
    'lud16',
    'nip05',
];

NostrMailApp.prototype.renderProfileFieldsList = function(fields) {
    const listDiv = document.getElementById('profile-fields-list');
    if (!listDiv) return;
    
    listDiv.innerHTML = '';
    
    if (!fields || Object.keys(fields).length === 0) {
        listDiv.innerHTML = '<div class="text-muted">No fields found.</div>';
        return;
    }

    // Use the top-level constant for field order
    for (const key of PROFILE_FIELD_ORDER) {
        if (fields.hasOwnProperty(key)) {
            this._renderProfileFieldItem(listDiv, key, fields[key]);
        }
    }

    // Render custom fields (not in PROFILE_FIELD_ORDER), sorted alphabetically
    const customKeys = Object.keys(fields)
        .filter(key => !PROFILE_FIELD_ORDER.includes(key))
        .sort();
    for (const key of customKeys) {
        this._renderProfileFieldItem(listDiv, key, fields[key]);
    }

    // Add real-time warning update for email field
    const emailInput = document.getElementById('profile-field-email');
    if (emailInput) {
        emailInput.addEventListener('input', () => {
            this.editableProfileFields.email = emailInput.value;
            this.renderProfileEmailWarning();
        });
    }
}

// Helper to render a single field item
NostrMailApp.prototype._renderProfileFieldItem = function(listDiv, key, value) {
    const fieldDiv = document.createElement('div');
    fieldDiv.className = 'form-group profile-field-item';
    const label = document.createElement('label');
    label.textContent = key.charAt(0).toUpperCase() + key.slice(1).replace(/_/g, ' ') + ':';
    label.setAttribute('for', `profile-field-${key}`);
    let input;
    if (key === 'about') {
        input = document.createElement('textarea');
        input.rows = 3;
    } else if (key === 'picture') {
        input = document.createElement('input');
        input.type = 'url';
        input.placeholder = 'https://example.com/avatar.png';
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
    // Remove button for custom fields (not for standard ones)
    if (!['display_name','about','email','nip05','picture','banner','lud16','name'].includes(key)) {
        const removeBtn = document.createElement('button');
        removeBtn.type = 'button';
        removeBtn.className = 'btn btn-danger btn-small';
        removeBtn.innerHTML = '<i class="fas fa-trash"></i>';
        removeBtn.title = 'Remove field';
        removeBtn.addEventListener('click', () => {
            delete this.editableProfileFields[key];
            this.renderProfileFieldsList(this.editableProfileFields);
        });
        fieldDiv.appendChild(removeBtn);
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
    const nprivKey = domManager.getValue('nprivKey')?.trim() || '';
    
    if (!nprivKey) {
        domManager.setValue('publicKeyDisplay', '');
        return;
    }
    
    try {
        const isValid = await TauriService.validatePrivateKey(nprivKey);
        
        if (!isValid) {
            domManager.setValue('publicKeyDisplay', 'Invalid private key');
            return;
        }
        
        const publicKey = await TauriService.getPublicKeyFromPrivate(nprivKey);
        domManager.setValue('publicKeyDisplay', publicKey);
        
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
            
            console.log(`[APP] Periodic update check: settingsPage=${isOnSettingsPage}, hasRelays=${hasRelays}`);
            
            if (isOnSettingsPage && hasRelays) {
                console.log('[APP] Running periodic relay status update...');
                await this.updateRelayStatusOnly();
            }
        } catch (error) {
            console.error('Error updating relay status:', error);
        }
    }, 10000); // Changed to 10 seconds as requested
}

// Toggle relay edit mode (collapse/expand)
NostrMailApp.prototype.toggleRelayEdit = function() {
    const relaysList = domManager.get('relaysList');
    const relayEditToggle = domManager.get('relayEditToggle');
    
    if (!relaysList || !relayEditToggle) return;
    
    const isCollapsed = relaysList.classList.contains('collapsed');
    
    if (isCollapsed) {
        // Expand - show edit mode
        relaysList.classList.remove('collapsed');
        relaysList.classList.add('expanded');
        relayEditToggle.classList.add('editing');
        relayEditToggle.innerHTML = '<i class="fas fa-times"></i> Done';
        
        // Render the full relay list
        this.renderRelays();
    } else {
        // Collapse - show summary mode
        relaysList.classList.remove('expanded');
        relaysList.classList.add('collapsed');
        relayEditToggle.classList.remove('editing');
        relayEditToggle.innerHTML = '<i class="fas fa-edit"></i> Edit';
        
        // Clear only the relay items to save memory, keep add-relay-section
        const relayItems = relaysList.querySelectorAll('.relay-item');
        relayItems.forEach(item => item.remove());
    }
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
        console.log('[APP] Got relay statuses:', relayStatuses);
        
        // Always update the summary
        this.updateRelaySummary(relayStatuses);
        
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

// Initialize when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    window.domManager = new DOMManager();
    console.log('🌐 DOM loaded - Initializing Nostr Mail interface...');
    
    // Set initial dark mode from localStorage
    const darkPref = localStorage.getItem('darkMode');
    window.app.setDarkMode(darkPref === '1');
    
    console.log('🎨 Dark mode initialized:', darkPref === '1' ? 'enabled' : 'disabled');
    
    // Initialize the application
    window.app.init();
    
    // Start relay status updates
    window.app.startRelayStatusUpdates();
}); 