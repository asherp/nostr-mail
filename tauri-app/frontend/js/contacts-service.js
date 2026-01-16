// Contacts Service
// Handles all contacts-related functionality including loading, rendering, and management

// Remove all import/export statements. Attach ContactsService and contactsService to window. Replace any usage of imported symbols with window equivalents if needed.

class ContactsService {
    constructor() {
        this.searchTimeout = null;
    }

    // Load contacts from database only
    async loadContacts() {
        console.log('[JS] loadContacts called - loading contacts from database...');

        // Optionally, you can keep the keypair check if you want
        if (!window.appState.hasKeypair()) {
            window.notificationService.showError('No keypair available');
            return;
        }

        try {
            const userPubkey = window.appState.getKeypair().public_key;
            const dbContacts = await window.DatabaseService.getAllContacts(userPubkey);
            // Convert DB format to frontend format if needed
            // Note: picture_data_url is not fetched in initial query to reduce IPC overhead
            // It will be loaded on-demand via loadContactImagesProgressively
            const contacts = dbContacts.map(contact => ({
                pubkey: contact.pubkey,
                name: contact.name,
                picture: contact.picture_url || contact.picture || '',
                email: contact.email || null,
                is_public: contact.is_public !== undefined ? contact.is_public : true, // Default to public if not set
                fields: {
                    name: contact.name,
                    display_name: contact.display_name || contact.name,
                    picture: contact.picture_url || contact.picture || '',
                    about: contact.about || '',
                    email: contact.email || ''
                },
                picture_data_url: null, // Will be loaded on-demand via getCachedProfileImage
                picture_loading: false,
                picture_loaded: false // Will be set when image is loaded
            }));
            
            // Add user's own profile to contacts list as private
            // This allows sent emails to find the user's avatar/profile
            try {
                const userProfile = await window.DatabaseService.getContact(userPubkey);
                if (userProfile) {
                    // Check if user is already in contacts (shouldn't be, but check anyway)
                    const existingUserContact = contacts.find(c => c.pubkey === userPubkey);
                    if (!existingUserContact) {
                        // Get profile picture from cache if available
                        let pictureDataUrl = userProfile.picture_data_url || null;
                        if (!pictureDataUrl) {
                            const cachedPictureDataUrl = localStorage.getItem('nostr_mail_profile_picture');
                            if (cachedPictureDataUrl && cachedPictureDataUrl.startsWith('data:image')) {
                                pictureDataUrl = cachedPictureDataUrl;
                            }
                        }
                        
                        const userContact = {
                            pubkey: userPubkey,
                            name: userProfile.name || userProfile.display_name || 'Me',
                            picture: userProfile.picture_url || userProfile.picture || '',
                            email: userProfile.email || null,
                            is_public: false, // Private - user's own profile
                            fields: {
                                name: userProfile.name || 'Me',
                                display_name: userProfile.display_name || userProfile.name || 'Me',
                                picture: userProfile.picture_url || userProfile.picture || '',
                                about: userProfile.about || '',
                                email: userProfile.email || ''
                            },
                            picture_data_url: pictureDataUrl,
                            picture_loading: false,
                            picture_loaded: !!pictureDataUrl
                        };
                        contacts.unshift(userContact); // Add at the beginning
                        console.log(`[JS] Added user's own profile to contacts list (private)`);
                    }
                } else {
                    console.log(`[JS] User profile not found in database, skipping self-contact addition`);
                }
            } catch (e) {
                console.warn(`[JS] Failed to add user's own profile to contacts:`, e);
            }
            
            window.appState.setContacts(contacts);
            console.log(`[JS] Contacts loaded from database: ${contacts.length} contacts`);
            this.renderContacts();
            console.log('[JS] Contacts rendered to UI');
            // Defer image loading so UI is responsive
            this.loadContactImagesProgressively(); // don't await
        } catch (e) {
            console.warn('Failed to load contacts from database:', e);
            window.appState.setContacts([]);
            this.renderContacts();
            window.notificationService.showError('Failed to load contacts from database');
        }
    }

    // Load a single contact image asynchronously
    async loadContactImageAsync(contact) {
        if (!contact.picture || contact.picture_loading) return;
        
        try {
            contact.picture_loading = true;
            let dataUrl = await window.TauriService.getCachedProfileImage(contact.pubkey);
            if (!dataUrl && contact.picture) {
                dataUrl = await window.TauriService.fetchImage(contact.picture);
                if (dataUrl) {
                    // Update UI immediately
                    contact.picture_data_url = dataUrl;
                    contact.picture_loaded = true;
                    this.renderContactItem(contact);
                    // Update detail view if this contact is selected
                    const selectedContact = window.appState.getSelectedContact();
                    if (selectedContact && selectedContact.pubkey === contact.pubkey) {
                        this.renderContactDetail(contact);
                    }
                    // Cache in database
                    await window.TauriService.cacheProfileImage(contact.pubkey, dataUrl);
                }
            } else if (dataUrl) {
                contact.picture_data_url = dataUrl;
                contact.picture_loaded = true;
                this.renderContactItem(contact);
                const selectedContact = window.appState.getSelectedContact();
                if (selectedContact && selectedContact.pubkey === contact.pubkey) {
                    this.renderContactDetail(contact);
                }
            }
        } catch (e) {
            console.warn(`Failed to load image for ${contact.name}:`, e);
        } finally {
            contact.picture_loading = false;
        }
    }

    // Load contact images progressively
    async loadContactImagesProgressively() {
        const contacts = window.appState.getContacts();
        if (contacts.length === 0) {
            console.log('[JS] loadContactImagesProgressively: No contacts to load images for');
            return;
        }

        console.log(`[JS] Loading images progressively for ${contacts.length} contacts`);

        // Helper to serialize DB writes
        let lastDbWrite = Promise.resolve();
        const queueDbWrite = (fn) => {
            lastDbWrite = lastDbWrite.then(fn, fn);
            return lastDbWrite;
        };

        const batchSize = 10;
        let i = 0;

        while (i < contacts.length) {
            const batch = contacts.slice(i, i + batchSize);
            await Promise.all(batch.map(async (contact) => {
                if (contact.picture && !contact.picture_data_url && !contact.picture_loading) {
                    try {
                        contact.picture_loading = true;
                        // Update UI to show loading spinner
                        this.renderContactItem(contact);
                        let dataUrl = await window.TauriService.getCachedProfileImage(contact.pubkey);
                        if (!dataUrl) {
                            dataUrl = await window.TauriService.fetchImage(contact.picture);
                            if (dataUrl) {
                                // Update UI immediately after fetch
                                contact.picture_data_url = dataUrl;
                                contact.picture_loaded = true;
                                this.renderContactItem(contact);
                                // Queue DB write, but don't await for UI
                                queueDbWrite(() => window.TauriService.cacheProfileImage(contact.pubkey, dataUrl));
                            }
                        } else {
                            // If found in cache, update UI immediately
                            contact.picture_data_url = dataUrl;
                            contact.picture_loaded = true;
                            this.renderContactItem(contact);
                        }
                    } catch (e) {
                        console.warn(`Failed to cache profile picture for ${contact.name}:`, e);
                    } finally {
                        contact.picture_loading = false;
                        // Always update UI in finally to remove spinner, even if loading failed
                        this.renderContactItem(contact);
                    }
                }
            }));
            i += batchSize;
        }
    }

    // Render individual contact item (for progressive image loading)
    renderContactItem(contact) {
        const contactsList = window.domManager.get('contactsList');
        if (!contactsList) return;
        
        // Find existing contact element
        const existingElement = contactsList.querySelector(`[data-pubkey="${contact.pubkey}"]`);
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
            // console.log(`[JS] Using cached data URL for ${contact.name}`);
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

    // Render contacts
    renderContacts(searchQuery = '') {
        const contactsList = window.domManager.get('contactsList');
        if (!contactsList) {
            console.warn('[JS] renderContacts: contactsList DOM element not found');
            return;
        }

        try {
            contactsList.innerHTML = '';

            // Filter contacts based on search query
            let filteredContacts = window.appState.getContacts();
            if (searchQuery) {
                filteredContacts = window.appState.getContacts().filter(contact => 
                    contact.name.toLowerCase().includes(searchQuery) ||
                    contact.pubkey.toLowerCase().includes(searchQuery) ||
                    (contact.email && contact.email.toLowerCase().includes(searchQuery))
                );
            }

            console.log(`[JS] renderContacts: Rendering ${filteredContacts.length} contacts (searchQuery: "${searchQuery}")`);
            if (filteredContacts && filteredContacts.length > 0) {
                // Sort: contacts with email first
                filteredContacts.sort((a, b) => {
                    const aHasEmail = !!(a.email && a.email.trim());
                    const bHasEmail = !!(b.email && b.email.trim());
                    if (aHasEmail === bHasEmail) return 0;
                    return aHasEmail ? -1 : 1;
                });
                filteredContacts.forEach((contact, index) => {
                    const contactElement = document.createElement('div');
                    contactElement.className = 'contact-item';
                    contactElement.setAttribute('data-pubkey', contact.pubkey);
                    
                    // Add active class if this contact is selected
                    if (window.appState.getSelectedContact() && window.appState.getSelectedContact().pubkey === contact.pubkey) {
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
                    
                    const isValidDataUrl = contact.picture_data_url && contact.picture_data_url.startsWith('data:image') && contact.picture_data_url !== 'data:application/octet-stream;base64,';
                    if (contact.picture_loading) {
                        avatarClass += ' loading';
                    } else if (isValidDataUrl) {
                        avatarSrc = contact.picture_data_url;
                    } else if (contact.picture) {
                        avatarSrc = contact.picture;
                    } else {
                        console.log(`[JS] Using default avatar for ${contact.name} (no cached image available)`);
                    }

                    // Add privacy indicator icon
                    const privacyIcon = contact.is_public === false 
                        ? '<i class="fas fa-lock contact-privacy-icon" title="Private contact"></i>'
                        : '<i class="fas fa-globe contact-privacy-icon" title="Public contact"></i>';
                    
                    contactElement.innerHTML = `
                        <img class="${avatarClass}" src="${avatarSrc}" alt="${contact.name}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
                        <div class="contact-info">
                            <div class="contact-name">${contact.name}</div>
                        </div>
                        <div class="contact-actions">
                            ${privacyIcon}
                            ${emailIcon}
                        </div>
                    `;
                    
                    // Add click event listener
                    contactElement.addEventListener('click', () => this.selectContact(contact));
                    
                    contactsList.appendChild(contactElement);
                });
            } else {
                const message = searchQuery 
                    ? `No contacts found matching "${searchQuery}"`
                    : 'You are not following anyone yet, or contacts could not be loaded.';
                contactsList.innerHTML = `<div class="text-muted text-center">${message}</div>`;
            }
        } catch (error) {
            console.error('Error rendering contacts:', error);
        }
    }

    // Select a contact
    async selectContact(contact) {
        try {
            window.appState.setSelectedContact(contact);
            
            // Update UI - remove active class from all contacts
            document.querySelectorAll('.contact-item').forEach(item => {
                item.classList.remove('active');
            });
            
            // Add active class to selected contact
            const contactElement = document.querySelector(`[data-pubkey="${contact.pubkey}"]`);
            if (contactElement) {
                contactElement.classList.add('active');
            }
            
            // Store the pubkey we're viewing and navigate to profile tab
            // switchTab will automatically load the profile when it sees viewingProfilePubkey is set
            window.appState.setViewingProfilePubkey(contact.pubkey);
            window.app.switchTab('profile');
            
        } catch (error) {
            console.error('Error selecting contact:', error);
            window.notificationService.showError('Failed to load contact profile');
        }
    }

    // Fetch fresh profile data and update contact (called in background)
    async fetchAndUpdateContactProfile(contact) {
        try {
            console.log('[JS] Fetching fresh profile data for:', contact.pubkey);
            const freshProfile = await window.TauriService.fetchProfilePersistent(contact.pubkey);
            
            if (freshProfile && freshProfile.fields) {
                // Merge fresh data with existing contact data
                const updatedContact = {
                    ...contact,
                    name: freshProfile.fields.name || freshProfile.fields.display_name || contact.name,
                    display_name: freshProfile.fields.display_name || freshProfile.fields.name || contact.display_name,
                    picture: freshProfile.fields.picture || contact.picture,
                    email: freshProfile.fields.email || contact.email,
                    about: freshProfile.fields.about || contact.about,
                    fields: {
                        ...contact.fields,
                        ...freshProfile.fields
                    },
                    updated_at: new Date().toISOString()
                };
                
                // Check if data actually changed
                const dataChanged = 
                    updatedContact.name !== contact.name ||
                    updatedContact.email !== contact.email ||
                    updatedContact.picture !== contact.picture ||
                    updatedContact.about !== contact.about;
                
                if (dataChanged) {
                    // Update contact in app state
                    const contacts = window.appState.getContacts();
                    const contactIndex = contacts.findIndex(c => c.pubkey === contact.pubkey);
                    if (contactIndex !== -1) {
                        contacts[contactIndex] = updatedContact;
                        window.appState.setContacts(contacts);
                        // Re-render the contact item in the list if needed
                        this.renderContactItem(updatedContact);
                    }
                    
                    // Update in database
                    try {
                        const userPubkey = window.appState.getKeypair().public_key;
                        const dbContact = window.DatabaseService.convertContactToDbFormat(updatedContact);
                        // Keep existing is_public status (default to true if updating)
                        await window.DatabaseService.saveContact(dbContact, userPubkey, true);
                        console.log('[JS] Updated contact in database:', contact.pubkey);
                    } catch (dbError) {
                        console.warn('[JS] Failed to update contact in database:', dbError);
                    }
                    
                    // Refresh compose dropdown in case email was added/updated
                    if (window.emailService) {
                        window.emailService.populateNostrContactDropdown();
                    }
                    
                    // Only update UI if this contact is still selected
                    const selectedContact = window.appState.getSelectedContact();
                    if (selectedContact && selectedContact.pubkey === contact.pubkey) {
                        // Render with fresh data
                        this.renderContactDetail(updatedContact);
                        window.appState.setSelectedContact(updatedContact);
                        
                        // Show subtle notification that profile was updated
                        window.notificationService.showInfo('Profile updated', 2000);
                    }
                    
                    // If picture URL changed, fetch and cache the new image
                    if (freshProfile.fields.picture && freshProfile.fields.picture !== contact.picture) {
                        // Load image in background
                        this.loadContactImageAsync(updatedContact);
                    }
                } else {
                    console.log('[JS] Profile data unchanged, no update needed');
                }
            } else {
                console.log('[JS] No fresh profile data found');
            }
        } catch (fetchError) {
            console.error('[JS] Failed to fetch fresh profile data:', fetchError);
            // Don't show error to user - they already see cached data
        }
    }


    // Render contact detail
    renderContactDetail(contact) {
        const contactsDetail = window.domManager.get('contactsDetail');
        if (!contactsDetail) return;
        
        try {
            const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
            
            // Determine avatar source
            let avatarSrc = defaultAvatar;
            const isValidDataUrl = contact.picture_data_url && contact.picture_data_url.startsWith('data:image') && contact.picture_data_url !== 'data:application/octet-stream;base64,';
            if (isValidDataUrl) {
                avatarSrc = contact.picture_data_url;
            } else if (contact.picture) {
                avatarSrc = contact.picture;
            }
            
            // Privacy toggle
            const isPublic = contact.is_public !== undefined ? contact.is_public : true;
            const privacyToggleHTML = `
                <div class="privacy-toggle-container">
                    <label class="privacy-toggle" id="privacy-toggle-${contact.pubkey}">
                        <input type="checkbox" ${isPublic ? 'checked' : ''} 
                               onchange="window.contactsService.toggleContactPrivacy('${contact.pubkey}', this.checked, this)">
                        <span class="privacy-toggle-slider"></span>
                        <span class="privacy-toggle-label" id="privacy-label-${contact.pubkey}">${isPublic ? 'Public' : 'Private'}</span>
                    </label>
                    <span class="privacy-toggle-description" id="privacy-desc-${contact.pubkey}">
                        ${isPublic 
                            ? 'Visible in your public follow list' 
                            : 'Not in your public follow list'}
                    </span>
                </div>
            `;
            
            // Always render avatar as an <img> tag
            let avatarElement = `
                <div class="contact-detail-header">
                    <img class="profile-picture" src="${avatarSrc}" alt="${contact.name}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='profile-picture';" style="width:120px;height:120px;object-fit:cover;border-radius:50%;margin-right:20px;">
                    <div class="contact-detail-info">
                        <h3>${contact.name}</h3>
                        <div class="contact-detail-pubkey">
                            <code>${contact.pubkey}</code>
                        </div>
                        ${contact.email ? `<div class="contact-detail-email">${contact.email}</div>` : ''}
                    </div>
                </div>
            `;
            
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
                    if (typeof value === 'string' && value.trim() !== '') {
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
                    ${avatarElement}
                    
                    <div class="contact-detail-section">
                        <h4>Profile Information</h4>
                        ${profileFieldsHTML}
                    </div>
                    
                    <div class="contact-detail-actions">
                        ${privacyToggleHTML}
                        <div class="contact-detail-actions-buttons">
                            ${contact.email ? `
                            <button class="btn btn-primary" onclick="window.contactsService.sendEmailToContact('${contact.email}')">
                                <i class="fas fa-envelope"></i> Send Email
                            </button>
                            ` : ''}
                            <button class="btn btn-secondary" onclick="window.contactsService.sendDirectMessageToContact('${contact.pubkey}')">
                                <i class="fas fa-comments"></i> Send DM
                            </button>
                            <button class="btn btn-secondary" onclick="window.contactsService.copyContactPubkey('${contact.pubkey}')">
                                <i class="fas fa-copy"></i> Copy Public Key
                            </button>
                        </div>
                    </div>
                </div>
            `;
            
            contactsDetail.innerHTML = detailHTML;
            
        } catch (error) {
            console.error('Error rendering contact detail:', error);
            contactsDetail.innerHTML = `
                <div class="contact-detail-placeholder">
                    <i class="fas fa-exclamation-triangle"></i>
                    <p>Error loading contact details</p>
                </div>
            `;
        }
    }

    // Show add contact form
    showAddContactModal(pubkey = '') {
        try {
            // Switch to contacts tab
            const contactsTab = document.querySelector('[data-tab="contacts"]');
            if (contactsTab) {
                contactsTab.click();
            }
            
            // Clear any selected contact
            window.appState.setSelectedContact(null);
            document.querySelectorAll('.contact-item').forEach(item => {
                item.classList.remove('active');
            });
            
            // Show add contact form in a modal
            const modalContent = `
                <div class="add-contact-form">
                    <p class="form-help">Enter a Nostr public key (npub) to fetch their profile</p>
                    
                    <form id="add-contact-form">
                        <div class="form-group">
                            <label for="contact-pubkey">Public Key (npub):</label>
                            <div class="input-with-button">
                                <input type="text" id="contact-pubkey" placeholder="npub1..." value="${pubkey}" required>
                                <button type="button" id="scan-qr-btn" class="btn btn-secondary">
                                    <i class="fas fa-qrcode"></i> Scan QR
                                </button>
                            </div>
                        </div>
                        <div class="form-actions">
                            <button type="submit" class="btn btn-primary">
                                <i class="fas fa-search"></i> Fetch Profile
                            </button>
                        </div>
                    </form>
                </div>
            `;
            
            window.app.showModal('Add New Contact', modalContent);
            
            // Set up event listeners after modal is shown
            setTimeout(() => {
                const form = document.getElementById('add-contact-form');
                const scanBtn = document.getElementById('scan-qr-btn');
                
                if (form) {
                    form.addEventListener('submit', (e) => {
                        e.preventDefault();
                        this.addContact(e);
                    });
                }
                if (scanBtn) {
                    scanBtn.addEventListener('click', () => this.scanQRCode());
                }
                
                // If pubkey is provided, focus the input
                if (pubkey) {
                    const pubkeyInput = document.getElementById('contact-pubkey');
                    if (pubkeyInput) {
                        pubkeyInput.focus();
                    }
                }
            }, 100);
        } catch (error) {
            console.error('Error showing add contact form:', error);
        }
    }

    // Scan QR code
    async scanQRCode() {
        try {
            // Show QR scanner in the detail panel
            const contactsDetail = window.domManager.get('contactsDetail');
            if (contactsDetail) {
                contactsDetail.innerHTML = `
                    <div id="qr-scanner-container">
                        <h3>Scan QR Code</h3>
                        <div id="qr-reader"></div>
                        <div class="qr-scanner-controls">
                            <button id="close-scanner-btn" class="btn btn-secondary">
                                <i class="fas fa-times"></i> Back to Form
                            </button>
                        </div>
                    </div>
                `;
            }
            
            // Let the browser handle camera permissions directly
            if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
                try {
                    const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' } });
                    
                    // Create video element for camera feed
                    const video = document.createElement('video');
                    video.style.width = '100%';
                    video.style.height = 'auto';
                    video.style.maxWidth = '400px';
                    video.autoplay = true;
                    video.muted = true;
                    video.playsInline = true;
                    
                    const qrReader = document.getElementById('qr-reader');
                    if (qrReader) {
                        qrReader.appendChild(video);
                        video.srcObject = stream;
                        
                        // Simple QR code detection (you might want to use a proper QR library)
                        const canvas = document.createElement('canvas');
                        const context = canvas.getContext('2d');
                        
                        const checkQR = () => {
                            if (video.videoWidth > 0) {
                                canvas.width = video.videoWidth;
                                canvas.height = video.videoHeight;
                                context.drawImage(video, 0, 0);
                                
                                // Here you would implement QR code detection
                                // For now, we'll just show the camera feed
                            }
                            requestAnimationFrame(checkQR);
                        };
                        
                        checkQR();
                    }
                    
                    // Handle close scanner button
                    const closeBtn = document.getElementById('close-scanner-btn');
                    if (closeBtn) {
                        closeBtn.addEventListener('click', () => {
                            stream.getTracks().forEach(track => track.stop());
                            this.showAddContactModal();
                        });
                    }
                    
                } catch (error) {
                    console.error('Camera access error:', error);
                    this.showCameraError();
                }
            } else {
                this.showCameraError();
            }
            
        } catch (error) {
            console.error('QR scanner error:', error);
            this.showCameraError();
        }
    }

    // Show camera error
    showCameraError() {
        const contactsDetail = window.domManager.get('contactsDetail');
        if (contactsDetail) {
            contactsDetail.innerHTML = `
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
                        <button onclick="window.contactsService.showAddContactModal()" class="btn btn-primary">Enter Manually</button>
                    </div>
                </div>
            `;
        }
    }

    // Add contact
    async addContact(event) {
        event.preventDefault();
        
        try {
            const pubkey = document.getElementById('contact-pubkey')?.value || '';
            
            // Validate public key
            try {
                if (!pubkey.startsWith('npub1') && !pubkey.startsWith('nsec1')) {
                    window.notificationService.showError('Invalid public key format. Must start with npub1 or nsec1');
                    return;
                }
            } catch (e) {
                window.notificationService.showError('Invalid public key format');
                return;
            }
            
            // Show loading notification
            window.notificationService.showInfo('Fetching profile...');
            
            // Close the modal
            window.app.hideModal();
            
            // Fetch the profile
            try {
                const activeRelays = window.appState.getActiveRelays();
                if (activeRelays.length === 0) {
                    window.notificationService.showError('No active relays to fetch profile');
                    return;
                }
                
                console.log('Fetching profile for:', pubkey);
                const profile = await window.TauriService.fetchProfilePersistent(pubkey);
                
                if (profile) {
                    // Navigate to profile page to show the profile
                    window.appState.setViewingProfilePubkey(pubkey);
                    window.app.switchTab('profile');
                    await window.app.loadProfile(pubkey);
                    
                    // Show notification that profile was fetched
                    window.notificationService.showSuccess('Profile fetched successfully');
                } else {
                    window.notificationService.showError('Could not fetch profile for this public key');
                    // Show the add contact form again
                    this.showAddContactModal(pubkey);
                }
            } catch (error) {
                console.error('Failed to fetch profile:', error);
                window.notificationService.showError('Failed to fetch profile: ' + error);
                // Show the add contact form again
                this.showAddContactModal(pubkey);
            }
            
        } catch (error) {
            console.error('Error adding contact:', error);
            window.notificationService.showError('Failed to add contact');
        }
    }

    // Helper function to update follow button text based on privacy toggle
    updateFollowButtonText(button, isPublic, contactExists) {
        if (contactExists) {
            button.innerHTML = '<i class="fas fa-sync"></i> Update Follow Status';
        } else {
            if (isPublic) {
                button.innerHTML = '<i class="fas fa-user-plus"></i> Follow and Add Contact Publicly';
            } else {
                button.innerHTML = '<i class="fas fa-user-plus"></i> Add Contact Privately';
            }
        }
    }

    // Show profile for adding (not following yet)
    showProfileForAdding(pubkey, profile) {
        const contactsDetail = window.domManager.get('contactsDetail');
        if (!contactsDetail) return;
        
        try {
            console.log('Fetched profile:', profile);
            console.log('Profile picture field:', profile.fields.picture);
            const profileName = profile.fields.name || profile.fields.display_name || pubkey.substring(0, 16) + '...';
            const profileAbout = profile.fields.about || 'No bio available';
            const profilePicture = profile.fields.picture || '';
            
            // Create a temporary contact object for rendering
            const tempContact = {
                pubkey: pubkey,
                name: profileName,
                picture: profilePicture,
                email: profile.fields.email || null,
                fields: profile.fields,
                picture_data_url: null,
                picture_loaded: false,
                picture_loading: false
            };
            
            // Use the existing contact detail rendering function
            this.renderContactDetail(tempContact);
            
            // Add a follow button to the actions
            const actionsDiv = contactsDetail.querySelector('.contact-detail-actions');
            if (actionsDiv) {
                // Remove any existing follow button and checkbox
                const existingFollowBtn = actionsDiv.querySelector('.follow-btn');
                if (existingFollowBtn) {
                    existingFollowBtn.remove();
                }
                const existingFollowSection = actionsDiv.querySelector('.follow-section');
                if (existingFollowSection) {
                    existingFollowSection.remove();
                }
                
                // Check if contact already exists in user's contacts
                const existingContacts = window.appState.getContacts() || [];
                const contactExists = existingContacts.some(c => c.pubkey === pubkey);
                
                // Create follow section
                const followSection = document.createElement('div');
                followSection.className = 'follow-section';
                followSection.style.marginBottom = '1rem';
                followSection.style.padding = '1rem';
                followSection.style.border = '1px solid var(--border-color, #333)';
                followSection.style.borderRadius = '8px';
                followSection.style.backgroundColor = 'var(--card-bg, rgba(255,255,255,0.05))';
                
                // Create follow button first so it can be referenced in checkbox handler
                const followBtn = document.createElement('button');
                followBtn.className = 'btn btn-primary follow-btn';
                followBtn.style.width = '100%';
                
                // Only show checkbox if contact already exists (allows toggling public/private)
                if (contactExists) {
                    // Find the existing contact to get its current is_public status
                    const existingContact = existingContacts.find(c => c.pubkey === pubkey);
                    const currentIsPublic = existingContact?.is_public !== undefined ? existingContact.is_public : true;
                    
                    // Add checkbox for public/private follow
                    const checkboxContainer = document.createElement('div');
                    checkboxContainer.style.marginBottom = '0.75rem';
                    checkboxContainer.style.display = 'flex';
                    checkboxContainer.style.alignItems = 'center';
                    checkboxContainer.style.gap = '0.5rem';
                    
                    const checkbox = document.createElement('input');
                    checkbox.type = 'checkbox';
                    checkbox.id = 'follow-public-checkbox';
                    checkbox.checked = currentIsPublic;
                    checkbox.style.cursor = 'pointer';
                    
                    const checkboxLabel = document.createElement('label');
                    checkboxLabel.htmlFor = 'follow-public-checkbox';
                    checkboxLabel.style.cursor = 'pointer';
                    checkboxLabel.style.display = 'flex';
                    checkboxLabel.style.alignItems = 'center';
                    checkboxLabel.style.gap = '0.5rem';
                    checkboxLabel.innerHTML = currentIsPublic ? `
                        <i class="fas fa-globe" style="color: #4CAF50;"></i>
                        <span>Public follow (visible in your follow list)</span>
                    ` : `
                        <i class="fas fa-lock" style="color: #FF9800;"></i>
                        <span>Private follow (not in your follow list)</span>
                    `;
                    
                    // Update label and button text when checkbox changes
                    const updateButtonText = () => {
                        const isPublic = checkbox.checked;
                        this.updateFollowButtonText(followBtn, isPublic, contactExists);
                    };
                    
                    checkbox.addEventListener('change', () => {
                        if (checkbox.checked) {
                            checkboxLabel.innerHTML = `
                                <i class="fas fa-globe" style="color: #4CAF50;"></i>
                                <span>Public follow (visible in your follow list)</span>
                            `;
                        } else {
                            checkboxLabel.innerHTML = `
                                <i class="fas fa-lock" style="color: #FF9800;"></i>
                                <span>Private follow (not in your follow list)</span>
                            `;
                        }
                        updateButtonText();
                    });
                    
                    checkboxContainer.appendChild(checkbox);
                    checkboxContainer.appendChild(checkboxLabel);
                    followSection.appendChild(checkboxContainer);
                    
                    // Set initial button text based on checkbox state
                    this.updateFollowButtonText(followBtn, currentIsPublic, contactExists);
                } else {
                    // For new contacts, default to public
                    this.updateFollowButtonText(followBtn, true, contactExists);
                }
                
                followBtn.addEventListener('click', () => {
                    // Check both the follow section checkbox (if exists) and the privacy toggle
                    const followCheckbox = followSection.querySelector('#follow-public-checkbox');
                    const privacyToggle = document.querySelector(`#privacy-toggle-${pubkey} input[type="checkbox"]`);
                    const isPublic = contactExists 
                        ? (followCheckbox?.checked ?? privacyToggle?.checked ?? true)
                        : (privacyToggle?.checked ?? true); // For new contacts, use privacy toggle
                    this.confirmFollowContact(pubkey, profile, profileName, profile.fields.email || '', isPublic);
                });
                
                followSection.appendChild(followBtn);
                
                // Insert at the beginning of actions
                actionsDiv.insertBefore(followSection, actionsDiv.firstChild);
                
                // For new contacts, connect the follow button to the privacy toggle
                if (!contactExists) {
                    const privacyToggle = document.querySelector(`#privacy-toggle-${pubkey} input[type="checkbox"]`);
                    if (privacyToggle) {
                        // Update button text when privacy toggle changes
                        const updateFollowButtonFromToggle = () => {
                            const isPublic = privacyToggle.checked;
                            this.updateFollowButtonText(followBtn, isPublic, contactExists);
                        };
                        
                        // Set initial button text based on privacy toggle state
                        updateFollowButtonFromToggle();
                        
                        // Listen to privacy toggle changes
                        privacyToggle.addEventListener('change', updateFollowButtonFromToggle);
                    }
                }
            }
            
            // Try to load and cache the profile picture (but don't re-render the entire profile)
            if (profilePicture) {
                tempContact.picture_loading = true;
                window.TauriService.fetchImage(profilePicture).then(dataUrl => {
                    if (dataUrl && dataUrl.startsWith('data:image')) {
                        console.log('Fetched image dataUrl (start):', dataUrl.substring(0, 80));
                        tempContact.picture_data_url = dataUrl;
                        tempContact.picture_loaded = true;
                        tempContact.picture_loading = false;
                        // Re-render the profile with the new image
                        this.renderContactDetail(tempContact);
                        // Re-add the follow button after re-render
                        const actionsDiv2 = contactsDetail.querySelector('.contact-detail-actions');
                        if (actionsDiv2) {
                            const existingFollowBtn2 = actionsDiv2.querySelector('.follow-btn');
                            if (existingFollowBtn2) existingFollowBtn2.remove();
                            const existingFollowSection2 = actionsDiv2.querySelector('.follow-section');
                            if (existingFollowSection2) existingFollowSection2.remove();
                            
                            // Check if contact already exists in user's contacts
                            const existingContacts = window.appState.getContacts() || [];
                            const contactExists = existingContacts.some(c => c.pubkey === pubkey);
                            
                            // Create follow section
                            const followSection2 = document.createElement('div');
                            followSection2.className = 'follow-section';
                            followSection2.style.marginBottom = '1rem';
                            followSection2.style.padding = '1rem';
                            followSection2.style.border = '1px solid var(--border-color, #333)';
                            followSection2.style.borderRadius = '8px';
                            followSection2.style.backgroundColor = 'var(--card-bg, rgba(255,255,255,0.05))';
                            
                            // Create follow button first so it can be referenced in checkbox handler
                            const followBtn2 = document.createElement('button');
                            followBtn2.className = 'btn btn-primary follow-btn';
                            followBtn2.style.width = '100%';
                            
                            // Only show checkbox if contact already exists
                            if (contactExists) {
                                const existingContact = existingContacts.find(c => c.pubkey === pubkey);
                                const currentIsPublic = existingContact?.is_public !== undefined ? existingContact.is_public : true;
                                
                                const checkboxContainer2 = document.createElement('div');
                                checkboxContainer2.style.marginBottom = '0.75rem';
                                checkboxContainer2.style.display = 'flex';
                                checkboxContainer2.style.alignItems = 'center';
                                checkboxContainer2.style.gap = '0.5rem';
                                
                                const checkbox2 = document.createElement('input');
                                checkbox2.type = 'checkbox';
                                checkbox2.id = 'follow-public-checkbox-2';
                                checkbox2.checked = currentIsPublic;
                                checkbox2.style.cursor = 'pointer';
                                
                                const checkboxLabel2 = document.createElement('label');
                                checkboxLabel2.htmlFor = 'follow-public-checkbox-2';
                                checkboxLabel2.style.cursor = 'pointer';
                                checkboxLabel2.style.display = 'flex';
                                checkboxLabel2.style.alignItems = 'center';
                                checkboxLabel2.style.gap = '0.5rem';
                                checkboxLabel2.innerHTML = currentIsPublic ? `
                                    <i class="fas fa-globe" style="color: #4CAF50;"></i>
                                    <span>Public follow (visible in your follow list)</span>
                                ` : `
                                    <i class="fas fa-lock" style="color: #FF9800;"></i>
                                    <span>Private follow (not in your follow list)</span>
                                `;
                                
                                // Update label and button text when checkbox changes
                                const updateButtonText2 = () => {
                                    const isPublic = checkbox2.checked;
                                    this.updateFollowButtonText(followBtn2, isPublic, contactExists);
                                };
                                
                                checkbox2.addEventListener('change', () => {
                                    if (checkbox2.checked) {
                                        checkboxLabel2.innerHTML = `
                                            <i class="fas fa-globe" style="color: #4CAF50;"></i>
                                            <span>Public follow (visible in your follow list)</span>
                                        `;
                                    } else {
                                        checkboxLabel2.innerHTML = `
                                            <i class="fas fa-lock" style="color: #FF9800;"></i>
                                            <span>Private follow (not in your follow list)</span>
                                        `;
                                    }
                                    updateButtonText2();
                                });
                                
                                checkboxContainer2.appendChild(checkbox2);
                                checkboxContainer2.appendChild(checkboxLabel2);
                                followSection2.appendChild(checkboxContainer2);
                                
                                // Set initial button text based on checkbox state
                                this.updateFollowButtonText(followBtn2, currentIsPublic, contactExists);
                            } else {
                                // For new contacts, default to public
                                this.updateFollowButtonText(followBtn2, true, contactExists);
                            }
                            
                            followBtn2.addEventListener('click', () => {
                                // Check both the follow section checkbox (if exists) and the privacy toggle
                                const followCheckbox = followSection2.querySelector('#follow-public-checkbox-2');
                                const privacyToggle = document.querySelector(`#privacy-toggle-${pubkey} input[type="checkbox"]`);
                                const isPublic = contactExists 
                                    ? (followCheckbox?.checked ?? privacyToggle?.checked ?? true)
                                    : (privacyToggle?.checked ?? true); // For new contacts, use privacy toggle
                                this.confirmFollowContact(pubkey, profile, profileName, profile.fields.email || '', isPublic);
                            });
                            
                            followSection2.appendChild(followBtn2);
                            actionsDiv2.insertBefore(followSection2, actionsDiv2.firstChild);
                            
                            // For new contacts, connect the follow button to the privacy toggle
                            if (!contactExists) {
                                const privacyToggle = document.querySelector(`#privacy-toggle-${pubkey} input[type="checkbox"]`);
                                if (privacyToggle) {
                                    // Update button text when privacy toggle changes
                                    const updateFollowButtonFromToggle = () => {
                                        const isPublic = privacyToggle.checked;
                                        this.updateFollowButtonText(followBtn2, isPublic, contactExists);
                                    };
                                    
                                    // Set initial button text based on privacy toggle state
                                    updateFollowButtonFromToggle();
                                    
                                    // Listen to privacy toggle changes
                                    privacyToggle.addEventListener('change', updateFollowButtonFromToggle);
                                }
                            }
                        }
                    } else {
                        // Fallback: use the raw URL as the picture
                        console.log('Fetched image dataUrl: null or not an image, falling back to raw URL');
                        tempContact.picture_data_url = null;
                        tempContact.picture = profilePicture;
                        tempContact.picture_loading = false;
                        this.renderContactDetail(tempContact);
                        // Re-add the follow button after re-render
                        const actionsDiv2 = contactsDetail.querySelector('.contact-detail-actions');
                        if (actionsDiv2) {
                            const existingFollowBtn2 = actionsDiv2.querySelector('.follow-btn');
                            if (existingFollowBtn2) existingFollowBtn2.remove();
                            const existingFollowSection2 = actionsDiv2.querySelector('.follow-section');
                            if (existingFollowSection2) existingFollowSection2.remove();
                            
                            // Check if contact already exists in user's contacts
                            const existingContacts = window.appState.getContacts() || [];
                            const contactExists = existingContacts.some(c => c.pubkey === pubkey);
                            
                            // Create follow section
                            const followSection2 = document.createElement('div');
                            followSection2.className = 'follow-section';
                            followSection2.style.marginBottom = '1rem';
                            followSection2.style.padding = '1rem';
                            followSection2.style.border = '1px solid var(--border-color, #333)';
                            followSection2.style.borderRadius = '8px';
                            followSection2.style.backgroundColor = 'var(--card-bg, rgba(255,255,255,0.05))';
                            
                            // Create follow button first so it can be referenced in checkbox handler
                            const followBtn2 = document.createElement('button');
                            followBtn2.className = 'btn btn-primary follow-btn';
                            followBtn2.style.width = '100%';
                            
                            // Only show checkbox if contact already exists
                            if (contactExists) {
                                const existingContact = existingContacts.find(c => c.pubkey === pubkey);
                                const currentIsPublic = existingContact?.is_public !== undefined ? existingContact.is_public : true;
                                
                                const checkboxContainer2 = document.createElement('div');
                                checkboxContainer2.style.marginBottom = '0.75rem';
                                checkboxContainer2.style.display = 'flex';
                                checkboxContainer2.style.alignItems = 'center';
                                checkboxContainer2.style.gap = '0.5rem';
                                
                                const checkbox2 = document.createElement('input');
                                checkbox2.type = 'checkbox';
                                checkbox2.id = 'follow-public-checkbox-3';
                                checkbox2.checked = currentIsPublic;
                                checkbox2.style.cursor = 'pointer';
                                
                                const checkboxLabel2 = document.createElement('label');
                                checkboxLabel2.htmlFor = 'follow-public-checkbox-3';
                                checkboxLabel2.style.cursor = 'pointer';
                                checkboxLabel2.style.display = 'flex';
                                checkboxLabel2.style.alignItems = 'center';
                                checkboxLabel2.style.gap = '0.5rem';
                                checkboxLabel2.innerHTML = currentIsPublic ? `
                                    <i class="fas fa-globe" style="color: #4CAF50;"></i>
                                    <span>Public follow (visible in your follow list)</span>
                                ` : `
                                    <i class="fas fa-lock" style="color: #FF9800;"></i>
                                    <span>Private follow (not in your follow list)</span>
                                `;
                                
                                // Update label and button text when checkbox changes
                                const updateButtonText3 = () => {
                                    const isPublic = checkbox2.checked;
                                    this.updateFollowButtonText(followBtn2, isPublic, contactExists);
                                };
                                
                                checkbox2.addEventListener('change', () => {
                                    if (checkbox2.checked) {
                                        checkboxLabel2.innerHTML = `
                                            <i class="fas fa-globe" style="color: #4CAF50;"></i>
                                            <span>Public follow (visible in your follow list)</span>
                                        `;
                                    } else {
                                        checkboxLabel2.innerHTML = `
                                            <i class="fas fa-lock" style="color: #FF9800;"></i>
                                            <span>Private follow (not in your follow list)</span>
                                        `;
                                    }
                                    updateButtonText3();
                                });
                                
                                checkboxContainer2.appendChild(checkbox2);
                                checkboxContainer2.appendChild(checkboxLabel2);
                                followSection2.appendChild(checkboxContainer2);
                                
                                // Set initial button text based on checkbox state
                                this.updateFollowButtonText(followBtn2, currentIsPublic, contactExists);
                            } else {
                                // For new contacts, default to public
                                this.updateFollowButtonText(followBtn2, true, contactExists);
                            }
                            
                            followBtn2.addEventListener('click', () => {
                                // Check both the follow section checkbox (if exists) and the privacy toggle
                                const followCheckbox = followSection2.querySelector('#follow-public-checkbox-3');
                                const privacyToggle = document.querySelector(`#privacy-toggle-${pubkey} input[type="checkbox"]`);
                                const isPublic = contactExists 
                                    ? (followCheckbox?.checked ?? privacyToggle?.checked ?? true)
                                    : (privacyToggle?.checked ?? true); // For new contacts, use privacy toggle
                                this.confirmFollowContact(pubkey, profile, profileName, profile.fields.email || '', isPublic);
                            });
                            
                            followSection2.appendChild(followBtn2);
                            actionsDiv2.insertBefore(followSection2, actionsDiv2.firstChild);
                            
                            // For new contacts, connect the follow button to the privacy toggle
                            if (!contactExists) {
                                const privacyToggle = document.querySelector(`#privacy-toggle-${pubkey} input[type="checkbox"]`);
                                if (privacyToggle) {
                                    // Update button text when privacy toggle changes
                                    const updateFollowButtonFromToggle = () => {
                                        const isPublic = privacyToggle.checked;
                                        this.updateFollowButtonText(followBtn2, isPublic, contactExists);
                                    };
                                    
                                    // Set initial button text based on privacy toggle state
                                    updateFollowButtonFromToggle();
                                    
                                    // Listen to privacy toggle changes
                                    privacyToggle.addEventListener('change', updateFollowButtonFromToggle);
                                }
                            }
                        }
                    }
                }).catch(e => {
                    console.warn('Failed to load profile picture:', e);
                    tempContact.picture_loading = false;
                });
            }
            
        } catch (error) {
            console.error('Error showing profile for adding:', error);
            contactsDetail.innerHTML = `
                <div class="contact-detail-placeholder">
                    <i class="fas fa-exclamation-triangle"></i>
                    <p>Error loading profile</p>
                </div>
            `;
        }
    }

    // Send direct message to contact
    sendDirectMessageToContact(pubkey) {
        // Switch to DM tab
        const dmTab = document.querySelector('[data-tab="dm"]');
        if (dmTab) {
            dmTab.click();
        }
        
        // Use the DM service to handle the DM functionality
        if (window.dmService && typeof window.dmService.sendDirectMessageToContact === 'function') {
            window.dmService.sendDirectMessageToContact(pubkey);
        } else {
            console.error('DM service not available');
            window.notificationService.showError('DM service not available');
        }
    }

    // Copy contact public key to clipboard
    copyContactPubkey(pubkey) {
        try {
            navigator.clipboard.writeText(pubkey).then(() => {
                window.notificationService.showSuccess('Public key copied to clipboard');
            }).catch(() => {
                // Fallback for older browsers
                const textArea = document.createElement('textarea');
                textArea.value = pubkey;
                document.body.appendChild(textArea);
                textArea.select();
                document.execCommand('copy');
                document.body.removeChild(textArea);
                window.notificationService.showSuccess('Public key copied to clipboard');
            });
        } catch (error) {
            console.error('Failed to copy public key:', error);
            window.notificationService.showError('Failed to copy public key');
        }
    }

    // Send email to contact
    sendEmailToContact(email) {
        // Switch to compose tab
        const composeTab = document.querySelector('[data-tab="compose"]');
        if (composeTab) {
            composeTab.click();
        }
        
        // Find the contact by email address
        const contacts = window.appState.getContacts();
        const contact = contacts.find(c => c.email && c.email.toLowerCase() === email.toLowerCase());
        
        // Wait a moment for the tab to switch, then fill in the form
        setTimeout(() => {
            // Fill in the email address
            const toAddressInput = window.domManager.get('toAddress');
            if (toAddressInput) {
                toAddressInput.value = email;
            }
            
            // If contact found, set it as selected for encryption
            if (contact && window.emailService) {
                window.emailService.selectedNostrContact = contact;
                
                // Update the Nostr contact dropdown
                const dropdown = window.domManager.get('nostrContactSelect');
                if (dropdown) {
                    dropdown.value = contact.pubkey;
                }
                
                // Display the recipient pubkey
                const pubkeyDisplay = document.getElementById('selected-recipient-pubkey');
                const pubkeyValue = document.getElementById('recipient-pubkey-value');
                if (pubkeyDisplay && pubkeyValue) {
                    pubkeyValue.textContent = contact.pubkey;
                    pubkeyDisplay.style.display = 'block';
                }
                
                // Update the UI to show it's an encrypted email
                if (toAddressInput) {
                    const isDarkMode = document.body.classList.contains('dark-mode');
                    toAddressInput.style.borderColor = '#667eea';
                    toAddressInput.style.backgroundColor = isDarkMode ? '#1a1f3a' : '#f8f9ff';
                }
                
                window.notificationService.showSuccess(`Email address and recipient pubkey filled in for ${contact.name}`);
            } else {
                window.notificationService.showSuccess(`Email address filled in: ${email}`);
            }
        }, 100);
    }

    // Refresh contacts from network (called by refresh button)
    async refreshContacts() {
        console.log('[JS] refreshContacts called - fetching fresh data from network...');
        if (!window.appState.hasKeypair()) {
            window.notificationService.showError('No keypair available');
            return;
        }
        const activeRelays = window.appState.getActiveRelays();
        if (activeRelays.length === 0) {
            window.notificationService.showError('No active relays configured');
            return;
        }
        try {
            // 1. Fetch followed npubs using persistent client (more efficient!)
            const followedPubkeys = await window.TauriService.fetchFollowingPubkeysPersistent(
                window.appState.getKeypair().public_key
            );
            console.log('[JS] Fetched followed npubs using persistent client:', followedPubkeys);
            if (!followedPubkeys || followedPubkeys.length === 0) {
                window.notificationService.showInfo('No follows found on the network');
                return;
            }
            const userPubkey = window.appState.getKeypair().public_key;
            
            // 2. Get existing public contacts from DB
            const existingPublicPubkeys = await window.__TAURI__.core.invoke('db_get_public_contact_pubkeys', {
                userPubkey: userPubkey
            });
            console.log('[JS] Existing public contacts in DB:', existingPublicPubkeys.length);
            
            // Handle case where no follows found on network
            if (!followedPubkeys || followedPubkeys.length === 0) {
                // If there were public contacts but now none, convert them all to private
                if (existingPublicPubkeys.length > 0) {
                    console.log('[JS] No follows found on network, converting all public contacts to private');
                    for (const pubkey of existingPublicPubkeys) {
                        await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                            userPubkey: userPubkey,
                            contactPubkey: pubkey,
                            isPublic: false
                        });
                    }
                    const localContacts = await window.DatabaseService.getAllContacts(userPubkey);
                    window.appState.setContacts(localContacts);
                    this.renderContacts();
                    window.notificationService.showInfo(`No follows found on network. ${existingPublicPubkeys.length} contacts converted to private.`);
                } else {
                    window.notificationService.showInfo('No follows found on the network');
                }
                return;
            }
            
            // 3. Find contacts that were publicly followed but are no longer in the follow list
            const followedPubkeysSet = new Set(followedPubkeys);
            const unfollowedPubkeys = existingPublicPubkeys.filter(pubkey => !followedPubkeysSet.has(pubkey));
            
            if (unfollowedPubkeys.length > 0) {
                console.log('[JS] Detected unfollowed contacts (no longer in public follow list):', unfollowedPubkeys);
                // Convert public follows to private (preserve contact data but mark as private)
                for (const pubkey of unfollowedPubkeys) {
                    await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                        userPubkey: userPubkey,
                        contactPubkey: pubkey,
                        isPublic: false
                    });
                }
                console.log(`[JS] Converted ${unfollowedPubkeys.length} public contacts to private`);
            }
            
            // 4. Filter out npubs already in the DB for this user
            const newPubkeys = await window.TauriService.filterNewContacts(userPubkey, followedPubkeys);
            console.log('[JS] New npubs not in DB:', newPubkeys);
            
            // 5. Fetch profiles for only new npubs using persistent client
            let newProfiles = [];
            if (newPubkeys.length > 0) {
                console.log(`[JS] Calling fetchProfilesPersistent for ${newPubkeys.length} pubkeys...`);
                newProfiles = await window.TauriService.fetchProfilesPersistent(newPubkeys);
                console.log(`[JS] Fetched new profiles using persistent client: ${newProfiles ? newProfiles.length : 0} profiles`);
                if (newProfiles && newProfiles.length > 0) {
                    console.log('[JS] Profile details:', newProfiles.map(p => ({ pubkey: p.pubkey, name: p.fields?.name || p.fields?.display_name || 'no name' })));
                } else {
                    console.warn('[JS] fetchProfilesPersistent returned empty or null result');
                }
            }
            
            // 6. Get all local contacts for this user (before checking re-follows)
            const allContacts = await window.DatabaseService.getAllContacts(userPubkey);
            const allContactPubkeys = new Set(allContacts.map(c => c.pubkey));
            
            // 7. Update is_public for contacts that are back in the follow list (were private, now public again)
            const reFollowedPubkeys = followedPubkeys.filter(pubkey => {
                // Contact exists in user_contacts but wasn't in the public list (must have been private)
                return allContactPubkeys.has(pubkey) && !existingPublicPubkeys.includes(pubkey);
            });
            if (reFollowedPubkeys.length > 0) {
                console.log('[JS] Contacts re-added to public follow list:', reFollowedPubkeys);
                for (const pubkey of reFollowedPubkeys) {
                    await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                        userPubkey: userPubkey,
                        contactPubkey: pubkey,
                        isPublic: true
                    });
                }
            }
            
            // 8. Get all local contacts for this user (after updates)
            const localContacts = await window.DatabaseService.getAllContacts(userPubkey);
            
            // 8. Add new contacts to app state and DB
            if (newProfiles.length > 0) {
                // Create new contact objects
                const newContacts = newProfiles.map(profile => ({
                    pubkey: profile.pubkey,
                    name: profile.fields.name || profile.fields.display_name || profile.pubkey.substring(0, 16) + '...',
                    picture: profile.fields.picture || '',
                    email: profile.fields.email || null,
                    fields: profile.fields || {},
                    picture_data_url: null,
                    picture_loading: false,
                    picture_loaded: false,
                    created_at: new Date().toISOString(),
                    updated_at: new Date().toISOString()
                }));
                // Add to app state
                const updatedContacts = [...localContacts, ...newContacts];
                window.appState.setContacts(updatedContacts);
                this.renderContacts();
                this.loadContactImagesProgressively();
                // Save new contacts to DB with user-contact relationship
                // These are public follows from Nostr, so is_public=true
                for (const contact of newContacts) {
                    const dbContact = window.DatabaseService.convertContactToDbFormat(contact);
                    await window.DatabaseService.saveContact(dbContact, userPubkey, true); // is_public=true for public follows
                }
                // Refresh compose dropdown to include new contacts
                if (window.emailService) {
                    window.emailService.populateNostrContactDropdown();
                }
                const message = unfollowedPubkeys.length > 0 
                    ? `Added ${newContacts.length} new contacts. ${unfollowedPubkeys.length} contacts converted to private.`
                    : `Added ${newContacts.length} new contacts`;
                window.notificationService.showSuccess(message);
            } else {
                // No new contacts, just re-render
                window.appState.setContacts(localContacts);
                this.renderContacts();
                if (unfollowedPubkeys.length > 0) {
                    window.notificationService.showInfo(`${unfollowedPubkeys.length} contacts converted to private (no longer in public follow list)`);
                } else {
                    window.notificationService.showInfo('No new contacts found in your follow list');
                }
            }
        } catch (error) {
            console.error('Failed to load contacts from network:', error);
            window.notificationService.showError('Failed to refresh contacts: ' + error);
        }
    }

    async confirmFollowContact(pubkey, profile, profileName, email, isPublic = true) {
        // Show a confirmation dialog
        const followType = isPublic ? 'publicly' : 'privately';
        const confirmed = await window.notificationService.showConfirmation(
            `Do you want to ${followType} follow nostr user ${pubkey}?`,
            'Confirm Follow'
        );
        if (!confirmed) return;

        try {
            // Get your private key and relays
            const privateKey = window.appState.getKeypair().private_key;
            const relays = window.appState.getActiveRelays();
            const userPubkey = window.appState.getKeypair().public_key;

            // Only call backend to follow the user on Nostr if it's a public follow
            if (isPublic) {
                await window.TauriService.followUser(privateKey, pubkey, relays);
            }

            // Save the contact locally with the appropriate is_public flag
            const now = new Date().toISOString();
            const contact = {
                pubkey,
                name: profileName,
                email,
                picture: profile.fields.picture || '',
                fields: profile.fields,
                picture_data_url: null,
                picture_loaded: false,
                picture_loading: false,
                created_at: now,
                updated_at: now
            };
            const dbContact = window.DatabaseService.convertContactToDbFormat(contact);
            await window.DatabaseService.saveContact(dbContact, userPubkey, isPublic);

            // Update app state and UI
            window.appState.addContact(contact);
            this.renderContacts();
            // Refresh compose dropdown to include new contact
            if (window.emailService) {
                window.emailService.populateNostrContactDropdown();
            }
            const followTypeMsg = isPublic ? 'publicly' : 'privately';
            window.notificationService.showSuccess(`You are now ${followTypeMsg} following ${profileName}!`);
        } catch (error) {
            window.notificationService.showError('Failed to follow and add contact: ' + error);
        }
    }

    // Refresh the selected contact's profile
    async refreshSelectedContactProfile() {
        const selectedContact = window.appState.getSelectedContact();
        if (!selectedContact) {
            return;
        }
        const pubkey = selectedContact.pubkey;
        const activeRelays = window.appState.getActiveRelays();
        if (!pubkey || activeRelays.length === 0) {
            return;
        }
        try {
            const profile = await window.TauriService.fetchProfilePersistent(pubkey);
            if (profile) {
                // Update the contact object with new profile fields
                selectedContact.name = profile.fields.name || profile.fields.display_name || pubkey.substring(0, 16) + '...';
                selectedContact.picture = profile.fields.picture || '';
                selectedContact.email = profile.fields.email || null;
                selectedContact.fields = profile.fields || {};
                // Optionally update picture_data_url if picture changed
                if (selectedContact.picture) {
                    try {
                        let dataUrl = await window.TauriService.getCachedProfileImage(pubkey);
                        if (!dataUrl) {
                            dataUrl = await window.TauriService.fetchImage(selectedContact.picture);
                            if (dataUrl) {
                                await window.TauriService.cacheProfileImage(pubkey, dataUrl);
                            }
                        }
                        if (dataUrl) {
                            selectedContact.picture_data_url = dataUrl;
                            selectedContact.picture_loaded = true;
                        }
                    } catch (e) {
                        // Ignore image errors
                    }
                }
                // Update in app state
                const contacts = window.appState.getContacts();
                const idx = contacts.findIndex(c => c.pubkey === pubkey);
                if (idx !== -1) {
                    contacts[idx] = selectedContact;
                    window.appState.setContacts(contacts);
                    // Save updated contact to the database
                    const userPubkey = window.appState.getKeypair().public_key;
                    const dbContact = window.DatabaseService.convertContactToDbFormat(selectedContact);
                    // Keep existing is_public status (default to true if updating)
                    await window.DatabaseService.saveContact(dbContact, userPubkey, true);
                }
                // Re-render the contact detail
                this.renderContactDetail(selectedContact);
                // Also re-render the contacts list to update the contact item
                this.renderContacts();
                // Refresh compose dropdown in case email was added/updated
                if (window.emailService) {
                    window.emailService.populateNostrContactDropdown();
                }
            }
        } catch (error) {
            console.error('Failed to refresh selected contact profile:', error);
        }
    }

    // Update contact profile from live events
    updateContactProfile(pubkey, profileFields) {
        try {
            // Find contact in current list
            const contact = this.contacts.find(c => c.pubkey === pubkey);
            if (contact) {
                // Update contact fields
                if (profileFields.name) contact.name = profileFields.name;
                if (profileFields.display_name) contact.display_name = profileFields.display_name;
                if (profileFields.about) contact.about = profileFields.about;
                if (profileFields.picture) contact.picture = profileFields.picture;
                if (profileFields.nip05) contact.nip05 = profileFields.nip05;
                
                console.log('[ContactsService] Updated contact profile for', pubkey);
                
                // Re-render contacts list if visible
                if (document.querySelector('.tab-content#contacts.active')) {
                    this.renderContacts();
                }
                
                // Update contact detail if this contact is currently selected
                const selectedContact = this.getSelectedContact();
                if (selectedContact && selectedContact.pubkey === pubkey) {
                    this.renderContactDetail(contact);
                }
                
                // Refresh compose dropdown in case email was added/updated
                if (window.emailService) {
                    window.emailService.populateNostrContactDropdown();
                }
            }
        } catch (error) {
            console.error('[ContactsService] Error updating contact profile:', error);
        }
    }

    // Toggle contacts search visibility
    toggleContactsSearch() {
        const searchContainer = window.domManager.get('contactsSearchContainer');
        const searchInput = window.domManager.get('contactsSearch');
        const searchToggle = window.domManager.get('contactsSearchToggle');
        
        if (!searchContainer) return;
        
        const isVisible = searchContainer.style.display !== 'none';
        
        if (isVisible) {
            // Hide search
            searchContainer.style.display = 'none';
            if (searchInput) {
                searchInput.value = '';
                this.filterContacts(); // Clear filter
            }
            if (searchToggle) {
                searchToggle.innerHTML = '<i class="fas fa-search"></i> Search';
            }
        } else {
            // Show search
            searchContainer.style.display = 'block';
            if (searchInput) {
                searchInput.focus();
            }
            if (searchToggle) {
                searchToggle.innerHTML = '<i class="fas fa-times"></i> Close';
            }
        }
    }

    // Filter contacts based on search query
    filterContacts() {
        const searchInput = window.domManager.get('contactsSearch');
        if (!searchInput) return;
        
        const searchQuery = searchInput.value.trim().toLowerCase();
        this.renderContacts(searchQuery);
    }

    // Toggle contact privacy status
    async toggleContactPrivacy(contactPubkey, isPublic, checkboxElement) {
        try {
            if (!window.appState.hasKeypair()) {
                window.notificationService.showError('No keypair available');
                // Revert checkbox if error
                if (checkboxElement) checkboxElement.checked = !isPublic;
                return;
            }

            const userPubkey = window.appState.getKeypair().public_key;
            const privateKey = window.appState.getKeypair().private_key;
            const relays = window.appState.getActiveRelays();
            
            // Update in database
            await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                userPubkey: userPubkey,
                contactPubkey: contactPubkey,
                isPublic: isPublic
            });

            // Publish the updated follow list to Nostr (always publish to keep it in sync)
            try {
                console.log('[JS] Publishing updated follow list to Nostr...');
                await window.TauriService.publishFollowList(privateKey, userPubkey, relays);
                console.log('[JS] Successfully published follow list');
            } catch (publishError) {
                console.error('[JS] Failed to publish follow list:', publishError);
                // Don't fail the whole operation, but warn the user
                window.notificationService.showWarning('Contact updated but failed to publish follow list: ' + publishError);
            }

            // Update in app state
            const contacts = window.appState.getContacts();
            const contactIndex = contacts.findIndex(c => c.pubkey === contactPubkey);
            if (contactIndex !== -1) {
                contacts[contactIndex].is_public = isPublic;
                window.appState.setContacts(contacts);
                
                // Update the selected contact if it's the one being toggled
                const selectedContact = window.appState.getSelectedContact();
                if (selectedContact && selectedContact.pubkey === contactPubkey) {
                    selectedContact.is_public = isPublic;
                    window.appState.setSelectedContact(selectedContact);
                    
                    // Update the label and description text immediately
                    const labelElement = document.getElementById(`privacy-label-${contactPubkey}`);
                    const descElement = document.getElementById(`privacy-desc-${contactPubkey}`);
                    
                    if (labelElement) {
                        labelElement.textContent = isPublic ? 'Public' : 'Private';
                    }
                    if (descElement) {
                        descElement.textContent = isPublic 
                            ? 'Visible in your public follow list' 
                            : 'Not in your public follow list';
                    }
                }
                
                // Re-render the contacts list to update the privacy icon
                this.renderContacts();
                
                const message = isPublic 
                    ? 'Contact is now public and follow list published' 
                    : 'Contact is now private and follow list updated';
                window.notificationService.showSuccess(message);
            }
        } catch (error) {
            console.error('Error toggling contact privacy:', error);
            window.notificationService.showError('Failed to update contact privacy: ' + error);
            // Revert checkbox if error
            if (checkboxElement) checkboxElement.checked = !isPublic;
        }
    }
}

// Create and export a singleton instance
window.ContactsService = ContactsService;
window.contactsService = new ContactsService();