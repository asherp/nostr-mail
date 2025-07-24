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

            // 3. Always fetch contact info from the database
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
                profileLoaded
            });

            // 6. Cache decrypted messages in appState
            window.appState.setDmMessages(contactPubkey, messages);
        }

        // 7. Set and render contacts
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
                
                contactElement.innerHTML = `
                    <img class="${avatarClass}" src="${avatarSrc}" alt="${contact.name}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
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
        // Check if messages are already cached
        if (window.appState.getDmMessages(contactPubkey) && window.appState.getDmMessages(contactPubkey).length > 0) {
            console.log(`[JS] Using cached messages for ${contactPubkey}`);
            this.renderDmMessages(contactPubkey);
            return;
        }
        try {
            const myPubkey = window.appState.getKeypair().public_key;
            const privateKey = window.appState.getKeypair().private_key;
            // Fetch conversation messages from the local database (decrypted)
            const messages = await window.__TAURI__.core.invoke('db_get_decrypted_dms_for_conversation', {
                privateKey: privateKey,
                userPubkey: myPubkey,
                contactPubkey: contactPubkey
            });
            // Convert to the format expected by the UI
            const formattedMessages = messages.map(msg => ({
                id: msg.id,
                content: msg.content,
                created_at: msg.created_at || msg.timestamp,
                sender_pubkey: msg.sender_pubkey,
                is_sent: msg.sender_pubkey === myPubkey,
                confirmed: true // All DB messages are confirmed
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
                    <div class="conversation-contact-info" style="display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 1.5rem 0 1rem 0;">
                        <img class="${avatarClass}" src="${avatarSrc}" alt="${contact ? contact.name : contactPubkey}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';" style="width: 64px; height: 64px; border-radius: 50%; margin-bottom: 1rem; object-fit: cover;">
                        <div class="conversation-contact-name" style="font-size: 1.3rem; font-weight: bold; text-align: center;">${contact ? contact.name : contactPubkey}</div>
                        <div class="conversation-contact-pubkey" style="font-size: 0.95rem; color: #aaa; text-align: center; margin-top: 0.2rem;">${contactPubkey}</div>
                    </div>
                `;
                dmMessages.appendChild(headerElement);
                
                // Create messages container
                const messagesContainer = document.createElement('div');
                messagesContainer.className = 'messages-container';
                
                // Sort messages from oldest to newest (top to bottom)
                const sortedMessages = [...messages].sort((a, b) => a.created_at - b.created_at);
                
                console.log('[JS] Sorted messages:', sortedMessages);
                
                const myPubkey = window.appState.getKeypair()?.public_key;
                sortedMessages.forEach((message, index) => {
                    const isMe = message.sender_pubkey === myPubkey;
                    console.log(`[DM DEBUG] sender_pubkey:`, message.sender_pubkey, '| myPubkey:', myPubkey, '| isMe:', isMe);
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
                    const time = dateObj.toString() === 'Invalid Date' ? 'Unknown' : dateObj.toLocaleTimeString([], { 
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
                            <div class="message-text">${window.Utils.escapeHtml(message.content)}</div>
                            <div class="message-meta">
                                <div class="message-time">${time}</div>
                                ${statusIcon}
                            </div>
                        </div>
                    `;
                    
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
        // Show loading notification
        window.notificationService.showInfo('Syncing DMs from network...');
        try {
            // Sync DMs from network to database
            const privateKey = window.appState.getKeypair()?.private_key;
            const relays = window.appState.getActiveRelays();
            if (!privateKey) {
                window.notificationService.showError('No private key available for DM sync');
                return;
            }
            if (!relays || relays.length === 0) {
                window.notificationService.showError('No active relays configured');
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
            window.notificationService.showSuccess('Conversations refreshed');
        } catch (error) {
            console.error('Failed to refresh DM conversations:', error);
            window.notificationService.showError('Failed to refresh conversations');
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
}

// Export the service to window
window.dmService = new DMService();