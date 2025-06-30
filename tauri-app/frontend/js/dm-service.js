// DM Service
// Handles all direct message functionality including conversations, messages, and management

import { appState } from './app-state.js';
import { domManager } from './dom-manager.js';
import { TauriService } from './tauri-service.js';
import { notificationService } from './notification-service.js';
import { Utils } from './utils.js';

export class DMService {
    constructor() {
        this.searchTimeout = null;
    }

    // Load DM contacts from backend and network
    async loadDmContacts() {
        console.log('[JS] loadDmContacts called - starting DM loading...');
        
        if (!appState.hasKeypair()) {
            notificationService.showError('No keypair available');
            return;
        }

        // Load cached contacts to get profile information (needed for both cached and network data)
        let cachedContacts = [];
        try {
            cachedContacts = await TauriService.getContacts();
            if (cachedContacts && cachedContacts.length > 0) {
                console.log(`[JS] Loaded ${cachedContacts.length} cached contacts for DM profiles`);
            }
        } catch (e) {
            console.warn('Failed to load cached contacts for DM profiles:', e);
        }

        // Try to load from backend storage first for instant display
        let cacheLoaded = false;
        try {
            console.log('[JS] Loading DM conversations from backend storage...');
            const cachedData = await TauriService.getConversations();
            
            if (cachedData && cachedData.length > 0) {
                console.log('[JS] Found cached DM conversations in backend, rendering immediately...');
                
                // Convert conversations to the format expected by the UI, using cached contact profiles
                const dmContacts = cachedData.map(conv => {
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
                const dmMessages = {};
                cachedData.forEach(conv => {
                    if (conv.messages && conv.messages.length > 0) {
                        // Preserve existing local cache for this contact
                        const existingMessages = appState.getDmMessages(conv.contact_pubkey) || [];
                        
                        dmMessages[conv.contact_pubkey] = conv.messages.map(msg => {
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
                dmContacts.sort((a, b) => b.lastMessageTime - a.lastMessageTime);
                
                appState.setDmContacts(dmContacts);
                Object.keys(dmMessages).forEach(pubkey => {
                    appState.setDmMessages(pubkey, dmMessages[pubkey]);
                });
                
                this.renderDmContacts();
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
            const activeRelays = appState.getActiveRelays();
            if (activeRelays.length === 0) {
                if (!cacheLoaded) {
                    notificationService.showError('No active relays configured');
                }
                return;
            }

            console.log('🔄 Loading conversations from network...');
            
            // Fetch conversations from Nostr
            const conversations = await TauriService.fetchConversations(
                appState.getKeypair().private_key,
                activeRelays
            );
            
            console.log('[JS] Network response:', {
                conversationsReceived: !!conversations,
                conversationsLength: conversations?.length || 0
            });
            
            // Only update if we actually got conversations from the network
            if (conversations && conversations.length > 0) {
                // Convert conversations to the format expected by the UI, using cached contact profiles
                const dmContacts = conversations.map(conv => {
                    // Try to find this contact in the cached profiles
                    const cachedContact = cachedContacts.find(c => c.pubkey === conv.contact_pubkey);
                    
                    // Store the messages from the conversation data
                    if (conv.messages && conv.messages.length > 0) {
                        // Preserve existing local cache for this contact
                        const existingMessages = appState.getDmMessages(conv.contact_pubkey) || [];
                        
                        const messages = conv.messages.map(msg => {
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
                        
                        appState.setDmMessages(conv.contact_pubkey, messages);
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
                dmContacts.sort((a, b) => b.lastMessageTime - a.lastMessageTime);
                
                appState.setDmContacts(dmContacts);
                
                // Render contacts immediately
                this.renderDmContacts();
                
                // Only load profiles for contacts that aren't already cached
                const uncachedContacts = dmContacts.filter(contact => !contact.profileLoaded);
                if (uncachedContacts.length > 0) {
                    console.log(`[JS] Loading profiles for ${uncachedContacts.length} uncached DM contacts`);
                    await this.loadDmContactProfiles();
                } else {
                    console.log('[JS] All DM contacts already have cached profiles');
                }
                
                console.log(`✅ Loaded ${dmContacts.length} conversations from network`);
                
                // Write to backend storage after successful load
                try {
                    // Add cached_at field to conversations before saving
                    const conversationsWithTimestamp = conversations.map(conv => ({
                        ...conv,
                        cached_at: new Date().toISOString()
                    }));
                    
                    await TauriService.setConversations(conversationsWithTimestamp);
                    console.log('[JS] Cached DM conversations in backend storage');
                } catch (e) {
                    console.warn('Failed to cache DM conversations in backend:', e);
                }
            } else {
                console.log('[JS] No conversations received from network, keeping cached data if available');
                if (!cacheLoaded) {
                    appState.setDmContacts([]);
                    this.renderDmContacts();
                }
            }
            
        } catch (error) {
            console.error('Failed to load DM contacts from network:', error);
            if (!cacheLoaded) {
                notificationService.showError('Failed to load conversations and no cached data available');
                appState.setDmContacts([]);
                this.renderDmContacts();
            } else {
                console.log('[JS] Network failed, but using cached data');
            }
        }
    }

    // Load profiles for DM contacts
    async loadDmContactProfiles() {
        // Only process contacts that don't already have profiles loaded
        const dmContacts = appState.getDmContacts();
        const uncachedContacts = dmContacts.filter(contact => !contact.profileLoaded);
        
        if (uncachedContacts.length === 0) {
            console.log('[JS] All DM contacts already have profiles loaded');
            return;
        }
        
        try {
            const activeRelays = appState.getActiveRelays();
            if (activeRelays.length === 0) return;
            
            console.log(`[JS] Loading profiles for ${uncachedContacts.length} uncached DM contacts`);
            
            // Fetch profiles for uncached DM contacts
            const pubkeys = uncachedContacts.map(contact => contact.pubkey);
            const profiles = await TauriService.fetchProfiles(pubkeys, activeRelays);
            
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
                            let dataUrl = await TauriService.getCachedProfileImage(contact.pubkey);
                            
                            // If not in cache, fetch and cache it
                            if (!dataUrl) {
                                dataUrl = await TauriService.fetchImage(contact.picture);
                                if (dataUrl) {
                                    // Cache in backend
                                    await TauriService.cacheProfileImage(contact.pubkey, dataUrl);
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
            
            appState.setDmContacts(dmContacts);
            
            // Re-render with updated names and pictures
            this.renderDmContacts();
            
            // Update the DM cache with the new profile data in backend storage
            try {
                // Get current conversations from backend
                const currentConversations = await TauriService.getConversations();
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
                    
                    await TauriService.setConversations(updatedConversations);
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
        const dmContacts = domManager.get('dmContacts');
        if (!dmContacts) return;
        
        try {
            dmContacts.innerHTML = '';
            
            // Filter contacts based on search query
            let filteredContacts = appState.getDmContacts();
            if (searchQuery) {
                filteredContacts = appState.getDmContacts().filter(contact => 
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
                const contactElement = document.createElement('div');
                contactElement.className = 'dm-contact-item';
                contactElement.dataset.pubkey = contact.pubkey;
                
                // Format the last message time
                const timeAgo = Utils.formatTimeAgo(contact.lastMessageTime);
                
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
            appState.setSelectedDmContact(contact);
            
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
        if (!appState.hasKeypair()) {
            notificationService.showError('No keypair available');
            return;
        }
        
        // Check if messages are already cached
        if (appState.getDmMessages(contactPubkey) && appState.getDmMessages(contactPubkey).length > 0) {
            console.log(`[JS] Using cached messages for ${contactPubkey}`);
            this.renderDmMessages(contactPubkey);
            return;
        }
        
        try {
            const activeRelays = appState.getActiveRelays();
            if (activeRelays.length === 0) {
                notificationService.showError('No active relays configured');
                return;
            }

            console.log(`🔄 Loading messages for ${contactPubkey}...`);
            
            // Fetch conversation messages from Nostr
            const messages = await TauriService.fetchConversationMessages(
                appState.getKeypair().private_key,
                contactPubkey,
                activeRelays
            );
            
            // Convert to the format expected by the UI
            const formattedMessages = messages.map(msg => ({
                id: msg.id,
                content: msg.content,
                created_at: msg.timestamp,
                pubkey: msg.sender_pubkey,
                is_sent: msg.is_sent,
                confirmed: msg.is_sent // If we can fetch it from network, it's confirmed
            }));
            
            appState.setDmMessages(contactPubkey, formattedMessages);
            this.renderDmMessages(contactPubkey);
            
            console.log(`✅ Loaded ${formattedMessages.length} messages`);
            
        } catch (error) {
            console.error('Failed to load DM messages:', error);
            notificationService.showError('Failed to load messages');
        }
    }

    // Render DM messages
    renderDmMessages(contactPubkey) {
        const dmMessages = domManager.get('dmMessages');
        if (!dmMessages) return;
        
        try {
            const messages = appState.getDmMessages(contactPubkey) || [];
            const contact = appState.getDmContacts().find(c => c.pubkey === contactPubkey);
            
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
                // Create conversation header
                const headerElement = document.createElement('div');
                headerElement.className = 'conversation-header';
                headerElement.innerHTML = `
                    <div class="conversation-contact-info">
                        <div class="conversation-contact-name">${contact ? contact.name : contactPubkey}</div>
                        <div class="conversation-contact-pubkey">${contactPubkey}</div>
                    </div>
                `;
                dmMessages.appendChild(headerElement);
                
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
                            <div class="message-text">${Utils.escapeHtml(message.content)}</div>
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

    // Send reply message (renamed from sendDmMessage to match original)
    async sendReplyMessage(contactPubkey) {
        const replyInput = document.getElementById('dm-reply-input');
        if (!replyInput) return;
        
        const message = replyInput.value.trim();
        if (!message) return;
        
        if (!appState.hasKeypair()) {
            notificationService.showError('No keypair available');
            return;
        }
        
        try {
            // Clear input
            replyInput.value = '';
            
            // Create temporary message for immediate display
            const tempMessage = {
                id: Utils.generateId(),
                content: message,
                created_at: Math.floor(Date.now() / 1000),
                pubkey: appState.getKeypair().public_key,
                is_sent: true,
                confirmed: false
            };
            
            // Add to messages
            const currentMessages = appState.getDmMessages(contactPubkey) || [];
            currentMessages.push(tempMessage);
            appState.setDmMessages(contactPubkey, currentMessages);
            
            // Re-render messages
            this.renderDmMessages(contactPubkey);
            
            // Send via Nostr
            const activeRelays = appState.getActiveRelays();
            if (activeRelays.length === 0) {
                notificationService.showError('No active relays configured');
                return;
            }
            
            const result = await TauriService.sendDirectMessage(
                appState.getKeypair().private_key,
                contactPubkey,
                message,
                activeRelays
            );
            
            // Update message as confirmed
            const updatedMessages = appState.getDmMessages(contactPubkey);
            const messageIndex = updatedMessages.findIndex(m => m.id === tempMessage.id);
            if (messageIndex !== -1) {
                updatedMessages[messageIndex].confirmed = true;
                updatedMessages[messageIndex].id = result.event_id || tempMessage.id;
                appState.setDmMessages(contactPubkey, updatedMessages);
                this.renderDmMessages(contactPubkey);
            }
            
            notificationService.showSuccess('Message sent');
            
        } catch (error) {
            console.error('Failed to send DM:', error);
            notificationService.showError('Failed to send message');
        }
    }

    // Render a single message (for backward compatibility)
    renderMessage(message) {
        const isSent = message.is_sent;
        const messageClass = isSent ? 'dm-message sent' : 'dm-message received';
        const timeAgo = Utils.formatTimeAgo(new Date(message.created_at * 1000));
        
        return `
            <div class="${messageClass}">
                <div class="dm-message-content">
                    <div class="dm-message-text">${Utils.escapeHtml(message.content)}</div>
                    <div class="dm-message-time">${timeAgo}</div>
                    ${isSent ? `<div class="dm-message-status">${message.confirmed ? '✓' : '⏳'}</div>` : ''}
                </div>
            </div>
        `;
    }

    // Send DM message (for backward compatibility)
    async sendDmMessage(contactPubkey) {
        return this.sendReplyMessage(contactPubkey);
    }

    // Filter DM contacts
    filterDmContacts() {
        const searchQuery = domManager.getValue('dmSearch')?.trim() || '';
        
        // Clear existing timeout
        if (this.searchTimeout) {
            clearTimeout(this.searchTimeout);
        }
        
        // Set a new timeout to debounce the search
        this.searchTimeout = setTimeout(() => {
            try {
                this.renderDmContacts(searchQuery);
            } catch (error) {
                console.error('Error filtering DM contacts:', error);
            }
        }, 300); // 300ms delay
    }

    // Toggle DM search
    toggleDmSearch() {
        const searchContainer = domManager.get('dmSearchContainer');
        if (searchContainer) {
            const isVisible = searchContainer.style.display !== 'none';
            searchContainer.style.display = isVisible ? 'none' : 'block';
            
            if (!isVisible) {
                // Focus the search input when showing
                const searchInput = domManager.get('dmSearch');
                if (searchInput) {
                    searchInput.focus();
                }
            } else {
                // Clear search when hiding
                domManager.clear('dmSearch');
                this.renderDmContacts();
            }
        }
    }

    // Send direct message to contact
    sendDirectMessageToContact(pubkey) {
        // Switch to DM tab
        const dmTab = document.querySelector('[data-tab="dm"]');
        if (dmTab) {
            dmTab.click();
        }
        
        // Immediately clear the DM message area to prevent flashing the previous conversation
        const dmMessages = domManager.get('dmMessages');
        if (dmMessages) {
            dmMessages.innerHTML = '';
        }
        
        // Try to find an existing DM contact
        let contact = appState.getDmContacts().find(c => c.pubkey === pubkey);
        if (!contact) {
            // Try to find the contact in the contacts list for name/picture
            const baseContact = appState.getContacts().find(c => c.pubkey === pubkey);
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
            const currentDmContacts = appState.getDmContacts();
            currentDmContacts.push(contact);
            appState.setDmContacts(currentDmContacts);
            appState.setDmMessages(pubkey, []);
        }
        
        // Render the skeleton UI immediately
        this.renderDmConversationSkeleton(contact);
        
        // Select the DM contact to show the conversation view (will fetch/load messages)
        this.selectDmContact(contact);
    }

    // Render DM conversation skeleton (loading state)
    renderDmConversationSkeleton(contact) {
        const dmMessages = domManager.get('dmMessages');
        if (!dmMessages) return;
        
        dmMessages.innerHTML = '';

        // Header
        const headerElement = document.createElement('div');
        headerElement.className = 'conversation-header';
        headerElement.innerHTML = `
            <div class="conversation-contact-info">
                <div class="conversation-contact-name">${contact ? contact.name : contact.pubkey}</div>
                <div class="conversation-contact-pubkey">${contact.pubkey}</div>
            </div>
        `;
        dmMessages.appendChild(headerElement);

        // Loading spinner for messages
        const loadingElement = document.createElement('div');
        loadingElement.className = 'messages-loading';
        loadingElement.innerHTML = `
            <div class="text-center text-muted" style="padding: 2rem;">
                <i class="fas fa-sync fa-spin" style="font-size: 2rem; margin-bottom: 1rem; opacity: 0.5;"></i>
                <p>Loading messages...</p>
            </div>
        `;
        dmMessages.appendChild(loadingElement);

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
        dmMessages.appendChild(messageInputContainer);

        // Add event listeners for the new input elements
        const replyInput = document.getElementById('dm-reply-input');
        const sendBtn = document.getElementById('dm-send-btn');
        
        if (replyInput && sendBtn) {
            // Send message on Enter key
            replyInput.addEventListener('keypress', (e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    this.sendReplyMessage(contact.pubkey);
                }
            });
            
            // Send message on button click
            sendBtn.addEventListener('click', () => {
                this.sendReplyMessage(contact.pubkey);
            });
            
            // Focus the input
            setTimeout(() => {
                replyInput.focus();
            }, 100);
        }
    }

    // Refresh DM conversations
    async refreshDmConversations() {
        console.log('[JS] Refreshing DM conversations...');
        
        // Clear DM cache from backend storage
        try {
            await TauriService.setConversations([]);
            console.log('[JS] Cleared DM conversations from backend storage');
        } catch (e) {
            console.warn('Failed to clear DM conversations from backend storage:', e);
        }
        
        try {
            // Clear current conversations
            appState.setDmContacts([]);
            appState.setDmMessages({});
            appState.setSelectedDmContact(null);
            
            // Clear the UI
            const dmContacts = domManager.get('dmContacts');
            if (dmContacts) {
                dmContacts.innerHTML = '<div class="text-center text-muted">Loading conversations...</div>';
            }
            const dmMessages = domManager.get('dmMessages');
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
            
            notificationService.showSuccess('Conversations refreshed');
            
        } catch (error) {
            console.error('Failed to refresh DM conversations:', error);
            notificationService.showError('Failed to refresh conversations');
        }
    }
}

// Create and export a singleton instance
export const dmService = new DMService();

// Make it available globally for other services to access
window.dmService = dmService; 