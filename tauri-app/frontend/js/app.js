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
        this.loadSettings();

        console.log('🌐 Loading relay configuration from database...');
        await this.loadRelaysFromDatabase();

        console.log('🔑 Loading/generating keypair...');
        await this.loadKeypair();
        
        console.log('🎯 Setting up event listeners...');
        this.setupEventListeners();
        
        console.log('📬 Loading initial data...');
        // Load contacts first so DM contacts can access cached profile photos
        await contactsService.loadContacts();
        // NOTE: We do NOT load emails here. Emails are only loaded when the inbox tab is clicked.
        // await emailService.loadEmails(); // <-- Remove or comment out this line so emails are not loaded on startup
        // await dmService.loadDmContacts(); // TODO: add this back in once we have stored DMs in the DB
        
        // Populate Nostr contact dropdown for compose page
        emailService.populateNostrContactDropdown();
        
        // Try to restore saved contact selection
        emailService.restoreContactSelection();
        
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
NostrMailApp.prototype.loadSettings = function() {
    try {
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

// Load relays from database only
NostrMailApp.prototype.loadRelaysFromDatabase = async function() {
    try {
        const relays = await TauriService.getDbRelays();
        console.log('Loaded relays from DB:', relays); // DEBUG
        appState.setRelays(relays);
        
        // Sync disconnected relays first, then render
        await this.syncDisconnectedRelays();
        await this.renderRelays();
    } catch (error) {
        console.error('Failed to load relays from database:', error);
        notificationService.showError('Could not load relays from database.');
    }
}

// Load keypair
NostrMailApp.prototype.loadKeypair = async function() {
    try {
        const stored = localStorage.getItem('nostr_keypair');
        if (stored) {
            const keypair = JSON.parse(stored);
            appState.setKeypair(keypair);
        } else {
            const keypair = await TauriService.generateKeypair();
            appState.setKeypair(keypair);
            localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
        }
        console.log('Keypair loaded:', appState.getKeypair().public_key.substring(0, 20) + '...');
        this.renderProfilePubkey();
        
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
            sendBtn.addEventListener('click', () => emailService.sendEmail());
        }
        const saveDraftBtn = domManager.get('saveDraftBtn');
        if (saveDraftBtn) {
            saveDraftBtn.addEventListener('click', () => emailService.saveDraft());
        }
        const encryptBtn = domManager.get('encryptBtn');
        if (encryptBtn) {
            console.log('[JS] Setting up encrypt button event listener');
            encryptBtn.dataset.encrypted = 'false';
            // Update DM checkbox visibility on initialization
            emailService.updateDmCheckboxVisibility();
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
                    if (!subjectValue || !messageBodyValue) {
                        notificationService.showError('Both subject and message body must be filled to encrypt.');
                        return;
                    }
                    const didEncrypt = await emailService.encryptEmailFields();
                    if (didEncrypt) {
                        if (iconSpan) iconSpan.className = 'fas fa-unlock';
                        if (labelSpan) labelSpan.textContent = 'Decrypt';
                        encryptBtn.dataset.encrypted = 'true';
                        // Disable editing
                        if (subjectInput) subjectInput.disabled = true;
                        if (messageBodyInput) messageBodyInput.disabled = true;
                        // Update DM checkbox visibility
                        emailService.updateDmCheckboxVisibility();
                    }
                } else {
                    // Decrypt mode
                    console.log('[JS] Decrypt button clicked');
                    // Get keys and contact
                    const privkey = appState.getKeypair().private_key;
                    const pubkey = emailService.selectedNostrContact?.pubkey;
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
                            const encryptedContent = match[2].replace(/\s+/g, '');
                            try {
                                const decrypted = await TauriService.decryptDmContent(privkey, pubkey, encryptedContent);
                                domManager.setValue('messageBody', decrypted);
                                notificationService.showSuccess('Body decrypted');
                                decryptedAny = true;
                            } catch (err) {
                                notificationService.showError('Failed to decrypt body: ' + err);
                            }
                        }
                        if (!decryptedAny) {
                            notificationService.showError('No encrypted message found in subject or body');
                        }
                    } else {
                        notificationService.showError('No encrypted message found or missing keys');
                    }
                    if (iconSpan) iconSpan.className = 'fas fa-lock';
                    if (labelSpan) labelSpan.textContent = 'Encrypt';
                    encryptBtn.dataset.encrypted = 'false';
                    // Re-enable editing
                    if (subjectInput) subjectInput.disabled = false;
                    if (messageBodyInput) messageBodyInput.disabled = false;
                    // Update DM checkbox visibility
                    emailService.updateDmCheckboxVisibility();
                }
            });
        } else {
            console.error('[JS] Encrypt button not found in DOM');
        }
        
        // Preview headers button
        const previewHeadersBtn = domManager.get('previewHeadersBtn');
        if (previewHeadersBtn) {
            console.log('[JS] Setting up preview headers button event listener');
            previewHeadersBtn.addEventListener('click', () => emailService.previewEmailHeaders());
        } else {
            console.error('[JS] Preview headers button not found in DOM');
        }
        
        // Nostr contact dropdown
        const nostrContactSelect = domManager.get('nostrContactSelect');
        if (nostrContactSelect) {
            console.log('[JS] Setting up Nostr contact dropdown event listener');
            nostrContactSelect.addEventListener('change', () => emailService.handleNostrContactSelection());
        }
        
        // Inbox
        const refreshInbox = domManager.get('refreshInbox');
        if (refreshInbox) {
            refreshInbox.addEventListener('click', async () => {
                // Clear search input
                domManager.clear('emailSearch');
                // Sync and load all emails (no search filter)
                try {
                    await emailService.syncInboxEmails();
                    await emailService.loadEmails();
                    notificationService.showSuccess('Inbox synced successfully');
                } catch (error) {
                    console.error('[JS] Error syncing inbox:', error);
                    notificationService.showError('Failed to sync inbox: ' + error.message);
                }
            });
        }
        
        // Back to inbox button
        const backToInboxBtn = document.getElementById('back-to-inbox');
        if (backToInboxBtn) {
            backToInboxBtn.addEventListener('click', () => emailService.showEmailList());
        }
        
        // Email Search
        const emailSearch = domManager.get('emailSearch');
        if (emailSearch) {
            emailSearch.addEventListener('input', () => emailService.filterEmails());
        }
        
        // DM elements
        const newDmBtn = domManager.get('newDmBtn');
        if (newDmBtn) {
            newDmBtn.addEventListener('click', () => this.showNewDmCompose());
        }
        
        const refreshDm = domManager.get('refreshDm');
        if (refreshDm) {
            refreshDm.addEventListener('click', () => dmService.refreshDmConversations());
        }
        
        const dmSearch = domManager.get('dmSearch');
        if (dmSearch) {
            dmSearch.addEventListener('input', () => dmService.filterDmContacts());
        }
        
        const dmSearchToggle = domManager.get('dmSearchToggle');
        if (dmSearchToggle) {
            dmSearchToggle.addEventListener('click', () => dmService.toggleDmSearch());
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
        const saveSettingsBtn = domManager.get('saveSettingsBtn');
        if (saveSettingsBtn) {
            saveSettingsBtn.addEventListener('click', () => this.saveSettings());
        }
        const testConnectionBtn = domManager.get('testConnectionBtn');
        if (testConnectionBtn) {
            testConnectionBtn.addEventListener('click', () => this.testConnection());
        }
        const testEmailConnectionBtn = domManager.get('testEmailConnectionBtn');
        if (testEmailConnectionBtn) {
            testEmailConnectionBtn.addEventListener('click', () => emailService.testEmailConnection());
        }
        // Add this block to wire up the add relay button
        const addRelayBtn = domManager.get('addRelayBtn');
        if (addRelayBtn) {
            addRelayBtn.addEventListener('click', () => this.addRelay());
        }
        // Email provider selection
        const emailProvider = domManager.get('emailProvider');
        if (emailProvider) {
            emailProvider.addEventListener('change', () => emailService.handleEmailProviderChange());
        }
        // Toggle private key visibility (eye button)
        const toggleNprivVisibilityBtn = domManager.get('toggleNprivVisibilityBtn');
        const nprivKeyInput = domManager.get('nprivKey');
        if (toggleNprivVisibilityBtn && nprivKeyInput) {
            toggleNprivVisibilityBtn.addEventListener('click', () => {
                if (nprivKeyInput.type === 'password') {
                    nprivKeyInput.type = 'text';
                    toggleNprivVisibilityBtn.title = 'Hide private key';
                } else {
                    nprivKeyInput.type = 'password';
                    toggleNprivVisibilityBtn.title = 'Show private key';
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
                try {
                    await emailService.syncSentEmails();
                    await emailService.loadSentEmails();
                    notificationService.showSuccess('Sent emails synced successfully');
                } catch (error) {
                    console.error('[JS] Error syncing sent emails:', error);
                    notificationService.showError('Failed to sync sent emails: ' + error.message);
                }
            });
        }
        const backToSentBtn = document.getElementById('back-to-sent');
        if (backToSentBtn) {
            backToSentBtn.addEventListener('click', () => emailService.showSentList());
        }
        
        // Drafts event listeners
        const refreshDrafts = domManager.get('refreshDrafts');
        if (refreshDrafts) {
            refreshDrafts.addEventListener('click', async () => {
                await emailService.loadDrafts();
            });
        }
        
        const backToDraftsBtn = domManager.get('backToDrafts');
        if (backToDraftsBtn) {
            backToDraftsBtn.addEventListener('click', () => emailService.showDraftsList());
        }
        
        console.log('Event listeners set up successfully');
    } catch (error) {
        console.error('Error setting up event listeners:', error);
    }
}

// Tab switching
NostrMailApp.prototype.switchTab = function(tabName) {
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
        if (!appState.getDmContacts() || appState.getDmContacts().length === 0) {
            dmService.loadDmContacts();
        } else {
            // Just render the existing DM contacts
            dmService.renderDmContacts();
        }
    }
    if (tabName === 'inbox') {
        emailService.loadEmails();
    }
    if (tabName === 'sent') {
        emailService.loadSentEmails();
    }
    if (tabName === 'drafts') {
        emailService.loadDrafts();
    }
    if (tabName === 'compose') {
        // Clear current draft state when switching to compose (unless we're loading a draft)
        if (!emailService.currentDraftId) {
            emailService.clearCurrentDraft();
        }
        // Try to restore contact selection when switching to compose
        emailService.restoreContactSelection();
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
NostrMailApp.prototype.saveSettings = async function() {
    try {
        // Validate npriv key if provided
        const nprivKey = domManager.getValue('nprivKey')?.trim() || '';
        if (nprivKey && !nprivKey.startsWith('npriv1') && !nprivKey.startsWith('nsec1')) {
            notificationService.showError('Invalid Nostr private key format. Should start with "npriv1" or "nsec1"');
            return;
        }
        
        const settings = {
            npriv_key: nprivKey,
            encryption_algorithm: domManager.getValue('encryptionAlgorithm') || 'nip44',
            email_address: domManager.getValue('emailAddress') || '',
            password: domManager.getValue('emailPassword') || '',
            smtp_host: domManager.getValue('smtpHost') || '',
            smtp_port: parseInt(domManager.getValue('smtpPort')) || 587,
            imap_host: domManager.getValue('imapHost') || '',
            imap_port: parseInt(domManager.getValue('imapPort')) || 993,
            use_tls: domManager.get('use-tls')?.checked || false
        };
        
        localStorage.setItem('nostr_mail_settings', JSON.stringify(settings));
        appState.setSettings(settings);
        appState.setNprivKey(nprivKey);
        
        // If a private key is provided, update appState.keypair and localStorage
        if (nprivKey && (nprivKey.startsWith('npriv1') || nprivKey.startsWith('nsec1'))) {
            const isValid = await TauriService.validatePrivateKey(nprivKey);
            if (!isValid) {
                notificationService.showError('Invalid private key');
                return;
            }
            const publicKey = await TauriService.getPublicKeyFromPrivate(nprivKey);
            const keypair = { private_key: nprivKey, public_key: publicKey };
            appState.setKeypair(keypair);
            localStorage.setItem('nostr_keypair', JSON.stringify(keypair));
            this.renderProfilePubkey();
            // If on profile tab, reload profile
            if (document.querySelector('.tab-content#profile.active')) {
                this.loadProfile();
            }
        }
        
        await this.saveRelays();
        this.saveRelaysToLocalStorage();
        notificationService.showSuccess('Settings saved successfully');
    } catch (error) {
        console.error('Error saving settings:', error);
        notificationService.showError('Failed to save settings');
    }
}

NostrMailApp.prototype.populateSettingsForm = async function() {
    console.log('[QR] populateSettingsForm called');
    const settings = appState.getSettings();
    if (!settings) return;
    
    try {
        domManager.setValue('nprivKey', settings.npriv_key || '');
        domManager.setValue('encryptionAlgorithm', settings.encryption_algorithm || 'nip44');
        domManager.setValue('emailAddress', settings.email_address || '');
        domManager.setValue('emailPassword', settings.password || '');
        domManager.setValue('smtpHost', settings.smtp_host || '');
        domManager.setValue('smtpPort', settings.smtp_port || '');
        domManager.setValue('imapHost', settings.imap_host || '');
        domManager.setValue('imapPort', settings.imap_port || '');
        domManager.get('use-tls').checked = settings.use_tls || false;
        
        // Detect and set the email provider based on saved settings
        const emailProvider = domManager.get('emailProvider');
        if (emailProvider) {
            const provider = Utils.detectEmailProvider(settings);
            emailProvider.value = provider;
        }
        
        // Update public key display if npriv is available
        await this.updatePublicKeyDisplay();
        this.setupQrCodeEventListeners();
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
        await emailService.loadEmails();
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
    relaysList.innerHTML = '';
    
    // Get relay connection statuses from backend
    let relayStatuses = [];
    try {
        relayStatuses = await TauriService.getRelayStatus();
        console.log('Relay statuses from backend:', relayStatuses);
    } catch (error) {
        console.error('Failed to get relay statuses:', error);
    }
    
    // Sort relays by updated_at descending
    const relays = [...appState.getRelays()].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at));
    relays.forEach((relay) => {
        // Find matching status from backend
        const status = relayStatuses.find(s => s.url === relay.url);
        const connectionStatus = status?.status || 'Disconnected';
        const statusClass = this.getRelayStatusClass(connectionStatus);
        const statusIcon = this.getRelayStatusIcon(connectionStatus);
        const statusText = this.getRelayStatusText(connectionStatus);
        
        const relayItem = document.createElement('div');
        relayItem.className = 'relay-item';
        relayItem.innerHTML = `
            <div class="relay-item-info">
                <span class="relay-item-url">${relay.url}</span>
                <div class="relay-status ${statusClass}">
                    <i class="fas ${statusIcon}"></i>
                    <span class="relay-status-text">${statusText}</span>
                </div>
            </div>
            <div class="relay-item-actions">
                <label class="toggle-switch">
                    <input type="checkbox" ${relay.is_active ? 'checked' : ''} data-relay-id="${relay.id}" data-relay-url="${relay.url}">
                </label>
                <button class="btn btn-danger btn-small" data-relay-id="${relay.id}" data-relay-url="${relay.url}">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;
        relaysList.appendChild(relayItem);
    });
    // Add event listeners after rendering
    relaysList.querySelectorAll('input[type="checkbox"]').forEach(toggle => {
        toggle.addEventListener('change', (e) => this.toggleRelayById(e.target.dataset.relayId, e.target.dataset.relayUrl));
    });
    relaysList.querySelectorAll('.btn-danger').forEach(button => {
        button.addEventListener('click', (e) => this.removeRelayById(e.currentTarget.dataset.relayId, e.currentTarget.dataset.relayUrl));
    });
}

// Helper methods for relay status display
NostrMailApp.prototype.getRelayStatusClass = function(connectionStatus) {
    switch (connectionStatus) {
        case 'Connected': return 'status-connected';
        case 'Disconnected': return 'status-disconnected';
        case 'Disabled': return 'status-disabled';
        default: return 'status-unknown';
    }
}

NostrMailApp.prototype.getRelayStatusIcon = function(connectionStatus) {
    switch (connectionStatus) {
        case 'Connected': return 'fa-circle';
        case 'Disconnected': return 'fa-circle';
        case 'Disabled': return 'fa-circle';
        default: return 'fa-question-circle';
    }
}

NostrMailApp.prototype.getRelayStatusText = function(connectionStatus) {
    switch (connectionStatus) {
        case 'Connected': return 'Connected';
        case 'Disconnected': return 'Disconnected';
        case 'Disabled': return 'Disabled';
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
                
                // Re-initialize client to pick up new relay
                await this.initializeNostrClient();
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
                console.log(`[APP] ${newActiveState ? 'Connected to' : 'Disconnected from'} relay: ${relayUrl}`);
            } catch (relayError) {
                // Log the error but don't fail the entire operation
                console.warn(`[APP] Relay connection update had issues: ${relayError}`);
                // Continue with UI updates since database was updated successfully
            }
            
            // Update the local state
            relay.is_active = newActiveState;
            relay.updated_at = new Date().toISOString();
            
            // Update just the toggle switch state without re-rendering everything
            const toggleElement = document.querySelector(`input[data-relay-url="${relayUrl}"]`);
            if (toggleElement) {
                toggleElement.checked = newActiveState;
            }
            
            // Update the status indicator immediately
            const expectedStatus = newActiveState ? 'Connected' : 'Disabled';
            this.updateSingleRelayStatus(relayUrl, expectedStatus);
            
            // Verify actual status after a short delay
            setTimeout(async () => {
                try {
                    const relayStatuses = await TauriService.getRelayStatus();
                    const status = relayStatuses.find(s => s.url === relayUrl);
                    if (status) {
                        this.updateSingleRelayStatus(relayUrl, status.status);
                    }
                } catch (error) {
                    console.error('Failed to verify relay status:', error);
                }
            }, 1000);
            
        } catch (error) {
            console.error('Failed to toggle relay:', error);
            notificationService.showError('Failed to update relay: ' + error);
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

    const activeRelays = appState.getActiveRelays();
    if (activeRelays.length === 0) {
        notificationService.showError('No active relays to fetch profile from.');
        this.renderProfilePubkey();
        if (Utils.isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'No active relays.';
            }
        }
        return;
    }

    try {
        const profile = await TauriService.fetchProfile(appState.getKeypair().public_key, activeRelays);

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

    const activeRelays = appState.getActiveRelays();
    if (activeRelays.length === 0) {
        notificationService.showError('No active relays configured');
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

        // Update profile on Nostr
        await TauriService.updateProfile(
            appState.getKeypair().private_key,
            cleanedFields,
            activeRelays
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
    // Update every 15 seconds
    this.relayStatusInterval = setInterval(async () => {
        try {
            // Only update if we're on the settings page and have relays
            const settingsPanel = domManager.get('settingsPanel');
            const isOnSettingsPage = settingsPanel && !settingsPanel.classList.contains('hidden');
            const hasRelays = appState.getRelays().length > 0;
            
            console.log(`[APP] Periodic update check: settingsPage=${isOnSettingsPage}, hasRelays=${hasRelays}`);
            
            if (isOnSettingsPage && hasRelays) {
                console.log('[APP] Running periodic relay sync...');
                // First sync disconnected relays (auto-disable them)
                await this.syncDisconnectedRelays();
                // Then update the display
                await this.renderRelays();
            }
        } catch (error) {
            console.error('Error updating relay status:', error);
        }
    }, 15000); // Increased to 15 seconds to reduce load
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