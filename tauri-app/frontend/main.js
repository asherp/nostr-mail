const { invoke } = window.__TAURI__.core;

let testBtn;
let resultEl;

// App state
let appState = {
    currentUser: null,
    contacts: [],
    emails: [],
    settings: null,
    keypair: null,
    dmContacts: [],
    dmMessages: {},
    selectedDmContact: null,
    selectedContact: null, // Track selected contact in contacts view
    nprivKey: null,
    relays: [],
    contacts_total: null, // To show progress, e.g. "15 / 72"
};

// DOM elements with error handling
function getElement(id) {
    const element = document.getElementById(id);
    if (!element) {
        console.warn(`Element with id '${id}' not found`);
    }
    return element;
}

function getElements(selector) {
    const elements = document.querySelectorAll(selector);
    if (elements.length === 0) {
        console.warn(`No elements found for selector '${selector}'`);
    }
    return elements;
}

const elements = {
    navItems: getElements('.nav-item'),
    tabContents: getElements('.tab-content'),
    modalOverlay: getElement('modal-overlay'),
    modalTitle: getElement('modal-title'),
    modalBody: getElement('modal-body'),
    modalClose: document.querySelector('.modal-close'),
    
    // Compose form
    toAddress: getElement('to-address'),
    subject: getElement('subject'),
    messageBody: getElement('message-body'),
    sendBtn: getElement('send-btn'),
    saveDraftBtn: getElement('save-draft-btn'),
    
    // Inbox
    emailList: getElement('email-list'),
    refreshInbox: getElement('refresh-inbox'),
    emailSearch: getElement('email-search'),
    
    // DM elements
    dmContacts: getElement('dm-contacts'),
    dmMessages: getElement('dm-messages'),
    dmRecipient: getElement('dm-recipient'),
    dmMessage: getElement('dm-message'),
    sendDmBtn: getElement('send-dm-btn'),
    newDmBtn: getElement('new-dm-btn'),
    refreshDm: getElement('refresh-dm'),
    dmSearch: getElement('dm-search'),
    dmSearchToggle: getElement('dm-search-toggle'),
    dmSearchContainer: getElement('dm-search-container'),
    
    // Contacts
    contactsList: getElement('contacts-list'),
    addContactBtn: getElement('add-contact-btn'),
    refreshContactsBtn: getElement('refresh-contacts-btn'),
    contactsDetail: getElement('contacts-detail'),
    contactsSearch: getElement('contacts-search'),
    contactsSearchToggle: getElement('contacts-search-toggle'),
    contactsSearchContainer: getElement('contacts-search-container'),
    
    // Profile
    displayName: getElement('display-name'),
    about: getElement('about'),
    email: getElement('email'),
    nip05: getElement('nip05'),
    updateProfileBtn: getElement('update-profile-btn'),
    
    // Relays
    relaysList: getElement('relays-list'),
    newRelayUrl: getElement('new-relay-url'),
    addRelayBtn: getElement('add-relay-btn'),
    
    // Settings
    nprivKey: getElement('npriv-key'),
    generateKeyBtn: getElement('generate-key-btn'),
    publicKeyDisplay: getElement('public-key-display'),
    copyPubkeyBtn: getElement('copy-pubkey-btn'),
    emailProvider: getElement('email-provider'),
    emailAddress: getElement('email-address'),
    emailPassword: getElement('email-password'),
    smtpHost: getElement('smtp-host'),
    smtpPort: getElement('smtp-port'),
    imapHost: getElement('imap-host'),
    imapPort: getElement('imap-port'),
    useTls: getElement('use-tls'),
    saveSettingsBtn: getElement('save-settings-btn'),
    testConnectionBtn: getElement('test-connection-btn'),
    testEmailConnectionBtn: getElement('test-email-connection-btn'),
    copyNprivBtn: getElement('copy-npriv-btn'),
    copyEmailPasswordBtn: getElement('copy-email-password-btn'),
    toggleNprivVisibilityBtn: getElement('toggle-npriv-visibility-btn'),
    toggleEmailPasswordVisibilityBtn: getElement('toggle-email-password-visibility-btn'),
};

// Helper function to safely call Tauri commands
async function tauriInvoke(command, args = {}) {
    try {
        return await invoke(command, args);
    } catch (error) {
        console.error(`Tauri command failed: ${command}`, error);
        throw error;
    }
}

// Test Tauri availability
function testTauriAvailability() {
    console.log('=== Tauri API Test ===');
    console.log('window.__TAURI__:', window.__TAURI__);
    console.log('window.__TAURI__?.invoke:', window.__TAURI__?.invoke);
    console.log('window.invoke:', window.invoke);
    console.log('window.tauri:', window.tauri);
    console.log('window.__TAURI__?.tauri:', window.__TAURI__?.tauri);
    
    // Try to access Tauri in different ways
    if (window.__TAURI__) {
        console.log('window.__TAURI__ keys:', Object.keys(window.__TAURI__));
        console.log('window.__TAURI__ type:', typeof window.__TAURI__);
        console.log('window.__TAURI__.invoke type:', typeof window.__TAURI__.invoke);
        
        // Try to call a simple command to test
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
    
    // Check if we're in a Tauri context
    console.log('User agent:', navigator.userAgent);
    console.log('=== End Tauri API Test ===');
}

// Initialize app
async function initApp() {
    console.log('🚀 ========================================');
    console.log('🚀   Nostr Mail - Starting Application');
    console.log('🚀 ========================================');
    console.log('📧 Email + 🔐 Nostr Integration');
    console.log('🌐 Version: 1.0.0');
    console.log('⏰ Started at:', new Date().toLocaleString());
    console.log('🚀 ========================================');
    
    try {
        console.log('📋 Loading application settings...');
        // Load settings from localStorage
        loadSettings();

        console.log('🌐 Loading relay configuration...');
        // Load relays from localStorage if present, else from backend
        loadRelaysFromStorageOrBackend();

        console.log('🔑 Loading/generating keypair...');
        // Generate or load keypair
        await loadKeypair();
        
        console.log('🎯 Setting up event listeners...');
        // Set up event listeners
        setupEventListeners();
        
        console.log('📬 Loading initial data...');
        // Load initial data
        // Load contacts first so DM contacts can access cached profile photos
        await loadContacts();
        await loadEmails();
        await loadDmContacts();
        
        console.log('✅ ========================================');
        console.log('✅   Nostr Mail - Successfully Started!');
        console.log('✅ ========================================');
        console.log('🎉 Application is ready for use');
        console.log('📱 UI: Modern email client with Nostr integration');
        console.log('🔐 Features: Email, DMs, Contacts, Profile Management');
        console.log('✅ ========================================');
    } catch (error) {
        console.error('❌ ========================================');
        console.error('❌   Nostr Mail - Startup Failed!');
        console.error('❌ ========================================');
        console.error('💥 Error during initialization:', error);
        console.error('❌ ========================================');
    }
}

function saveRelaysToLocalStorage() {
    localStorage.setItem('nostr_mail_relays', JSON.stringify(appState.relays));
}

function loadRelaysFromStorageOrBackend() {
    const stored = localStorage.getItem('nostr_mail_relays');
    if (stored) {
        try {
            appState.relays = JSON.parse(stored);
            // Sync to backend
            tauriInvoke('set_relays', { relays: appState.relays });
        } catch (e) {
            console.error('Failed to parse relays from localStorage:', e);
            loadRelays(); // fallback to backend
        }
    } else {
        loadRelays(); // fallback to backend
    }
}

// Event listeners setup with error handling
function setupEventListeners() {
    console.log('Setting up event listeners...');
    
    try {
        // Navigation
        if (elements.navItems && elements.navItems.length > 0) {
            elements.navItems.forEach(item => {
                item.addEventListener('click', () => switchTab(item.dataset.tab));
            });
        }
        
        // Modal
        if (elements.modalClose) {
            elements.modalClose.addEventListener('click', hideModal);
        }
        if (elements.modalOverlay) {
            elements.modalOverlay.addEventListener('click', (e) => {
                if (e.target === elements.modalOverlay) hideModal();
            });
        }
        
        // Compose form
        if (elements.sendBtn) {
            console.log('[JS] Setting up send button event listener');
            elements.sendBtn.addEventListener('click', sendEmail);
        }
        if (elements.saveDraftBtn) {
            elements.saveDraftBtn.addEventListener('click', saveDraft);
        }
        
        // Inbox
        if (elements.refreshInbox) {
            elements.refreshInbox.addEventListener('click', async () => {
                // Clear search input
                if (elements.emailSearch) {
                    elements.emailSearch.value = '';
                }
                // Load all emails (no search filter)
                await loadEmails();
            });
        }
        
        // Back to inbox button
        const backToInboxBtn = getElement('back-to-inbox');
        if (backToInboxBtn) {
            backToInboxBtn.addEventListener('click', showEmailList);
        }
        
        // Email Search
        if (elements.emailSearch) {
            elements.emailSearch.addEventListener('input', filterEmails);
        }
        
        // DM functionality
        if (elements.sendDmBtn) {
            elements.sendDmBtn.addEventListener('click', sendDirectMessage);
        }
        if (elements.newDmBtn) {
            elements.newDmBtn.addEventListener('click', showNewDmCompose);
        }
        if (elements.refreshDm) {
            elements.refreshDm.addEventListener('click', refreshDmConversations);
        }
        
        // DM Search
        if (elements.dmSearch) {
            elements.dmSearch.addEventListener('input', filterDmContacts);
        }
        
        // DM Search Toggle
        if (elements.dmSearchToggle) {
            elements.dmSearchToggle.addEventListener('click', toggleDmSearch);
        }
        
        // Contacts Search
        if (elements.contactsSearch) {
            elements.contactsSearch.addEventListener('input', filterContacts);
        }
        
        // Contacts Search Toggle
        if (elements.contactsSearchToggle) {
            elements.contactsSearchToggle.addEventListener('click', toggleContactsSearch);
        }
        
        // Contacts
        if (elements.addContactBtn) {
            elements.addContactBtn.addEventListener('click', showAddContactModal);
        }
        
        // Profile
        if (elements.updateProfileBtn) {
            elements.updateProfileBtn.addEventListener('click', updateProfile);
        }
        
        // Relays
        if (elements.addRelayBtn) {
            elements.addRelayBtn.addEventListener('click', addRelay);
        }
        
        // Settings
        if (elements.saveSettingsBtn) {
            elements.saveSettingsBtn.addEventListener('click', saveSettings);
        }
        if (elements.testConnectionBtn) {
            elements.testConnectionBtn.addEventListener('click', testConnection);
        }
        if (elements.testEmailConnectionBtn) {
            elements.testEmailConnectionBtn.addEventListener('click', testEmailConnection);
        }
        if (elements.generateKeyBtn) {
            elements.generateKeyBtn.addEventListener('click', generateNewKeypair);
        }
        if (elements.copyPubkeyBtn) {
            elements.copyPubkeyBtn.addEventListener('click', copyPublicKey);
        }
        if (elements.copyNprivBtn) {
            elements.copyNprivBtn.addEventListener('click', copyNprivKey);
        }
        if (elements.copyEmailPasswordBtn) {
            elements.copyEmailPasswordBtn.addEventListener('click', copyEmailPassword);
        }
        if (elements.toggleNprivVisibilityBtn) {
            elements.toggleNprivVisibilityBtn.addEventListener('click', toggleNprivVisibility);
        }
        if (elements.toggleEmailPasswordVisibilityBtn) {
            elements.toggleEmailPasswordVisibilityBtn.addEventListener('click', toggleEmailPasswordVisibility);
        }
        if (elements.nprivKey) {
            elements.nprivKey.addEventListener('input', updatePublicKeyDisplay);
        }
        
        // Email provider selection
        if (elements.emailProvider) {
            elements.emailProvider.addEventListener('change', handleEmailProviderChange);
        }
        
        // Contacts
        if (elements.refreshContactsBtn) {
            elements.refreshContactsBtn.addEventListener('click', refreshContacts);
        }
        
        console.log('Event listeners set up successfully');
    } catch (error) {
        console.error('Error setting up event listeners:', error);
    }
}

// Tab switching
function switchTab(tabName) {
    if (elements.tabContents) {
        elements.tabContents.forEach(tab => {
            tab.classList.remove('active');
        });
    }
    
    const newTab = getElement(tabName);
    if (newTab) {
        newTab.classList.add('active');
    }
    
    if (elements.navItems) {
        elements.navItems.forEach(item => {
            item.classList.remove('active');
            if (item.dataset.tab === tabName) {
                item.classList.add('active');
            }
        });
    }

    if (tabName === 'profile') {
        loadProfile();
    }
    if (tabName === 'settings') {
        loadRelays();
    }
    if (tabName === 'contacts') {
        // Only load contacts if they haven't been loaded yet
        if (!appState.contacts || appState.contacts.length === 0) {
            loadContacts();
        } else {
            // Just render the existing contacts
            renderContacts();
        }
    }
}

// Modal functions
function showModal(title, content) {
    try {
        if (elements.modalTitle) elements.modalTitle.textContent = title;
        if (elements.modalBody) elements.modalBody.innerHTML = content;
        if (elements.modalOverlay) elements.modalOverlay.classList.remove('hidden');
    } catch (error) {
        console.error('Error showing modal:', error);
    }
}

function hideModal() {
    try {
        if (elements.modalOverlay) elements.modalOverlay.classList.add('hidden');
    } catch (error) {
        console.error('Error hiding modal:', error);
    }
}

// Keypair management
async function loadKeypair() {
    try {
        const stored = localStorage.getItem('nostr_keypair');
        if (stored) {
            appState.keypair = JSON.parse(stored);
        } else {
            appState.keypair = await tauriInvoke('generate_keypair');
            localStorage.setItem('nostr_keypair', JSON.stringify(appState.keypair));
        }
        console.log('Keypair loaded:', appState.keypair.public_key.substring(0, 20) + '...');
        renderProfilePubkey();
    } catch (error) {
        console.error('Failed to load keypair:', error);
        showError('Failed to load encryption keys');
    }
}

// DM Functions
async function loadDmContacts() {
    console.log('[JS] loadDmContacts called - starting DM loading...');
    
    if (!appState.keypair) {
        showError('No keypair available');
        return;
    }

    // Load cached contacts to get profile information (needed for both cached and network data)
    let cachedContacts = [];
    try {
        cachedContacts = await tauriInvoke('get_contacts');
        if (cachedContacts && cachedContacts.length > 0) {
            console.log(`[JS] Loaded ${cachedContacts.length} cached contacts for DM profiles`);
            
            // Debug: Check if Aria and EllyPembroke have picture_data_url
            const ariaContact = cachedContacts.find(c => c.name === 'Aria' || c.name?.includes('Aria'));
            const ellyContact = cachedContacts.find(c => c.name === 'EllyPembroke' || c.name?.includes('EllyPembroke'));
            
            if (ariaContact) {
                console.log('[JS] Aria contact from backend:', {
                    name: ariaContact.name,
                    pubkey: ariaContact.pubkey,
                    hasPictureDataUrl: !!ariaContact.picture_data_url,
                    pictureDataUrlLength: ariaContact.picture_data_url?.length || 0
                });
            }
            if (ellyContact) {
                console.log('[JS] EllyPembroke contact from backend:', {
                    name: ellyContact.name,
                    pubkey: ellyContact.pubkey,
                    hasPictureDataUrl: !!ellyContact.picture_data_url,
                    pictureDataUrlLength: ellyContact.picture_data_url?.length || 0
                });
            }
            
            // Debug: Show all cached contact names and pubkeys
            console.log('[JS] All cached contact names:', cachedContacts.map(c => ({ name: c.name, pubkey: c.pubkey })));
        }
    } catch (e) {
        console.warn('Failed to load cached contacts for DM profiles:', e);
    }

    // Try to load from backend storage first for instant display
    let cacheLoaded = false;
    try {
        console.log('[JS] Loading DM conversations from backend storage...');
        const cachedData = await tauriInvoke('get_conversations');
        
        if (cachedData && cachedData.length > 0) {
            console.log('[JS] Found cached DM conversations in backend, rendering immediately...');
            
            // Convert conversations to the format expected by the UI, using cached contact profiles
            appState.dmContacts = cachedData.map(conv => {
                // Try to find this contact in the cached profiles
                const cachedContact = cachedContacts.find(c => c.pubkey === conv.contact_pubkey);
                
                return {
                    pubkey: conv.contact_pubkey,
                    name: cachedContact?.name || conv.contact_name || conv.contact_pubkey.substring(0, 16) + '...',
                    lastMessage: conv.last_message,
                    lastMessageTime: new Date(conv.last_timestamp * 1000),
                    messageCount: conv.message_count,
                    picture_data_url: cachedContact?.picture_data_url || cachedContact?.picture || null,
                    profileLoaded: cachedContact !== undefined
                };
            });
            
            // Load messages from cached data
            appState.dmMessages = {};
            cachedData.forEach(conv => {
                if (conv.messages && conv.messages.length > 0) {
                    // Preserve existing local cache for this contact
                    const existingMessages = appState.dmMessages[conv.contact_pubkey] || [];
                    
                    appState.dmMessages[conv.contact_pubkey] = conv.messages.map(msg => {
                        // Check if this message exists in local cache
                        const existingMessage = existingMessages.find(existing => existing.id === msg.id);
                        
                        return {
                            id: msg.id,
                            content: msg.content,
                            created_at: msg.timestamp,
                            pubkey: msg.sender_pubkey,
                            is_sent: msg.is_sent,
                            // Preserve local confirmation status if it exists, otherwise use network status
                            confirmed: existingMessage ? existingMessage.confirmed : msg.is_sent
                        };
                    });
                }
            });
            
            // Sort by most recent message
            appState.dmContacts.sort((a, b) => b.lastMessageTime - a.lastMessageTime);
            
            renderDmContacts();
            cacheLoaded = true;
            
            console.log('[JS] DM conversations loaded from backend storage with profile data');
        } else {
            console.log('[JS] No cached DM conversations found in backend storage.');
        }
    } catch (e) {
        console.warn('Failed to load cached DM conversations from backend:', e);
    }
    
    // Try to fetch fresh data from network (but don't fail if offline)
    try {
        const activeRelays = getActiveRelays();
        if (activeRelays.length === 0) {
            if (!cacheLoaded) {
                showError('No active relays configured');
            }
            return;
        }

        console.log('🔄 Loading conversations from network...');
        
        // Fetch conversations from Nostr
        const conversations = await tauriInvoke('fetch_conversations', {
            privateKey: appState.keypair.private_key,
            relays: activeRelays
        });
        
        console.log('[JS] Network response:', {
            conversationsReceived: !!conversations,
            conversationsLength: conversations?.length || 0
        });
        
        // Only update if we actually got conversations from the network
        if (conversations && conversations.length > 0) {
            // Convert conversations to the format expected by the UI, using cached contact profiles
            appState.dmContacts = conversations.map(conv => {
                // Try to find this contact in the cached profiles
                const cachedContact = cachedContacts.find(c => c.pubkey === conv.contact_pubkey);
                
                // Store the messages from the conversation data
                if (conv.messages && conv.messages.length > 0) {
                    // Preserve existing local cache for this contact
                    const existingMessages = appState.dmMessages[conv.contact_pubkey] || [];
                    
                    appState.dmMessages[conv.contact_pubkey] = conv.messages.map(msg => {
                        // Check if this message exists in local cache
                        const existingMessage = existingMessages.find(existing => existing.id === msg.id);
                        
                        return {
                            id: msg.id,
                            content: msg.content,
                            created_at: msg.timestamp,
                            pubkey: msg.sender_pubkey,
                            is_sent: msg.is_sent,
                            // Preserve local confirmation status if it exists, otherwise use network status
                            confirmed: existingMessage ? existingMessage.confirmed : msg.is_sent
                        };
                    });
                }
                
                return {
                    pubkey: conv.contact_pubkey,
                    name: cachedContact?.name || conv.contact_name || conv.contact_pubkey.substring(0, 16) + '...',
                    lastMessage: conv.last_message,
                    lastMessageTime: new Date(conv.last_timestamp * 1000),
                    messageCount: conv.message_count,
                    picture_data_url: cachedContact?.picture_data_url || cachedContact?.picture || null,
                    profileLoaded: cachedContact !== undefined
                };
            });
            
            // Sort by most recent message
            appState.dmContacts.sort((a, b) => b.lastMessageTime - a.lastMessageTime);
            
            // Render contacts immediately
            renderDmContacts();
            
            // Only load profiles for contacts that aren't already cached
            const uncachedContacts = appState.dmContacts.filter(contact => !contact.profileLoaded);
            if (uncachedContacts.length > 0) {
                console.log(`[JS] Loading profiles for ${uncachedContacts.length} uncached DM contacts`);
                await loadDmContactProfiles();
            } else {
                console.log('[JS] All DM contacts already have cached profiles');
            }
            
            console.log(`✅ Loaded ${appState.dmContacts.length} conversations from network`);
            
            // Write to backend storage after successful load
            try {
                // Add cached_at field to conversations before saving
                const conversationsWithTimestamp = conversations.map(conv => ({
                    ...conv,
                    cached_at: new Date().toISOString()
                }));
                
                await tauriInvoke('set_conversations', { conversations: conversationsWithTimestamp });
                console.log('[JS] Cached DM conversations in backend storage');
            } catch (e) {
                console.warn('Failed to cache DM conversations in backend:', e);
            }
        } else {
            console.log('[JS] No conversations received from network, keeping cached data if available');
            if (!cacheLoaded) {
                appState.dmContacts = [];
                appState.dmMessages = {};
                renderDmContacts();
            }
        }
        
    } catch (error) {
        console.error('Failed to load DM contacts from network:', error);
        if (!cacheLoaded) {
            showError('Failed to load conversations and no cached data available');
            appState.dmContacts = [];
            appState.dmMessages = {};
            renderDmContacts();
        } else {
            console.log('[JS] Network failed, but using cached data');
        }
    }
}

// Filter DM contacts based on search query
function filterDmContacts() {
    const searchQuery = elements.dmSearch.value.toLowerCase().trim();
    renderDmContacts(searchQuery);
}

// Toggle DM search visibility
function toggleDmSearch() {
    if (elements.dmSearchContainer) {
        const isVisible = elements.dmSearchContainer.style.display !== 'none';
        elements.dmSearchContainer.style.display = isVisible ? 'none' : 'block';
        
        if (!isVisible) {
            // Focus the search input when showing it
            setTimeout(() => {
                if (elements.dmSearch) {
                    elements.dmSearch.focus();
                }
            }, 100);
        } else {
            // Clear search when hiding it
            if (elements.dmSearch) {
                elements.dmSearch.value = '';
                filterDmContacts(); // Reset to show all contacts
            }
        }
    }
}

function renderDmContacts(searchQuery = '') {
    if (!elements.dmContacts) return;
    
    try {
        elements.dmContacts.innerHTML = '';
        
        // Filter contacts based on search query
        let filteredContacts = appState.dmContacts;
        if (searchQuery) {
            filteredContacts = appState.dmContacts.filter(contact => 
                contact.name.toLowerCase().includes(searchQuery) ||
                contact.pubkey.toLowerCase().includes(searchQuery) ||
                contact.lastMessage.toLowerCase().includes(searchQuery)
            );
        }
        
        if (filteredContacts.length === 0) {
            const message = searchQuery 
                ? `No contacts found matching "${searchQuery}"`
                : 'No conversations yet';
            elements.dmContacts.innerHTML = `<div class="text-center text-muted">${message}</div>`;
            return;
        }
        
        filteredContacts.forEach(contact => {
            const contactElement = document.createElement('div');
            contactElement.className = 'dm-contact-item';
            contactElement.dataset.pubkey = contact.pubkey;
            
            // Format the last message time
            const timeAgo = formatTimeAgo(contact.lastMessageTime);
            
            // Create preview text
            let previewText = contact.lastMessage;
            if (previewText.length > 50) {
                previewText = previewText.substring(0, 50) + '...';
            }
            
            // Create avatar or use picture if available
            let avatarHtml = '';
            if (contact.picture_data_url) {
                // Use cached data URL for offline support
                avatarHtml = `<img src="${contact.picture_data_url}" alt="${contact.name}" class="contact-avatar" onerror="this.style.display='none'">`;
                console.log(`[JS] Using cached data URL for DM contact ${contact.name}`);
            } else {
                // Use placeholder avatar - don't try to load from URL when offline
                avatarHtml = `<div class="contact-avatar-placeholder">${contact.name.charAt(0).toUpperCase()}</div>`;
                console.log(`[JS] Using placeholder avatar for DM contact ${contact.name} (no cached image available)`);
            }
            
            contactElement.innerHTML = `
                ${avatarHtml}
                <div class="dm-contact-content">
                    <div class="dm-contact-header">
                        <div class="dm-contact-name">${contact.name}</div>
                        <div class="dm-contact-time">${timeAgo}</div>
                    </div>
                    <div class="dm-contact-preview">${previewText}</div>
                    <div class="dm-contact-meta">
                        <span class="dm-message-count">${contact.messageCount} message${contact.messageCount !== 1 ? 's' : ''}</span>
                    </div>
                </div>
            `;
            
            contactElement.addEventListener('click', () => selectDmContact(contact));
            elements.dmContacts.appendChild(contactElement);
        });
    } catch (error) {
        console.error('Error rendering DM contacts:', error);
    }
}

// New function to load profiles for DM contacts
async function loadDmContactProfiles() {
    // Only process contacts that don't already have profiles loaded
    const uncachedContacts = appState.dmContacts.filter(contact => !contact.profileLoaded);
    
    if (uncachedContacts.length === 0) {
        console.log('[JS] All DM contacts already have profiles loaded');
        return;
    }
    
    try {
        const activeRelays = getActiveRelays();
        if (activeRelays.length === 0) return;
        
        console.log(`[JS] Loading profiles for ${uncachedContacts.length} uncached DM contacts`);
        
        // Fetch profiles for uncached DM contacts
        const pubkeys = uncachedContacts.map(contact => contact.pubkey);
        const profiles = await tauriInvoke('fetch_profiles', {
            pubkeys: pubkeys,
            relays: activeRelays
        });
        
        // Update contacts with profile information
        for (const profile of profiles) {
            const contactIndex = appState.dmContacts.findIndex(c => c.pubkey === profile.pubkey);
            if (contactIndex !== -1) {
                const contact = appState.dmContacts[contactIndex];
                contact.name = profile.fields.name || profile.fields.display_name || contact.pubkey.substring(0, 16) + '...';
                contact.picture = profile.fields.picture || null;
                contact.profileLoaded = true;
                
                // Try to cache the profile picture as a data URL for offline use
                if (contact.picture) {
                    try {
                        // First try to get from backend cache
                        let dataUrl = await getCachedProfileImageFromBackend(contact.pubkey);
                        
                        // If not in cache, fetch and cache it
                        if (!dataUrl) {
                            dataUrl = await fetchImageAsDataUrl(contact.picture);
                            if (dataUrl) {
                                // Cache in backend
                                await cacheProfileImageInBackend(contact.pubkey, dataUrl);
                            }
                        }
                        
                        if (dataUrl) {
                            contact.picture_data_url = dataUrl;
                            console.log(`[JS] Cached profile picture for ${contact.name}`);
                        }
                    } catch (e) {
                        console.warn(`Failed to cache profile picture for ${contact.name}:`, e);
                    }
                }
                
                // Update the contact in the array
                appState.dmContacts[contactIndex] = contact;
            }
        }
        
        // Re-render with updated names and pictures
        renderDmContacts();
        
        // Update the DM cache with the new profile data in backend storage
        try {
            // Get current conversations from backend
            const currentConversations = await tauriInvoke('get_conversations');
            if (currentConversations && currentConversations.length > 0) {
                // Update conversations with new profile data
                const updatedConversations = currentConversations.map(conv => {
                    const updatedContact = appState.dmContacts.find(c => c.pubkey === conv.contact_pubkey);
                    if (updatedContact) {
                        return {
                            ...conv,
                            contact_name: updatedContact.name,
                            cached_at: new Date().toISOString()
                        };
                    }
                    return conv;
                });
                
                await tauriInvoke('set_conversations', { conversations: updatedConversations });
                console.log('[JS] Updated DM conversations in backend storage with profile data');
            }
        } catch (e) {
            console.warn('Failed to update DM conversations in backend storage:', e);
        }
        
        console.log(`[JS] Updated ${profiles.length} DM contact profiles`);
        
    } catch (error) {
        console.error('Failed to load DM contact profiles:', error);
        // Don't show error to user as this is just for display enhancement
    }
}

function selectDmContact(contact) {
    try {
        appState.selectedDmContact = contact;
        
        // Update UI
        document.querySelectorAll('.dm-contact-item').forEach(item => {
            item.classList.remove('active');
        });
        
        const contactElement = document.querySelector(`[data-pubkey="${contact.pubkey}"]`);
        if (contactElement) {
            contactElement.classList.add('active');
        }
        
        // Load messages for this contact
        loadDmMessages(contact.pubkey);
        
    } catch (error) {
        console.error('Error selecting DM contact:', error);
    }
}

async function loadDmMessages(contactPubkey) {
    if (!appState.keypair) {
        showError('No keypair available');
        return;
    }
    
    // Check if messages are already cached
    if (appState.dmMessages[contactPubkey] && appState.dmMessages[contactPubkey].length > 0) {
        console.log(`[JS] Using cached messages for ${contactPubkey}`);
        renderDmMessages(contactPubkey);
        return;
    }
    
    try {
        const activeRelays = getActiveRelays();
        if (activeRelays.length === 0) {
            showError('No active relays configured');
            return;
        }

        console.log(`🔄 Loading messages for ${contactPubkey}...`);
        
        // Fetch conversation messages from Nostr
        const messages = await tauriInvoke('fetch_conversation_messages', {
            privateKey: appState.keypair.private_key,
            contactPubkey: contactPubkey,
            relays: activeRelays
        });
        
        // Convert to the format expected by the UI
        const formattedMessages = messages.map(msg => ({
            id: msg.id,
            content: msg.content,
            created_at: msg.timestamp,
            pubkey: msg.sender_pubkey,
            is_sent: msg.is_sent,
            confirmed: msg.is_sent // If we can fetch it from network, it's confirmed
        }));
        
        appState.dmMessages[contactPubkey] = formattedMessages;
        renderDmMessages(contactPubkey);
        
        console.log(`✅ Loaded ${formattedMessages.length} messages`);
        
        // Write to backend storage after loading messages
        try {
            // Get current conversations from backend
            const currentConversations = await tauriInvoke('get_conversations');
            if (currentConversations && currentConversations.length > 0) {
                // Update the specific conversation with new messages
                const updatedConversations = currentConversations.map(conv => {
                    if (conv.contact_pubkey === contactPubkey) {
                        return {
                            ...conv,
                            messages: formattedMessages.map(msg => ({
                                id: msg.id,
                                content: msg.content,
                                timestamp: msg.created_at,
                                sender_pubkey: msg.pubkey,
                                receiver_pubkey: contactPubkey,
                                is_sent: msg.is_sent,
                                confirmed: msg.confirmed
                            })),
                            cached_at: new Date().toISOString()
                        };
                    }
                    return conv;
                });
                
                await tauriInvoke('set_conversations', { conversations: updatedConversations });
                console.log('[JS] Updated DM messages in backend storage');
            }
        } catch (e) {
            console.warn('Failed to update DM messages in backend storage:', e);
        }
        
    } catch (error) {
        console.error('Failed to load DM messages:', error);
        showError('Failed to load messages');
    }
}

function renderDmMessages(contactPubkey) {
    if (!elements.dmMessages) return;
    try {
        const messages = appState.dmMessages[contactPubkey] || [];
        const contact = appState.dmContacts.find(c => c.pubkey === contactPubkey);
        
        console.log('[JS] renderDmMessages called for contact:', contactPubkey);
        console.log('[JS] Number of messages to render:', messages.length);
        console.log('[JS] Messages:', messages);
        
        elements.dmMessages.innerHTML = '';
        
        if (messages.length === 0) {
            elements.dmMessages.innerHTML = `
                <div class="text-center text-muted" style="padding: 2rem;">
                    <i class="fas fa-comments" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
                    <p>No messages yet</p>
                    <p>Start a conversation with ${contact ? contact.name : 'this contact'}!</p>
                </div>
            `;
        } else {
            // Create conversation header
            const headerElement = document.createElement('div');
            headerElement.className = 'conversation-header';
            headerElement.innerHTML = `
                <div class="conversation-contact-info">
                    <div class="conversation-contact-name">${contact ? contact.name : contactPubkey}</div>
                    <div class="conversation-contact-pubkey">${contactPubkey}</div>
                </div>
            `;
            elements.dmMessages.appendChild(headerElement);
            
            // Create messages container
            const messagesContainer = document.createElement('div');
            messagesContainer.className = 'messages-container';
            
            // Sort messages from oldest to newest (top to bottom)
            const sortedMessages = [...messages].sort((a, b) => a.created_at - b.created_at);
            
            console.log('[JS] Sorted messages:', sortedMessages);
            
            sortedMessages.forEach((message, index) => {
                console.log(`[JS] Rendering message ${index}:`, message);
                
                const messageElement = document.createElement('div');
                messageElement.className = `message ${message.is_sent ? 'message-sent' : 'message-received'}`;
                
                const time = new Date(message.created_at * 1000).toLocaleTimeString([], { 
                    hour: '2-digit', 
                    minute: '2-digit' 
                });
                
                // Add checkmark for sent messages
                let statusIcon = '';
                if (message.is_sent) {
                    if (message.confirmed) {
                        // Double checkmark for confirmed messages
                        statusIcon = '<i class="fas fa-check-double message-status confirmed"></i>';
                    } else if (message.id && !message.id.startsWith('temp_')) {
                        // Single checkmark for sent but not yet confirmed
                        statusIcon = '<i class="fas fa-check message-status sent"></i>';
                    } else {
                        // Clock icon for pending messages (temporary IDs)
                        statusIcon = '<i class="fas fa-clock message-status pending"></i>';
                    }
                }
                
                messageElement.innerHTML = `
                    <div class="message-content">
                        <div class="message-text">${escapeHtml(message.content)}</div>
                        <div class="message-meta">
                            <div class="message-time">${time}</div>
                            ${statusIcon}
                        </div>
                    </div>
                `;
                
                messagesContainer.appendChild(messageElement);
            });
            
            elements.dmMessages.appendChild(messagesContainer);
            
            // Scroll to bottom to show the newest messages
            messagesContainer.scrollTop = messagesContainer.scrollHeight;
        }
        
        // Add message input box at the bottom
        const messageInputContainer = document.createElement('div');
        messageInputContainer.className = 'dm-message-input-container';
        messageInputContainer.innerHTML = `
            <div class="dm-message-input-wrapper">
                <input type="text" id="dm-reply-input" class="dm-message-input" placeholder="Type your message..." maxlength="1000">
                <button id="dm-send-btn" class="dm-send-btn">
                    <i class="fas fa-paper-plane"></i>
                </button>
            </div>
        `;
        elements.dmMessages.appendChild(messageInputContainer);
        
        // Add event listeners for the new input elements
        const replyInput = document.getElementById('dm-reply-input');
        const sendBtn = document.getElementById('dm-send-btn');
        
        if (replyInput && sendBtn) {
            // Send message on Enter key
            replyInput.addEventListener('keypress', (e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    sendReplyMessage(contactPubkey);
                }
            });
            
            // Send message on button click
            sendBtn.addEventListener('click', () => {
                sendReplyMessage(contactPubkey);
            });
            
            // Focus the input
            setTimeout(() => {
                replyInput.focus();
            }, 100);
        }
        
        console.log('[JS] renderDmMessages completed successfully');
        
    } catch (error) {
        console.error('Error rendering DM messages:', error);
    }
}

// Helper function to escape HTML
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

async function sendDirectMessage(recipientPubkey, message) {
    if (!recipientPubkey || !message) {
        showError('Recipient and message are required');
        return;
    }
    
    if (!appState.keypair) {
        showError('No keypair available');
        return;
    }

    const activeRelays = getActiveRelays();
    if (activeRelays.length === 0) {
        showError('No active relays configured');
        return;
    }
    
    try {
        // Disable the send button while sending
        const sendBtn = document.getElementById('dm-send-btn');
        if (sendBtn) {
            sendBtn.disabled = true;
            sendBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Sending...';
        }
        
        const result = await tauriInvoke('send_direct_message', {
            privateKey: appState.keypair.private_key,
            recipientPubkey,
            message,
            relays: activeRelays
        });
        
        console.log('[JS] Backend returned result:', result);
        
        // Re-enable the send button
        if (sendBtn) {
            sendBtn.disabled = false;
            sendBtn.innerHTML = '<i class="fas fa-paper-plane"></i> Send';
        }
        
        // Clear the message input
        const messageInput = document.getElementById('dm-message');
        if (messageInput) {
            messageInput.value = '';
        }
        
        // Show success message
        showSuccess('DM sent successfully');
        
        // Refresh the conversation list to show the new message
        await loadDmContacts();
        
        // Start checking for message confirmation if we got an event ID
        if (result && !result.startsWith('temp_')) {
            checkMessageConfirmation(recipientPubkey, result);
        }
        
        // Save the new conversation to backend storage
        try {
            // Get current conversations from backend
            const currentConversations = await tauriInvoke('get_conversations');
            const newConversation = {
                contact_pubkey: recipientPubkey,
                contact_name: null, // Will be updated when profile is loaded
                last_message: message,
                last_timestamp: Math.floor(Date.now() / 1000),
                message_count: 1,
                messages: [{
                    id: result || `temp_${Date.now()}`,
                    content: message,
                    timestamp: Math.floor(Date.now() / 1000),
                    sender_pubkey: appState.keypair.public_key,
                    receiver_pubkey: recipientPubkey,
                    is_sent: true,
                    confirmed: false
                }],
                cached_at: new Date().toISOString()
            };
            
            const updatedConversations = [...(currentConversations || []), newConversation];
            await tauriInvoke('set_conversations', { conversations: updatedConversations });
            console.log('[JS] Saved new conversation to backend storage');
        } catch (e) {
            console.warn('Failed to save new conversation to backend storage:', e);
        }
        
    } catch (error) {
        console.error('Error in sendDirectMessage:', error);
        showError('Failed to send DM');
        
        // Re-enable the send button on error
        const sendBtn = document.getElementById('dm-send-btn');
        if (sendBtn) {
            sendBtn.disabled = false;
            sendBtn.innerHTML = '<i class="fas fa-paper-plane"></i> Send';
        }
    }
}

async function sendReplyMessage(contactPubkey) {
    const replyInput = document.getElementById('dm-reply-input');
    const message = replyInput?.value?.trim() || '';
    
    if (!message) {
        showError('Message cannot be empty');
        return;
    }
    
    if (!appState.keypair) {
        showError('No keypair available');
        return;
    }

    const activeRelays = getActiveRelays();
    if (activeRelays.length === 0) {
        showError('No active relays configured');
        return;
    }
    
    try {
        console.log('[JS] Sending reply message to:', contactPubkey);
        console.log('[JS] Message content:', message);
        console.log('[JS] Active relays:', activeRelays);
        
        // Disable the send button and input while sending
        const sendBtn = document.getElementById('dm-send-btn');
        if (sendBtn) {
            sendBtn.disabled = true;
            sendBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i>';
        }
        if (replyInput) {
            replyInput.disabled = true;
        }
        
        const result = await tauriInvoke('send_direct_message', {
            privateKey: appState.keypair.private_key,
            recipientPubkey: contactPubkey,
            message,
            relays: activeRelays
        });
        
        console.log('[JS] Backend returned result:', result);
        console.log('[JS] Result type:', typeof result);
        
        // Clear the message input
        if (replyInput) {
            replyInput.value = '';
            replyInput.disabled = false;
        }
        
        // Re-enable the send button
        if (sendBtn) {
            sendBtn.disabled = false;
            sendBtn.innerHTML = '<i class="fas fa-paper-plane"></i>';
        }
        
        // Immediately add the sent message to the local cache
        const newMessage = {
            id: result || `temp_${Date.now()}`, // Backend now returns event ID directly
            content: message,
            created_at: Math.floor(Date.now() / 1000),
            pubkey: appState.keypair.public_key,
            is_sent: true,
            confirmed: false // Will be updated when confirmed
        };
        
        console.log('[JS] Created new message object:', newMessage);
        
        // Initialize the messages array if it doesn't exist
        if (!appState.dmMessages[contactPubkey]) {
            appState.dmMessages[contactPubkey] = [];
        }
        
        // Add the new message to the cache
        appState.dmMessages[contactPubkey].push(newMessage);
        
        console.log('[JS] Added message to cache. Total messages for this contact:', appState.dmMessages[contactPubkey].length);
        console.log('[JS] Current cache for this contact:', appState.dmMessages[contactPubkey]);
        
        // Save the updated conversation to backend storage
        try {
            // Get current conversations from backend
            const currentConversations = await tauriInvoke('get_conversations');
            if (currentConversations && currentConversations.length > 0) {
                // Update the specific conversation with the new message
                const updatedConversations = currentConversations.map(conv => {
                    if (conv.contact_pubkey === contactPubkey) {
                        return {
                            ...conv,
                            messages: appState.dmMessages[contactPubkey].map(msg => ({
                                id: msg.id,
                                content: msg.content,
                                timestamp: msg.created_at,
                                sender_pubkey: msg.pubkey,
                                receiver_pubkey: contactPubkey,
                                is_sent: msg.is_sent,
                                confirmed: msg.confirmed
                            })),
                            last_message: newMessage.content,
                            last_timestamp: newMessage.created_at,
                            message_count: appState.dmMessages[contactPubkey].length,
                            cached_at: new Date().toISOString()
                        };
                    }
                    return conv;
                });
                
                await tauriInvoke('set_conversations', { conversations: updatedConversations });
                console.log('[JS] Saved updated conversation to backend storage');
            }
        } catch (e) {
            console.warn('Failed to save updated conversation to backend storage:', e);
        }
        
        // Re-render the messages to show the new message immediately
        renderDmMessages(contactPubkey);
        
        // Refresh the conversation list to show the new message in the sidebar
        await loadDmContacts();
        
        // Start checking for message confirmation
        if (newMessage.id && !newMessage.id.startsWith('temp_')) {
            console.log('[JS] Starting confirmation check for message:', newMessage.id);
            checkMessageConfirmation(contactPubkey, newMessage.id);
        }
        
        showSuccess('Message sent successfully');
        
    } catch (error) {
        console.error('Error in sendReplyMessage:', error);
        showError('Failed to send message');
        
        // Re-enable the input and button on error
        if (replyInput) {
            replyInput.disabled = false;
        }
        const sendBtn = document.getElementById('dm-send-btn');
        if (sendBtn) {
            sendBtn.disabled = false;
            sendBtn.innerHTML = '<i class="fas fa-paper-plane"></i>';
        }
    }
}

// Function to check if a sent message has been observed on the network
async function checkMessageConfirmation(contactPubkey, messageId) {
    if (!appState.keypair) return;
    
    const activeRelays = getActiveRelays();
    if (activeRelays.length === 0) return;
    
    try {
        console.log('[JS] Checking message confirmation for:', messageId);
        console.log('[JS] Using relays:', activeRelays);
        
        // Check if the message has been observed on the network
        const isConfirmed = await tauriInvoke('check_message_confirmation', {
            eventId: messageId,
            relays: activeRelays
        });
        
        console.log('[JS] Confirmation result:', isConfirmed);
        
        if (isConfirmed) {
            console.log('[JS] Message confirmed! Updating cache...');
            
            // Update the message in the cache
            const messageIndex = appState.dmMessages[contactPubkey]?.findIndex(msg => msg.id === messageId);
            console.log('[JS] Found message at index:', messageIndex);
            
            if (messageIndex !== -1 && messageIndex !== undefined) {
                appState.dmMessages[contactPubkey][messageIndex].confirmed = true;
                console.log('[JS] Updated message to confirmed status');
                
                // Re-render to show the confirmation
                renderDmMessages(contactPubkey);
                console.log('[JS] Re-rendered messages to show confirmation');
                
                // Also update the backend storage with the confirmed status
                try {
                    const currentConversations = await tauriInvoke('get_conversations');
                    if (currentConversations && currentConversations.length > 0) {
                        const updatedConversations = currentConversations.map(conv => {
                            if (conv.contact_pubkey === contactPubkey) {
                                return {
                                    ...conv,
                                    messages: conv.messages.map(msg => 
                                        msg.id === messageId 
                                            ? { ...msg, confirmed: true }
                                            : msg
                                    ),
                                    cached_at: new Date().toISOString()
                                };
                            }
                            return conv;
                        });
                        
                        await tauriInvoke('set_conversations', { conversations: updatedConversations });
                        console.log('[JS] Updated backend storage with confirmed status');
                    }
                } catch (e) {
                    console.warn('Failed to update backend storage with confirmed status:', e);
                }
                
                console.log(`Message ${messageId} confirmed on network`);
            } else {
                console.warn('[JS] Message not found in cache for confirmation update');
            }
        } else {
            console.log('[JS] Message not yet confirmed, will retry in 3 seconds');
            // Retry after a delay if not confirmed yet
            setTimeout(() => {
                checkMessageConfirmation(contactPubkey, messageId);
            }, 3000); // Check again in 3 seconds
        }
    } catch (error) {
        console.warn('Error checking message confirmation:', error);
        // Retry after a delay on error
        setTimeout(() => {
            checkMessageConfirmation(contactPubkey, messageId);
        }, 5000); // Check again in 5 seconds
    }
}

function showNewDmCompose(recipientPubkey = null) {
    try {
        appState.selectedDmContact = null;
        
        // Clear any selected contact
        document.querySelectorAll('.dm-contact-item').forEach(item => {
            item.classList.remove('active');
        });
        
        // Show compose form instead of placeholder
        if (elements.dmMessages) {
            elements.dmMessages.innerHTML = `
                <div class="dm-compose-form" style="padding: 1rem;">
                    <div class="form-group">
                        <label for="dm-recipient">To:</label>
                        <input type="text" id="dm-recipient" placeholder="npub1..." value="${recipientPubkey || ''}" ${recipientPubkey ? 'readonly' : ''}>
                    </div>
                    
                    <div class="form-group">
                        <label for="dm-message">Message:</label>
                        <textarea id="dm-message" rows="8" placeholder="Type your message here..."></textarea>
                    </div>
                    
                    <div class="form-actions">
                        <button id="dm-send-btn" class="btn btn-primary">
                            <i class="fas fa-paper-plane"></i> Send
                        </button>
                        <button id="dm-cancel-btn" class="btn btn-secondary">
                            <i class="fas fa-times"></i> Cancel
                        </button>
                    </div>
                </div>
            `;
            
            // Set up event listeners for the new form
            const sendBtn = document.getElementById('dm-send-btn');
            const cancelBtn = document.getElementById('dm-cancel-btn');
            const recipientInput = document.getElementById('dm-recipient');
            const messageInput = document.getElementById('dm-message');
            
            if (sendBtn) {
                sendBtn.addEventListener('click', () => {
                    const recipient = recipientInput?.value?.trim();
                    const message = messageInput?.value?.trim();
                    
                    if (!recipient) {
                        showError('Please enter a recipient public key');
                        return;
                    }
                    
                    if (!message) {
                        showError('Please enter a message');
                        return;
                    }
                    
                    // Send the message using the new function
                    sendDirectMessage(recipient, message);
                });
            }
            
            if (cancelBtn) {
                cancelBtn.addEventListener('click', () => {
                    showNewDmCompose(); // Reset to placeholder
                });
            }
            
            // Focus the message input if recipient is pre-filled
            if (recipientPubkey && messageInput) {
                messageInput.focus();
            } else if (recipientInput) {
                recipientInput.focus();
            }
        }
        
    } catch (error) {
        console.error('Error showing new DM compose:', error);
    }
}

// Settings management
function loadSettings() {
    try {
        const stored = localStorage.getItem('nostr_mail_settings');
        if (stored) {
            appState.settings = JSON.parse(stored);
            populateSettingsForm();
        }
    } catch (error) {
        console.error('Error loading settings:', error);
    }
}

async function saveSettings() {
    try {
        // Validate npriv key if provided
        const nprivKey = elements.nprivKey?.value?.trim() || '';
        if (nprivKey && !nprivKey.startsWith('npriv1') && !nprivKey.startsWith('nsec1')) {
            showError('Invalid Nostr private key format. Should start with "npriv1" or "nsec1"');
            return;
        }
        
        const settings = {
            npriv_key: nprivKey,
            email_address: elements.emailAddress?.value || '',
            password: elements.emailPassword?.value || '',
            smtp_host: elements.smtpHost?.value || '',
            smtp_port: parseInt(elements.smtpPort?.value) || 587,
            imap_host: elements.imapHost?.value || '',
            imap_port: parseInt(elements.imapPort?.value) || 993,
            use_tls: elements.useTls?.checked || false
        };
        
        localStorage.setItem('nostr_mail_settings', JSON.stringify(settings));
        appState.settings = settings;
        appState.nprivKey = nprivKey;
        
        // If a private key is provided, update appState.keypair and localStorage
        if (nprivKey && (nprivKey.startsWith('npriv1') || nprivKey.startsWith('nsec1'))) {
            const isValid = await tauriInvoke('validate_private_key', { privateKey: nprivKey });
            if (!isValid) {
                showError('Invalid private key');
                return;
            }
            const publicKey = await tauriInvoke('get_public_key_from_private', { privateKey: nprivKey });
            appState.keypair = { private_key: nprivKey, public_key: publicKey };
            localStorage.setItem('nostr_keypair', JSON.stringify(appState.keypair));
            renderProfilePubkey();
            // If on profile tab, reload profile
            if (document.querySelector('.tab-content#profile.active')) {
                loadProfile();
            }
        }
        
        await saveRelays();
        saveRelaysToLocalStorage();
        showSuccess('Settings saved successfully');
    } catch (error) {
        console.error('Error saving settings:', error);
        showError('Failed to save settings');
    }
}

function populateSettingsForm() {
    if (!appState.settings) return;
    
    try {
        if (elements.nprivKey) elements.nprivKey.value = appState.settings.npriv_key || '';
        if (elements.emailAddress) elements.emailAddress.value = appState.settings.email_address || '';
        if (elements.emailPassword) elements.emailPassword.value = appState.settings.password || '';
        if (elements.smtpHost) elements.smtpHost.value = appState.settings.smtp_host || '';
        if (elements.smtpPort) elements.smtpPort.value = appState.settings.smtp_port || '';
        if (elements.imapHost) elements.imapHost.value = appState.settings.imap_host || '';
        if (elements.imapPort) elements.imapPort.value = appState.settings.imap_port || '';
        if (elements.useTls) elements.useTls.checked = appState.settings.use_tls || false;
        
        // Detect and set the email provider based on saved settings
        if (elements.emailProvider) {
            const provider = detectEmailProvider(appState.settings);
            elements.emailProvider.value = provider;
        }
        
        // Update public key display if npriv is available
        updatePublicKeyDisplay();
    } catch (error) {
        console.error('Error populating settings form:', error);
    }
}

// Function to detect email provider from saved settings
function detectEmailProvider(settings) {
    if (!settings.smtp_host || !settings.imap_host) return '';
    
    const smtpHost = settings.smtp_host.toLowerCase();
    const imapHost = settings.imap_host.toLowerCase();
    
    if (smtpHost.includes('gmail.com') && imapHost.includes('gmail.com')) {
        return 'gmail';
    } else if (smtpHost.includes('outlook.com') && imapHost.includes('office365.com')) {
        return 'outlook';
    } else if (smtpHost.includes('yahoo.com') && imapHost.includes('yahoo.com')) {
        return 'yahoo';
    } else {
        return 'custom';
    }
}

// Email functions
async function sendEmail() {
    console.log('[JS] sendEmail function called');
    console.log('[JS] appState.settings:', appState.settings);
    
    if (!appState.settings) {
        showError('Please configure your email settings first');
        return;
    }
    
    const toAddress = elements.toAddress?.value?.trim() || '';
    const subject = elements.subject?.value?.trim() || '';
    const body = elements.messageBody?.value?.trim() || '';
    
    console.log('[JS] Form values:', { toAddress, subject, body });
    
    if (!toAddress || !subject || !body) {
        console.log('[JS] Form validation failed - missing fields');
        showError('Please fill in all fields');
        return;
    }
    
    console.log('[JS] Form validation passed');
    
    // Check if using Gmail and warn about App Password
    if (appState.settings.smtp_host === 'smtp.gmail.com') {
        console.log('[JS] Gmail detected, checking for App Password warning');
        const isGmailAddress = appState.settings.email_address?.includes('@gmail.com');
        if (isGmailAddress) {
            // Show an informational message about App Passwords for Gmail (non-blocking)
            console.log('[JS] Showing Gmail App Password info message');
            showSuccess('Gmail detected: Make sure you\'re using an App Password, not your regular password. If you haven\'t set up an App Password, go to Google Account > Security > 2-Step Verification > App passwords.');
        }
    }
    
    console.log('[JS] About to enter try block');
    
    try {
        if (elements.sendBtn) {
            elements.sendBtn.disabled = true;
            elements.sendBtn.innerHTML = '<span class="loading"></span> Sending...';
        }
        
        // Determine TLS setting - automatically enable for Gmail if not set
        let useTls = appState.settings.use_tls;
        if (appState.settings.smtp_host === 'smtp.gmail.com' && !useTls) {
            console.log('[JS] Auto-enabling TLS for Gmail (was disabled)');
            useTls = true;
        }
        
        console.log('[JS] Email config debug:', {
            smtp_host: appState.settings.smtp_host,
            smtp_port: appState.settings.smtp_port,
            use_tls_setting: appState.settings.use_tls,
            use_tls_final: useTls,
            email: appState.settings.email_address
        });
        
        const emailConfig = {
            email_address: appState.settings.email_address,
            password: appState.settings.password,
            smtp_host: appState.settings.smtp_host,
            smtp_port: appState.settings.smtp_port,
            imap_host: appState.settings.imap_host,
            imap_port: appState.settings.imap_port,
            use_tls: useTls
        };
        
        console.log('[JS] About to call tauriInvoke send_email with config:', emailConfig);
        
        await tauriInvoke('send_email', {
            emailConfig: emailConfig,
            toAddress: toAddress,
            subject: subject,
            body: body
        });
        
        console.log('[JS] tauriInvoke send_email completed successfully');
        
        // Clear form
        if (elements.toAddress) elements.toAddress.value = '';
        if (elements.subject) elements.subject.value = '';
        if (elements.messageBody) elements.messageBody.value = '';
        
        showSuccess('Email sent successfully');
        
    } catch (error) {
        console.error('[JS] Error in sendEmail function:', error);
        console.error('[JS] Error stack:', error.stack);
        showError('Failed to send email: ' + error);
    } finally {
        if (elements.sendBtn) {
            elements.sendBtn.disabled = false;
            elements.sendBtn.innerHTML = '<i class="fas fa-paper-plane"></i> Send';
        }
    }
}

async function loadEmails(searchQuery = '') {
    if (!appState.settings) {
        showError('Please configure your email settings first');
        return;
    }
    
    try {
        if (elements.refreshInbox) {
            elements.refreshInbox.disabled = true;
            elements.refreshInbox.innerHTML = '<span class="loading"></span> Loading...';
        }
        
        const emailConfig = {
            email_address: appState.settings.email_address,
            password: appState.settings.password,
            smtp_host: appState.settings.smtp_host,
            smtp_port: appState.settings.smtp_port,
            imap_host: appState.settings.imap_host,
            imap_port: appState.settings.imap_port,
            use_tls: appState.settings.use_tls
        };
        
        // Pass the search query to the backend
        const searchParam = searchQuery.trim() || null;
        appState.emails = await tauriInvoke('fetch_emails', {
            emailConfig: emailConfig,
            limit: 10,
            searchQuery: searchParam
        });
        
        renderEmails();
        
    } catch (error) {
        console.error('Failed to load emails:', error);
        showError('Failed to load emails: ' + error);
    } finally {
        if (elements.refreshInbox) {
            elements.refreshInbox.disabled = false;
            elements.refreshInbox.innerHTML = '<i class="fas fa-sync"></i> Refresh';
        }
    }
}

function renderEmails() {
    if (!elements.emailList) return;
    
    try {
        elements.emailList.innerHTML = '';
        
        if (appState.emails.length === 0) {
            elements.emailList.innerHTML = '<div class="text-center text-muted">No emails found</div>';
            return;
        }
        
        appState.emails.forEach(email => {
            const emailElement = document.createElement('div');
            emailElement.className = `email-item ${!email.is_read ? 'unread' : ''}`;
            emailElement.dataset.emailId = email.id;
            
            // Format the date nicely
            const emailDate = new Date(email.date);
            const now = new Date();
            const diffInHours = (now - emailDate) / (1000 * 60 * 60);
            
            let dateDisplay;
            if (diffInHours < 24) {
                dateDisplay = emailDate.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            } else if (diffInHours < 168) { // 7 days
                dateDisplay = emailDate.toLocaleDateString([], { weekday: 'short' });
            } else {
                dateDisplay = emailDate.toLocaleDateString();
            }
            
            emailElement.innerHTML = `
                <div class="email-header">
                    <div class="email-sender">${escapeHtml(email.from)}</div>
                    <div class="email-date">${dateDisplay}</div>
                </div>
                <div class="email-subject">${escapeHtml(email.subject)}</div>
                <div class="email-preview">${escapeHtml(email.body.substring(0, 100))}${email.body.length > 100 ? '...' : ''}</div>
            `;
            
            emailElement.addEventListener('click', () => showEmailDetail(email.id));
            elements.emailList.appendChild(emailElement);
        });
    } catch (error) {
        console.error('Error rendering emails:', error);
    }
}

// Debounced search function
let searchTimeout;
function filterEmails() {
    const searchQuery = elements.emailSearch?.value?.trim() || '';
    
    // Clear existing timeout
    if (searchTimeout) {
        clearTimeout(searchTimeout);
    }
    
    // Set a new timeout to debounce the search
    searchTimeout = setTimeout(async () => {
        try {
            await loadEmails(searchQuery);
        } catch (error) {
            console.error('Error filtering emails:', error);
        }
    }, 1500); // 1.5 second delay - increased from 500ms
}

function showEmailDetail(emailId) {
    try {
        const email = appState.emails.find(e => e.id === emailId);
        if (!email) return;
        
        // Hide the email list and show the detail view
        const emailList = getElement('email-list');
        const emailDetailView = getElement('email-detail-view');
        const inboxActions = getElement('inbox-actions');
        const inboxTitle = getElement('inbox-title');
        
        if (emailList) emailList.style.display = 'none';
        if (emailDetailView) emailDetailView.style.display = 'flex';
        if (inboxActions) inboxActions.style.display = 'none';
        if (inboxTitle) inboxTitle.textContent = 'Email Detail';
        
        // Populate the email detail content
        const emailDetailContent = getElement('email-detail-content');
        if (emailDetailContent) {
            emailDetailContent.innerHTML = `
                <div class="email-detail">
                    <div class="email-detail-header vertical">
                        <div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${escapeHtml(email.from)}</span></div>
                        <div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${escapeHtml(email.to)}</span></div>
                        <div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
                        <div class="email-header-row"><span class="email-header-label">Subject:</span> <span class="email-header-value">${escapeHtml(email.subject)}</span></div>
                    </div>
                    <div class="email-detail-body">
                        ${escapeHtml(email.body).replace(/\n/g, '<br>')}
                    </div>
                </div>
            `;
        }
        
    } catch (error) {
        console.error('Error showing email detail:', error);
    }
}

function showEmailList() {
    try {
        // Show the email list and hide the detail view
        const emailList = getElement('email-list');
        const emailDetailView = getElement('email-detail-view');
        const inboxActions = getElement('inbox-actions');
        const inboxTitle = getElement('inbox-title');
        
        if (emailList) emailList.style.display = 'block';
        if (emailDetailView) emailDetailView.style.display = 'none';
        if (inboxActions) inboxActions.style.display = 'flex';
        if (inboxTitle) inboxTitle.textContent = 'Inbox';
        
    } catch (error) {
        console.error('Error showing email list:', error);
    }
}

// New function to create a lightweight version of contacts for localStorage caching
function createLightweightContactsCache(contacts) {
    return contacts.map(contact => ({
        pubkey: contact.pubkey,
        name: contact.name,
        email: contact.email,
        picture: contact.picture, // Keep the URL, not the data URL
        fields: contact.fields || {},
        // Don't include picture_data_url, picture_loaded, picture_loading in localStorage
        // These will be loaded from backend cache when needed
    }));
}

// New function to restore contacts from lightweight cache
function restoreContactsFromLightweightCache(lightweightContacts) {
    return lightweightContacts.map(contact => ({
        ...contact,
        picture_data_url: null,
        picture_loaded: false,
        picture_loading: false
    }));
}

async function loadContacts() {
    console.log('[JS] loadContacts called');
    
    // Try to load from backend storage first for instant display
    let cacheLoaded = false;
    try {
        console.log('[JS] Loading contacts from backend storage...');
        const cachedContacts = await tauriInvoke('get_contacts');
        
        if (cachedContacts && cachedContacts.length > 0) {
            console.log('[JS] Found cached contacts in backend, rendering immediately...');
            
            // Ensure cached contacts have all the necessary fields for offline display
            appState.contacts = cachedContacts.map(contact => ({
                pubkey: contact.pubkey,
                name: contact.name || contact.display_name || contact.pubkey.substring(0, 16) + '...',
                picture: contact.picture || '',
                email: contact.email || null,
                fields: {
                    name: contact.name,
                    display_name: contact.display_name,
                    picture: contact.picture,
                    about: contact.about,
                    email: contact.email
                },
                // Load cached image data URL from backend storage
                picture_data_url: contact.picture_data_url || null,
                picture_loaded: !!contact.picture_data_url,
                picture_loading: false
            }));
            
            // Sort cached contacts alphabetically by name
            appState.contacts.sort((a, b) => {
                const nameA = a.name.toLowerCase();
                const nameB = b.name.toLowerCase();
                return nameA.localeCompare(nameB);
            });
            
            appState.selectedContact = null; // Clear selected contact when refreshing
            renderContacts();
            cacheLoaded = true;
            
            // Load images progressively for contacts that need them
            await loadContactImagesProgressively();
            console.log('[JS] Contacts loaded from backend storage with cached images');
        } else {
            console.log('[JS] No cached contacts found in backend storage.');
        }
    } catch (e) {
        console.warn('Failed to load cached contacts from backend:', e);
    }

    if (!appState.keypair) {
        console.warn('No keypair available for fetching contacts.');
        if (!appState.contacts || appState.contacts.length === 0) {
            appState.contacts = [];
            renderContacts();
        }
        return;
    }

    // Try to fetch fresh data from network (but don't fail if offline)
    // Only fetch if we don't have cached contacts, or if we want to refresh
    if (!cacheLoaded) {
        try {
            console.log('[JS] Fetching following profiles from backend...');
            const followingProfiles = await tauriInvoke('fetch_following_profiles', {
                privateKey: appState.keypair.private_key,
                relays: getActiveRelays()
            });
            console.log(`[JS] Received ${followingProfiles.length} profiles from backend.`, followingProfiles);

            // Only update contacts if we actually got data from the network
            if (followingProfiles && followingProfiles.length > 0) {
                // Create contacts immediately with placeholder images
                const newContacts = followingProfiles.map(profile => ({
                    pubkey: profile.pubkey,
                    name: profile.fields.name || profile.fields.display_name || profile.pubkey.substring(0, 16) + '...',
                    picture: profile.fields.picture || '',
                    email: profile.fields.email || null,
                    fields: profile.fields || {}, // Include all profile fields
                    picture_data_url: null,
                    picture_loading: false,
                    picture_loaded: false
                }));

                // Update contacts in place instead of clearing the list
                updateContactsInPlace(newContacts);

                // Cache the contacts in backend storage (without images)
                try {
                    // Convert frontend contact format to backend storage format
                    const backendContacts = appState.contacts.map(contact => ({
                        pubkey: contact.pubkey,
                        name: contact.name,
                        display_name: contact.fields.display_name || contact.name,
                        picture: contact.picture,
                        picture_data_url: contact.picture_data_url || null,
                        about: contact.fields.about || null,
                        email: contact.email,
                        cached_at: new Date().toISOString()
                    }));
                    
                    await tauriInvoke('set_contacts', { contacts: backendContacts });
                    console.log('[JS] Cached contacts in backend storage');
                } catch (e) {
                    console.warn('Failed to cache contacts in backend:', e);
                }

                // Load images progressively in the background
                await loadContactImagesProgressively();
            } else {
                console.log('[JS] No profiles received from network, keeping cached data if available');
                if (!cacheLoaded) {
                    appState.contacts = [];
                    renderContacts();
                }
            }

        } catch (error) {
            console.error('Failed to load contacts from network:', error);
            if (!cacheLoaded) {
                showError('Failed to fetch your follow list and no cached data available: ' + error);
                appState.contacts = [];
                renderContacts();
            } else {
                console.log('[JS] Network failed, but using cached contacts');
            }
        }
    } else {
        console.log('[JS] Using cached contacts from backend storage, skipping network fetch');
    }
}

// New function to load images progressively with backend caching
async function loadContactImagesProgressively() {
    console.log('[JS] Starting progressive image loading with backend caching...');
    
    // First, render any contacts that already have picture_data_url but haven't been rendered
    const contactsWithCachedImages = appState.contacts.filter(contact => 
        contact.picture_data_url && !contact.picture_loaded
    );
    
    if (contactsWithCachedImages.length > 0) {
        console.log(`[JS] Rendering ${contactsWithCachedImages.length} contacts with cached images`);
        contactsWithCachedImages.forEach(contact => {
            const contactIndex = appState.contacts.findIndex(c => c.pubkey === contact.pubkey);
            if (contactIndex !== -1) {
                appState.contacts[contactIndex].picture_loaded = true;
                renderContactItem(contactIndex);
            }
        });
    }
    
    // Process contacts in batches to avoid overwhelming the network
    const batchSize = 15; // Increased from 10 to 15 for even faster loading
    const contactsWithPictures = appState.contacts.filter(contact => contact.picture && !contact.picture_loaded);
    
    console.log(`[JS] Found ${contactsWithPictures.length} contacts with pictures that need loading`);
    console.log('[JS] Contacts needing images:', contactsWithPictures.map(c => c.name));
    
    // Debug: Check what contacts are in backend storage
    try {
        const storedContacts = await tauriInvoke('get_contacts');
        console.log(`[JS] Backend storage has ${storedContacts.length} contacts`);
        const storedPubkeys = storedContacts.map(c => c.pubkey);
        console.log('[JS] Stored pubkeys:', storedPubkeys.slice(0, 5), '...');
    } catch (e) {
        console.warn('[JS] Failed to check backend storage:', e);
    }
    
    for (let i = 0; i < contactsWithPictures.length; i += batchSize) {
        const batch = contactsWithPictures.slice(i, i + batchSize);
        console.log(`[JS] Processing batch ${Math.floor(i/batchSize) + 1}/${Math.ceil(contactsWithPictures.length/batchSize)}:`, batch.map(c => c.name));
        
        // Mark all contacts in this batch as loading
        batch.forEach(contact => {
            const contactIndex = appState.contacts.findIndex(c => c.pubkey === contact.pubkey);
            if (contactIndex !== -1) {
                appState.contacts[contactIndex].picture_loading = true;
                renderContactItem(contactIndex);
            }
        });

        try {
            // First try to get all images from backend cache
            const cachePromises = batch.map(async (contact) => {
                const dataUrl = await getCachedProfileImageFromBackend(contact.pubkey);
                return { contact, dataUrl };
            });
            
            const cachedResults = await Promise.all(cachePromises);
            const uncachedContacts = cachedResults.filter(result => !result.dataUrl).map(result => result.contact);
            const cachedContacts = cachedResults.filter(result => result.dataUrl);
            
            console.log(`[JS] Found ${cachedContacts.length} cached images, need to fetch ${uncachedContacts.length} new images`);
            
            // Update cached contacts immediately
            for (const { contact, dataUrl } of cachedContacts) {
                const contactIndex = appState.contacts.findIndex(c => c.pubkey === contact.pubkey);
                if (contactIndex !== -1) {
                    appState.contacts[contactIndex].picture_data_url = dataUrl;
                    appState.contacts[contactIndex].picture_loaded = true;
                    appState.contacts[contactIndex].picture_loading = false;
                    renderContactItem(contactIndex);
                    
                    // Save the cached image data URL to backend storage
                    try {
                        console.log(`[JS] Attempting to save cached image data URL to backend storage for ${contact.name} (${contact.pubkey})`);
                        
                        // First check if the contact exists in storage
                        const storedContact = await tauriInvoke('get_contact', { pubkey: contact.pubkey });
                        if (!storedContact) {
                            console.warn(`[JS] Contact ${contact.name} (${contact.pubkey}) not found in backend storage, skipping update`);
                            continue;
                        }
                        
                        await tauriInvoke('update_contact_picture_data_url', {
                            pubkey: contact.pubkey,
                            // Note: Rust parameters in snake_case (picture_data_url) are automatically 
                            // converted to camelCase (pictureDataUrl) for JavaScript by Tauri.
                            // To use snake_case in JS, you need #[tauri::command(rename_all = "snake_case")]
                            pictureDataUrl: dataUrl
                        });
                        console.log(`[JS] Successfully saved cached image data URL to backend storage for ${contact.name}`);
                    } catch (e) {
                        console.error(`[JS] Failed to save cached image data URL to backend storage for ${contact.name}:`, e);
                    }
                }
            }
            
            // Fetch uncached images concurrently using the new backend function
            if (uncachedContacts.length > 0) {
                const imageUrls = uncachedContacts.map(contact => contact.picture);
                console.log(`[JS] Fetching ${imageUrls.length} images concurrently from backend...`);
                
                const fetchedImages = await tauriInvoke('fetch_multiple_images', { urls: imageUrls });
                
                // Process fetched images
                for (const contact of uncachedContacts) {
                    const contactIndex = appState.contacts.findIndex(c => c.pubkey === contact.pubkey);
                    if (contactIndex !== -1) {
                        const dataUrl = fetchedImages[contact.picture];
                        if (dataUrl) {
                            // Cache in backend
                            await cacheProfileImageInBackend(contact.pubkey, dataUrl);
                            
                            appState.contacts[contactIndex].picture_data_url = dataUrl;
                            appState.contacts[contactIndex].picture_loaded = true;
                            console.log(`[JS] Successfully loaded image for ${contact.name}`);
                            
                            // Save the updated contact to backend storage immediately
                            try {
                                console.log(`[JS] Attempting to save image data URL to backend storage for ${contact.name} (${contact.pubkey})`);
                                
                                // First check if the contact exists in storage
                                const storedContact = await tauriInvoke('get_contact', { pubkey: contact.pubkey });
                                if (!storedContact) {
                                    console.warn(`[JS] Contact ${contact.name} (${contact.pubkey}) not found in backend storage, skipping update`);
                                    continue;
                                }
                                
                                await tauriInvoke('update_contact_picture_data_url', {
                                    pubkey: contact.pubkey,
                                    // Note: Rust parameters in snake_case (picture_data_url) are automatically 
                                    // converted to camelCase (pictureDataUrl) for JavaScript by Tauri.
                                    // To use snake_case in JS, you need #[tauri::command(rename_all = "snake_case")]
                                    pictureDataUrl: dataUrl
                                });
                                console.log(`[JS] Successfully saved image data URL to backend storage for ${contact.name}`);
                            } catch (e) {
                                console.error(`[JS] Failed to save image data URL to backend storage for ${contact.name}:`, e);
                            }
                        } else {
                            console.warn(`[JS] Failed to get image data URL for ${contact.name}`);
                        }
                        appState.contacts[contactIndex].picture_loading = false;
                        renderContactItem(contactIndex);
                    }
                }
            }
            
        } catch (e) {
            console.warn(`Failed to fetch images for batch:`, e);
            // Mark all contacts in this batch as not loading
            batch.forEach(contact => {
                const contactIndex = appState.contacts.findIndex(c => c.pubkey === contact.pubkey);
                if (contactIndex !== -1) {
                    appState.contacts[contactIndex].picture_loading = false;
                    renderContactItem(contactIndex);
                }
            });
        }
        
        console.log(`[JS] Completed batch ${Math.floor(i/batchSize) + 1}`);
        
        // Individual contacts are now updated efficiently, no need for batch updates
        
        // Small delay between batches to be nice to the network
        if (i + batchSize < contactsWithPictures.length) {
            console.log('[JS] Waiting 30ms before next batch...'); // Reduced delay from 50ms to 30ms
            await new Promise(resolve => setTimeout(resolve, 30));
        }
    }
    
    console.log('[JS] Progressive image loading with backend caching completed');
}

// New function to render individual contact items
function renderContactItem(index) {
    if (!elements.contactsList) return;
    
    const contact = appState.contacts[index];
    if (!contact) return;

    // Find existing contact element
    const existingElement = elements.contactsList.querySelector(`[data-pubkey="${contact.pubkey}"]`);
    if (!existingElement) return;

    const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
    
    let emailIcon = '';
    if (contact.email) {
        emailIcon = `<a href="mailto:${contact.email}" class="contact-email-icon" title="Send email to ${contact.email}"><i class="fas fa-envelope"></i></a>`;
    }

    // Determine avatar source
    let avatarSrc = defaultAvatar;
    let avatarClass = 'contact-avatar';
    
    if (contact.picture_loading) {
        avatarClass += ' loading';
        // You could add a loading spinner here if desired
    } else if (contact.picture_data_url) {
        avatarSrc = contact.picture_data_url;
        console.log(`[JS] Using cached data URL for ${contact.name}`);
    } else {
        console.log(`[JS] Using default avatar for ${contact.name} (no cached image available)`);
    }

    // Update the avatar image
    const avatarImg = existingElement.querySelector('.contact-avatar');
    if (avatarImg) {
        avatarImg.src = avatarSrc;
        avatarImg.className = avatarClass;
        avatarImg.onerror = function() {
            this.onerror = null;
            this.src = defaultAvatar;
            this.className = 'contact-avatar';
        };
    }
}

function renderContacts(searchQuery = '') {
    if (!elements.contactsList) return;

    try {
        elements.contactsList.innerHTML = '';

        // Filter contacts based on search query
        let filteredContacts = appState.contacts;
        if (searchQuery) {
            filteredContacts = appState.contacts.filter(contact => 
                contact.name.toLowerCase().includes(searchQuery) ||
                contact.pubkey.toLowerCase().includes(searchQuery) ||
                (contact.email && contact.email.toLowerCase().includes(searchQuery))
            );
        }

        if (filteredContacts && filteredContacts.length > 0) {
            filteredContacts.forEach((contact, index) => {
                const contactElement = document.createElement('div');
                contactElement.className = 'contact-item';
                contactElement.setAttribute('data-pubkey', contact.pubkey);
                
                // Add active class if this contact is selected
                if (appState.selectedContact && appState.selectedContact.pubkey === contact.pubkey) {
                    contactElement.classList.add('active');
                }

                const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
                
                let emailIcon = '';
                if (contact.email) {
                    emailIcon = `<a href="mailto:${contact.email}" class="contact-email-icon" title="Send email to ${contact.email}"><i class="fas fa-envelope"></i></a>`;
                }

                // Determine avatar source and class - only use cached data URLs to prevent offline errors
                let avatarSrc = defaultAvatar;
                let avatarClass = 'contact-avatar';
                
                if (contact.picture_loading) {
                    avatarClass += ' loading';
                } else if (contact.picture_data_url) {
                    avatarSrc = contact.picture_data_url;
                    console.log(`[JS] Using cached data URL for ${contact.name}`);
                } else {
                    console.log(`[JS] Using default avatar for ${contact.name} (no cached image available)`);
                }

                contactElement.innerHTML = `
                    <img class="${avatarClass}" src="${avatarSrc}" alt="${contact.name}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
                    <div class="contact-info">
                        <div class="contact-name">${contact.name}</div>
                    </div>
                    <div class="contact-actions">
                        ${emailIcon}
                    </div>
                `;
                
                // Add click event listener
                contactElement.addEventListener('click', () => selectContact(contact));
                
                elements.contactsList.appendChild(contactElement);
            });
        } else {
            const message = searchQuery 
                ? `No contacts found matching "${searchQuery}"`
                : 'You are not following anyone yet, or contacts could not be loaded.';
            elements.contactsList.innerHTML = `<div class="text-muted text-center">${message}</div>`;
        }
    } catch (error) {
        console.error('Error rendering contacts:', error);
    }
}

function showAddContactModal() {
    try {
        const content = `
            <form id="add-contact-form">
                <div class="form-group">
                    <label for="contact-name">Name:</label>
                    <input type="text" id="contact-name" required>
                </div>
                <div class="form-group">
                    <label for="contact-pubkey">Public Key:</label>
                    <div class="input-with-button">
                        <input type="text" id="contact-pubkey" placeholder="npub1..." required>
                        <button type="button" id="scan-qr-btn" class="btn btn-secondary">
                            <i class="fas fa-qrcode"></i> Scan QR
                        </button>
                    </div>
                </div>
                <div class="form-group">
                    <label for="contact-email">Email (optional):</label>
                    <input type="email" id="contact-email">
                </div>
                <div class="form-actions">
                    <button type="submit" class="btn btn-primary">Add Contact</button>
                    <button type="button" class="btn btn-secondary" onclick="hideModal()">Cancel</button>
                </div>
            </form>
        `;
        
        showModal('Add Contact', content);
        
        const form = document.getElementById('add-contact-form');
        const scanBtn = document.getElementById('scan-qr-btn');
        
        if (form) {
            form.addEventListener('submit', addContact);
        }
        if (scanBtn) {
            scanBtn.addEventListener('click', scanQRCode);
        }
    } catch (error) {
        console.error('Error showing add contact modal:', error);
    }
}

async function scanQRCode() {
    try {
        const modalContent = `
            <div id="qr-scanner-container">
                <div id="qr-reader"></div>
                <div class="qr-scanner-controls">
                    <button id="close-scanner-btn" class="btn btn-secondary">
                        <i class="fas fa-times"></i> Close Scanner
                    </button>
                </div>
            </div>
        `;
        
        // Update modal content
        if (elements.modalBody) {
            elements.modalBody.innerHTML = modalContent;
        }
        if (elements.modalTitle) {
            elements.modalTitle.textContent = 'Scan QR Code';
        }
        
        // Let the browser handle camera permissions directly
        const stream = await navigator.mediaDevices.getUserMedia({ 
            video: { 
                facingMode: 'environment', // Use back camera if available
                width: { ideal: 1280 },
                height: { ideal: 720 }
            } 
        });
        
        // Stop the test stream immediately
        stream.getTracks().forEach(track => track.stop());
        
        const html5QrcodeScanner = new Html5QrcodeScanner(
            "qr-reader", 
            { 
                fps: 10, 
                qrbox: { width: 250, height: 250 },
                aspectRatio: 1.0
            }
        );
        
        html5QrcodeScanner.render((decodedText) => {
            // Check if the QR code contains a Nostr public key
            if (decodedText.startsWith('npub1')) {
                // Extract just the npub part if there's additional data
                const npubMatch = decodedText.match(/npub1[a-zA-Z0-9]+/);
                if (npubMatch) {
                    const npub = npubMatch[0];
                    
                    // Stop the scanner
                    html5QrcodeScanner.clear();
                    
                    // Go back to the add contact form
                    showAddContactModal();
                    
                    // Set the scanned public key
                    setTimeout(() => {
                        const pubkeyInput = document.getElementById('contact-pubkey');
                        if (pubkeyInput) {
                            pubkeyInput.value = npub;
                            pubkeyInput.focus();
                        }
                    }, 100);
                    
                    showSuccess('QR code scanned successfully!');
                }
            } else {
                showError('Invalid QR code: Not a Nostr public key');
            }
        }, (error) => {
            // Handle scan errors silently
            console.log('QR scan error:', error);
        });
        
        // Add close button functionality
        const closeBtn = document.getElementById('close-scanner-btn');
        if (closeBtn) {
            closeBtn.addEventListener('click', () => {
                html5QrcodeScanner.clear();
                showAddContactModal();
            });
        }
        
    } catch (error) {
        console.error('Camera access error:', error);
        
        // Show helpful error message for camera permission issues
        const errorMessage = `
            <div class="camera-error">
                <h3>Camera Access Required</h3>
                <p>To scan QR codes, this app needs access to your camera.</p>
                <p><strong>Browser Permission:</strong></p>
                <ul>
                    <li>When prompted, click "Allow" to grant camera access</li>
                    <li>If denied, refresh the page and try again</li>
                    <li>Check your browser's camera permissions in the address bar</li>
                </ul>
                <p><strong>Alternative:</strong> You can manually enter the public key instead.</p>
                <div class="form-actions">
                    <button onclick="showAddContactModal()" class="btn btn-primary">Enter Manually</button>
                    <button onclick="hideModal()" class="btn btn-secondary">Cancel</button>
                </div>
            </div>
        `;
        
        if (elements.modalBody) {
            elements.modalBody.innerHTML = errorMessage;
        }
    }
}

async function addContact(event) {
    event.preventDefault();
    
    try {
        const name = document.getElementById('contact-name')?.value || '';
        const pubkey = document.getElementById('contact-pubkey')?.value || '';
        const email = document.getElementById('contact-email')?.value || '';
        
        // Validate public key
        try {
            console.log('Validating public key:', pubkey);
            const isValid = await tauriInvoke('validate_public_key', { publicKey: pubkey });
            console.log('Validation result:', isValid, typeof isValid);
            
            if (!isValid) {
                showError('Invalid public key format');
                return;
            }
        } catch (error) {
            console.error('Validation error:', error);
            showError('Failed to validate public key');
            return;
        }
        
        appState.contacts.push({ name, pubkey, email });
        renderContacts();
        hideModal();
        showSuccess('Contact added successfully');
    } catch (error) {
        console.error('Error adding contact:', error);
        showError('Failed to add contact');
    }
}

// Store the current editable fields in memory
let editableProfileFields = {};

function renderProfileFromObject(profile, cachedPictureDataUrl) {
    // Build editable fields from profile.fields, always include email
    editableProfileFields = { ...(profile && profile.fields ? profile.fields : {}) };
    if (!('email' in editableProfileFields)) {
        editableProfileFields.email = '';
    }
    renderProfileFieldsList(editableProfileFields);
    // Show profile picture if present
    const profilePicture = document.getElementById('profile-picture');
    if (profilePicture) {
        if (cachedPictureDataUrl) {
            profilePicture.src = cachedPictureDataUrl;
            profilePicture.style.display = '';
        } else if (editableProfileFields.picture) {
            profilePicture.src = editableProfileFields.picture;
            profilePicture.style.display = '';
        } else {
            profilePicture.style.display = 'none';
        }
    }
}

function renderProfileFieldsList(fields) {
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
            editableProfileFields[key] = e.target.value;
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
                delete editableProfileFields[key];
                renderProfileFieldsList(editableProfileFields);
            });
            fieldDiv.appendChild(removeBtn);
        }
        listDiv.appendChild(fieldDiv);
    }
}

// UpdateProfile: collect all fields and submit
async function updateProfile() {
    if (!appState.keypair) {
        showError('No keypair available');
        return;
    }
    // Remove empty fields
    const fieldsToSave = {};
    for (const [key, value] of Object.entries(editableProfileFields)) {
        if (value && value.trim() !== '') {
            fieldsToSave[key] = value;
        }
    }
    try {
        if (elements.updateProfileBtn) {
            elements.updateProfileBtn.disabled = true;
            elements.updateProfileBtn.innerHTML = '<span class="loading"></span> Updating...';
        }
        await tauriInvoke('publish_nostr_event', {
            privateKey: appState.keypair.private_key,
            content: JSON.stringify(fieldsToSave),
            kind: 0,
            tags: [],
            relays: getActiveRelays()
        });

        // --- Update local cache and UI ---
        const updatedProfile = {
            pubkey: appState.keypair.public_key,
            fields: fieldsToSave,
            raw_content: JSON.stringify(fieldsToSave, null, 2)
        };
        localStorage.setItem('nostr_mail_profile', JSON.stringify(updatedProfile));

        if (fieldsToSave.picture) {
            try {
                const dataUrl = await fetchImageAsDataUrl(fieldsToSave.picture);
                if (dataUrl) {
                    localStorage.setItem('nostr_mail_profile_picture', dataUrl);
                } else {
                    localStorage.removeItem('nostr_mail_profile_picture');
                }
            } catch (e) {
                console.warn('Failed to cache new profile picture:', e);
                localStorage.removeItem('nostr_mail_profile_picture');
            }
        } else {
            localStorage.removeItem('nostr_mail_profile_picture');
        }

        // Reload the profile display to show the new data from cache
        await loadProfile();
        // --- End of update ---

        showSuccess('Profile updated successfully');
    } catch (error) {
        console.error('Failed to update profile:', error);
        showError('Failed to update profile: ' + error);
    } finally {
        if (elements.updateProfileBtn) {
            elements.updateProfileBtn.disabled = false;
            elements.updateProfileBtn.innerHTML = '<i class="fas fa-save"></i> Update Profile';
        }
    }
}

async function testConnection() {
    if (!appState.settings) {
        showError('Please save your settings first');
        return;
    }
    
    try {
        if (elements.testConnectionBtn) {
            elements.testConnectionBtn.disabled = true;
            elements.testConnectionBtn.innerHTML = '<span class="loading"></span> Testing...';
        }
        
        // Try to load emails as a connection test
        await loadEmails();
        showSuccess('Connection test successful');
        
    } catch (error) {
        console.error('Connection test failed:', error);
        showError('Connection test failed: ' + error);
    } finally {
        if (elements.testConnectionBtn) {
            elements.testConnectionBtn.disabled = false;
            elements.testConnectionBtn.innerHTML = '<i class="fas fa-test-tube"></i> Test Connection';
        }
    }
}

// Utility functions
function showSuccess(message) {
    showNotification(message, 'success');
}

function showError(message) {
    showNotification(message, 'error');
}

function showNotification(message, type) {
    try {
        const notification = document.createElement('div');
        notification.className = `notification notification-${type}`;
        notification.innerHTML = `
            <div class="notification-content">
                <i class="fas fa-${type === 'success' ? 'check-circle' : 'exclamation-circle'}"></i>
                <span>${message}</span>
            </div>
        `;
        
        // Add styles
        notification.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            padding: 15px 20px;
            border-radius: 8px;
            color: white;
            font-weight: 500;
            z-index: 1001;
            animation: slideIn 0.3s ease;
            background: ${type === 'success' ? 'linear-gradient(135deg, #28a745, #20c997)' : 'linear-gradient(135deg, #dc3545, #fd7e14)'};
        `;
        
        document.body.appendChild(notification);
        
        // Remove after 5 seconds
        setTimeout(() => {
            notification.style.animation = 'slideOut 0.3s ease';
            setTimeout(() => {
                if (notification.parentNode) {
                    notification.parentNode.removeChild(notification);
                }
            }, 300);
        }, 5000);
    } catch (error) {
        console.error('Error showing notification:', error);
    }
}

// Add CSS animations
try {
    const style = document.createElement('style');
    style.textContent = `
        @keyframes slideIn {
            from { transform: translateX(100%); opacity: 0; }
            to { transform: translateX(0); opacity: 1; }
        }
        @keyframes slideOut {
            from { transform: translateX(0); opacity: 1; }
            to { transform: translateX(100%); opacity: 0; }
        }
    `;
    document.head.appendChild(style);
} catch (error) {
    console.error('Error adding CSS animations:', error);
}

// Npriv key management functions
async function generateNewKeypair() {
    try {
        if (elements.generateKeyBtn) {
            elements.generateKeyBtn.disabled = true;
            elements.generateKeyBtn.innerHTML = '<span class="loading"></span> Generating...';
        }
        
        const keypair = await tauriInvoke('generate_keypair');
        
        // Set the private key in the form
        if (elements.nprivKey) {
            elements.nprivKey.value = keypair.private_key;
        }
        
        // Update the public key display
        if (elements.publicKeyDisplay) {
            elements.publicKeyDisplay.value = keypair.public_key;
        }
        
        // Update app state
        appState.nprivKey = keypair.private_key;
        appState.keypair = keypair;
        
        showSuccess('New keypair generated successfully');
        renderProfilePubkey();
    } catch (error) {
        console.error('Failed to generate keypair:', error);
        showError('Failed to generate keypair: ' + error);
    } finally {
        if (elements.generateKeyBtn) {
            elements.generateKeyBtn.disabled = false;
            elements.generateKeyBtn.innerHTML = '<i class="fas fa-key"></i> Generate New Keypair';
        }
    }
}

async function updatePublicKeyDisplay() {
    const nprivKey = elements.nprivKey?.value?.trim() || '';
    
    if (!nprivKey) {
        if (elements.publicKeyDisplay) {
            elements.publicKeyDisplay.value = '';
        }
        return;
    }
    
    try {
        // Validate the private key first
        const isValid = await tauriInvoke('validate_private_key', { privateKey: nprivKey });
        
        if (!isValid) {
            if (elements.publicKeyDisplay) {
                elements.publicKeyDisplay.value = 'Invalid private key';
            }
            return;
        }
        
        // Get the public key from the private key
        const publicKey = await tauriInvoke('get_public_key_from_private', { privateKey: nprivKey });
        if (elements.publicKeyDisplay) {
            elements.publicKeyDisplay.value = publicKey;
        }
        
    } catch (error) {
        console.error('Failed to get public key:', error);
        if (elements.publicKeyDisplay) {
            elements.publicKeyDisplay.value = 'Error getting public key';
        }
    }
}

async function copyPublicKey() {
    const publicKey = elements.publicKeyDisplay?.value || '';
    
    if (!publicKey || publicKey === 'Invalid private key' || publicKey === 'Error getting public key') {
        showError('No valid public key to copy');
        return;
    }
    
    try {
        await navigator.clipboard.writeText(publicKey);
        showSuccess('Public key copied to clipboard');
    } catch (error) {
        console.error('Failed to copy to clipboard:', error);
        showError('Failed to copy public key');
    }
}

async function copyEmailPassword() {
    const password = elements.emailPassword?.value || '';
    
    if (!password) {
        showError('No email password to copy');
        return;
    }
    
    try {
        await navigator.clipboard.writeText(password);
        showSuccess('Email password copied to clipboard');
    } catch (error) {
        console.error('Failed to copy to clipboard:', error);
        showError('Failed to copy email password');
    }
}

async function copyNprivKey() {
    const nprivKey = elements.nprivKey?.value || '';
    
    if (!nprivKey) {
        showError('No private key to copy');
        return;
    }
    
    try {
        await navigator.clipboard.writeText(nprivKey);
        showSuccess('Private key copied to clipboard');
    } catch (error) {
        console.error('Failed to copy to clipboard:', error);
        showError('Failed to copy private key');
    }
}

function toggleNprivVisibility() {
    const input = elements.nprivKey;
    const button = elements.toggleNprivVisibilityBtn;
    
    if (input.type === 'password') {
        input.type = 'text';
        button.innerHTML = '<i class="fas fa-eye-slash"></i>';
        button.title = 'Hide private key';
    } else {
        input.type = 'password';
        button.innerHTML = '<i class="fas fa-eye"></i>';
        button.title = 'Show private key';
    }
}

function toggleEmailPasswordVisibility() {
    const input = elements.emailPassword;
    const button = elements.toggleEmailPasswordVisibilityBtn;
    
    if (input.type === 'password') {
        input.type = 'text';
        button.innerHTML = '<i class="fas fa-eye-slash"></i>';
        button.title = 'Hide password';
    } else {
        input.type = 'password';
        button.innerHTML = '<i class="fas fa-eye"></i>';
        button.title = 'Show password';
    }
}

function saveDraft() {
    try {
        const draft = {
            to: elements.toAddress?.value || '',
            subject: elements.subject?.value || '',
            body: elements.messageBody?.value || '',
            timestamp: Date.now()
        };
        
        localStorage.setItem('email_draft', JSON.stringify(draft));
        showSuccess('Draft saved');
    } catch (error) {
        console.error('Error saving draft:', error);
        showError('Failed to save draft');
    }
}

// Dark mode toggle logic
function setDarkMode(enabled) {
    document.body.classList.toggle('dark-mode', enabled);
    const icon = document.getElementById('dark-mode-icon');
    if (icon) {
        icon.className = enabled ? 'fas fa-sun' : 'fas fa-moon';
    }
    localStorage.setItem('darkMode', enabled ? '1' : '0');
}

function toggleDarkMode() {
    const enabled = !document.body.classList.contains('dark-mode');
    setDarkMode(enabled);
}

document.addEventListener('DOMContentLoaded', () => {
    console.log('🌐 DOM loaded - Initializing Nostr Mail interface...');
    
    // Set initial dark mode from localStorage
    const darkPref = localStorage.getItem('darkMode');
    setDarkMode(darkPref === '1');
    // Add event listener for dark mode toggle
    const darkToggle = document.getElementById('dark-mode-toggle');
    if (darkToggle) {
        darkToggle.addEventListener('click', toggleDarkMode);
    }
    
    console.log('🎨 Dark mode initialized:', darkPref === '1' ? 'enabled' : 'disabled');
});

// Initialize app when DOM is loaded
document.addEventListener('DOMContentLoaded', initApp);

// Relay Management
function getActiveRelays() {
    return appState.relays.filter(r => r.is_active).map(r => r.url);
}

async function loadRelays() {
    try {
        appState.relays = await tauriInvoke('get_relays');
        renderRelays();
    } catch (error) {
        console.error('Failed to load relays:', error);
        showError('Could not load relays from backend.');
    }
}

function renderRelays() {
    if (!elements.relaysList) return;
    elements.relaysList.innerHTML = '';
    
    appState.relays.forEach((relay, index) => {
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
        
        elements.relaysList.appendChild(relayItem);
    });

    // Add event listeners after rendering
    elements.relaysList.querySelectorAll('input[type="checkbox"]').forEach(toggle => {
        toggle.addEventListener('change', (e) => toggleRelay(e.target.dataset.index));
    });
    
    elements.relaysList.querySelectorAll('.btn-danger').forEach(button => {
        button.addEventListener('click', (e) => removeRelay(e.currentTarget.dataset.index));
    });
}

async function saveRelays() {
    try {
        await tauriInvoke('set_relays', { relays: appState.relays });
    } catch (error) {
        console.error('Failed to save relays:', error);
        showError('Could not save relays to backend.');
    }
}

function addRelay() {
    const url = elements.newRelayUrl.value.trim();
    if (url && (url.startsWith('ws://') || url.startsWith('wss://'))) {
        if (!appState.relays.some(r => r.url === url)) {
            appState.relays.push({ url, is_active: true });
            elements.newRelayUrl.value = '';
            renderRelays();
            saveRelays();
            saveRelaysToLocalStorage();
        } else {
            showError('Relay already exists.');
        }
    } else {
        showError('Invalid relay URL. Must start with ws:// or wss://');
    }
}

function toggleRelay(index) {
    if (appState.relays[index]) {
        appState.relays[index].is_active = !appState.relays[index].is_active;
        saveRelays();
        saveRelaysToLocalStorage();
    }
}

function removeRelay(index) {
    if (appState.relays[index]) {
        appState.relays.splice(index, 1);
        renderRelays();
        saveRelays();
        saveRelaysToLocalStorage();
    }
}

function isDevMode() {
    // Tauri dev mode: window.location.hostname is localhost or 127.0.0.1
    return window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';
}

async function loadProfile() {
    // Try to load cached profile first
    let cachedProfile = null;
    let cachedPictureDataUrl = null;
    try {
        const cached = localStorage.getItem('nostr_mail_profile');
        if (cached) {
            cachedProfile = JSON.parse(cached);
            cachedPictureDataUrl = localStorage.getItem('nostr_mail_profile_picture');
            renderProfileFromObject(cachedProfile, cachedPictureDataUrl);
        }
    } catch (e) {
        console.warn('Failed to load cached profile:', e);
    }

    if (!appState.keypair || !appState.keypair.public_key) {
        console.log('No public key available to fetch profile.');
        renderProfilePubkey();
        if (isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'No public key available.';
            }
        }
        return;
    }

    const activeRelays = getActiveRelays();
    if (activeRelays.length === 0) {
        showError('No active relays to fetch profile from.');
        renderProfilePubkey();
        if (isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'No active relays.';
            }
        }
        return;
    }

    try {
        const profile = await tauriInvoke('fetch_profile', {
            pubkey: appState.keypair.public_key,
            relays: activeRelays
        });

        if (profile) {
            // If there's a new picture URL, fetch and cache the image as a data URL
            if (profile.fields && profile.fields.picture) {
                const pictureUrl = profile.fields.picture;
                try {
                    const dataUrl = await fetchImageAsDataUrl(pictureUrl);
                    if (dataUrl) {
                        localStorage.setItem('nostr_mail_profile_picture', dataUrl);
                        renderProfileFromObject(profile, dataUrl);
                    } else {
                        localStorage.removeItem('nostr_mail_profile_picture');
                        renderProfileFromObject(profile, null);
                    }
                } catch (e) {
                    console.warn('Failed to cache profile picture:', e);
                    renderProfileFromObject(profile, null);
                }
            } else {
                localStorage.removeItem('nostr_mail_profile_picture');
                renderProfileFromObject(profile, null);
            }
            // Cache the profile in localStorage
            localStorage.setItem('nostr_mail_profile', JSON.stringify(profile));
        }
        renderProfilePubkey();
        if (isDevMode()) {
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
        showError('Could not fetch profile from relays.');
        renderProfilePubkey();
        if (isDevMode()) {
            const rawJsonBox = document.getElementById('profile-raw-json');
            if (rawJsonBox) {
                rawJsonBox.style.display = '';
                rawJsonBox.value = 'Error: ' + error;
            }
        }
    }
}

function renderProfilePubkey() {
    const pubkeyDiv = document.getElementById('profile-pubkey');
    if (pubkeyDiv && appState.keypair && appState.keypair.public_key) {
        pubkeyDiv.textContent = `Your npub: ${appState.keypair.public_key}`;
    } else if (pubkeyDiv) {
        pubkeyDiv.textContent = '';
    }
}

// Helper function to cache profile image in backend
async function cacheProfileImageInBackend(pubkey, dataUrl) {
    try {
        await tauriInvoke('cache_profile_image', {
            pubkey: pubkey,
            dataUrl: dataUrl
        });
        console.log(`[JS] Cached image in backend for ${pubkey}`);
    } catch (error) {
        console.warn(`Failed to cache image in backend for ${pubkey}:`, error);
    }
}

// Helper function to get cached profile image from backend
async function getCachedProfileImageFromBackend(pubkey) {
    try {
        const cachedDataUrl = await tauriInvoke('get_cached_profile_image', {
            pubkey: pubkey
        });
        if (cachedDataUrl) {
            console.log(`[JS] Found cached image in backend for ${pubkey}`);
            return cachedDataUrl;
        }
    } catch (error) {
        console.warn(`Failed to get cached image from backend for ${pubkey}:`, error);
    }
    return null;
}

// Updated function to fetch image with backend caching
async function fetchImageAsDataUrl(url) {
    if (!url) return null;
    try {
        // Use Tauri command to bypass CORS
        return await tauriInvoke('fetch_image', { url });
    } catch (e) {
        console.warn(`Failed to fetch image via backend for url: ${url}`, e);
        return null;
    }
}

// New function to clear contacts cache and refresh
async function refreshContacts() {
    console.log('[JS] Refreshing contacts...');
    
    // Clear the backend cache
    try {
        await tauriInvoke('set_contacts', { contacts: [] });
        console.log('[JS] Cleared contacts from backend storage');
    } catch (e) {
        console.warn('Failed to clear contacts from backend storage:', e);
    }
    
    // Don't clear the contacts list - let the in-place update handle it
    // This prevents the UI from going blank during refresh
    
    // Reload contacts from network (will use in-place update)
    await loadContacts();
}

// New function to handle contact selection
function selectContact(contact) {
    try {
        appState.selectedContact = contact;
        
        // Update UI - remove active class from all contacts
        document.querySelectorAll('.contact-item').forEach(item => {
            item.classList.remove('active');
        });
        
        // Add active class to selected contact
        const contactElement = document.querySelector(`[data-pubkey="${contact.pubkey}"]`);
        if (contactElement) {
            contactElement.classList.add('active');
        }
        
        // Render the contact detail
        renderContactDetail(contact);
        
    } catch (error) {
        console.error('Error selecting contact:', error);
    }
}

// New function to render contact detail
function renderContactDetail(contact) {
    if (!elements.contactsDetail) return;
    
    try {
        const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
        
        // Determine avatar source
        let avatarSrc = defaultAvatar;
        if (contact.picture_data_url) {
            avatarSrc = contact.picture_data_url;
        } else if (contact.picture) {
            avatarSrc = contact.picture;
        }
        
        // Create avatar element
        let avatarElement;
        if (avatarSrc === defaultAvatar) {
            avatarElement = `<div class="contact-detail-avatar">${contact.name.charAt(0).toUpperCase()}</div>`;
        } else {
            avatarElement = `
                <div class="contact-detail-avatar">
                    <img src="${avatarSrc}" alt="${contact.name}'s avatar" onerror="this.style.display='none';this.parentElement.textContent='${contact.name.charAt(0).toUpperCase()}';">
                    ${contact.name.charAt(0).toUpperCase()}
                </div>
            `;
        }
        
        // Get all available profile fields from the contact
        const profileFields = [];
        
        // Add standard fields if they exist
        if (contact.name) {
            profileFields.push({ key: 'Display Name', value: contact.name });
        }
        if (contact.email) {
            profileFields.push({ key: 'Email Address', value: contact.email, isEmail: true });
        }
        if (contact.pubkey) {
            profileFields.push({ key: 'Public Key', value: contact.pubkey, isPubkey: true });
        }
        
        // Add any additional fields from the profile data
        if (contact.fields) {
            Object.entries(contact.fields).forEach(([key, value]) => {
                if (value && value.trim() !== '') {
                    // Skip fields we already handled
                    if (['name', 'display_name', 'email', 'pubkey', 'picture'].includes(key)) {
                        return;
                    }
                    
                    // Format the key nicely
                    const formattedKey = key.split('_').map(word => 
                        word.charAt(0).toUpperCase() + word.slice(1)
                    ).join(' ');
                    
                    profileFields.push({ key: formattedKey, value: value });
                }
            });
        }
        
        // Build profile fields HTML
        const profileFieldsHTML = profileFields.map(field => {
            let valueHTML = field.value;
            
            if (field.isEmail) {
                valueHTML = `<a href="mailto:${field.value}" class="contact-detail-email">${field.value}</a>`;
            } else if (field.isPubkey) {
                valueHTML = `<code class="contact-detail-pubkey">${field.value}</code>`;
            } else if (field.value.startsWith('http')) {
                valueHTML = `<a href="${field.value}" target="_blank" rel="noopener noreferrer">${field.value}</a>`;
            }
            
            return `
                <div class="contact-detail-field">
                    <label>${field.key}</label>
                    <div class="value">${valueHTML}</div>
                </div>
            `;
        }).join('');
        
        // Build contact detail HTML
        const detailHTML = `
            <div class="contact-detail-content">
                <div class="contact-detail-header">
                    ${avatarElement}
                    <div class="contact-detail-info">
                        <h3>${contact.name}</h3>
                        <div class="contact-detail-pubkey">${contact.pubkey}</div>
                    </div>
                </div>
                
                <div class="contact-detail-section">
                    <h4>Profile Information</h4>
                    ${profileFieldsHTML}
                </div>
                
                <div class="contact-detail-actions">
                    ${contact.email ? `
                    <button class="btn btn-primary" onclick="sendEmailToContact('${contact.email}')">
                        <i class="fas fa-envelope"></i> Send Email
                    </button>
                    ` : ''}
                    <button class="btn btn-secondary" onclick="sendDirectMessageToContact('${contact.pubkey}')">
                        <i class="fas fa-comments"></i> Send DM
                    </button>
                    <button class="btn btn-secondary" onclick="copyContactPubkey('${contact.pubkey}')">
                        <i class="fas fa-copy"></i> Copy Public Key
                    </button>
                </div>
            </div>
        `;
        
        elements.contactsDetail.innerHTML = detailHTML;
        
    } catch (error) {
        console.error('Error rendering contact detail:', error);
        elements.contactsDetail.innerHTML = `
            <div class="contact-detail-placeholder">
                <i class="fas fa-exclamation-triangle"></i>
                <p>Error loading contact details</p>
            </div>
        `;
    }
}

// Helper functions for contact actions
function sendEmailToContact(email) {
    // Switch to compose tab and pre-fill the email
    switchTab('compose');
    if (elements.toAddress) {
        elements.toAddress.value = email;
        elements.toAddress.focus();
    }
}

function sendDirectMessageToContact(pubkey) {
    // Switch to DM tab
    switchTab('dm');
    // Immediately clear the DM message area to prevent flashing the previous conversation
    if (elements.dmMessages) {
        elements.dmMessages.innerHTML = '';
    }
    // Try to find an existing DM contact
    let contact = appState.dmContacts.find(c => c.pubkey === pubkey);
    if (!contact) {
        // Try to find the contact in the contacts list for name/picture
        const baseContact = appState.contacts.find(c => c.pubkey === pubkey);
        contact = {
            pubkey: pubkey,
            name: baseContact?.name || pubkey.substring(0, 16) + '...',
            lastMessage: '',
            lastMessageTime: new Date(),
            messageCount: 0,
            picture_data_url: baseContact?.picture_data_url || null,
            profileLoaded: !!baseContact
        };
        // Add to DM contacts and initialize empty messages
        appState.dmContacts.push(contact);
        appState.dmMessages[pubkey] = [];
    }
    // Render the skeleton UI immediately
    renderDmConversationSkeleton(contact);
    // Select the DM contact to show the conversation view (will fetch/load messages)
    selectDmContact(contact);
}

async function copyContactPubkey(pubkey) {
    try {
        await navigator.clipboard.writeText(pubkey);
        showSuccess('Public key copied to clipboard');
    } catch (error) {
        console.error('Failed to copy to clipboard:', error);
        showError('Failed to copy public key');
    }
}

// New function to update contacts in place
function updateContactsInPlace(newContacts) {
    console.log('[JS] Updating contacts in place...');
    
    // Create a map of existing contacts for quick lookup
    const existingContactsMap = new Map();
    appState.contacts.forEach(contact => {
        existingContactsMap.set(contact.pubkey, contact);
    });
    
    // Create a map of new contacts
    const newContactsMap = new Map();
    newContacts.forEach(contact => {
        newContactsMap.set(contact.pubkey, contact);
    });
    
    // Update existing contacts and add new ones
    const updatedContacts = [];
    newContacts.forEach(newContact => {
        const existingContact = existingContactsMap.get(newContact.pubkey);
        if (existingContact) {
            // Update existing contact while preserving loaded images
            updatedContacts.push({
                ...newContact,
                picture_data_url: existingContact.picture_data_url,
                picture_loaded: existingContact.picture_loaded,
                picture_loading: existingContact.picture_loading
            });
        } else {
            // Add new contact
            updatedContacts.push(newContact);
        }
    });
    
    // Sort contacts alphabetically by name
    updatedContacts.sort((a, b) => {
        const nameA = a.name.toLowerCase();
        const nameB = b.name.toLowerCase();
        return nameA.localeCompare(nameB);
    });
    
    // Update app state
    appState.contacts = updatedContacts;
    
    // Clear selected contact if it's no longer in the list
    if (!newContactsMap.has(appState.selectedContact?.pubkey)) {
        appState.selectedContact = null;
        // Show placeholder in detail panel
        if (elements.contactsDetail) {
            elements.contactsDetail.innerHTML = `
                <div class="contact-detail-placeholder">
                    <i class="fas fa-user"></i>
                    <p>Select a contact to view their profile</p>
                </div>
            `;
        }
    }
    
    // Render the updated list
    renderContacts();
    
    console.log(`[JS] Updated contacts: ${updatedContacts.length} total (${newContacts.length} from network)`);
}

// New function to refresh DM conversations
async function refreshDmConversations() {
    console.log('[JS] Refreshing DM conversations...');
    
    // Clear DM cache from backend storage
    try {
        await tauriInvoke('set_conversations', { conversations: [] });
        console.log('[JS] Cleared DM conversations from backend storage');
    } catch (e) {
        console.warn('Failed to clear DM conversations from backend storage:', e);
    }
    
    try {
        // Clear current conversations
        appState.dmContacts = [];
        appState.dmMessages = {};
        appState.selectedDmContact = null;
        
        // Clear the UI
        if (elements.dmContacts) {
            elements.dmContacts.innerHTML = '<div class="text-center text-muted">Loading conversations...</div>';
        }
        if (elements.dmMessages) {
            elements.dmMessages.innerHTML = `
                <div class="text-center text-muted" style="padding: 2rem;">
                    <i class="fas fa-sync fa-spin" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
                    <p>Refreshing conversations...</p>
                </div>
            `;
        }
        
        // Reload conversations
        await loadDmContacts();
        
        showSuccess('Conversations refreshed');
        
    } catch (error) {
        console.error('Failed to refresh DM conversations:', error);
        showError('Failed to refresh conversations');
    }
}

// Helper function to format time ago
function formatTimeAgo(date) {
    // Ensure date is a Date object (handle both Date objects and date strings from cache)
    const dateObj = date instanceof Date ? date : new Date(date);
    
    // Check if the date is valid
    if (isNaN(dateObj.getTime())) {
        return 'Unknown time';
    }
    
    const now = new Date();
    const diffMs = now - dateObj;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);
    
    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    
    return dateObj.toLocaleDateString();
}

// Filter contacts based on search query
function filterContacts() {
    const searchQuery = elements.contactsSearch.value.toLowerCase().trim();
    renderContacts(searchQuery);
}

// Toggle contacts search visibility
function toggleContactsSearch() {
    if (elements.contactsSearchContainer) {
        const isVisible = elements.contactsSearchContainer.style.display !== 'none';
        elements.contactsSearchContainer.style.display = isVisible ? 'none' : 'block';
        
        if (!isVisible) {
            // Focus the search input when showing it
            setTimeout(() => {
                if (elements.contactsSearch) {
                    elements.contactsSearch.focus();
                }
            }, 100);
        } else {
            // Clear search when hiding it
            if (elements.contactsSearch) {
                elements.contactsSearch.value = '';
                filterContacts(); // Reset to show all contacts
            }
        }
    }
}

async function testEmailConnection() {
    if (!appState.settings) {
        showError('Please save your settings first');
        return;
    }
    
    // Validate that required settings are present
    if (!appState.settings.email_address || !appState.settings.email_address.trim()) {
        showError('Email address is required. Please fill in your email address.');
        return;
    }
    
    if (!appState.settings.password || !appState.settings.password.trim()) {
        showError('Password is required. Please fill in your email password.');
        return;
    }
    
    if (!appState.settings.smtp_host || !appState.settings.smtp_host.trim()) {
        showError('SMTP host is required. Please fill in the SMTP host field.');
        return;
    }
    
    if (!appState.settings.imap_host || !appState.settings.imap_host.trim()) {
        showError('IMAP host is required. Please fill in the IMAP host field.');
        return;
    }
    
    try {
        if (elements.testEmailConnectionBtn) {
            elements.testEmailConnectionBtn.disabled = true;
            elements.testEmailConnectionBtn.innerHTML = '<span class="loading"></span> Testing...';
        }
        
        const emailConfig = {
            email_address: appState.settings.email_address,
            password: appState.settings.password,
            smtp_host: appState.settings.smtp_host,
            smtp_port: appState.settings.smtp_port,
            imap_host: appState.settings.imap_host,
            imap_port: appState.settings.imap_port,
            use_tls: appState.settings.use_tls
        };
        
        console.log('[JS] Testing email connections with config:', {
            smtp_host: emailConfig.smtp_host,
            smtp_port: emailConfig.smtp_port,
            imap_host: emailConfig.imap_host,
            imap_port: emailConfig.imap_port,
            use_tls: emailConfig.use_tls,
            email: emailConfig.email_address
        });
        
        // Test both IMAP and SMTP connections
        const results = await Promise.allSettled([
            tauriInvoke('test_imap_connection', { emailConfig }),
            tauriInvoke('test_smtp_connection', { emailConfig })
        ]);
        
        const imapResult = results[0];
        const smtpResult = results[1];
        
        // Check results and provide comprehensive feedback
        if (imapResult.status === 'fulfilled' && smtpResult.status === 'fulfilled') {
            showSuccess('✅ Email connection test successful!\n\n• IMAP: Connected and authenticated\n• SMTP: Connected and authenticated\n\nYour email settings are working correctly.');
        } else if (imapResult.status === 'fulfilled' && smtpResult.status === 'rejected') {
            showError(`⚠️ Partial success:\n\n✅ IMAP: Connected and authenticated\n❌ SMTP: ${smtpResult.reason}\n\nYou can receive emails but may have issues sending them.`);
        } else if (imapResult.status === 'rejected' && smtpResult.status === 'fulfilled') {
            showError(`⚠️ Partial success:\n\n❌ IMAP: ${imapResult.reason}\n✅ SMTP: Connected and authenticated\n\nYou can send emails but may have issues receiving them.`);
        } else {
            const imapError = imapResult.status === 'rejected' ? imapResult.reason : 'Unknown error';
            const smtpError = smtpResult.status === 'rejected' ? smtpResult.reason : 'Unknown error';
            showError(`❌ Email connection test failed:\n\n• IMAP: ${imapError}\n• SMTP: ${smtpError}\n\nPlease check your email settings and try again.`);
        }
        
    } catch (error) {
        console.error('Email connection test failed:', error);
        showError('Email connection test failed: ' + error);
    } finally {
        if (elements.testEmailConnectionBtn) {
            elements.testEmailConnectionBtn.disabled = false;
            elements.testEmailConnectionBtn.innerHTML = '<i class="fas fa-envelope"></i> Test Email Connection';
        }
    }
}

// Function to handle email provider selection
function handleEmailProviderChange() {
    const provider = elements.emailProvider?.value || '';
    
    if (!provider || provider === 'custom') {
        return; // Don't auto-populate for custom or empty selection
    }
    
    const providerSettings = {
        gmail: {
            smtp_host: 'smtp.gmail.com',
            smtp_port: 587,
            imap_host: 'imap.gmail.com',
            imap_port: 993,
            use_tls: true
        },
        outlook: {
            smtp_host: 'smtp-mail.outlook.com',
            smtp_port: 587,
            imap_host: 'outlook.office365.com',
            imap_port: 993,
            use_tls: true
        },
        yahoo: {
            smtp_host: 'smtp.mail.yahoo.com',
            smtp_port: 587,
            imap_host: 'imap.mail.yahoo.com',
            imap_port: 993,
            use_tls: true
        }
    };
    
    const settings = providerSettings[provider];
    if (settings) {
        // Populate the form fields
        if (elements.smtpHost) elements.smtpHost.value = settings.smtp_host;
        if (elements.smtpPort) elements.smtpPort.value = settings.smtp_port;
        if (elements.imapHost) elements.imapHost.value = settings.imap_host;
        if (elements.imapPort) elements.imapPort.value = settings.imap_port;
        if (elements.useTls) elements.useTls.checked = settings.use_tls;
        
        // Show a helpful message
        let message = `${provider.charAt(0).toUpperCase() + provider.slice(1)} settings applied.`;
        
        if (provider === 'gmail') {
            message += ' For Gmail, you must use an App Password instead of your regular password. Go to your Google Account settings > Security > 2-Step Verification > App passwords to generate one.';
        }
        
        // Add TLS info
        if (settings.use_tls) {
            message += ' TLS has been automatically enabled (required for secure connections).';
        }
        
        showSuccess(message);
    }
}

// Function to refresh all data for a new keypair

function renderDmConversationSkeleton(contact) {
    if (!elements.dmMessages) return;
    elements.dmMessages.innerHTML = '';

    // Header
    const headerElement = document.createElement('div');
    headerElement.className = 'conversation-header';
    headerElement.innerHTML = `
        <div class="conversation-contact-info">
            <div class="conversation-contact-name">${contact ? contact.name : contact.pubkey}</div>
            <div class="conversation-contact-pubkey">${contact.pubkey}</div>
        </div>
    `;
    elements.dmMessages.appendChild(headerElement);

    // Loading spinner for messages
    const loadingElement = document.createElement('div');
    loadingElement.className = 'messages-loading';
    loadingElement.innerHTML = `
        <div class="text-center text-muted" style="padding: 2rem;">
            <i class="fas fa-sync fa-spin" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
            <p>Loading messages...</p>
        </div>
    `;
    elements.dmMessages.appendChild(loadingElement);

    // Message input at the bottom
    const messageInputContainer = document.createElement('div');
    messageInputContainer.className = 'dm-message-input-container';
    messageInputContainer.innerHTML = `
        <div class="dm-message-input-wrapper">
            <input type="text" id="dm-reply-input" class="dm-message-input" placeholder="Type your message..." maxlength="1000">
            <button id="dm-send-btn" class="dm-send-btn">
                <i class="fas fa-paper-plane"></i>
            </button>
        </div>
    `;
    elements.dmMessages.appendChild(messageInputContainer);

    // Add event listeners for the new input elements
    const replyInput = document.getElementById('dm-reply-input');
    const sendBtn = document.getElementById('dm-send-btn');
    if (replyInput && sendBtn) {
        // Send message on Enter key
        replyInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                sendReplyMessage(contact.pubkey);
            }
        });
        // Send message on button click
        sendBtn.addEventListener('click', () => {
            sendReplyMessage(contact.pubkey);
        });
        // Focus the input
        setTimeout(() => {
            replyInput.focus();
        }, 100);
    }
}

// Add this helper function near your modal helpers
async function showQrCodeModal(label, value) {
    showModal(`${label} QR Code`, `<div id='qr-modal-container' style='display:flex;flex-direction:column;align-items:center;justify-content:center;'><div id='qr-code' style='margin:20px;text-align:center;'><i class="fas fa-spinner fa-spin" style="font-size:2rem;color:#666;"></i><br><small>Generating QR code...</small></div><div style='word-break:break-all;font-size:0.9em;margin-top:10px;'>${value}</div></div>`);
    
    try {
        // Generate QR code using Rust backend
        const qrDataUrl = await tauriInvoke('generate_qr_code', { data: value, size: 200 });
        
        // Update the modal with the generated QR code
        const qrElement = document.getElementById('qr-code');
        if (qrElement) {
            qrElement.innerHTML = `<img src="${qrDataUrl}" alt="QR Code" style="max-width:200px;max-height:200px;border:1px solid #ddd;border-radius:4px;">`;
        }
    } catch (error) {
        console.error('Failed to generate QR code:', error);
        const qrElement = document.getElementById('qr-code');
        if (qrElement) {
            qrElement.innerHTML = '<span style="color:red">Failed to generate QR code</span>';
        }
    }
}

// Add event listeners for QR code buttons after DOMContentLoaded
// (add to setupEventListeners or after DOMContentLoaded)
document.addEventListener('DOMContentLoaded', () => {
    // ... existing code ...
    
    // npriv QR
    const qrNprivBtn = document.getElementById('qr-npriv-btn');
    if (qrNprivBtn) {
        qrNprivBtn.addEventListener('click', async () => {
            const npriv = document.getElementById('npriv-key')?.value || '';
            if (npriv) {
                await showQrCodeModal('Private Key', npriv);
            } else {
                showError('No private key to show');
            }
        });
    }
    // npub QR
    const qrNpubBtn = document.getElementById('qr-npub-btn');
    if (qrNpubBtn) {
        qrNpubBtn.addEventListener('click', async () => {
            const npub = document.getElementById('public-key-display')?.value || '';
            if (npub) {
                await showQrCodeModal('Public Key', npub);
            } else {
                showError('No public key to show');
            }
        });
    }
});
