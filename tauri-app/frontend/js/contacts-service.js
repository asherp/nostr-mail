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
            const dbContacts = await window.DatabaseService.getAllContacts();
            // Convert DB format to frontend format if needed
            const contacts = dbContacts.map(contact => ({
                pubkey: contact.pubkey,
                name: contact.name,
                picture: contact.picture_url || contact.picture || '',
                email: contact.email || null,
                fields: {
                    name: contact.name,
                    display_name: contact.display_name || contact.name,
                    picture: contact.picture_url || contact.picture || '',
                    about: contact.about || '',
                    email: contact.email || ''
                },
                picture_data_url: contact.picture_data_url || null,
                picture_loading: false,
                picture_loaded: !!contact.picture_data_url
            }));
            window.appState.setContacts(contacts);
            this.renderContacts();
            console.log('[JS] Contacts loaded from database');
            // Defer image loading so UI is responsive
            this.loadContactImagesProgressively(); // don't await
        } catch (e) {
            console.warn('Failed to load contacts from database:', e);
            window.appState.setContacts([]);
            this.renderContacts();
            window.notificationService.showError('Failed to load contacts from database');
        }
    }

    // Load contact images progressively
    async loadContactImagesProgressively() {
        const contacts = window.appState.getContacts();
        if (contacts.length === 0) return;

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
        if (!contactsList) return;

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

            if (filteredContacts && filteredContacts.length > 0) {
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
                    
                    if (contact.picture_loading) {
                        avatarClass += ' loading';
                    } else if (contact.picture_data_url) {
                        avatarSrc = contact.picture_data_url;
                        // console.log(`[JS] Using cached data URL for ${contact.name}`);
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
    selectContact(contact) {
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
            
            // Render the contact detail
            this.renderContactDetail(contact);
            
        } catch (error) {
            console.error('Error selecting contact:', error);
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
    showAddContactModal() {
        try {
            // Switch to contacts tab and show the add contact form in the detail panel
            const contactsTab = document.querySelector('[data-tab="contacts"]');
            if (contactsTab) {
                contactsTab.click();
            }
            
            // Clear any selected contact
            window.appState.setSelectedContact(null);
            document.querySelectorAll('.contact-item').forEach(item => {
                item.classList.remove('active');
            });
            
            // Show add contact form in the detail panel
            const contactsDetail = window.domManager.get('contactsDetail');
            if (contactsDetail) {
                contactsDetail.innerHTML = `
                    <div class="add-contact-form">
                        <h3>Add New Contact</h3>
                        <p class="form-help">Enter a Nostr public key (npub) to fetch their profile and follow them</p>
                        
                        <form id="add-contact-form">
                            <div class="form-group">
                                <label for="contact-pubkey">Public Key (npub):</label>
                                <div class="input-with-button">
                                    <input type="text" id="contact-pubkey" placeholder="npub1..." required>
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
                
                // Set up event listeners
                const form = document.getElementById('add-contact-form');
                const scanBtn = document.getElementById('scan-qr-btn');
                
                if (form) {
                    form.addEventListener('submit', (e) => this.addContact(e));
                }
                if (scanBtn) {
                    scanBtn.addEventListener('click', () => this.scanQRCode());
                }
            }
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
            
            // Show loading state in the detail panel
            const contactsDetail = window.domManager.get('contactsDetail');
            if (contactsDetail) {
                contactsDetail.innerHTML = `
                    <div class="loading-profile">
                        <i class="fas fa-spinner fa-spin" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
                        <p>Fetching profile...</p>
                    </div>
                `;
            }
            
            // Fetch the profile
            try {
                const activeRelays = window.appState.getActiveRelays();
                if (activeRelays.length === 0) {
                    window.notificationService.showError('No active relays to fetch profile');
                    return;
                }
                
                console.log('Fetching profile for:', pubkey);
                const profile = await window.TauriService.fetchProfile(pubkey, activeRelays);
                
                if (profile) {
                    // Show the profile in the detail panel with follow button
                    this.showProfileForAdding(pubkey, profile);
                } else {
                    window.notificationService.showError('Could not fetch profile for this public key');
                    // Go back to the add contact form
                    this.showAddContactModal();
                }
            } catch (error) {
                console.error('Failed to fetch profile:', error);
                window.notificationService.showError('Failed to fetch profile: ' + error);
                // Go back to the add contact form
                this.showAddContactModal();
            }
            
        } catch (error) {
            console.error('Error adding contact:', error);
            window.notificationService.showError('Failed to add contact');
        }
    }

    // Show profile for adding (not following yet)
    showProfileForAdding(pubkey, profile) {
        const contactsDetail = window.domManager.get('contactsDetail');
        if (!contactsDetail) return;
        
        try {
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
                // Remove any existing follow button
                const existingFollowBtn = actionsDiv.querySelector('.follow-btn');
                if (existingFollowBtn) {
                    existingFollowBtn.remove();
                }
                
                // Add follow button at the beginning
                const followBtn = document.createElement('button');
                followBtn.className = 'btn btn-primary follow-btn';
                followBtn.innerHTML = '<i class="fas fa-user-plus"></i> Follow & Add Contact';
                followBtn.addEventListener('click', () => {
                    this.confirmFollowContact(pubkey, profile, profileName, profile.fields.email || '');
                });
                
                // Insert at the beginning of actions
                actionsDiv.insertBefore(followBtn, actionsDiv.firstChild);
            }
            
            // Try to load and cache the profile picture (but don't re-render the entire profile)
            if (profilePicture) {
                tempContact.picture_loading = true;
                window.TauriService.fetchImage(profilePicture).then(dataUrl => {
                    if (dataUrl) {
                        tempContact.picture_data_url = dataUrl;
                        tempContact.picture_loaded = true;
                        
                        // Update just the avatar image without re-rendering the entire profile
                        const avatarImg = contactsDetail.querySelector('.contact-detail-avatar img');
                        if (avatarImg) {
                            avatarImg.src = dataUrl;
                            avatarImg.style.display = 'block';
                        }
                    }
                    tempContact.picture_loading = false;
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
        
        // Fill in the email address
        const toAddressInput = window.domManager.get('toAddress');
        if (toAddressInput) {
            toAddressInput.value = email;
        }
        
        window.notificationService.showSuccess(`Email address filled in: ${email}`);
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
            console.log('🔄 Loading following profiles from network...');
            
            // Fetch following profiles from Nostr
            const followingProfiles = await window.TauriService.fetchFollowingProfiles(
                window.appState.getKeypair().private_key,
                activeRelays
            );
            
            console.log('[JS] Network response:', {
                profilesReceived: !!followingProfiles,
                profilesLength: followingProfiles?.length || 0
            });

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
                window.appState.setContacts(newContacts);
                this.renderContacts();
                // Defer image loading so UI is responsive
                this.loadContactImagesProgressively(); // don't await

                // Also update the database with the new contacts
                for (const contact of newContacts) {
                    const dbContact = window.DatabaseService.convertContactToDbFormat(contact);
                    await window.DatabaseService.saveContact(dbContact);
                }

                // Cache the contacts in backend storage (without images)
                try {
                    // Convert frontend contact format to backend storage format
                    const backendContacts = window.appState.getContacts().map(contact => ({
                        pubkey: contact.pubkey,
                        name: contact.name,
                        display_name: contact.fields.display_name || contact.name,
                        picture: contact.picture,
                        picture_data_url: contact.picture_data_url || null,
                        about: contact.fields.about || null,
                        email: contact.email,
                        cached_at: new Date().toISOString()
                    }));
                    
                    await window.TauriService.setContacts(backendContacts);
                    console.log('[JS] Cached contacts in backend storage');
                } catch (e) {
                    console.warn('Failed to cache contacts in backend:', e);
                }
                
                console.log(`✅ Loaded ${newContacts.length} contacts from network`);
                window.notificationService.showSuccess(`Refreshed ${newContacts.length} contacts`);
            } else {
                console.log('[JS] No profiles received from network');
                window.notificationService.showInfo('No contacts found in your follow list');
            }
            
        } catch (error) {
            console.error('Failed to load contacts from network:', error);
            window.notificationService.showError('Failed to refresh contacts: ' + error);
        }
    }
}

// Create and export a singleton instance
window.ContactsService = ContactsService;
window.contactsService = new ContactsService();