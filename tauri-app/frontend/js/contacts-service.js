// Contacts Service
// Handles all contacts-related functionality including loading, rendering, and management

// Remove all import/export statements. Attach ContactsService and contactsService to window. Replace any usage of imported symbols with window equivalents if needed.

class ContactsService {
    constructor() {
        this.searchTimeout = null;
        this.imageLoadingInProgress = false; // Prevent multiple concurrent image loading operations
    }

    // Helper function to reconstruct fields object from DB contact data
    reconstructContactFields(dbContact) {
        const fields = {};
        if (dbContact.name) fields.name = dbContact.name;
        if (dbContact.email) fields.email = dbContact.email;
        if (dbContact.picture_url) fields.picture = dbContact.picture_url;
        if (dbContact.about) fields.about = dbContact.about;
        // Also check if display_name exists (might be in name field)
        if (dbContact.name && dbContact.name !== dbContact.pubkey.substring(0, 16) + '...') {
            fields.display_name = dbContact.name;
        }
        return {
            ...dbContact,
            fields: fields,
            picture: dbContact.picture_url || '',
            picture_data_url: dbContact.picture_data_url || null,
            picture_loading: false,
            picture_loaded: !!dbContact.picture_data_url
        };
    }

    // Load contacts from database only
    async loadContacts() {
        console.log('[JS] loadContacts called - loading contacts from database...');

        // Optionally, you can keep the keypair check if you want
        if (!window.appState.hasKeypair()) {
            return;
        }

        try {
            const userPubkey = window.appState.getKeypair().public_key;
            const dbContacts = await window.DatabaseService.getAllContacts(userPubkey);
            // Convert DB format to frontend format and reconstruct fields object
            // Note: picture_data_url is not fetched in initial query to reduce IPC overhead
            // It will be loaded on-demand via loadContactImagesProgressively
            const contacts = dbContacts.map(dbContact => {
                const reconstructed = this.reconstructContactFields(dbContact);
                // Preserve is_public status
                reconstructed.is_public = dbContact.is_public !== undefined ? dbContact.is_public : true;
                return reconstructed;
            });
            
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
            
            // Initialize view state - show list view by default if no contact is selected
            const selectedContact = window.appState.getSelectedContact();
            if (!selectedContact) {
                this.showContactsListView();
            }
            
            // Images are NOT loaded automatically on startup to prevent excessive fetching
            // Images will be loaded lazily when:
            // 1. Contacts tab is opened (via setupLazyImageLoading in switchTab)
            // 2. Contacts scroll into view (via IntersectionObserver)
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
            // Pass picture URL to validate cache - if URL changed, cache is invalid
            let dataUrl = await window.TauriService.getCachedProfileImage(contact.pubkey, contact.picture);
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

    // Load contact images progressively - only for visible contacts
    async loadContactImagesProgressively() {
        // Guard is already set before setTimeout, but check again in case of direct calls
        if (this.imageLoadingInProgress) {
            console.log('[JS] loadContactImagesProgressively: Already in progress, skipping');
            return;
        }
        
        // Guard should already be set, but ensure it's set here too for direct calls
        this.imageLoadingInProgress = true;
        try {
            const contacts = window.appState.getContacts();
            if (contacts.length === 0) {
                console.log('[JS] loadContactImagesProgressively: No contacts to load images for');
                return;
            }

        // Only load images for visible contacts initially (first 20)
        // Remaining images will be loaded lazily as user scrolls
        const initialBatchSize = 20;
        const visibleContacts = contacts.slice(0, initialBatchSize);
        
        console.log(`[JS] Loading images progressively for ${visibleContacts.length} visible contacts (out of ${contacts.length} total)`);

        // Helper to serialize DB writes
        let lastDbWrite = Promise.resolve();
        const queueDbWrite = (fn) => {
            lastDbWrite = lastDbWrite.then(fn, fn);
            return lastDbWrite;
        };

        const batchSize = 5; // Smaller batches to reduce concurrent requests
        let i = 0;

        while (i < visibleContacts.length) {
            const batch = visibleContacts.slice(i, i + batchSize);
            await Promise.all(batch.map(async (contact) => {
                if (contact.picture && !contact.picture_data_url && !contact.picture_loading) {
                    try {
                        contact.picture_loading = true;
                        // Update UI to show loading spinner
                        this.renderContactItem(contact);
                        // Pass picture URL to validate cache - if URL changed, cache is invalid
                        let dataUrl = await window.TauriService.getCachedProfileImage(contact.pubkey, contact.picture);
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
            // Add small delay between batches to avoid overwhelming the network
            if (i < visibleContacts.length) {
                await new Promise(resolve => setTimeout(resolve, 100));
            }
        }

            // Set up lazy loading for remaining contacts using IntersectionObserver
            if (contacts.length > initialBatchSize) {
                setTimeout(() => {
                    this.setupLazyImageLoading();
                }, 1000); // Wait a bit before setting up lazy loading
            }
        } finally {
            this.imageLoadingInProgress = false;
        }
    }

    // Set up lazy loading for contacts using IntersectionObserver
    // Only loads images from cache for first few visible contacts, then uses observer for rest
    async setupLazyImageLoading() {
        const contactsList = window.domManager.get('contactsList');
        if (!contactsList) return;

        const allContacts = window.appState.getContacts();
        
        // First, quickly load cached images for the first 5 visible contacts (cache-only, no network)
        // This improves perceived performance without causing network requests
        const initialVisibleContacts = allContacts.slice(0, 5);
        for (const contact of initialVisibleContacts) {
            if (contact.picture && !contact.picture_data_url && !contact.picture_loading) {
                try {
                    // Only check cache, don't fetch from network
                    // Pass picture URL to validate cache - if URL changed, cache is invalid
                    const cachedDataUrl = await window.TauriService.getCachedProfileImage(contact.pubkey, contact.picture);
                    if (cachedDataUrl) {
                        contact.picture_data_url = cachedDataUrl;
                        contact.picture_loaded = true;
                        this.renderContactItem(contact);
                    }
                } catch (e) {
                    // Ignore cache errors, will load via observer if needed
                }
            }
        }
        
        // Use IntersectionObserver to detect when contacts become visible
        const observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    const contactElement = entry.target;
                    const pubkey = contactElement.dataset.pubkey;
                    if (pubkey) {
                        const contact = allContacts.find(c => c.pubkey === pubkey);
                        if (contact && contact.picture && !contact.picture_data_url && !contact.picture_loading) {
                            // Load image for this contact (will check cache first, then fetch if needed)
                            this.loadContactImageAsync(contact);
                        }
                    }
                    observer.unobserve(contactElement);
                }
            });
        }, {
            rootMargin: '50px' // Start loading 50px before contact becomes visible
        });

        // Observe all contact elements that don't have images loaded yet
        const contactElements = contactsList.querySelectorAll('.contact-item[data-pubkey]');
        contactElements.forEach(contactElement => {
            const pubkey = contactElement.dataset.pubkey;
            const contact = allContacts.find(c => c.pubkey === pubkey);
            if (contact && contact.picture && !contact.picture_data_url && !contact.picture_loading) {
                observer.observe(contactElement);
            }
        });
        
        // Check if contacts 6-20 are already visible and need immediate loading
        const contactsNeedingLoad = allContacts.slice(5, 20).filter(c => c.picture && !c.picture_data_url && !c.picture_loading);
        if (contactsNeedingLoad.length > 0) {
            // Load these contacts immediately since they're likely already visible
            contactsNeedingLoad.forEach(contact => {
                this.loadContactImageAsync(contact);
            });
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
            emailIcon = `<button class="contact-email-icon" title="Send email to ${contact.email}" aria-label="Send email to ${contact.email}" data-email="${contact.email}">
                <i class="fas fa-envelope"></i>
            </button>`;
        }

        // Determine avatar source
        let avatarSrc = defaultAvatar;
        let avatarClass = 'contact-avatar';
        
        // Prioritize picture_data_url if it exists (image is loaded)
        if (contact.picture_data_url) {
            avatarSrc = contact.picture_data_url;
            // Only show loading spinner if image is still loading AND not yet loaded
            if (contact.picture_loading && !contact.picture_loaded) {
                avatarClass += ' loading';
            }
        } else if (contact.picture_loading) {
            // Image is loading but not yet available
            avatarClass += ' loading';
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
                        emailIcon = `<button class="contact-email-icon" title="Send email to ${contact.email}" aria-label="Send email to ${contact.email}" data-email="${contact.email}">
                            <i class="fas fa-envelope"></i>
                        </button>`;
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

                    // Add privacy toggle switch
                    const isPublic = contact.is_public !== undefined ? contact.is_public : true;
                    const privacyIcon = isPublic 
                        ? '<i class="fas fa-globe privacy-toggle-icon" title="Public follow - visible in your public follow list"></i>'
                        : '<i class="fas fa-lock privacy-toggle-icon" title="Private follow - not in your public follow list"></i>';
                    const privacyToggleHTML = `
                        <div class="privacy-toggle-wrapper" title="${isPublic ? 'Public follow - visible in your public follow list. Click to make private.' : 'Private follow - not in your public follow list. Click to make public.'}">
                            ${privacyIcon}
                            <label class="privacy-toggle privacy-toggle-compact" id="privacy-toggle-list-${contact.pubkey}">
                                <input type="checkbox" ${isPublic ? 'checked' : ''} 
                                       aria-label="${isPublic ? 'Public follow - click to make private' : 'Private follow - click to make public'}">
                                <span class="privacy-toggle-slider"></span>
                            </label>
                        </div>
                    `;
                    
                    // Add delete icon for private contacts (only visible on hover)
                    const deleteIconHTML = !isPublic 
                        ? `<button class="contact-delete-icon" data-pubkey="${contact.pubkey}" title="Remove this private contact" aria-label="Remove contact ${contact.name}">
                            <i class="fas fa-trash-alt"></i>
                           </button>`
                        : '';
                    
                    contactElement.innerHTML = `
                        <img class="${avatarClass}" src="${avatarSrc}" alt="${contact.name}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
                        <div class="contact-info">
                            <div class="contact-name">${contact.name}</div>
                        </div>
                        <div class="contact-actions">
                            ${deleteIconHTML}
                            ${privacyToggleHTML}
                            ${emailIcon}
                        </div>
                    `;
                    
                    // Add click event listener for contact selection
                    contactElement.addEventListener('click', (e) => {
                        // Don't select contact if clicking on toggle wrapper, toggle, delete icon, or email icon
                        if (e.target.closest('.privacy-toggle-wrapper') || e.target.closest('.privacy-toggle') || e.target.closest('.contact-delete-icon') || e.target.closest('.contact-email-icon')) {
                            return;
                        }
                        this.selectContact(contact);
                    });
                    
                    // Add event listener for delete icon (if present)
                    if (!isPublic) {
                        const deleteButton = contactElement.querySelector('.contact-delete-icon');
                        if (deleteButton) {
                            deleteButton.addEventListener('click', (e) => {
                                e.stopPropagation();
                                this.deletePrivateContact(contact);
                            });
                        }
                    }
                    
                    // Add event listener for email icon
                    if (contact.email) {
                        const emailButton = contactElement.querySelector('.contact-email-icon');
                        if (emailButton) {
                            emailButton.addEventListener('click', (e) => {
                                e.stopPropagation();
                                this.sendEmailToContact(contact.email);
                            });
                        }
                    }
                    
                    // Add event listener for privacy toggle
                    const toggleCheckbox = contactElement.querySelector(`#privacy-toggle-list-${contact.pubkey} input[type="checkbox"]`);
                    const toggleWrapper = contactElement.querySelector('.privacy-toggle-wrapper');
                    if (toggleCheckbox && toggleWrapper) {
                        toggleCheckbox.addEventListener('change', (e) => {
                            e.stopPropagation();
                            const newIsPublic = e.target.checked;
                            
                            // Update icon immediately for visual feedback
                            const iconElement = toggleWrapper.querySelector('.privacy-toggle-icon');
                            if (iconElement) {
                                if (newIsPublic) {
                                    iconElement.className = 'fas fa-globe privacy-toggle-icon';
                                    iconElement.title = 'Public follow - visible in your public follow list';
                                    toggleWrapper.title = 'Public follow - visible in your public follow list. Click to make private.';
                                } else {
                                    iconElement.className = 'fas fa-lock privacy-toggle-icon';
                                    iconElement.title = 'Private follow - not in your public follow list';
                                    toggleWrapper.title = 'Private follow - not in your public follow list. Click to make public.';
                                }
                            }
                            
                            this.toggleContactPrivacy(contact.pubkey, newIsPublic, e.target);
                        });
                    }
                    
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
            // Ensure contacts tab is active before showing detail view
            const contactsTab = document.querySelector('[data-tab="contacts"]');
            const isContactsTabActive = contactsTab && contactsTab.classList.contains('active');
            
            if (!isContactsTabActive) {
                // Switch to contacts tab first
                window.app.switchTab('contacts');
                // Wait a bit for the tab to switch and contacts to render
                await new Promise(resolve => setTimeout(resolve, 100));
            }
            
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
            
            // Show contact detail view in contacts tab (don't navigate to profile tab)
            this.showContactDetailView(contact);
            
        } catch (error) {
            console.error('Error selecting contact:', error);
            window.notificationService.showError('Failed to load contact profile');
        }
    }

    // Show contact detail view within contacts tab
    showContactDetailView(contact) {
        const contactsContainer = document.querySelector('.contacts-container');
        const contactsSidebar = document.querySelector('.contacts-sidebar');
        const contactsDetail = window.domManager.get('contactsDetail');
        const backButton = document.getElementById('contacts-back-btn');
        
        if (!contactsContainer || !contactsDetail) {
            console.warn('[JS] Contacts container or detail element not found');
            return;
        }
        
        // Switch to detail view state
        contactsContainer.classList.remove('contacts-list-view');
        contactsContainer.classList.add('contacts-detail-view');
        
        // Show back button
        if (backButton) {
            backButton.style.display = 'inline-flex';
        }
        
        // Render contact detail
        this.renderContactDetail(contact);
        
        // Fetch fresh profile data in background
        this.fetchAndUpdateContactProfile(contact).catch(error => {
            console.error('[JS] Background profile fetch failed:', error);
            // Silently fail - user already sees cached data
        });
    }

    // Show contacts list view
    showContactsListView() {
        const contactsContainer = document.querySelector('.contacts-container');
        const contactsDetail = window.domManager.get('contactsDetail');
        const backButton = document.getElementById('contacts-back-btn');
        
        if (!contactsContainer) {
            console.warn('[JS] Contacts container not found');
            return;
        }
        
        // Switch to list view state
        contactsContainer.classList.remove('contacts-detail-view');
        contactsContainer.classList.add('contacts-list-view');
        
        // Hide back button
        if (backButton) {
            backButton.style.display = 'none';
        }
        
        // Clear selected contact
        window.appState.setSelectedContact(null);
        
        // Remove active class from all contacts
        document.querySelectorAll('.contact-item').forEach(item => {
            item.classList.remove('active');
        });
        
        // Show placeholder in detail view
        if (contactsDetail) {
            contactsDetail.innerHTML = `
                <div class="contact-detail-placeholder">
                    <i class="fas fa-user"></i>
                    <p>Select a contact to view their profile</p>
                </div>
            `;
        }
    }

    // Handle back button click to return to contacts list
    handleBackToContactsList() {
        this.showContactsListView();
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
        
        // #region agent log
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:721',message:'renderContactDetail called',data:{pubkey:contact?.pubkey?.substring(0,20),hasFields:!!contact?.fields,fieldsKeys:contact?.fields?Object.keys(contact.fields).join(','):'none',hasName:!!contact?.name,hasEmail:!!contact?.email},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'C'})}).catch(()=>{});
        // #endregion
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
            
            // Clear any selected contact and show list view
            window.appState.setSelectedContact(null);
            this.showContactsListView();
            
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
        let html5QrCode = null;
        let scannerStarted = false;
        let isCleanedUp = false;
        
        // Check if Html5Qrcode is available
        if (typeof Html5Qrcode === 'undefined') {
            window.notificationService.showError('QR scanner library not loaded. Please refresh the page.');
            return;
        }
        
        // Check camera availability
        if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            window.notificationService.showError('Camera access is not available in this browser.');
            return;
        }
        
        // Show QR scanner in the modal
        const modalContent = `
            <div id="qr-scanner-container" style="text-align:center;">
                <p style="margin-bottom:15px;">Point your camera at the QR code</p>
                <div id="qr-reader" style="width:100%;max-width:400px;margin:0 auto;min-height:300px;"></div>
                <div id="qr-scanner-error" style="display:none;color:#dc3545;margin-top:15px;"></div>
                <div style="margin-top:20px;">
                    <button id="close-scanner-btn" class="btn btn-secondary">
                        <i class="fas fa-times"></i> Back to Form
                    </button>
                </div>
            </div>
        `;
        
        // Show the modal properly - ensure overlay is visible
        window.app.showModal('Scan QR Code', modalContent);
        
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
        
        // Setup close scanner button
        const closeBtn = document.getElementById('close-scanner-btn');
        if (closeBtn) {
            closeBtn.addEventListener('click', () => {
                cleanup();
                // Restore the add contact form
                this.showAddContactModal();
            });
        }
        
        // Wait for modal to be fully visible and element to have dimensions
        const startScanner = async () => {
            try {
                const qrReader = document.getElementById('qr-reader');
                if (!qrReader) {
                    console.error('[QR] QR reader element not found');
                    return;
                }
                
                // Wait for element to be visible and have dimensions
                let attempts = 0;
                const maxAttempts = 50; // 5 seconds max wait
                while (attempts < maxAttempts) {
                    const rect = qrReader.getBoundingClientRect();
                    if (rect.width > 0 && rect.height > 0) {
                        break;
                    }
                    await new Promise(resolve => setTimeout(resolve, 100));
                    attempts++;
                }
                
                html5QrCode = new Html5Qrcode('qr-reader');
                
                // Detect if we're on mobile or desktop
                const isMobile = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(navigator.userAgent);
                const cameraConfig = isMobile 
                    ? { facingMode: 'environment' } // Back camera on mobile
                    : { facingMode: 'user' }; // Front camera on desktop
                
                // Start scanning
                await html5QrCode.start(
                    cameraConfig,
                    {
                        fps: 10,
                        qrbox: { width: 250, height: 250 },
                        aspectRatio: 1.0
                    },
                    (decodedText, decodedResult) => {
                        console.log('[QR] QR code scanned:', decodedText);
                        
                        // Validate the scanned text is a public key
                        const scannedKey = decodedText.trim();
                        if (!scannedKey.startsWith('npub1')) {
                            const errorDiv = document.getElementById('qr-scanner-error');
                            if (errorDiv) {
                                errorDiv.style.display = 'block';
                                errorDiv.textContent = 'Invalid QR code: Not a valid public key format (should start with npub1)';
                            }
                            return;
                        }
                        
                        // Stop scanning
                        cleanup();
                        
                        // Restore the add contact form with the scanned key
                        // This will replace the scanner view with the form, populated with the scanned key
                        this.showAddContactModal(scannedKey);
                    },
                    (errorMessage) => {
                        // Ignore scanning errors (they're frequent during scanning)
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
                window.notificationService.showError('Failed to start camera');
                cleanup();
            }
        };
        
        // Wait for next animation frame to ensure modal is rendered
        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                startScanner();
            });
        });
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
                    // Switch to contacts tab
                    window.app.switchTab('contacts');
                    // Wait a bit for the tab to switch
                    await new Promise(resolve => setTimeout(resolve, 100));
                    // Show profile for adding with follow button
                    this.showProfileForAdding(pubkey, profile);
                    
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
        const contactsContainer = document.querySelector('.contacts-container');
        const contactsDetail = window.domManager.get('contactsDetail');
        const backButton = document.getElementById('contacts-back-btn');
        
        if (!contactsContainer || !contactsDetail) {
            console.warn('[JS] Contacts container or detail element not found');
            return;
        }
        
        // Switch to detail view state
        contactsContainer.classList.remove('contacts-list-view');
        contactsContainer.classList.add('contacts-detail-view');
        
        // Show back button
        if (backButton) {
            backButton.style.display = 'inline-flex';
        }
        
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
        // #region agent log
        const startTime = Date.now();
        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1727',message:'refreshContacts started',data:{timestamp:Date.now()},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
        // #endregion
        console.log('[JS] refreshContacts called - fetching fresh data from network...');
        if (!window.appState.hasKeypair()) {
            return;
        }
        const activeRelays = window.appState.getActiveRelays();
        if (activeRelays.length === 0) {
            window.notificationService.showError('No active relays configured');
            return;
        }
        
        let loadingNotification = null;
        try {
            // Show loading notification
            loadingNotification = window.notificationService.showLoading('Refreshing contacts...');
            
            // 1. Fetch followed npubs using persistent client (more efficient!)
            loadingNotification.innerHTML = `
                <div class="notification-content">
                    <i class="fas fa-spinner fa-spin"></i>
                    <span>Fetching follow list...</span>
                </div>
            `;
            // #region agent log
            const fetchFollowListStart = Date.now();
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1750',message:'fetchFollowingPubkeysPersistent start',data:{timestamp:Date.now()},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
            // #endregion
            const followedPubkeys = await window.TauriService.fetchFollowingPubkeysPersistent(
                window.appState.getKeypair().public_key
            );
            // #region agent log
            const fetchFollowListDuration = Date.now() - fetchFollowListStart;
            fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1753',message:'fetchFollowingPubkeysPersistent end',data:{duration:fetchFollowListDuration,count:followedPubkeys?.length||0},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
            // #endregion
            console.log('[JS] Fetched followed npubs using persistent client:', followedPubkeys);
            
            const userPubkey = window.appState.getKeypair().public_key;
            
            // Handle case where no follows found on network
            if (!followedPubkeys || followedPubkeys.length === 0) {
                // Get existing public contacts from DB
                const existingPublicPubkeys = await window.__TAURI__.core.invoke('db_get_public_contact_pubkeys', {
                    userPubkey: userPubkey
                });
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
                    window.notificationService.hideLoading(loadingNotification);
                    window.notificationService.showInfo(`No follows found on network. ${existingPublicPubkeys.length} contacts converted to private.`);
                } else {
                    window.notificationService.hideLoading(loadingNotification);
                    window.notificationService.showInfo('No follows found on the network');
                }
                return;
            }
            
            // 2. Get existing public contacts from DB
            const existingPublicPubkeys = await window.__TAURI__.core.invoke('db_get_public_contact_pubkeys', {
                userPubkey: userPubkey
            });
            console.log('[JS] Existing public contacts in DB:', existingPublicPubkeys.length);
            
            // 3. Find contacts that were publicly followed but are no longer in the follow list
            const followedPubkeysSet = new Set(followedPubkeys);
            const unfollowedPubkeys = existingPublicPubkeys.filter(pubkey => !followedPubkeysSet.has(pubkey));
            
            if (unfollowedPubkeys.length > 0) {
                console.log('[JS] Detected unfollowed contacts (no longer in public follow list):', unfollowedPubkeys);
                // Convert public follows to private (preserve contact data but mark as private)
                // #region agent log
                const unfollowStart = Date.now();
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1798',message:'unfollowUpdates start',data:{count:unfollowedPubkeys.length},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'D'})}).catch(()=>{});
                // #endregion
                for (const pubkey of unfollowedPubkeys) {
                    await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                        userPubkey: userPubkey,
                        contactPubkey: pubkey,
                        isPublic: false
                    });
                }
                // #region agent log
                const unfollowDuration = Date.now() - unfollowStart;
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1805',message:'unfollowUpdates end',data:{duration:unfollowDuration,count:unfollowedPubkeys.length},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'D'})}).catch(()=>{});
                // #endregion
                console.log(`[JS] Converted ${unfollowedPubkeys.length} public contacts to private`);
            }
            
            // 4. Filter out npubs already in the DB for this user
            loadingNotification.innerHTML = `
                <div class="notification-content">
                    <i class="fas fa-spinner fa-spin"></i>
                    <span>Checking for new contacts...</span>
                </div>
            `;
            const newPubkeys = await window.TauriService.filterNewContacts(userPubkey, followedPubkeys);
            console.log('[JS] New npubs not in DB:', newPubkeys);
            
            // 5. Create minimal contacts immediately and save to DB
            if (newPubkeys.length > 0) {
                loadingNotification.innerHTML = `
                    <div class="notification-content">
                        <i class="fas fa-spinner fa-spin"></i>
                        <span>Adding new contacts... (${newPubkeys.length} contacts)</span>
                    </div>
                `;
                
                // Create minimal contacts with placeholder data
                const minimalContacts = newPubkeys.map(pubkey => ({
                    pubkey: pubkey,
                    name: pubkey.substring(0, 16) + '...', // Placeholder name
                    email: null,
                    picture: '',
                    fields: {},
                    picture_data_url: null,
                    picture_loading: false,
                    picture_loaded: false,
                    created_at: new Date().toISOString(),
                    updated_at: new Date().toISOString()
                }));
                
                // Save minimal contacts to DB immediately (is_public=true for public follows)
                // #region agent log
                const saveMinimalStart = Date.now();
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1842',message:'saveMinimalContacts start',data:{count:minimalContacts.length},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                // #endregion
                for (const contact of minimalContacts) {
                    // #region agent log
                    const saveOneStart = Date.now();
                    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1844',message:'saveContact start',data:{pubkey:contact.pubkey.substring(0,20)},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                    // #endregion
                    const dbContact = window.DatabaseService.convertContactToDbFormat(contact);
                    await window.DatabaseService.saveContact(dbContact, userPubkey, true);
                    // #region agent log
                    const saveOneDuration = Date.now() - saveOneStart;
                    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1846',message:'saveContact end',data:{duration:saveOneDuration},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                    // #endregion
                }
                // #region agent log
                const saveMinimalDuration = Date.now() - saveMinimalStart;
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1847',message:'saveMinimalContacts end',data:{duration:saveMinimalDuration,count:minimalContacts.length,avgPerContact:saveMinimalDuration/minimalContacts.length},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                // #endregion
                console.log(`[JS] Saved ${minimalContacts.length} minimal contacts to DB`);
                
                // Get all local contacts (including the newly added minimal ones)
                const dbContacts = await window.DatabaseService.getAllContacts(userPubkey);
                const localContacts = dbContacts.map(dbContact => this.reconstructContactFields(dbContact));
                
                // Update UI immediately so users see contacts right away
                window.appState.setContacts(localContacts);
                this.renderContacts();
                
                // Set up lazy image loading
                setTimeout(() => {
                    this.setupLazyImageLoading();
                }, 100);
                
                // 6. Fetch profiles for new contacts (this can take time)
                loadingNotification.innerHTML = `
                    <div class="notification-content">
                        <i class="fas fa-spinner fa-spin"></i>
                        <span>Fetching profiles from relays... (${newPubkeys.length} profiles)</span>
                    </div>
                `;
                console.log(`[JS] Calling fetchProfilesPersistent for ${newPubkeys.length} pubkeys...`);
                // #region agent log
                const fetchProfilesStart = Date.now();
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1868',message:'fetchProfilesPersistent start',data:{count:newPubkeys.length},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'B'})}).catch(()=>{});
                // #endregion
                const newProfiles = await window.TauriService.fetchProfilesPersistent(newPubkeys);
                // #region agent log
                const fetchProfilesDuration = Date.now() - fetchProfilesStart;
                fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1869',message:'fetchProfilesPersistent end',data:{duration:fetchProfilesDuration,count:newProfiles?.length||0},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'B'})}).catch(()=>{});
                // #endregion
                console.log(`[JS] Fetched new profiles using persistent client: ${newProfiles ? newProfiles.length : 0} profiles`);
                
                // 7. Update contacts with profile data incrementally
                if (newProfiles && newProfiles.length > 0) {
                    loadingNotification.innerHTML = `
                        <div class="notification-content">
                            <i class="fas fa-spinner fa-spin"></i>
                            <span>Updating contact details... (${newProfiles.length} profiles)</span>
                        </div>
                    `;
                    
                    // Update contacts with profile data (saveContact does UPSERT, so it will update existing contacts)
                    // #region agent log
                    const updateProfilesStart = Date.now();
                    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1881',message:'updateProfiles start',data:{count:newProfiles.length},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                    // #endregion
                    for (const profile of newProfiles) {
                        // #region agent log
                        const updateOneStart = Date.now();
                        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1883',message:'updateContact start',data:{pubkey:profile.pubkey.substring(0,20)},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                        // #endregion
                        const updatedContact = {
                            pubkey: profile.pubkey,
                            name: profile.fields.name || profile.fields.display_name || profile.pubkey.substring(0, 16) + '...',
                            picture: profile.fields.picture || '',
                            email: profile.fields.email || null,
                            fields: profile.fields || {},
                            picture_data_url: null,
                            picture_loading: false,
                            picture_loaded: false,
                            updated_at: new Date().toISOString()
                        };
                        const dbContact = window.DatabaseService.convertContactToDbFormat(updatedContact);
                        await window.DatabaseService.saveContact(dbContact, userPubkey, true);
                        // #region agent log
                        const updateOneDuration = Date.now() - updateOneStart;
                        fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1895',message:'updateContact end',data:{duration:updateOneDuration},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                        // #endregion
                    }
                    // #region agent log
                    const updateProfilesDuration = Date.now() - updateProfilesStart;
                    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1896',message:'updateProfiles end',data:{duration:updateProfilesDuration,count:newProfiles.length,avgPerContact:updateProfilesDuration/newProfiles.length},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                    // #endregion
                    
                    // Get current contacts from appState to preserve full fields data
                    const currentContacts = window.appState.getContacts();
                    const updatedContactsInMemory = [];
                    
                    // Update contacts in memory with profile data (preserving full fields)
                    for (const profile of newProfiles) {
                        const existingContact = currentContacts.find(c => c.pubkey === profile.pubkey);
                        const updatedContact = {
                            ...existingContact,
                            pubkey: profile.pubkey,
                            name: profile.fields.name || profile.fields.display_name || profile.pubkey.substring(0, 16) + '...',
                            picture: profile.fields.picture || '',
                            email: profile.fields.email || null,
                            fields: profile.fields || {}, // Preserve full fields object in memory
                            picture_data_url: existingContact?.picture_data_url || null,
                            picture_loading: false,
                            picture_loaded: !!existingContact?.picture_data_url,
                            updated_at: new Date().toISOString(),
                            is_public: existingContact?.is_public !== undefined ? existingContact.is_public : true
                        };
                        updatedContactsInMemory.push(updatedContact);
                    }
                    
                    // Merge updated contacts with existing contacts (preserve contacts not in update list)
                    const updatedContactPubkeys = new Set(updatedContactsInMemory.map(c => c.pubkey));
                    const otherContacts = currentContacts.filter(c => !updatedContactPubkeys.has(c.pubkey));
                    const allUpdatedContacts = [...otherContacts, ...updatedContactsInMemory];
                    
                    // Update appState with contacts that have full fields data preserved (don't reload from DB)
                    window.appState.setContacts(allUpdatedContacts);
                    this.renderContacts();
                    
                    // Refresh compose dropdown to include new contacts
                    if (window.emailService) {
                        window.emailService.populateNostrContactDropdown();
                    }
                    
                    // #region agent log
                    const totalDuration = Date.now() - startTime;
                    fetch('http://127.0.0.1:7242/ingest/b8f7161b-abb4-4d62-b1ad-efed0c555360',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({location:'contacts-service.js:1907',message:'refreshContacts completed',data:{totalDuration:totalDuration},timestamp:Date.now(),sessionId:'debug-session',runId:'run1',hypothesisId:'A'})}).catch(()=>{});
                    // #endregion
                    window.notificationService.hideLoading(loadingNotification);
                    const message = unfollowedPubkeys.length > 0 
                        ? `Added ${newProfiles.length} new contacts. ${unfollowedPubkeys.length} contacts converted to private.`
                        : `Added ${newProfiles.length} new contacts`;
                    window.notificationService.showSuccess(message);
                } else {
                    // Profiles fetch returned empty, but contacts are already saved with minimal data
                    window.notificationService.hideLoading(loadingNotification);
                    const message = unfollowedPubkeys.length > 0 
                        ? `Added ${minimalContacts.length} new contacts (profiles pending). ${unfollowedPubkeys.length} contacts converted to private.`
                        : `Added ${minimalContacts.length} new contacts (profiles pending)`;
                    window.notificationService.showInfo(message);
                }
            } else {
                // No new contacts, handle re-follows and re-render
                // Get all local contacts for this user (before checking re-follows)
                const dbAllContacts = await window.DatabaseService.getAllContacts(userPubkey);
                const allContacts = dbAllContacts.map(dbContact => this.reconstructContactFields(dbContact));
                const allContactPubkeys = new Set(allContacts.map(c => c.pubkey));
                
                // Update is_public for contacts that are back in the follow list (were private, now public again)
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
                
                // Get all local contacts for this user (after updates)
                const dbLocalContacts = await window.DatabaseService.getAllContacts(userPubkey);
                const localContacts = dbLocalContacts.map(dbContact => this.reconstructContactFields(dbContact));
                window.appState.setContacts(localContacts);
                this.renderContacts();
                
                window.notificationService.hideLoading(loadingNotification);
                if (unfollowedPubkeys.length > 0) {
                    window.notificationService.showInfo(`${unfollowedPubkeys.length} contacts converted to private (no longer in public follow list)`);
                } else {
                    window.notificationService.showInfo('No new contacts found in your follow list');
                }
            }
        } catch (error) {
            console.error('Failed to load contacts from network:', error);
            if (loadingNotification) {
                window.notificationService.hideLoading(loadingNotification);
            }
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
                        // Pass picture URL to validate cache - if URL changed, cache is invalid
                        let dataUrl = await window.TauriService.getCachedProfileImage(pubkey, selectedContact.picture);
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

    // Delete a private contact
    async deletePrivateContact(contact) {
        try {
            if (!window.appState.hasKeypair()) {
                return;
            }

            const userPubkey = window.appState.getKeypair().public_key;
            const contactName = contact.name || contact.pubkey;
            
            // Show confirmation dialog
            const confirmed = await window.notificationService.showConfirmation(
                `Are you sure you want to remove ${contactName} from your contacts? This will remove the private follow relationship.`,
                'Remove Contact'
            );
            
            if (!confirmed) return;

            // Call backend to remove the relationship and cleanup if needed
            const result = await window.__TAURI__.core.invoke('db_remove_user_contact_and_cleanup', {
                userPubkey: userPubkey,
                contactPubkey: contact.pubkey
            });

            const [success, contactDeleted] = result;
            
            if (success) {
                // Remove contact from app state
                const contacts = window.appState.getContacts();
                const updatedContacts = contacts.filter(c => c.pubkey !== contact.pubkey);
                window.appState.setContacts(updatedContacts);
                
                // Clear selected contact if it was the deleted one
                const selectedContact = window.appState.getSelectedContact();
                if (selectedContact && selectedContact.pubkey === contact.pubkey) {
                    window.appState.setSelectedContact(null);
                }
                
                // Re-render contacts list
                this.renderContacts();
                
                // Show success message
                const message = contactDeleted 
                    ? `${contactName} has been removed from your contacts and deleted from the database.`
                    : `${contactName} has been removed from your contacts.`;
                window.notificationService.showSuccess(message);
            } else {
                window.notificationService.showError('Failed to remove contact');
            }
        } catch (error) {
            console.error('Error deleting private contact:', error);
            window.notificationService.showError('Failed to remove contact: ' + error);
        }
    }
}

// Create and export a singleton instance
window.ContactsService = ContactsService;
window.contactsService = new ContactsService();