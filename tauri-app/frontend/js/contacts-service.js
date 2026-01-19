// Contacts Service
// Handles all contacts-related functionality including loading, rendering, and management

// Remove all import/export statements. Attach ContactsService and contactsService to window. Replace any usage of imported symbols with window equivalents if needed.

class ContactsService {
    constructor() {
        this.searchTimeout = null;
        this.imageLoadingInProgress = false; // Prevent multiple concurrent image loading operations
        this.sectionStates = {
            public: true,  // Default: expanded
            private: true  // Default: expanded
        };
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
        const hasKeypair = window.appState.hasKeypair();
        if (!hasKeypair) {
            return;
        }

        try {
            const userPubkey = window.appState.getKeypair().public_key;
            const dbContacts = await window.DatabaseService.getAllContacts(userPubkey);
            // Convert DB format to frontend format and reconstruct fields object
            // Note: picture_data_url is not fetched in initial query to reduce IPC overhead
            // It will be loaded on-demand via loadContactImagesProgressively
            const contacts = dbContacts.map((dbContact, index) => {
                try {
                    const reconstructed = this.reconstructContactFields(dbContact);
                    // Preserve is_public status
                    reconstructed.is_public = dbContact.is_public !== undefined ? dbContact.is_public : true;
                    return reconstructed;
                } catch (e) {
                    throw e;
                }
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
            // Always clear the container first
            contactsList.innerHTML = '';

            // Filter contacts based on search query
            const allContacts = window.appState.getContacts() || [];
            let filteredContacts = allContacts;
            if (searchQuery && allContacts.length > 0) {
                filteredContacts = allContacts.filter(contact => 
                    contact.name && contact.name.toLowerCase().includes(searchQuery) ||
                    contact.pubkey && contact.pubkey.toLowerCase().includes(searchQuery) ||
                    (contact.email && contact.email.toLowerCase().includes(searchQuery))
                );
            }

            console.log(`[JS] renderContacts: Rendering ${filteredContacts ? filteredContacts.length : 0} contacts (searchQuery: "${searchQuery}")`);
            if (filteredContacts && filteredContacts.length > 0) {
                // Group contacts by privacy status
                // is_public can be: true, false, undefined, or null
                // Default to public if undefined/null
                const publicContacts = [];
                const privateContacts = [];
                
                filteredContacts.forEach(c => {
                    // Determine if contact is public
                    // Handle various cases: true, false, undefined, null, 1, 0
                    let isPublic = true; // default to public
                    if (c.is_public !== undefined && c.is_public !== null) {
                        // Convert to boolean if needed (handles 1/0 from database)
                        isPublic = c.is_public === true || c.is_public === 1 || c.is_public === 'true';
                    }
                    
                    if (isPublic) {
                        publicContacts.push(c);
                    } else {
                        privateContacts.push(c);
                    }
                });
                
                console.log(`[JS] Grouped contacts: ${publicContacts.length} public, ${privateContacts.length} private (total: ${filteredContacts.length})`);
                console.log(`[JS] Sample contact is_public values:`, filteredContacts.slice(0, 3).map(c => ({name: c.name, is_public: c.is_public, type: typeof c.is_public})));
                
                // Sort contacts: contacts with emails first, then alphabetically within each group
                const sortContacts = (a, b) => {
                    const aHasEmail = !!(a.email && a.email.trim());
                    const bHasEmail = !!(b.email && b.email.trim());
                    
                    // If one has email and the other doesn't, prioritize the one with email
                    if (aHasEmail && !bHasEmail) return -1;
                    if (!aHasEmail && bHasEmail) return 1;
                    
                    // Both have email or both don't - sort alphabetically by name
                    return (a.name || '').localeCompare(b.name || '');
                };
                publicContacts.sort(sortContacts);
                privateContacts.sort(sortContacts);
                
                // Always render both sections, even if empty
                // Private contacts appear first, then public contacts
                this.renderContactSection(contactsList, 'private', privateContacts, searchQuery);
                this.renderContactSection(contactsList, 'public', publicContacts, searchQuery);
            } else {
                // If no contacts at all, show message but still render empty sections
                const message = searchQuery 
                    ? `No contacts found matching "${searchQuery}"`
                    : 'You are not following anyone yet, or contacts could not be loaded.';
                
                // Still show sections structure even when empty
                this.renderContactSection(contactsList, 'private', [], searchQuery);
                this.renderContactSection(contactsList, 'public', [], searchQuery);
                
                // Also show a general message
                const messageDiv = document.createElement('div');
                messageDiv.className = 'text-muted text-center';
                messageDiv.style.padding = '20px';
                messageDiv.textContent = message;
                contactsList.insertBefore(messageDiv, contactsList.firstChild);
            }
        } catch (error) {
            console.error('Error rendering contacts:', error);
            // On error, at least show sections structure
            try {
                if (contactsList) {
                    contactsList.innerHTML = '';
                    this.renderContactSection(contactsList, 'private', [], '');
                    this.renderContactSection(contactsList, 'public', []);
                    const errorDiv = document.createElement('div');
                    errorDiv.className = 'text-muted text-center';
                    errorDiv.style.padding = '20px';
                    errorDiv.style.color = '#dc3545';
                    errorDiv.textContent = 'Error loading contacts. Please try refreshing.';
                    contactsList.insertBefore(errorDiv, contactsList.firstChild);
                }
            } catch (e) {
                console.error('Error in error handler:', e);
            }
        }
    }

    // Render a contact section (public or private)
    renderContactSection(container, sectionType, contacts, searchQuery = '') {
        const sectionLabel = sectionType === 'public' ? 'Public Contacts' : 'Private Contacts';
        const contactCount = contacts.length;
        // Collapse empty sections by default, but preserve user's manual toggle state if section has contacts
        // Default to expanded if state is not set
        const isExpanded = contactCount === 0 ? false : (this.sectionStates[sectionType] !== undefined ? this.sectionStates[sectionType] : true);
        
        console.log(`[JS] Rendering ${sectionLabel}: ${contactCount} contacts, expanded: ${isExpanded}`);
        
        // Create section container
        const sectionDiv = document.createElement('div');
        sectionDiv.className = 'contact-section';
        sectionDiv.setAttribute('data-section', sectionType);
        
        // Create section header
        const headerDiv = document.createElement('div');
        headerDiv.className = 'contact-section-header';
        headerDiv.setAttribute('data-section', sectionType);
        headerDiv.innerHTML = `
            <i class="fas fa-chevron-${isExpanded ? 'down' : 'right'} section-chevron"></i>
            <span class="section-label">${sectionLabel} (${contactCount})</span>
        `;
        
        // Add click handler to toggle section
        headerDiv.addEventListener('click', (e) => {
            e.stopPropagation();
            this.toggleSection(sectionType);
        });
        
        // Create section content container
        const contentDiv = document.createElement('div');
        contentDiv.className = 'contact-section-content';
        contentDiv.style.display = isExpanded ? 'block' : 'none';
        
        // Render contacts in this section
        if (contacts.length > 0) {
            contacts.forEach((contact, index) => {
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

                    // Add delete icon for private contacts (only visible on hover)
                    const isPublic = contact.is_public !== undefined ? contact.is_public : true;
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
                            ${emailIcon}
                        </div>
                    `;
                    
                    // Add click event listener for contact selection
                    contactElement.addEventListener('click', (e) => {
                        // Don't select contact if clicking on delete icon or email icon
                        if (e.target.closest('.contact-delete-icon') || e.target.closest('.contact-email-icon')) {
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
                    
                    contentDiv.appendChild(contactElement);
                });
            } else {
                // Show message if section is empty
                const emptyMessage = searchQuery 
                    ? `No ${sectionLabel.toLowerCase()} found matching "${searchQuery}"`
                    : `No ${sectionLabel.toLowerCase()} yet`;
                contentDiv.innerHTML = `<div class="text-muted text-center section-empty-message">${emptyMessage}</div>`;
            }
            
            // Assemble section
            sectionDiv.appendChild(headerDiv);
            sectionDiv.appendChild(contentDiv);
            container.appendChild(sectionDiv);
    }

    // Toggle section expand/collapse
    toggleSection(sectionType) {
        this.sectionStates[sectionType] = !this.sectionStates[sectionType];
        // Preserve current search query when re-rendering
        const searchInput = window.domManager.get('contactsSearch');
        const searchQuery = searchInput ? searchInput.value.trim() : '';
        this.renderContacts(searchQuery);
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
                        
                        // Stop scanning immediately to prevent multiple scans
                        cleanup();
                        
                        // Handle async decode in a separate async function
                        // Capture 'this' context to ensure it's available in the async IIFE
                        const self = this;
                        (async () => {
                            try {
                                // Decode the scanned identifier (handles nostr: prefix, nprofile1, npub1, etc.)
                                const scannedKey = decodedText.trim();
                                console.log('[QR] Calling decodeNostrIdentifier with:', scannedKey);
                                
                                if (!window.TauriService || !window.TauriService.decodeNostrIdentifier) {
                                    throw new Error('TauriService.decodeNostrIdentifier is not available');
                                }
                                
                                const decodedPubkey = await window.TauriService.decodeNostrIdentifier(scannedKey);
                                console.log('[QR] Successfully decoded to:', decodedPubkey);
                                
                                // Restore the add contact form with the decoded pubkey
                                if (!self || typeof self.showAddContactModal !== 'function') {
                                    throw new Error('showAddContactModal is not available');
                                }
                                self.showAddContactModal(decodedPubkey);
                            } catch (error) {
                                console.error('[QR] Failed to decode identifier:', error);
                                // Tauri errors can be strings or Error objects
                                let errorMessage = 'Not a valid Nostr identifier';
                                if (typeof error === 'string') {
                                    errorMessage = error;
                                } else if (error?.message) {
                                    errorMessage = error.message;
                                } else if (error?.toString) {
                                    errorMessage = error.toString();
                                }
                                
                                // Show error in modal or restore form
                                const errorDiv = document.getElementById('qr-scanner-error');
                                if (errorDiv) {
                                    errorDiv.style.display = 'block';
                                    errorDiv.textContent = `Invalid QR code: ${errorMessage}`;
                                } else {
                                    // If error div doesn't exist, restore the form and show notification
                                    window.notificationService.showError(`Failed to decode QR code: ${errorMessage}`);
                                    this.showAddContactModal();
                                }
                            }
                        })();
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
        
        const submitButton = event.target.querySelector('button[type="submit"]');
        const pubkeyInput = document.getElementById('contact-pubkey');
        const scanBtn = document.getElementById('scan-qr-btn');
        const formHelp = document.querySelector('.add-contact-form .form-help');
        
        try {
            let pubkey = pubkeyInput?.value || '';
            
            // Decode/validate Nostr identifier (handles nostr: prefix, nprofile1, npub1, hex, etc.)
            try {
                pubkey = await window.TauriService.decodeNostrIdentifier(pubkey);
                // Update the input with the decoded npub
                if (pubkeyInput) {
                    pubkeyInput.value = pubkey;
                }
            } catch (e) {
                // Tauri errors can be strings or Error objects
                let errorMsg = 'Invalid Nostr identifier';
                if (typeof e === 'string') {
                    errorMsg = e;
                } else if (e?.message) {
                    errorMsg = e.message;
                } else if (e?.toString) {
                    errorMsg = e.toString();
                }
                console.error('[ContactsService] Failed to decode identifier:', e);
                window.notificationService.showError(`Invalid Nostr identifier: ${errorMsg}`);
                return;
            }
            
            // Show loading state
            if (submitButton) {
                submitButton.disabled = true;
                submitButton.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Fetching Profile...';
            }
            if (pubkeyInput) {
                pubkeyInput.disabled = true;
            }
            if (scanBtn) {
                scanBtn.disabled = true;
            }
            if (formHelp) {
                formHelp.textContent = 'Fetching profile from relays...';
                formHelp.style.color = 'var(--primary-color, #007bff)';
            }
            
            // Fetch the profile
            try {
                const activeRelays = window.appState.getActiveRelays();
                if (activeRelays.length === 0) {
                    window.notificationService.showError('No active relays to fetch profile');
                    // Reset loading state
                    if (submitButton) {
                        submitButton.disabled = false;
                        submitButton.innerHTML = '<i class="fas fa-search"></i> Fetch Profile';
                    }
                    if (pubkeyInput) pubkeyInput.disabled = false;
                    if (scanBtn) scanBtn.disabled = false;
                    if (formHelp) {
                        formHelp.textContent = 'Enter a Nostr public key (npub) to fetch their profile';
                        formHelp.style.color = '';
                    }
                    return;
                }
                
                console.log('Fetching profile for:', pubkey);
                const profile = await window.TauriService.fetchProfilePersistent(pubkey);
                
                // Close the modal
                window.app.hideModal();
                
                if (profile) {
                    // Switch to contacts tab
                    window.app.switchTab('contacts');
                    // Wait for contacts to load and render before showing profile detail
                    await this.loadContacts();
                    // Wait a bit more for the UI to settle
                    await new Promise(resolve => setTimeout(resolve, 100));
                    // Show profile for adding with follow button
                    await this.showProfileForAdding(pubkey, profile);
                    
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
            // Reset loading state on error
            if (submitButton) {
                submitButton.disabled = false;
                submitButton.innerHTML = '<i class="fas fa-search"></i> Fetch Profile';
            }
            if (pubkeyInput) pubkeyInput.disabled = false;
            if (scanBtn) scanBtn.disabled = false;
            if (formHelp) {
                formHelp.textContent = 'Enter a Nostr public key (npub) to fetch their profile';
                formHelp.style.color = '';
            }
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
    async showProfileForAdding(pubkey, profile) {
        const contactsContainer = document.querySelector('.contacts-container');
        let contactsDetail = window.domManager.get('contactsDetail');
        const backButton = document.getElementById('contacts-back-btn');
        
        // Wait for detail element to be available (in case contacts are still loading)
        if (!contactsDetail) {
            console.log('[JS] Waiting for contacts detail element to be available...');
            for (let i = 0; i < 10; i++) {
                await new Promise(resolve => setTimeout(resolve, 100));
                contactsDetail = window.domManager.get('contactsDetail');
                if (contactsDetail) break;
            }
        }
        
        if (!contactsContainer || !contactsDetail) {
            console.warn('[JS] Contacts container or detail element not found after waiting');
            window.notificationService.showError('Failed to show profile detail. Please try again.');
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
                    this.confirmFollowContact(pubkey, profile, profileName, profile.fields.email || '', isPublic, followBtn);
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
                                this.confirmFollowContact(pubkey, profile, profileName, profile.fields.email || '', isPublic, followBtn2);
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
                                this.confirmFollowContact(pubkey, profile, profileName, profile.fields.email || '', isPublic, followBtn2);
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
        const startTime = performance.now();
        console.log('[JS] refreshContacts called - fetching fresh data from network...');
        console.log('[DEBUG] refreshContacts entry', {timestamp: Date.now()});
        
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
            let followListResult = null;
            let fetchSucceeded = false;
            const fetchFollowListStart = performance.now();
            try {
                followListResult = await window.TauriService.fetchFollowingPubkeysPersistent(
                    window.appState.getKeypair().public_key
                );
                fetchSucceeded = true; // Mark that fetch completed successfully
                console.log('[JS] Fetched follow list result using persistent client:', followListResult);
            } catch (error) {
                console.error('[JS] Error fetching followed pubkeys from relays:', error);
                // If fetch fails, we'll fall back to loading from database
                // This handles cases like: client not initialized, network errors, etc.
                followListResult = null;
                fetchSucceeded = false; // Mark that fetch failed
            }
            
            const userPubkey = window.appState.getKeypair().public_key;
            
            // Handle case where no kind 3 event found on any relay
            // If no event is found, abort sync to preserve existing DB state
            // This prevents accidental changes when relays are misconfigured
            // Note: event_found=false means no event found, event_found=true with empty pubkeys means valid empty list
            if (!fetchSucceeded || !followListResult || !followListResult.event_found) {
                console.log('[JS] No kind 3 event found on any relay - aborting sync to preserve existing contacts');
                window.notificationService.hideLoading(loadingNotification);
                if (!fetchSucceeded) {
                    window.notificationService.showError('Could not fetch follow list from relays. Your contacts were not updated. Please check your relay configuration.');
                } else {
                    window.notificationService.showError('No follow list found on relays. Your contacts were not updated. Please check your relay configuration.');
                }
                return;
            }
            
            // Extract pubkeys from the result (could be empty array if event has 0 tags - that's valid)
            const followedPubkeys = followListResult.pubkeys || [];
            console.log('[JS] Follow list event found (event_id: ' + (followListResult.event_id || 'unknown') + '), pubkeys: ' + followedPubkeys.length);
            
            // 2. Get all contacts from DB to sync is_public status with published list
            const getAllContacts1Start = performance.now();
            const dbAllContacts = await window.DatabaseService.getAllContacts(userPubkey);
            const followedPubkeysSet = new Set(followedPubkeys);
            
            // 3. Update is_public status for all contacts based on published kind 3 list
            // Contacts in published list → is_public=true, contacts not in list → is_public=false
            const contactsToUpdate = [];
            for (const dbContact of dbAllContacts) {
                const shouldBePublic = followedPubkeysSet.has(dbContact.pubkey);
                const currentlyPublic = dbContact.is_public !== undefined ? dbContact.is_public : true;
                
                // Only update if status needs to change
                if (shouldBePublic !== currentlyPublic) {
                    contactsToUpdate.push({ pubkey: dbContact.pubkey, isPublic: shouldBePublic });
                }
            }
            
            // Update all contacts that need status changes (batch operation for performance)
            if (contactsToUpdate.length > 0) {
                console.log(`[JS] Batch updating is_public status for ${contactsToUpdate.length} contacts based on published follow list`);
                const batchUpdateStart = performance.now();
                try {
                    // Convert to array of [pubkey, isPublic] tuples for batch update
                    const batchUpdates = contactsToUpdate.map(({ pubkey, isPublic }) => [pubkey, isPublic]);
                    await window.__TAURI__.core.invoke('db_batch_update_user_contact_public_status', {
                        userPubkey: userPubkey,
                        updates: batchUpdates
                    });
                    console.log(`[JS] Successfully batch updated is_public status for ${contactsToUpdate.length} contacts`);
                } catch (error) {
                    console.error(`[JS] Failed to batch update is_public status:`, error);
                    // Fallback to individual updates if batch fails
                    console.log(`[JS] Falling back to individual updates...`);
                    const failedUpdates = [];
                    for (const { pubkey, isPublic } of contactsToUpdate) {
                        try {
                            await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                                userPubkey: userPubkey,
                                contactPubkey: pubkey,
                                isPublic: isPublic
                            });
                        } catch (err) {
                            console.error(`[JS] Failed to update is_public status for contact ${pubkey}:`, err);
                            failedUpdates.push(pubkey);
                        }
                    }
                    if (failedUpdates.length > 0) {
                        console.warn(`[JS] Failed to update is_public status for ${failedUpdates.length} contacts:`, failedUpdates);
                    }
                }
            }
            
            const existingPublicPubkeys = dbAllContacts
                .filter(c => c.is_public !== undefined ? c.is_public : true)
                .map(c => c.pubkey);
            const unfollowedPubkeys = existingPublicPubkeys.filter(pubkey => !followedPubkeysSet.has(pubkey));
            
            // 4. Filter out npubs already in the DB for this user
            loadingNotification.innerHTML = `
                <div class="notification-content">
                    <i class="fas fa-spinner fa-spin"></i>
                    <span>Checking for new contacts...</span>
                </div>
            `;
            const filterNewContactsStart = performance.now();
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
                
                // Save minimal contacts to DB immediately using batch operation (is_public=true for public follows)
                const batchSaveStart = performance.now();
                try {
                    const dbContacts = minimalContacts.map(contact => 
                        window.DatabaseService.convertContactToDbFormat(contact)
                    );
                    await window.__TAURI__.core.invoke('db_batch_save_contacts', {
                        userPubkey: userPubkey,
                        contacts: dbContacts,
                        isPublic: true
                    });
                    console.log(`[JS] Batch saved ${minimalContacts.length} minimal contacts to DB`);
                } catch (error) {
                    console.error(`[JS] Batch save failed, falling back to individual saves:`, error);
                    // Fallback to individual saves if batch fails
                    const failedSaves = [];
                    for (const contact of minimalContacts) {
                        try {
                            const dbContact = window.DatabaseService.convertContactToDbFormat(contact);
                            await window.DatabaseService.saveContact(dbContact, userPubkey, true);
                        } catch (err) {
                            console.error(`[JS] Failed to save minimal contact ${contact.pubkey}:`, err);
                            failedSaves.push(contact.pubkey);
                        }
                    }
                    const savedCount = minimalContacts.length - failedSaves.length;
                    console.log(`[JS] Saved ${savedCount}/${minimalContacts.length} minimal contacts to DB`);
                    if (failedSaves.length > 0) {
                        console.warn(`[JS] Failed to save ${failedSaves.length} contacts:`, failedSaves);
                    }
                }
                
                // Get all local contacts (including the newly added minimal ones)
                const getAllContacts2Start = performance.now();
                const dbContacts = await window.DatabaseService.getAllContacts(userPubkey);
                // Sync is_public status with current relay state (followedPubkeysSet is defined above)
                const localContacts = dbContacts.map(dbContact => {
                    const reconstructed = this.reconstructContactFields(dbContact);
                    // Set is_public based on whether contact is in the public follow list from relays
                    reconstructed.is_public = followedPubkeysSet.has(dbContact.pubkey);
                    return reconstructed;
                });
                
                // Update UI immediately so users see contacts right away
                window.appState.setContacts(localContacts);
                const renderContacts1Start = performance.now();
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
                const fetchProfilesStart = performance.now();
                const newProfiles = await window.TauriService.fetchProfilesPersistent(newPubkeys);
                console.log(`[JS] Fetched new profiles using persistent client: ${newProfiles ? newProfiles.length : 0} profiles`);
                
                // 7. Update contacts with profile data incrementally
                if (newProfiles && newProfiles.length > 0) {
                    loadingNotification.innerHTML = `
                        <div class="notification-content">
                            <i class="fas fa-spinner fa-spin"></i>
                            <span>Updating contact details... (${newProfiles.length} profiles)</span>
                        </div>
                    `;
                    
                    // Update contacts with profile data using batch operation (much faster)
                    const batchUpdateProfilesStart = performance.now();
                    try {
                        const updatedContacts = newProfiles.map(profile => {
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
                            return window.DatabaseService.convertContactToDbFormat(updatedContact);
                        });
                        await window.__TAURI__.core.invoke('db_batch_save_contacts', {
                            userPubkey: userPubkey,
                            contacts: updatedContacts,
                            isPublic: true
                        });
                        console.log(`[JS] Batch updated ${newProfiles.length} contact profiles`);
                    } catch (error) {
                        console.error(`[JS] Batch profile update failed, falling back to individual updates:`, error);
                        // Fallback to individual updates if batch fails
                        const failedProfileUpdates = [];
                        for (const profile of newProfiles) {
                            try {
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
                            } catch (err) {
                                console.error(`[JS] Failed to update contact profile for ${profile.pubkey}:`, err);
                                failedProfileUpdates.push(profile.pubkey);
                            }
                        }
                        if (failedProfileUpdates.length > 0) {
                            console.warn(`[JS] Failed to update ${failedProfileUpdates.length} contact profiles:`, failedProfileUpdates);
                        }
                    }
                    
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
                            // Sync is_public with current relay state (followedPubkeysSet is defined above)
                            is_public: followedPubkeysSet.has(profile.pubkey)
                        };
                        updatedContactsInMemory.push(updatedContact);
                    }
                    
                    // Merge updated contacts with existing contacts (preserve contacts not in update list)
                    const updatedContactPubkeys = new Set(updatedContactsInMemory.map(c => c.pubkey));
                    const otherContacts = currentContacts.filter(c => !updatedContactPubkeys.has(c.pubkey));
                    const allUpdatedContacts = [...otherContacts, ...updatedContactsInMemory];
                    
                    // Update appState with contacts that have full fields data preserved (don't reload from DB)
                    window.appState.setContacts(allUpdatedContacts);
                    const renderContacts2Start = performance.now();
                    this.renderContacts();
                    
                    // Refresh compose dropdown to include new contacts
                    if (window.emailService) {
                        window.emailService.populateNostrContactDropdown();
                    }
                    
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
                const getAllContacts3Start = performance.now();
                const dbAllContacts = await window.DatabaseService.getAllContacts(userPubkey);
                // Sync is_public status with current relay state (followedPubkeysSet is defined above)
                const allContacts = dbAllContacts.map(dbContact => {
                    const reconstructed = this.reconstructContactFields(dbContact);
                    // Set is_public based on whether contact is in the public follow list from relays
                    reconstructed.is_public = followedPubkeysSet.has(dbContact.pubkey);
                    return reconstructed;
                });
                const allContactPubkeys = new Set(allContacts.map(c => c.pubkey));
                
                // Update is_public for contacts that are back in the follow list (were private, now public again)
                const reFollowedPubkeys = followedPubkeys.filter(pubkey => {
                    // Contact exists in user_contacts but wasn't in the public list (must have been private)
                    return allContactPubkeys.has(pubkey) && !existingPublicPubkeys.includes(pubkey);
                });
                if (reFollowedPubkeys.length > 0) {
                    console.log('[JS] Contacts re-added to public follow list:', reFollowedPubkeys);
                    const failedReFollows = [];
                    for (const pubkey of reFollowedPubkeys) {
                        try {
                            await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                                userPubkey: userPubkey,
                                contactPubkey: pubkey,
                                isPublic: true
                            });
                        } catch (error) {
                            console.error(`[JS] Failed to update re-followed contact ${pubkey}:`, error);
                            failedReFollows.push(pubkey);
                        }
                    }
                    if (failedReFollows.length > 0) {
                        console.warn(`[JS] Failed to update ${failedReFollows.length} re-followed contacts:`, failedReFollows);
                    }
                }
                
                // Get all local contacts for this user (after updates)
                const getAllContacts4Start = performance.now();
                const dbLocalContacts = await window.DatabaseService.getAllContacts(userPubkey);
                // Sync is_public status with current relay state (followedPubkeysSet is defined above)
                const localContacts = dbLocalContacts.map(dbContact => {
                    const reconstructed = this.reconstructContactFields(dbContact);
                    // Set is_public based on whether contact is in the public follow list from relays
                    reconstructed.is_public = followedPubkeysSet.has(dbContact.pubkey);
                    return reconstructed;
                });
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

    async confirmFollowContact(pubkey, profile, profileName, email, isPublic = true, buttonElement = null) {
        // Store original button state for restoration
        let originalButtonHTML = null;
        let originalButtonDisabled = false;
        
        // On mobile, ensure we're showing the contacts detail view before showing confirmation
        // This prevents the modal from appearing on the navigation page
        if (window.app && window.app.isMobilePortrait && window.app.isMobilePortrait()) {
            const contactsContainer = document.querySelector('.contacts-container');
            if (contactsContainer && !contactsContainer.classList.contains('contacts-detail-view')) {
                // Ensure detail view is active
                contactsContainer.classList.remove('contacts-list-view');
                contactsContainer.classList.add('contacts-detail-view');
            }
            // Ensure contacts tab is active
            window.app.switchTab('contacts');
            // Wait a moment for the view to settle
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        
        // Show a confirmation dialog
        const followType = isPublic ? 'publicly' : 'privately';
        const confirmed = await window.notificationService.showConfirmation(
            `Do you want to ${followType} follow nostr user ${pubkey}?`,
            'Confirm Follow'
        );
        if (!confirmed) {
            return;
        }
        
        // Set loading state on button after confirmation
        if (buttonElement) {
            originalButtonHTML = buttonElement.innerHTML;
            originalButtonDisabled = buttonElement.disabled;
            buttonElement.disabled = true;
            buttonElement.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Adding Contact...';
        }

        try {
            // Get your private key
            const privateKey = window.appState.getKeypair().private_key;
            const userPubkey = window.appState.getKeypair().public_key;

            // Only call backend to follow the user on Nostr if it's a public follow
            // Uses persistent client (no relays parameter needed)
            if (isPublic) {
                await window.TauriService.followUser(privateKey, pubkey);
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
            try {
                await window.DatabaseService.saveContact(dbContact, userPubkey, isPublic);
            } catch (error) {
                console.error('[JS] Failed to save contact to database:', error);
                if (buttonElement) {
                    buttonElement.disabled = originalButtonDisabled;
                    buttonElement.innerHTML = originalButtonHTML;
                }
                window.notificationService.showError('Failed to save contact: ' + error);
                return;
            }

            // Update app state and UI
            window.appState.addContact(contact);
            
            // Ensure we stay on the page - keep detail view active
            const contactsContainer = document.querySelector('.contacts-container');
            const isInDetailView = contactsContainer && contactsContainer.classList.contains('contacts-detail-view');
            
            if (isInDetailView) {
                // Re-render the contact detail with updated info (now it's a saved contact)
                this.renderContactDetail(contact);
                // Update selected contact in app state
                window.appState.setSelectedContact(contact);
            } else {
                // If not in detail view, just render contacts list
                this.renderContacts();
            }
            
            // Refresh compose dropdown to include new contact
            if (window.emailService) {
                window.emailService.populateNostrContactDropdown();
            }
            const followTypeMsg = isPublic ? 'publicly' : 'privately';
            window.notificationService.showSuccess(`You are now ${followTypeMsg} following ${profileName}!`);
            
            // Restore button state after successful completion
            if (buttonElement && originalButtonHTML !== null) {
                // Button will be removed/replaced by renderContactDetail, but restore just in case
                buttonElement.disabled = originalButtonDisabled;
                buttonElement.innerHTML = originalButtonHTML;
            }
        } catch (error) {
            window.notificationService.showError('Failed to follow and add contact: ' + error);
            
            // Restore button state on error
            if (buttonElement && originalButtonHTML !== null) {
                buttonElement.disabled = originalButtonDisabled;
                buttonElement.innerHTML = originalButtonHTML;
            }
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
                    try {
                        const userPubkey = window.appState.getKeypair().public_key;
                        const dbContact = window.DatabaseService.convertContactToDbFormat(selectedContact);
                        // Keep existing is_public status (default to true if updating)
                        await window.DatabaseService.saveContact(dbContact, userPubkey, true);
                    } catch (dbError) {
                        console.error('[JS] Failed to save updated contact to database:', dbError);
                        // Continue anyway - contact is updated in memory
                    }
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
            // Get contacts from appState
            const contacts = window.appState.getContacts() || [];
            // Find contact in current list
            const contactIndex = contacts.findIndex(c => c.pubkey === pubkey);
            if (contactIndex !== -1) {
                const contact = contacts[contactIndex];
                // Update contact fields
                if (profileFields.name) contact.name = profileFields.name;
                if (profileFields.display_name) contact.display_name = profileFields.display_name;
                if (profileFields.about) contact.about = profileFields.about;
                if (profileFields.picture) contact.picture = profileFields.picture;
                if (profileFields.nip05) contact.nip05 = profileFields.nip05;
                
                // Also update fields object if it exists
                if (contact.fields) {
                    if (profileFields.name) contact.fields.name = profileFields.name;
                    if (profileFields.display_name) contact.fields.display_name = profileFields.display_name;
                    if (profileFields.about) contact.fields.about = profileFields.about;
                    if (profileFields.picture) contact.fields.picture = profileFields.picture;
                    if (profileFields.nip05) contact.fields.nip05 = profileFields.nip05;
                }
                
                // Persist changes to appState
                window.appState.setContacts(contacts);
                
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
            } else {
                console.log('[ContactsService] Contact not found in contacts list:', pubkey);
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
            const activeRelays = window.appState.getActiveRelays();
            
            if (activeRelays.length === 0) {
                window.notificationService.showError('No active relays configured. Cannot publish follow list.');
                if (checkboxElement) checkboxElement.checked = !isPublic;
                return;
            }
            
            // Update in database
            await window.__TAURI__.core.invoke('db_update_user_contact_public_status', {
                userPubkey: userPubkey,
                contactPubkey: contactPubkey,
                isPublic: isPublic
            });

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
                
                // Re-render the contacts list
                this.renderContacts();
            }
            
            // Automatically publish the updated follow list to relays
            // This ensures that privacy changes are immediately reflected on relays
            try {
                // getActiveRelays() already returns an array of URLs (strings)
                await window.TauriService.publishFollowList(privateKey, userPubkey, activeRelays);
                console.log(`[JS] Successfully published follow list after ${isPublic ? 'making' : 'removing'} contact public`);
            } catch (publishError) {
                console.error('[JS] Error publishing follow list:', publishError);
                // Don't fail the entire operation - the database update succeeded
                // Just show a warning that the change is local only
                window.notificationService.showWarning(
                    `Contact privacy updated locally, but failed to publish to relays: ${publishError}. ` +
                    'You may need to manually publish your follow list later.'
                );
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