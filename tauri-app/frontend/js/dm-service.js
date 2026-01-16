// Tauri command parameter naming:
// Tauri automatically converts camelCase keys in JS to snake_case for Rust command parameters.
// For example, passing { userEmail: ... } from JS will be received as user_email in Rust.
// You can use camelCase in JS and it will map to the expected snake_case Rust parameter.
// See: https://tauri.app/v1/guides/features/command/#naming-conventions
// DM Service
// Handles all direct message functionality including conversations, messages, and management

// Remove all import/export statements. Attach DMService and dmService to window. Replace any usage of imported symbols with window equivalents if needed.

class DMService {
    constructor() {
        this.searchTimeout = null;
    }

    // Load DM contacts from backend and network
    async loadDmContacts() {
        console.log('[JS] loadDmContacts from database (decrypted in backend)...');
        if (!window.appState.hasKeypair()) {
            window.notificationService.showError('No keypair available');
            return;
        }

        // 1. Get sorted list of DM pubkeys
        const pubkeys = await window.__TAURI__.core.invoke('db_get_all_dm_pubkeys_sorted');
        const myPubkey = window.appState.getKeypair().public_key;
        const privateKey = window.appState.getKeypair().private_key;
        const dmContacts = [];

        for (const contactPubkey of pubkeys) {
            // LOG: Show which pubkeys are being used for the conversation fetch
            console.log(`[DM DEBUG] Fetching DMs for userPubkey: ${myPubkey}, contactPubkey: ${contactPubkey}`);
            // 1. Fetch decrypted messages for this conversation
            const messages = await window.__TAURI__.core.invoke('db_get_decrypted_dms_for_conversation', {
                privateKey: privateKey,
                userPubkey: myPubkey,
                contactPubkey: contactPubkey
            });
            // LOG: Show how many messages were returned
            console.log(`[DM DEBUG] Got ${messages?.length || 0} messages for userPubkey: ${myPubkey}, contactPubkey: ${contactPubkey}`);
            if (!messages || messages.length === 0) continue;

            // 2. Find the most recent message
            const lastMessageObj = messages[messages.length - 1];
            const lastMessage = lastMessageObj.content;
            const lastMessageTime = new Date(lastMessageObj.created_at);

            // 3. Check if any messages have email matches
            let hasEmailMatch = false;
            for (const msg of messages) {
                const emailMatch = await window.__TAURI__.core.invoke('db_check_dm_matches_email_encrypted', {
                    dmEventId: msg.event_id,
                    userPubkey: myPubkey,
                    contactPubkey: contactPubkey
                });
                if (emailMatch) {
                    hasEmailMatch = true;
                    break;
                }
            }

            // 4. Always fetch contact info from the database
            const profile = await window.DatabaseService.getContact(contactPubkey);
            const name = profile?.name || contactPubkey.substring(0, 16) + '...';
            // Use picture_data_url for avatars, fallback to picture_url or picture
            const picture_data_url = profile?.picture_data_url || null;
            const picture = profile?.picture_url || profile?.picture || '';
            const profileLoaded = !!profile;

            dmContacts.push({
                pubkey: contactPubkey,
                name,
                lastMessage,
                lastMessageTime,
                messageCount: messages.length,
                picture_data_url,
                picture, // ensure this is set for avatar fallback
                profileLoaded,
                hasEmailMatch // New field to track if this conversation has email matches
            });

            // 5. Cache decrypted messages in appState
            window.appState.setDmMessages(contactPubkey, messages);
        }

        // 6. Set and render contacts
        window.appState.setDmContacts(dmContacts);
        this.renderDmContacts();
    }

    // Load profiles for DM contacts
    async loadDmContactProfiles() {
        // Only process contacts that don't already have profiles loaded
        const dmContacts = window.appState.getDmContacts();
        const uncachedContacts = dmContacts.filter(contact => !contact.profileLoaded);
        
        if (uncachedContacts.length === 0) {
            console.log('[JS] All DM contacts already have profiles loaded');
            return;
        }
        
        try {
            const activeRelays = window.appState.getActiveRelays();
            if (activeRelays.length === 0) return;
            
            console.log(`[JS] Loading profiles for ${uncachedContacts.length} uncached DM contacts`);
            
            // Fetch profiles for uncached DM contacts
            const pubkeys = uncachedContacts.map(contact => contact.pubkey);
            const profiles = await window.TauriService.fetchProfiles(pubkeys, activeRelays);
            
            // Update contacts with profile information
            for (const profile of profiles) {
                const contactIndex = dmContacts.findIndex(c => c.pubkey === profile.pubkey);
                if (contactIndex !== -1) {
                    const contact = dmContacts[contactIndex];
                    contact.name = profile.fields.name || profile.fields.display_name || contact.pubkey.substring(0, 16) + '...';
                    contact.picture = profile.fields.picture || null;
                    contact.profileLoaded = true;
                    
                    // Try to cache the profile picture as a data URL for offline use
                    if (contact.picture) {
                        try {
                            // First try to get from backend cache
                            let dataUrl = await window.TauriService.getCachedProfileImage(contact.pubkey);
                            
                            // If not in cache, fetch and cache it
                            if (!dataUrl) {
                                dataUrl = await window.TauriService.fetchImage(contact.picture);
                                if (dataUrl) {
                                    // Cache in backend
                                    await window.TauriService.cacheProfileImage(contact.pubkey, dataUrl);
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
                    dmContacts[contactIndex] = contact;
                }
            }
            
            window.appState.setDmContacts(dmContacts);
            
            // Re-render with updated names and pictures
            this.renderDmContacts();
            
            // Update the DM cache with the new profile data in backend storage
            try {
                // Get current conversations from backend
                const currentConversations = await window.TauriService.getConversations();
                if (currentConversations && currentConversations.length > 0) {
                    // Update conversations with new profile data
                    const updatedConversations = currentConversations.map(conv => {
                        const updatedContact = dmContacts.find(c => c.pubkey === conv.contact_pubkey);
                        if (updatedContact) {
                            return {
                                ...conv,
                                contact_name: updatedContact.name,
                                cached_at: new Date().toISOString()
                            };
                        }
                        return conv;
                    });
                    
                    await window.TauriService.setConversations(updatedConversations);
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

    // Render DM contacts
    renderDmContacts(searchQuery = '') {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;
        
        try {
            dmContacts.innerHTML = '';
            
            // Filter contacts based on search query
            let filteredContacts = window.appState.getDmContacts();
            if (searchQuery) {
                filteredContacts = window.appState.getDmContacts().filter(contact => 
                    contact.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
                    contact.pubkey.toLowerCase().includes(searchQuery.toLowerCase()) ||
                    contact.lastMessage.toLowerCase().includes(searchQuery.toLowerCase())
                );
            }
            
            if (filteredContacts.length === 0) {
                const message = searchQuery 
                    ? `No contacts found matching "${searchQuery}"`
                    : 'No conversations yet';
                dmContacts.innerHTML = `<div class="text-center text-muted">${message}</div>`;
                return;
            }
            
            filteredContacts.forEach(contact => {
                // Sync picture_data_url from main contacts list if available
                const mainContact = window.appState.getContacts().find(c => c.pubkey === contact.pubkey);
                if (mainContact && mainContact.picture_data_url) {
                    contact.picture_data_url = mainContact.picture_data_url;
                }
                const contactElement = document.createElement('div');
                contactElement.className = 'dm-contact-item';
                contactElement.dataset.pubkey = contact.pubkey;
                
                // Format the last message time
                let dateObj;
                if (typeof contact.lastMessageTime === 'number') {
                    dateObj = new Date(contact.lastMessageTime * 1000);
                } else {
                    dateObj = new Date(contact.lastMessageTime);
                }
                const timeAgo = dateObj.toString() === 'Invalid Date' ? 'Unknown time' : window.Utils.formatTimeAgo(dateObj);
                
                // Create preview text
                let previewText = contact.lastMessage;
                if (previewText.length > 50) {
                    previewText = previewText.substring(0, 50) + '...';
                }
                
                // --- Avatar fallback logic (copied from contacts-service.js) ---
                const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
                let avatarSrc = defaultAvatar;
                let avatarClass = 'contact-avatar';
                const isValidDataUrl = contact.picture_data_url && contact.picture_data_url.startsWith('data:image') && contact.picture_data_url !== 'data:application/octet-stream;base64,';
                if (contact.picture_loading) {
                    avatarClass += ' loading';
                    console.log(`[DM AVATAR] ${contact.name}: picture_loading true, using default avatar.`);
                } else if (isValidDataUrl) {
                    avatarSrc = contact.picture_data_url;
                    console.log(`[DM AVATAR] ${contact.name}: using picture_data_url.`);
                } else if (contact.picture_data_url && !isValidDataUrl && contact.picture) {
                    avatarSrc = contact.picture;
                    console.log(`[DM AVATAR] ${contact.name}: invalid picture_data_url, falling back to picture (URL or fallback).`);
                } else if (contact.picture) {
                    avatarSrc = contact.picture;
                    console.log(`[DM AVATAR] ${contact.name}: using picture (URL or fallback).`);
                } else {
                    console.log(`[DM AVATAR] ${contact.name}: using default avatar (no image available).`);
                }
                // --- End avatar fallback logic ---
                
                // Add email emoji if this conversation has email matches
                let emailEmoji = '';
                if (contact.hasEmailMatch) {
                    emailEmoji = '<span class="email-emoji"><i class="fas fa-envelope"></i></span>';
                }
                
                contactElement.innerHTML = `
                    <img class="${avatarClass}" src="${avatarSrc}" alt="${contact.name}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
                    <div class="dm-contact-content">
                        <div class="dm-contact-header">
                            <div class="dm-contact-name">${contact.name} ${emailEmoji}</div>
                            <div class="dm-contact-time">${timeAgo}</div>
                        </div>
                        <div class="dm-contact-preview">${previewText}</div>
                        <div class="dm-contact-meta">
                            <span class="dm-message-count">${contact.messageCount} message${contact.messageCount !== 1 ? 's' : ''}</span>
                        </div>
                    </div>
                `;
                
                contactElement.addEventListener('click', () => this.selectDmContact(contact));
                dmContacts.appendChild(contactElement);
            });
        } catch (error) {
            console.error('Error rendering DM contacts:', error);
        }
    }

    // Select DM contact
    selectDmContact(contact) {
        try {
            window.appState.setSelectedDmContact(contact);
            
            // Update UI
            document.querySelectorAll('.dm-contact-item').forEach(item => {
                item.classList.remove('active');
            });
            
            const contactElement = document.querySelector(`[data-pubkey="${contact.pubkey}"]`);
            if (contactElement) {
                contactElement.classList.add('active');
            }
            
            // Load messages for this contact
            this.loadDmMessages(contact.pubkey);
            
        } catch (error) {
            console.error('Error selecting DM contact:', error);
        }
    }

    // Load DM messages
    async loadDmMessages(contactPubkey) {
        if (!window.appState.hasKeypair()) {
            window.notificationService.showError('No keypair available');
            return;
        }
        
        const myPubkey = window.appState.getKeypair().public_key;
        const privateKey = window.appState.getKeypair().private_key;
        
        // Check if messages are already cached
        const cachedMessages = window.appState.getDmMessages(contactPubkey);
        if (cachedMessages && cachedMessages.length > 0) {
            try {
                console.log(`[JS] Using cached messages for ${contactPubkey}`);
                // Even with cached messages, we need to check for email matches
                const messagesWithEmailMatches = await Promise.all(cachedMessages.map(async (msg) => {
                    // Check if this DM content matches an encrypted email subject (only if event_id exists)
                    let hasEmailMatch = false;
                    if (msg.event_id) {
                        try {
                            const contentPreview = msg.content ? msg.content.substring(0, 50) : '(no content)';
                            console.log(`[JS] Checking email match for cached message: ${contentPreview}...`);
                            hasEmailMatch = await window.__TAURI__.core.invoke('db_check_dm_matches_email_encrypted', {
                                dmEventId: msg.event_id,
                                userPubkey: myPubkey,
                                contactPubkey: contactPubkey
                            });
                            console.log(`[JS] Email match result for cached message: ${hasEmailMatch}`);
                        } catch (error) {
                            console.error(`[JS] Error checking email match for cached message:`, error);
                            hasEmailMatch = false;
                        }
                    }
                    
                    return {
                        ...msg,
                        event_id: msg.event_id || null,
                        hasEmailMatch: hasEmailMatch
                    };
                }));
                
                window.appState.setDmMessages(contactPubkey, messagesWithEmailMatches);
                this.renderDmMessages(contactPubkey);
                return;
            } catch (error) {
                console.error(`[JS] Error processing cached messages:`, error);
                // Fall through to fetch fresh messages
            }
        }
        
        try {
            // Fetch conversation messages from the local database (decrypted)
            const messages = await window.__TAURI__.core.invoke('db_get_decrypted_dms_for_conversation', {
                privateKey: privateKey,
                userPubkey: myPubkey,
                contactPubkey: contactPubkey
            });
            
            if (!messages || !Array.isArray(messages)) {
                console.error('[JS] Invalid messages response:', messages);
                window.notificationService.showError('Failed to load messages: invalid response');
                return;
            }
            
            // Check for email matches for each message
            const formattedMessages = await Promise.all(messages.map(async (msg) => {
                // Check if this DM content matches an encrypted email subject (only if event_id exists)
                let hasEmailMatch = false;
                if (msg.event_id) {
                    try {
                        const contentPreview = msg.content ? msg.content.substring(0, 50) : '(no content)';
                        console.log(`[JS] Checking email match for message: ${contentPreview}...`);
                        hasEmailMatch = await window.__TAURI__.core.invoke('db_check_dm_matches_email_encrypted', {
                            dmEventId: msg.event_id,
                            userPubkey: myPubkey,
                            contactPubkey: contactPubkey
                        });
                        console.log(`[JS] Email match result for message: ${hasEmailMatch}`);
                    } catch (error) {
                        console.error(`[JS] Error checking email match for message:`, error);
                        hasEmailMatch = false;
                    }
                }
                
                return {
                    id: msg.id,
                    event_id: msg.event_id || null,
                    content: msg.content || '',
                    created_at: msg.created_at || msg.timestamp,
                    sender_pubkey: msg.sender_pubkey,
                    is_sent: msg.sender_pubkey === myPubkey,
                    confirmed: true, // All DB messages are confirmed
                    hasEmailMatch: hasEmailMatch // New field to track email matches
                };
            }));
            
            window.appState.setDmMessages(contactPubkey, formattedMessages);
            this.renderDmMessages(contactPubkey);
            console.log(`✅ Loaded ${formattedMessages.length} messages from DB`);
        } catch (error) {
            console.error('Failed to load DM messages:', error);
            window.notificationService.showError('Failed to load messages');
        }
    }

    // Render DM messages
    renderDmMessages(contactPubkey) {
        const dmMessages = window.domManager.get('dmMessages');
        if (!dmMessages) return;
        
        try {
            const messages = window.appState.getDmMessages(contactPubkey) || [];
            const contact = window.appState.getDmContacts().find(c => c.pubkey === contactPubkey);
            
            console.log('[JS] renderDmMessages called for contact:', contactPubkey);
            console.log('[JS] Number of messages to render:', messages.length);
            console.log('[JS] Messages:', messages);
            
            dmMessages.innerHTML = '';
            
            if (messages.length === 0) {
                dmMessages.innerHTML = `
                    <div class="text-center text-muted" style="padding: 2rem;">
                        <i class="fas fa-comments" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
                        <p>No messages yet</p>
                        <p>Start a conversation with ${contact ? contact.name : 'this contact'}!</p>
                    </div>
                `;
            } else {
                // --- Avatar fallback logic for conversation header ---
                const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
                let avatarSrc = defaultAvatar;
                let avatarClass = 'contact-avatar';
                const isValidDataUrl = contact && contact.picture_data_url && contact.picture_data_url.startsWith('data:image') && contact.picture_data_url !== 'data:application/octet-stream;base64,';
                if (contact && contact.picture_loading) {
                    avatarClass += ' loading';
                    console.log(`[DM HEADER AVATAR] ${contact.name}: picture_loading true, using default avatar.`);
                } else if (isValidDataUrl) {
                    avatarSrc = contact.picture_data_url;
                    console.log(`[DM HEADER AVATAR] ${contact.name}: using picture_data_url.`);
                } else if (contact && contact.picture_data_url && !isValidDataUrl && contact.picture) {
                    avatarSrc = contact.picture;
                    console.log(`[DM HEADER AVATAR] ${contact.name}: invalid picture_data_url, falling back to picture (URL or fallback).`);
                } else if (contact && contact.picture) {
                    avatarSrc = contact.picture;
                    console.log(`[DM HEADER AVATAR] ${contact.name}: using picture (URL or fallback).`);
                } else {
                    console.log(`[DM HEADER AVATAR] ${contact ? contact.name : contactPubkey}: using default avatar (no image available).`);
                }
                // --- End avatar fallback logic ---
                const headerElement = document.createElement('div');
                headerElement.className = 'conversation-header';
                headerElement.innerHTML = `
                    <div class="conversation-contact-info">
                        <img class="contact-avatar" src="${avatarSrc}" alt="${contact ? contact.name : contactPubkey}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';" style="cursor: pointer;" data-pubkey="${contactPubkey}">
                        <div class="conversation-contact-details">
                            <div class="conversation-contact-name">${contact ? contact.name : contactPubkey}</div>
                            <div class="conversation-contact-pubkey">${contactPubkey}</div>
                        </div>
                    </div>
                `;
                dmMessages.appendChild(headerElement);
                
                // Add click handler to avatar to navigate to contacts page
                const avatarElement = headerElement.querySelector('.contact-avatar');
                if (avatarElement) {
                    avatarElement.addEventListener('click', () => {
                        this.navigateToContact(contactPubkey);
                    });
                }
                
                // Create messages container
                const messagesContainer = document.createElement('div');
                messagesContainer.className = 'messages-container';
                
                // Sort messages from oldest to newest (top to bottom)
                const sortedMessages = [...messages].sort((a, b) => a.created_at - b.created_at);
                
                console.log('[JS] Sorted messages:', sortedMessages);
                
                const myPubkey = window.appState.getKeypair()?.public_key;
                sortedMessages.forEach((message, index) => {
                    const isMe = message.sender_pubkey === myPubkey;
                    // Ensure is_sent is set correctly
                    if (message.is_sent === undefined) {
                        message.is_sent = isMe;
                    }
                    console.log(`[DM DEBUG] sender_pubkey: ${message.sender_pubkey} | myPubkey: ${myPubkey} | isMe: ${isMe} | message.is_sent: ${message.is_sent}`);
                    console.log(`[JS] Rendering message ${index}:`, message);
                    // Determine if this message is from me
                    const messageElement = document.createElement('div');
                    messageElement.className = `message ${isMe ? 'message-sent' : 'message-received'}`;
                    
                    // Defensive date handling
                    let dateObj;
                    if (typeof message.created_at === 'number') {
                        dateObj = new Date(message.created_at * 1000);
                    } else {
                        dateObj = new Date(message.created_at);
                    }
                    
                    // Format date and time
                    let dateTimeDisplay = 'Unknown';
                    if (dateObj.toString() !== 'Invalid Date') {
                        const now = new Date();
                        const isToday = dateObj.toDateString() === now.toDateString();
                        const isYesterday = new Date(now.getTime() - 24 * 60 * 60 * 1000).toDateString() === dateObj.toDateString();
                        
                        if (isToday) {
                            // Today: show time only
                            dateTimeDisplay = dateObj.toLocaleTimeString([], { 
                                hour: '2-digit', 
                                minute: '2-digit' 
                            });
                        } else if (isYesterday) {
                            // Yesterday: show "Yesterday" and time
                            dateTimeDisplay = `Yesterday ${dateObj.toLocaleTimeString([], { 
                                hour: '2-digit', 
                                minute: '2-digit' 
                            })}`;
                        } else {
                            // Other days: show date and time
                            dateTimeDisplay = dateObj.toLocaleDateString([], {
                                month: 'short',
                                day: 'numeric'
                            }) + ' ' + dateObj.toLocaleTimeString([], { 
                                hour: '2-digit', 
                                minute: '2-digit' 
                            });
                        }
                    }
                    
                    // Add checkmark for sent messages
                    let statusIcon = '';
                    if (message.is_sent) {
                        if (message.confirmed) {
                            // Double checkmark for confirmed messages
                            statusIcon = '<i class="fas fa-check-double message-status confirmed"></i>';
                        } else if (message.id && !String(message.id).startsWith('temp_')) {
                            // Single checkmark for sent but not yet confirmed
                            statusIcon = '<i class="fas fa-check message-status sent"></i>';
                        } else {
                            // Clock icon for pending messages (temporary IDs)
                            statusIcon = '<i class="fas fa-clock message-status pending"></i>';
                        }
                    }
                    
                    // Add email emoji if it's an email match
                    let emailEmoji = '';
                    console.log(`[JS] Message ${index} hasEmailMatch:`, message.hasEmailMatch);
                    if (message.hasEmailMatch) {
                        emailEmoji = '<span class="email-emoji"><i class="fas fa-envelope"></i></span>';
                        console.log(`[JS] Adding email emoji to message ${index}`);
                    }

                    if (message.hasEmailMatch) {
                        // For messages with email matches, replace DM content with details
                        const dmContent = `
                            <div class="email-body-expandable">
                                <div class="email-body-summary" data-expanded="false">${window.Utils.escapeHtml(message.content)}</div>
                            </div>
                        `;
                        
                        messageElement.innerHTML = `
                            <div class="message-content">
                                ${dmContent}
                                <div class="message-meta">
                                    <div class="message-time">${dateTimeDisplay}</div>
                                    ${statusIcon}
                                    <span class="email-emoji email-emoji-clickable" style="cursor: pointer;" title="${isMe ? 'Click to view sent email' : 'Click to view received email'}"><i class="fas fa-envelope"></i></span>
                                </div>
                            </div>
                        `;
                        
                        // Handle click toggle to load email body
                        const expandableElement = messageElement.querySelector('.email-body-expandable');
                        const summaryElement = messageElement.querySelector('.email-body-summary');
                        if (expandableElement && summaryElement) {
                            summaryElement.addEventListener('click', async (event) => {
                                const isExpanded = summaryElement.getAttribute('data-expanded') === 'true';
                                if (!isExpanded) {
                                    summaryElement.setAttribute('data-expanded', 'true');
                                    await this.loadEmailBody(message, expandableElement, contactPubkey);
                                } else {
                                    summaryElement.setAttribute('data-expanded', 'false');
                                    // Remove email content when collapsing
                                    const emailContent = expandableElement.querySelector('.email-body-content-text');
                                    if (emailContent) {
                                        expandableElement.removeChild(emailContent);
                                    }
                                }
                            });
                        }
                        
                        // Handle email icon click - navigate to email (sent or inbox)
                        if (message.hasEmailMatch && message.event_id) {
                            const emailIcon = messageElement.querySelector('.email-emoji-clickable');
                            console.log(`[JS] DM: Looking for clickable email icon, found:`, emailIcon, '| event_id:', message.event_id, '| isMe:', isMe);
                            if (emailIcon) {
                                emailIcon.addEventListener('click', async (event) => {
                                    event.stopPropagation();
                                    event.preventDefault();
                                    console.log(`[JS] DM: Email icon clicked for message with event_id:`, message.event_id, '| isMe:', isMe);
                                    try {
                                        // Find the matching email ID and message_id
                                        const emailResult = await window.__TAURI__.core.invoke('db_get_matching_email_id', {
                                            dmEventId: message.event_id
                                        });
                                        
                                        console.log(`[JS] DM: Found matching email result:`, emailResult);
                                        
                                        if (emailResult && emailResult.email_id && emailResult.message_id) {
                                            const emailId = emailResult.email_id;
                                            const messageId = emailResult.message_id;
                                            
                                            if (isMe) {
                                                // Sent message - navigate to sent tab
                                                // Switch to sent tab first
                                                if (window.app && window.app.switchTab) {
                                                    window.app.switchTab('sent');
                                                } else {
                                                    console.error('[JS] DM: window.app.switchTab not available');
                                                    return;
                                                }
                                                
                                                // Ensure sent emails are loaded
                                                if (!appState.getSentEmails() || appState.getSentEmails().length === 0) {
                                                    await window.emailService.loadSentEmails();
                                                }
                                                
                                                // Check if email exists in appState, if not fetch it
                                                let email = appState.getSentEmails().find(e => e.id == emailId || e.id === emailId.toString());
                                                if (!email) {
                                                    console.log(`[JS] DM: Email ${emailId} not in appState, fetching from database`);
                                                    const dbEmail = await window.TauriService.getDbEmail(messageId);
                                                    if (dbEmail) {
                                                        // Convert DbEmail to EmailMessage format and add to appState
                                                        const emailMessage = window.DatabaseService.convertDbEmailToEmailMessage(dbEmail);
                                                        const sentEmails = appState.getSentEmails();
                                                        sentEmails.push(emailMessage);
                                                        appState.setSentEmails(sentEmails);
                                                        email = emailMessage;
                                                    }
                                                }
                                                
                                                // Wait a bit for tab switch to complete, then show the email detail
                                                setTimeout(() => {
                                                    if (email) {
                                                        // Use the email's ID (which might be string) for showSentDetail
                                                        window.emailService.showSentDetail(email.id);
                                                        console.log(`[JS] DM: Navigated to sent email ${email.id}`);
                                                    } else {
                                                        window.notificationService.showError('Email not found');
                                                    }
                                                }, 100);
                                            } else {
                                                // Received message - navigate to inbox tab
                                                // Switch to inbox tab first
                                                if (window.app && window.app.switchTab) {
                                                    window.app.switchTab('inbox');
                                                } else {
                                                    console.error('[JS] DM: window.app.switchTab not available');
                                                    return;
                                                }
                                                
                                                // Ensure inbox emails are loaded
                                                if (!appState.getEmails() || appState.getEmails().length === 0) {
                                                    await window.emailService.loadEmails();
                                                }
                                                
                                                // Check if email exists in appState, if not fetch it
                                                let email = appState.getEmails().find(e => e.id == emailId || e.id === emailId.toString());
                                                if (!email) {
                                                    console.log(`[JS] DM: Email ${emailId} not in appState, fetching from database`);
                                                    const dbEmail = await window.TauriService.getDbEmail(messageId);
                                                    if (dbEmail) {
                                                        // Convert DbEmail to EmailMessage format and add to appState
                                                        const emailMessage = window.DatabaseService.convertDbEmailToEmailMessage(dbEmail);
                                                        const emails = appState.getEmails();
                                                        emails.push(emailMessage);
                                                        appState.setEmails(emails);
                                                        email = emailMessage;
                                                    }
                                                }
                                                
                                                // Wait a bit for tab switch to complete, then show the email detail
                                                setTimeout(() => {
                                                    if (email) {
                                                        // Use the email's ID (which might be string) for showEmailDetail
                                                        window.emailService.showEmailDetail(email.id);
                                                        console.log(`[JS] DM: Navigated to inbox email ${email.id}`);
                                                    } else {
                                                        window.notificationService.showError('Email not found');
                                                    }
                                                }, 100);
                                            }
                                        } else {
                                            window.notificationService.showError('Email not found');
                                        }
                                    } catch (error) {
                                        console.error('[JS] DM: Failed to navigate to email:', error);
                                        window.notificationService.showError('Failed to find email: ' + error.message);
                                    }
                                });
                                console.log(`[JS] DM: Click handler attached to email icon`);
                            } else {
                                console.warn(`[JS] DM: Email icon not found. Element classes:`, messageElement.className);
                            }
                        }
                    } else {
                        // For regular messages, use normal message structure
                        messageElement.innerHTML = `
                            <div class="message-content">
                                <div class="message-text">${window.Utils.escapeHtml(message.content)}</div>
                                <div class="message-meta">
                                    <div class="message-time">${dateTimeDisplay}</div>
                                    ${statusIcon}
                                </div>
                            </div>
                        `;
                    }
                    
                    messagesContainer.appendChild(messageElement);
                });
                
                dmMessages.appendChild(messagesContainer);
                
                // Scroll to bottom to show the newest messages
                setTimeout(() => {
                    messagesContainer.scrollTop = messagesContainer.scrollHeight;
                }, 0);
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
            dmMessages.appendChild(messageInputContainer);
            
            // Add event listeners for the new input elements
            const replyInput = document.getElementById('dm-reply-input');
            const sendBtn = document.getElementById('dm-send-btn');
            
            if (replyInput && sendBtn) {
                // Send message on Enter key
                replyInput.addEventListener('keypress', (e) => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        this.sendReplyMessage(contactPubkey);
                    }
                });
                
                // Send message on button click
                sendBtn.addEventListener('click', () => {
                    this.sendReplyMessage(contactPubkey);
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

    // Send reply message
    async sendReplyMessage(contactPubkey) {
        const replyInput = document.getElementById('dm-reply-input');
        const sendBtn = document.getElementById('dm-send-btn');
        const replyText = replyInput.value.trim();

        if (!replyText) {
            window.notificationService.showError('Message cannot be empty');
            return;
        }
        
        if (!window.appState.hasKeypair()) {
            window.notificationService.showError('No keypair available');
            return;
        }
        
        try {
            const activeRelays = window.appState.getActiveRelays();
            if (activeRelays.length === 0) {
                window.notificationService.showError('No active relays configured');
                return;
            }
            
            console.log(`🔄 Sending message to ${contactPubkey}...`);
            
            // Send message to Nostr
            const message = await window.TauriService.sendDirectMessage(
                window.appState.getKeypair().private_key,
                contactPubkey,
                replyText,
                activeRelays
            );
            
            // Add the message to the UI
                this.renderDmMessages(contactPubkey);
            
            console.log(`✅ Message sent to ${contactPubkey}`);
            replyInput.value = ''; // Clear input after sending
            sendBtn.disabled = true; // Disable button until new message is sent
            
            // Update the DM cache with the new message in backend storage
            try {
                // Get current conversations from backend
                const currentConversations = await window.TauriService.getConversations();
                if (currentConversations && currentConversations.length > 0) {
                    // Update conversations with new message
                    const updatedConversations = currentConversations.map(conv => {
                        if (conv.contact_pubkey === contactPubkey) {
                            return {
                                ...conv,
                                last_message: replyText,
                                last_message_time: new Date().toISOString(),
                                message_count: conv.message_count + 1,
                                cached_at: new Date().toISOString()
                            };
                        }
                        return conv;
                    });
                    
                    await window.TauriService.setConversations(updatedConversations);
                    console.log('[JS] Updated DM conversations in backend storage with new message');
                }
            } catch (e) {
                console.warn('Failed to update DM conversations in backend storage:', e);
            }
            
        } catch (error) {
            console.error('Failed to send DM message:', error);
            window.notificationService.showError('Failed to send message');
        }
    }

    // Handle message status updates (e.g., from backend)
    async handleMessageStatusUpdate(messageId, status) {
        const dmMessages = window.domManager.get('dmMessages');
        if (!dmMessages) return;

        const messageElement = dmMessages.querySelector(`.message[data-message-id="${messageId}"]`);
        if (messageElement) {
            const statusIcon = messageElement.querySelector('.message-status');
            if (statusIcon) {
                statusIcon.className = `fas fa-${status} message-status`;
            }
        }
    }

    // Handle conversation status updates (e.g., from backend)
    async handleConversationStatusUpdate(contactPubkey, status) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contactElement = dmContacts.querySelector(`.dm-contact-item[data-pubkey="${contactPubkey}"]`);
        if (contactElement) {
            const statusIcon = contactElement.querySelector('.dm-message-count');
            if (statusIcon) {
                statusIcon.className = `dm-message-count ${status}`;
            }
        }
    }

    // Handle conversation deletion (e.g., from backend)
    async handleConversationDeletion(contactPubkey) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contactElement = dmContacts.querySelector(`.dm-contact-item[data-pubkey="${contactPubkey}"]`);
        if (contactElement) {
            contactElement.remove();
        }
        window.appState.removeDmContact(contactPubkey);
        console.log(`[JS] Conversation with ${contactPubkey} deleted from UI`);
        }
        
    // Handle new conversation (e.g., from backend)
    async handleNewConversation(conversation) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contact = window.appState.getDmContacts().find(c => c.pubkey === conversation.contact_pubkey);
        if (!contact) {
            // If contact is not in the current list, add it
            const newContact = {
                pubkey: conversation.contact_pubkey,
                name: conversation.contact_name,
                lastMessage: conversation.last_message,
                lastMessageTime: new Date(conversation.last_message_time),
                messageCount: conversation.message_count,
                picture_data_url: null, // Will be loaded later
                profileLoaded: false
            };
            window.appState.addDmContact(newContact);
            this.renderDmContacts(); // Re-render to show new contact
        } else {
            // If contact is already in the list, update its last message and time
            contact.lastMessage = conversation.last_message;
            contact.lastMessageTime = new Date(conversation.last_message_time);
            contact.messageCount = conversation.message_count;
            window.appState.setDmContacts(window.appState.getDmContacts()); // Force re-render
        }
        console.log(`[JS] New conversation with ${conversation.contact_pubkey} added to UI`);
    }

    // Handle conversation update (e.g., from backend)
    async handleConversationUpdate(conversation) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contact = window.appState.getDmContacts().find(c => c.pubkey === conversation.contact_pubkey);
        if (contact) {
            contact.name = conversation.contact_name;
            contact.picture = conversation.picture;
            contact.profileLoaded = true;
            window.appState.setDmContacts(window.appState.getDmContacts()); // Force re-render
        }
        console.log(`[JS] Conversation with ${conversation.contact_pubkey} updated in UI`);
    }

    // Handle conversation profile update (e.g., from backend)
    async handleConversationProfileUpdate(profile) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contact = window.appState.getDmContacts().find(c => c.pubkey === profile.pubkey);
        if (contact) {
            contact.name = profile.fields.name || profile.fields.display_name || contact.pubkey.substring(0, 16) + '...';
            contact.picture = profile.fields.picture || null;
            contact.profileLoaded = true;
            window.appState.setDmContacts(window.appState.getDmContacts()); // Force re-render
        }
        console.log(`[JS] Conversation profile with ${profile.pubkey} updated in UI`);
    }

    // Handle conversation deletion (e.g., from backend)
    async handleConversationDeletion(contactPubkey) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contactElement = dmContacts.querySelector(`.dm-contact-item[data-pubkey="${contactPubkey}"]`);
        if (contactElement) {
            contactElement.remove();
        }
        window.appState.removeDmContact(contactPubkey);
        console.log(`[JS] Conversation with ${contactPubkey} deleted from UI`);
    }

    // Handle new conversation (e.g., from backend)
    async handleNewConversation(conversation) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contact = window.appState.getDmContacts().find(c => c.pubkey === conversation.contact_pubkey);
        if (!contact) {
            // If contact is not in the current list, add it
            const newContact = {
                pubkey: conversation.contact_pubkey,
                name: conversation.contact_name,
                lastMessage: conversation.last_message,
                lastMessageTime: new Date(conversation.last_message_time),
                messageCount: conversation.message_count,
                picture_data_url: null, // Will be loaded later
                profileLoaded: false
            };
            window.appState.addDmContact(newContact);
            this.renderDmContacts(); // Re-render to show new contact
        } else {
            // If contact is already in the list, update its last message and time
            contact.lastMessage = conversation.last_message;
            contact.lastMessageTime = new Date(conversation.last_message_time);
            contact.messageCount = conversation.message_count;
            window.appState.setDmContacts(window.appState.getDmContacts()); // Force re-render
        }
        console.log(`[JS] New conversation with ${conversation.contact_pubkey} added to UI`);
    }

    // Handle conversation update (e.g., from backend)
    async handleConversationUpdate(conversation) {
        const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contact = window.appState.getDmContacts().find(c => c.pubkey === conversation.contact_pubkey);
        if (contact) {
            contact.name = conversation.contact_name;
            contact.picture = conversation.picture;
            contact.profileLoaded = true;
            window.appState.setDmContacts(window.appState.getDmContacts()); // Force re-render
        }
        console.log(`[JS] Conversation with ${conversation.contact_pubkey} updated in UI`);
    }

    // Handle conversation profile update (e.g., from backend)
    async handleConversationProfileUpdate(profile) {
            const dmContacts = window.domManager.get('dmContacts');
        if (!dmContacts) return;

        const contact = window.appState.getDmContacts().find(c => c.pubkey === profile.pubkey);
        if (contact) {
            contact.name = profile.fields.name || profile.fields.display_name || contact.pubkey.substring(0, 16) + '...';
            contact.picture = profile.fields.picture || null;
            contact.profileLoaded = true;
            window.appState.setDmContacts(window.appState.getDmContacts()); // Force re-render
        }
        console.log(`[JS] Conversation profile with ${profile.pubkey} updated in UI`);
    }

    // Refresh DM conversations
    async refreshDmConversations() {
        console.log('[JS] Refreshing DM conversations...');
        
        // Show loading state on refresh button
        const refreshBtn = window.domManager.get('refreshDm');
        const originalRefreshBtnHTML = refreshBtn ? refreshBtn.innerHTML : null;
        if (refreshBtn) {
            refreshBtn.disabled = true;
            refreshBtn.innerHTML = '<span class="loading"></span> Refreshing...';
        }
        
        // Show loading notification
        window.notificationService.showInfo('Syncing DMs from network...');
        try {
            // Sync DMs from network to database
            const privateKey = window.appState.getKeypair()?.private_key;
            const relays = window.appState.getActiveRelays();
            if (!privateKey) {
                window.notificationService.showError('No private key available for DM sync');
                // Reset refresh button
                if (refreshBtn && originalRefreshBtnHTML) {
                    refreshBtn.disabled = false;
                    refreshBtn.innerHTML = originalRefreshBtnHTML;
                }
                return;
            }
            if (!relays || relays.length === 0) {
                window.notificationService.showError('No active relays configured');
                // Reset refresh button
                if (refreshBtn && originalRefreshBtnHTML) {
                    refreshBtn.disabled = false;
                    refreshBtn.innerHTML = originalRefreshBtnHTML;
                }
                return;
            }
            await window.__TAURI__.core.invoke('sync_direct_messages_with_network', {
                privateKey,
                relays
            });
            window.notificationService.showSuccess('DMs synced from network');
        } catch (error) {
            console.error('Failed to sync DMs from network:', error);
            window.notificationService.showError('Failed to sync DMs from network');
            // Reset refresh button on error
            if (refreshBtn && originalRefreshBtnHTML) {
                refreshBtn.disabled = false;
                refreshBtn.innerHTML = originalRefreshBtnHTML;
            }
            return;
        }
        // Clear DM cache from backend storage
        try {
            await window.TauriService.setConversations([]);
            console.log('[JS] Cleared DM conversations from backend storage');
        } catch (e) {
            console.warn('Failed to clear DM conversations from backend storage:', e);
        }
        try {
            // Clear current conversations
            window.appState.setDmContacts([]);
            window.appState.setDmMessages({});
            window.appState.setSelectedDmContact(null);
            // Clear the UI
            const dmContacts = window.domManager.get('dmContacts');
            if (dmContacts) {
                dmContacts.innerHTML = '<div class="text-center text-muted">Loading conversations...</div>';
            }
            const dmMessages = window.domManager.get('dmMessages');
            if (dmMessages) {
                dmMessages.innerHTML = `
                    <div class="text-center text-muted" style="padding: 2rem;">
                        <i class="fas fa-sync fa-spin" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
                        <p>Refreshing conversations...</p>
                    </div>
                `;
            }
            // Reload conversations
            await this.loadDmContacts();
            
            // Clear the spinner in dmMessages - it will be populated by loadDmMessages if a contact is selected
            const selectedContact = window.appState.getSelectedDmContact();
            if (!selectedContact && dmMessages) {
                // No contact selected, show placeholder
                dmMessages.innerHTML = `
                    <div class="text-center text-muted" style="padding: 2rem;">
                        <i class="fas fa-comments" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
                        <p>Select a conversation to view messages</p>
                    </div>
                `;
            } else if (selectedContact && dmMessages) {
                // Contact is selected, reload messages to clear spinner
                await this.loadDmMessages(selectedContact.pubkey);
            }
            
            // Reset refresh button
            if (refreshBtn && originalRefreshBtnHTML) {
                refreshBtn.disabled = false;
                refreshBtn.innerHTML = originalRefreshBtnHTML;
            }
            
            window.notificationService.showSuccess('Conversations refreshed');
        } catch (error) {
            console.error('Failed to refresh DM conversations:', error);
            window.notificationService.showError('Failed to refresh conversations');
            // Reset refresh button on error
            if (refreshBtn && originalRefreshBtnHTML) {
                refreshBtn.disabled = false;
                refreshBtn.innerHTML = originalRefreshBtnHTML;
            }
        }
    }

    // Send direct message to contact from contacts page
    sendDirectMessageToContact(pubkey) {
        // Switch to the DM tab
        const dmTab = document.querySelector('[data-tab="dm"]');
        if (dmTab) {
            dmTab.click();
        }
        // Find the contact in main contacts list
        const contact = window.appState.getContacts().find(c => c.pubkey === pubkey);
        const myPubkey = window.appState.getKeypair()?.public_key;
        if (!contact && pubkey !== myPubkey) {
            window.notificationService.showError('Contact not found');
            return;
        }
        // Special case: DM to self
        if (pubkey === myPubkey) {
            console.log('[DM DEBUG] Starting DM to self');
            // Add to DM contacts if not already present
            if (!window.appState.getDmContacts().find(c => c.pubkey === pubkey)) {
                window.appState.addDmContact({
                    pubkey: myPubkey,
                    name: 'Myself',
                    lastMessage: '',
                    lastMessageTime: new Date(),
                    messageCount: 0,
                    picture_data_url: null,
                    profileLoaded: true
                });
            }
            const dmContact = window.appState.getDmContacts().find(c => c.pubkey === pubkey);
            window.appState.setSelectedDmContact(dmContact);
            this.loadDmMessages(pubkey);
            return;
        }
        // Add to DM contacts if not already present
        if (!window.appState.getDmContacts().find(c => c.pubkey === pubkey)) {
            window.appState.addDmContact({
                pubkey: contact.pubkey,
                name: contact.name,
                lastMessage: '',
                lastMessageTime: new Date(),
                messageCount: 0,
                picture_data_url: contact.picture_data_url || null,
                profileLoaded: true
            });
        }
        // Select the DM contact and load messages
        const dmContact = window.appState.getDmContacts().find(c => c.pubkey === pubkey) || contact;
        window.appState.setSelectedDmContact(dmContact);
        this.loadDmMessages(pubkey);
    }

    // Navigate to contacts page and select contact if it exists
    navigateToContact(pubkey) {
        try {
            // Switch to contacts tab
            const contactsTab = document.querySelector('[data-tab="contacts"]');
            if (contactsTab) {
                contactsTab.click();
            }
            
            // Wait a bit for the tab to switch and contacts to render
            setTimeout(() => {
                // Find the contact in the contacts list
                const contacts = window.appState.getContacts() || [];
                const contact = contacts.find(c => c.pubkey === pubkey);
                
                if (contact) {
                    // Select the contact
                    if (window.contactsService && typeof window.contactsService.selectContact === 'function') {
                        window.contactsService.selectContact(contact);
                    } else {
                        console.warn('ContactsService not available');
                    }
                } else {
                    // Contact not found in contacts list - open add contact modal with pubkey pre-filled
                    if (window.contactsService && typeof window.contactsService.showAddContactModal === 'function') {
                        window.contactsService.showAddContactModal(pubkey);
                    } else {
                        console.warn('ContactsService not available');
                        window.notificationService.showInfo('Contact not found in your contacts list');
                    }
                }
            }, 100);
        } catch (error) {
            console.error('Error navigating to contact:', error);
            window.notificationService.showError('Failed to navigate to contact');
        }
    }

    // Load email body into expandable element
    async loadEmailBody(message, expandableElement, contactPubkey) {
        // Check if content is already loaded
        if (expandableElement.querySelector('.email-body-content-text')) {
            return;
        }
        
        // Show loading state
        const loadingDiv = document.createElement('div');
        loadingDiv.className = 'email-loading';
        loadingDiv.style.cssText = 'padding: 6px; text-align: center; color: #666; font-size: 12px;';
        loadingDiv.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Loading email...';
        expandableElement.appendChild(loadingDiv);
        
        try {
            // Fetch and display email body
            const privateKey = window.appState.getKeypair().private_key;
            const userPubkey = window.appState.getKeypair().public_key;
            
            const emailResult = await window.__TAURI__.core.invoke('db_get_matching_email_body', {
                dmEventId: message.event_id,
                privateKey: privateKey,
                userPubkey: userPubkey,
                contactPubkey: contactPubkey
            });
            
            // Remove loading state
            expandableElement.removeChild(loadingDiv);
            
            if (emailResult && emailResult.body) {
                // Check if the emailBody is a manifest JSON (starts with { and contains "body" and "ciphertext")
                let finalBody = emailResult.body;
                let manifest = null;
                try {
                    const parsed = JSON.parse(emailResult.body);
                    if (parsed.body && parsed.body.ciphertext && parsed.body.key_wrap) {
                        console.log('[JS] DM: Detected manifest format, decrypting body...');
                        // This is a manifest - extract and decrypt the body
                        manifest = parsed;
                        const bodyKey = manifest.body.key_wrap;
                        const encryptedBodyData = manifest.body.ciphertext;
                        
                        // Use emailService's decryptWithAES method
                        if (window.emailService && typeof window.emailService.decryptWithAES === 'function') {
                            try {
                                const decryptedBodyBase64 = await window.emailService.decryptWithAES(encryptedBodyData, bodyKey);
                                finalBody = atob(decryptedBodyBase64);
                                console.log('[JS] DM: Successfully decrypted manifest body');
                            } catch (aesError) {
                                console.error('[JS] DM: AES decryption failed:', aesError);
                                finalBody = `[AES Decryption Failed: ${aesError.message}]`;
                            }
                        } else {
                            console.error('[JS] DM: emailService.decryptWithAES not available');
                            finalBody = '[Manifest detected but AES decryption not available]';
                        }
                    }
                } catch (parseError) {
                    // Not JSON, use as-is (legacy format)
                    console.log('[JS] DM: Not a manifest format, using body as-is');
                }
                
                // Display email body
                const emailDiv = document.createElement('div');
                emailDiv.className = 'email-body-content-text';
                emailDiv.textContent = finalBody;
                expandableElement.appendChild(emailDiv);
                
                // Display attachments if email_id is available
                if (emailResult.email_id) {
                    try {
                        const attachments = await TauriService.getAttachmentsForEmail(emailResult.email_id);
                        console.log(`[JS] DM: Found ${attachments.length} attachments for email ${emailResult.email_id}`);
                        
                        if (attachments && attachments.length > 0) {
                            // Build attachment display data
                            let attachmentDisplayData = [];
                            
                            if (manifest && manifest.attachments) {
                                // Map database attachments to manifest metadata
                                attachmentDisplayData = attachments.map(dbAttachment => {
                                    if (dbAttachment.encryption_method === 'manifest_aes') {
                                        // Find corresponding manifest entry by opaque ID
                                        const opaqueId = dbAttachment.filename.replace('.dat', ''); // a1.dat -> a1
                                        const manifestAttachment = manifest.attachments.find(ma => ma.id === opaqueId);
                                        
                                        if (manifestAttachment) {
                                            return {
                                                ...dbAttachment,
                                                displayName: manifestAttachment.orig_filename,
                                                displaySize: manifestAttachment.orig_size || dbAttachment.size,
                                                displayMime: manifestAttachment.orig_mime || dbAttachment.mime_type
                                            };
                                        }
                                    }
                                    return {
                                        ...dbAttachment,
                                        displayName: dbAttachment.filename,
                                        displaySize: dbAttachment.size,
                                        displayMime: dbAttachment.mime_type
                                    };
                                });
                            } else {
                                // Plain attachments - use database data directly
                                attachmentDisplayData = attachments.map(att => ({
                                    ...att,
                                    displayName: att.filename,
                                    displaySize: att.size,
                                    displayMime: att.mime_type
                                }));
                            }
                            
                            // Create attachments HTML
                            const attachmentsHtml = `
                                <div class="email-attachments" style="margin: 15px 0;">
                                    <h4 style="margin-bottom: 10px;">Attachments (${attachmentDisplayData.length})</h4>
                                    <div class="attachment-list">
                                        ${attachmentDisplayData.map(attachment => {
                                            const sizeFormatted = (attachment.displaySize / 1024).toFixed(2) + ' KB';
                                            const isEncrypted = attachment.encryption_method === 'manifest_aes';
                                            const statusIcon = isEncrypted ? '🔒' : '📄';
                                            const statusText = isEncrypted ? 'Encrypted' : 'Plain';
                                            
                                            return `
                                            <div class="attachment-item" style="display: flex; justify-content: space-between; align-items: center; padding: 10px; border: 1px solid #ddd; border-radius: 4px; margin: 5px 0;">
                                                <div class="attachment-info" style="display: flex; align-items: center;">
                                                    <i class="fas fa-file" style="margin-right: 10px;"></i>
                                                    <div class="attachment-details">
                                                        <div class="attachment-name" style="font-weight: bold;">${window.Utils.escapeHtml(attachment.displayName)}</div>
                                                        <div class="attachment-meta" style="font-size: 0.9em; color: #666;">
                                                            ${sizeFormatted} • ${statusIcon} ${statusText}
                                                        </div>
                                                    </div>
                                                </div>
                                                <div class="attachment-actions">
                                                    <button class="btn btn-sm btn-outline-primary" onclick="dmService.downloadDmAttachment(${attachment.id}, ${emailResult.email_id})">
                                                        <i class="fas fa-download"></i> Download
                                                    </button>
                                                </div>
                                            </div>`;
                                        }).join('')}
                                    </div>
                                </div>
                            `;
                            
                            const attachmentsDiv = document.createElement('div');
                            attachmentsDiv.innerHTML = attachmentsHtml;
                            expandableElement.appendChild(attachmentsDiv);
                        }
                    } catch (attachmentError) {
                        console.error('[JS] DM: Failed to load attachments:', attachmentError);
                        // Don't show error to user, just log it
                    }
                }
            } else {
                const noEmailDiv = document.createElement('div');
                noEmailDiv.style.cssText = 'padding: 6px; color: #888; text-align: center; font-size: 12px;';
                noEmailDiv.textContent = 'No matching email found';
                expandableElement.appendChild(noEmailDiv);
            }
        } catch (error) {
            console.error('Failed to fetch email body:', error);
            // Remove loading state
            if (expandableElement.contains(loadingDiv)) {
                expandableElement.removeChild(loadingDiv);
            }
            const errorDiv = document.createElement('div');
            errorDiv.style.cssText = 'padding: 6px; color: #dc3545; text-align: center; font-size: 12px;';
            errorDiv.textContent = 'Failed to load email body';
            expandableElement.appendChild(errorDiv);
        }
    }

    // Download attachment from DM conversation email
    async downloadDmAttachment(attachmentId, emailId) {
        try {
            console.log(`[JS] DM: Downloading attachment ${attachmentId} from email ${emailId}`);
            
            const attachment = await TauriService.getAttachment(attachmentId);
            if (!attachment) {
                window.notificationService.showError('Attachment not found');
                return;
            }
            
            // Check if attachment is encrypted and needs decryption
            if (attachment.encryption_method === 'manifest_aes') {
                // For manifest-encrypted attachments, we need to get the email and decrypt the manifest
                // Try to find email in inbox or sent emails
                let email = null;
                const inboxEmails = appState.getEmails() || [];
                const sentEmails = appState.getSentEmails() || [];
                email = inboxEmails.find(e => e.id == emailId) || sentEmails.find(e => e.id == emailId);
                
                if (!email) {
                    // Email not in cache - we need the email body to decrypt the manifest
                    // For now, show an error. In the future, we could fetch the email from backend
                    window.notificationService.showError('Email not found in cache. Please refresh the email list to download attachments.');
                    return;
                }
                
                // Extract encrypted content from email body
                const encryptedBodyMatch = email.body.match(/-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/);
                if (!encryptedBodyMatch) {
                    window.notificationService.showError('Cannot decrypt attachment: no encrypted manifest found');
                    return;
                }
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    window.notificationService.showError('No keypair available for decryption');
                    return;
                }
                
                // Decrypt the manifest to get attachment keys
                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                
                // Try to decrypt manifest (for received emails, use sender's pubkey; for sent emails, use recipient's pubkey)
                let manifestResult;
                try {
                    // Check if this is a sent or received email
                    const settings = appState.getSettings();
                    const userEmail = settings?.email_address;
                    const isSentEmail = (email.from_address && email.from_address === userEmail) || (email.from && email.from === userEmail);
                    
                    if (isSentEmail) {
                        manifestResult = await window.emailService.decryptSentManifestMessage(email, encryptedContent, keypair);
                    } else {
                        manifestResult = await window.emailService.decryptManifestMessage(email, encryptedContent, keypair);
                    }
                } catch (e) {
                    console.error('[JS] DM: Failed to decrypt manifest:', e);
                    window.notificationService.showError('Failed to decrypt manifest: ' + e.message);
                    return;
                }
                
                if (manifestResult.type !== 'manifest') {
                    window.notificationService.showError('Cannot decrypt attachment: invalid manifest');
                    return;
                }
                
                // Find attachment metadata in manifest
                const opaqueId = attachment.filename.replace('.dat', '');
                const attachmentMeta = manifestResult.manifest.attachments.find(a => a.id === opaqueId);
                
                if (!attachmentMeta) {
                    window.notificationService.showError('Attachment metadata not found in manifest');
                    return;
                }
                
                // Decrypt attachment data
                const decryptedData = await window.emailService.decryptWithAES(attachment.data, attachmentMeta.key_wrap, true);
                
                // Save decrypted attachment to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    attachmentMeta.orig_filename || attachmentMeta.orig_mime?.split('/')[1] || 'attachment',
                    decryptedData,
                    attachmentMeta.orig_mime || attachment.mime_type || 'application/octet-stream'
                );
                
                console.log(`[JS] DM: Downloaded decrypted attachment: ${attachmentMeta.orig_filename} to ${filePath}`);
                window.notificationService.showSuccess(`Attachment saved to: ${filePath}`);
                
            } else {
                // Plain attachment - save directly to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    attachment.filename,
                    attachment.data,
                    attachment.content_type || attachment.mime_type || 'application/octet-stream'
                );
                
                console.log(`[JS] DM: Downloaded plain attachment: ${attachment.filename} to ${filePath}`);
                window.notificationService.showSuccess(`Attachment saved to: ${filePath}`);
            }
            
        } catch (error) {
            console.error('[JS] DM: Failed to download attachment:', error);
            window.notificationService.showError('Failed to download attachment: ' + error.message);
        }
    }
}

// Export the service to window
window.dmService = new DMService();