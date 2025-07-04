// Main Application
// Coordinates all modules and handles application initialization

import { appState } from './app-state.js';
import { domManager } from './dom-manager.js';
import { TauriService } from './tauri-service.js';
import { notificationService } from './notification-service.js';
import { emailService } from './email-service.js';
import { contactsService } from './contacts-service.js';
import { dmService } from './dm-service.js';
import { Utils } from './utils.js';

// Try to import Tauri APIs if available
let tauriDialog, tauriFs;
try {
    tauriDialog = window.__TAURI__ ? window.__TAURI__.dialog : undefined;
    tauriFs = window.__TAURI__ ? window.__TAURI__.fs : undefined;
} catch (e) {
    tauriDialog = undefined;
    tauriFs = undefined;
}

export class NostrMailApp {
    constructor() {
        this.initialized = false;
    }

    // Initialize the application
    async init() {
        console.log('🚀 ========================================');
        console.log('🚀   Nostr Mail - Starting Application');
        console.log('🚀 ========================================');
        console.log('📧 Email + 🔐 Nostr Integration');
        console.log('🌐 Version: 1.0.0');
        console.log('⏰ Started at:', new Date().toLocaleString());
        console.log('🚀 ========================================');
        
        try {
            console.log('📋 Loading application settings...');
            this.loadSettings();

            console.log('🌐 Loading relay configuration...');
            this.loadRelaysFromStorageOrBackend();

            console.log('🔑 Loading/generating keypair...');
            await this.loadKeypair();
            
            console.log('🎯 Setting up event listeners...');
            this.setupEventListeners();
            
            console.log('📬 Loading initial data...');
            // Load contacts first so DM contacts can access cached profile photos
            await contactsService.loadContacts();
            await emailService.loadEmails();
            await dmService.loadDmContacts();
            
            // Populate Nostr contact dropdown for compose page
            emailService.populateNostrContactDropdown();
            
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
    loadSettings() {
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

    // Load relays from storage or backend
    loadRelaysFromStorageOrBackend() {
        const stored = localStorage.getItem('nostr_mail_relays');
        if (stored) {
            try {
                const relays = JSON.parse(stored);
                appState.setRelays(relays);
                // Sync to backend
                TauriService.setRelays(relays);
            } catch (e) {
                console.error('Failed to parse relays from localStorage:', e);
                this.loadRelays(); // fallback to backend
            }
        } else {
            this.loadRelays(); // fallback to backend
        }
    }

    // Load keypair
    async loadKeypair() {
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
        } catch (error) {
            console.error('Failed to load keypair:', error);
            notificationService.showError('Failed to load encryption keys');
        }
    }

    // Load relays from backend
    async loadRelays() {
        try {
            const relays = await TauriService.getRelays();
            appState.setRelays(relays);
            this.renderRelays();
        } catch (error) {
            console.error('Failed to load relays:', error);
            notificationService.showError('Could not load relays from backend.');
        }
    }

    // Setup event listeners
    setupEventListeners() {
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
                encryptBtn.addEventListener('click', () => {
                    console.log('[JS] Encrypt button clicked');
                    emailService.encryptEmailFields();
                });
            } else {
                console.error('[JS] Encrypt button not found in DOM');
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
                    // Load all emails (no search filter)
                    await emailService.loadEmails();
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
                refreshContactsBtn.addEventListener('click', () => contactsService.refreshContacts());
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
            
            // Email provider selection
            const emailProvider = domManager.get('emailProvider');
            if (emailProvider) {
                emailProvider.addEventListener('change', () => emailService.handleEmailProviderChange());
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
            
            console.log('Event listeners set up successfully');
        } catch (error) {
            console.error('Error setting up event listeners:', error);
        }
    }

    // Tab switching
    switchTab(tabName) {
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
            this.loadRelays();
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
    }

    // Modal functions
    showModal(title, content) {
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

    hideModal() {
        try {
            const modalOverlay = domManager.get('modalOverlay');
            if (modalOverlay) modalOverlay.classList.add('hidden');
        } catch (error) {
            console.error('Error hiding modal:', error);
        }
    }

    // Show new DM compose
    showNewDmCompose() {
        // This would open a modal to compose a new DM
        // For now, just show a placeholder
        notificationService.showInfo('New DM functionality coming soon');
    }

    // Settings management
    async saveSettings() {
        try {
            // Validate npriv key if provided
            const nprivKey = domManager.getValue('nprivKey')?.trim() || '';
            if (nprivKey && !nprivKey.startsWith('npriv1') && !nprivKey.startsWith('nsec1')) {
                notificationService.showError('Invalid Nostr private key format. Should start with "npriv1" or "nsec1"');
                return;
            }
            
            const settings = {
                npriv_key: nprivKey,
                email_address: domManager.getValue('emailAddress') || '',
                password: domManager.getValue('emailPassword') || '',
                smtp_host: domManager.getValue('smtpHost') || '',
                smtp_port: parseInt(domManager.getValue('smtpPort')) || 587,
                imap_host: domManager.getValue('imapHost') || '',
                imap_port: parseInt(domManager.getValue('imapPort')) || 993,
                use_tls: domManager.get('useTls')?.checked || false
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

    populateSettingsForm() {
        const settings = appState.getSettings();
        if (!settings) return;
        
        try {
            domManager.setValue('nprivKey', settings.npriv_key || '');
            domManager.setValue('emailAddress', settings.email_address || '');
            domManager.setValue('emailPassword', settings.password || '');
            domManager.setValue('smtpHost', settings.smtp_host || '');
            domManager.setValue('smtpPort', settings.smtp_port || '');
            domManager.setValue('imapHost', settings.imap_host || '');
            domManager.setValue('imapPort', settings.imap_port || '');
            domManager.get('useTls').checked = settings.use_tls || false;
            
            // Detect and set the email provider based on saved settings
            const emailProvider = domManager.get('emailProvider');
            if (emailProvider) {
                const provider = Utils.detectEmailProvider(settings);
                emailProvider.value = provider;
            }
            
            // Update public key display if npriv is available
            this.updatePublicKeyDisplay();
        } catch (error) {
            console.error('Error populating settings form:', error);
        }
    }

    // Test connection
    async testConnection() {
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
    async saveRelays() {
        try {
            await TauriService.setRelays(appState.getRelays());
        } catch (error) {
            console.error('Failed to save relays:', error);
            notificationService.showError('Could not save relays to backend.');
        }
    }

    saveRelaysToLocalStorage() {
        localStorage.setItem('nostr_mail_relays', JSON.stringify(appState.getRelays()));
    }

    renderRelays() {
        const relaysList = domManager.get('relaysList');
        if (!relaysList) return;
        
        relaysList.innerHTML = '';
        
        const relays = appState.getRelays();
        relays.forEach((relay, index) => {
            const relayItem = document.createElement('div');
            relayItem.className = 'relay-item';
            
            relayItem.innerHTML = `
                <span class="relay-item-url">${relay.url}</span>
                <div class="relay-item-actions">
                    <label class="toggle-switch">
                        <input type="checkbox" ${relay.is_active ? 'checked' : ''} data-index="${index}">
                    </label>
                    <button class="btn btn-danger btn-small" data-index="${index}">
                        <i class="fas fa-trash"></i>
                    </button>
                </div>
            `;
            
            relaysList.appendChild(relayItem);
        });

        // Add event listeners after rendering
        relaysList.querySelectorAll('input[type="checkbox"]').forEach(toggle => {
            toggle.addEventListener('change', (e) => this.toggleRelay(e.target.dataset.index));
        });
        
        relaysList.querySelectorAll('.btn-danger').forEach(button => {
            button.addEventListener('click', (e) => this.removeRelay(e.currentTarget.dataset.index));
        });
    }

    addRelay() {
        const url = domManager.getValue('newRelayUrl')?.trim();
        if (url && (url.startsWith('ws://') || url.startsWith('wss://'))) {
            const relays = appState.getRelays();
            if (!relays.some(r => r.url === url)) {
                relays.push({ url, is_active: true });
                appState.setRelays(relays);
                domManager.clear('newRelayUrl');
                this.renderRelays();
                this.saveRelays();
                this.saveRelaysToLocalStorage();
            } else {
                notificationService.showError('Relay already exists.');
            }
        } else {
            notificationService.showError('Invalid relay URL. Must start with ws:// or wss://');
        }
    }

    toggleRelay(index) {
        const relays = appState.getRelays();
        if (relays[index]) {
            relays[index].is_active = !relays[index].is_active;
            appState.setRelays(relays);
            this.saveRelays();
            this.saveRelaysToLocalStorage();
        }
    }

    removeRelay(index) {
        const relays = appState.getRelays();
        if (relays[index]) {
            relays.splice(index, 1);
            appState.setRelays(relays);
            this.renderRelays();
            this.saveRelays();
            this.saveRelaysToLocalStorage();
        }
    }

    // Profile management
    async loadProfile() {
        // Try to load cached profile first
        let cachedProfile = null;
        let cachedPictureDataUrl = null;
        try {
            const cached = localStorage.getItem('nostr_mail_profile');
            if (cached) {
                cachedProfile = JSON.parse(cached);
                cachedPictureDataUrl = localStorage.getItem('nostr_mail_profile_picture');
                this.renderProfileFromObject(cachedProfile, cachedPictureDataUrl);
            }
        } catch (e) {
            console.warn('Failed to load cached profile:', e);
        }

        if (!appState.hasKeypair() || !appState.getKeypair().public_key) {
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
                        console.warn('Failed to cache profile picture:', e);
                        this.renderProfileFromObject(profile, null);
                    }
                } else {
                    localStorage.removeItem('nostr_mail_profile_picture');
                    this.renderProfileFromObject(profile, null);
                }
                // Cache the profile in localStorage
                localStorage.setItem('nostr_mail_profile', JSON.stringify(profile));
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
    }

    renderProfilePubkey() {
        const pubkeyDiv = document.getElementById('profile-pubkey');
        if (pubkeyDiv && appState.hasKeypair() && appState.getKeypair().public_key) {
            pubkeyDiv.textContent = `Your npub: ${appState.getKeypair().public_key}`;
        } else if (pubkeyDiv) {
            pubkeyDiv.textContent = '';
        }
    }

    // Store the current editable fields in memory
    editableProfileFields = {};

    renderProfileFromObject(profile, cachedPictureDataUrl) {
        // Build editable fields from profile.fields, always include email
        this.editableProfileFields = { ...(profile && profile.fields ? profile.fields : {}) };
        if (!('email' in this.editableProfileFields)) {
            this.editableProfileFields.email = '';
        }
        this.renderProfileFieldsList(this.editableProfileFields);
        
        // Show profile picture if present
        const profilePicture = document.getElementById('profile-picture');
        if (profilePicture) {
            if (cachedPictureDataUrl) {
                profilePicture.src = cachedPictureDataUrl;
                profilePicture.style.display = '';
            } else if (this.editableProfileFields.picture) {
                profilePicture.src = this.editableProfileFields.picture;
                profilePicture.style.display = '';
            } else {
                profilePicture.style.display = 'none';
            }
        }
    }

    renderProfileFieldsList(fields) {
        const listDiv = document.getElementById('profile-fields-list');
        if (!listDiv) return;
        
        listDiv.innerHTML = '';
        
        if (!fields || Object.keys(fields).length === 0) {
            listDiv.innerHTML = '<div class="text-muted">No fields found.</div>';
            return;
        }
        
        for (const [key, value] of Object.entries(fields)) {
            const fieldDiv = document.createElement('div');
            fieldDiv.className = 'form-group profile-field-item';
            
            const label = document.createElement('label');
            label.textContent = key.charAt(0).toUpperCase() + key.slice(1) + ':';
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
    }

    // Add new profile field
    addProfileField() {
        const fieldName = prompt('Enter field name:');
        if (fieldName && fieldName.trim()) {
            const key = fieldName.trim().toLowerCase().replace(/\s+/g, '_');
            this.editableProfileFields[key] = '';
            this.renderProfileFieldsList(this.editableProfileFields);
        }
    }

    // Update profile
    async updateProfile() {
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
            localStorage.setItem('nostr_mail_profile', JSON.stringify(updatedProfile));

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

    async updatePublicKeyDisplay() {
        const nprivKey = domManager.getValue('nprivKey')?.trim() || '';
        
        if (!nprivKey) {
            domManager.setValue('publicKeyDisplay', '');
            return;
        }
        
        try {
            // Validate the private key first
            const isValid = await TauriService.validatePrivateKey(nprivKey);
            
            if (!isValid) {
                domManager.setValue('publicKeyDisplay', 'Invalid private key');
                return;
            }
            
            // Get the public key from the private key
            const publicKey = await TauriService.getPublicKeyFromPrivate(nprivKey);
            domManager.setValue('publicKeyDisplay', publicKey);
            
        } catch (error) {
            console.error('Failed to get public key:', error);
            domManager.setValue('publicKeyDisplay', 'Error getting public key');
        }
    }

    // Dark mode management
    setDarkMode(enabled) {
        document.body.classList.toggle('dark-mode', enabled);
        const icon = document.getElementById('dark-mode-icon');
        if (icon) {
            icon.className = enabled ? 'fas fa-sun' : 'fas fa-moon';
        }
        localStorage.setItem('darkMode', enabled ? '1' : '0');
    }

    toggleDarkMode() {
        const enabled = !document.body.classList.contains('dark-mode');
        this.setDarkMode(enabled);
    }
}

// Create and export the main application instance
export const app = new NostrMailApp();

// Initialize when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    console.log('🌐 DOM loaded - Initializing Nostr Mail interface...');
    
    // Set initial dark mode from localStorage
    const darkPref = localStorage.getItem('darkMode');
    app.setDarkMode(darkPref === '1');
    
    console.log('🎨 Dark mode initialized:', darkPref === '1' ? 'enabled' : 'disabled');
    
    // Initialize the application
    app.init();
}); 