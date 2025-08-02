// Tauri command parameter naming:
// Tauri automatically converts camelCase keys in JS to snake_case for Rust command parameters.
// For example, passing { userEmail: ... } from JS will be received as user_email in Rust.
// You can use camelCase in JS and it will map to the expected snake_case Rust parameter.
// See: https://tauri.app/v1/guides/features/command/#naming-conventions
// Email Service
// Handles all email-related functionality including sending, fetching, and management

class EmailService {
    constructor() {
        this.searchTimeout = null;
        this.selectedNostrContact = null;
        this.plaintextSubject = ''; // Store plaintext subject
        this.plaintextBody = ''; // Store plaintext body
        this.currentDraftId = null; // Track the current draft being edited
        this.currentDraftDbId = null; // Track the database ID of the current draft
        this.currentMessageId = null; // Store the UUID for reuse
    }

    // Populate Nostr contact dropdown with contacts that have email addresses
    populateNostrContactDropdown() {
        const dropdown = domManager.get('nostrContactSelect');
        if (!dropdown) return;

        // Store the currently selected value
        const prevValue = dropdown.value;

        // Clear existing options except the first one
        dropdown.innerHTML = '<option value="">Select a Nostr contact with email...</option>';

        const contacts = appState.getContacts();
        const contactsWithEmail = contacts.filter(contact => contact.email && contact.email.trim() !== '');

        if (contactsWithEmail.length === 0) {
            const option = document.createElement('option');
            option.value = '';
            option.textContent = 'No contacts with email addresses found';
            option.disabled = true;
            dropdown.appendChild(option);
            return;
        }

        // Sort contacts alphabetically by name
        contactsWithEmail.sort((a, b) => {
            const nameA = a.name.toLowerCase();
            const nameB = b.name.toLowerCase();
            return nameA.localeCompare(nameB);
        });

        // Add contacts to dropdown
        contactsWithEmail.forEach(contact => {
            const option = document.createElement('option');
            // Format: Name — email (email lighter if possible)
            option.value = contact.pubkey;
            option.textContent = `${contact.name} — ${contact.email}`;
            option.dataset.email = contact.email;
            option.dataset.name = contact.name;
            dropdown.appendChild(option);
        });

        // Restore the selection if it still exists
        if (prevValue && Array.from(dropdown.options).some(opt => opt.value === prevValue)) {
            dropdown.value = prevValue;
        }

        console.log(`[JS] Populated Nostr contact dropdown with ${contactsWithEmail.length} contacts`);
    }

    // Handle Nostr contact selection
    handleNostrContactSelection() {
        const select = domManager.get('nostrContactSelect');
        const selectedValue = select.value;
        const sendMatchingDmGroup = document.querySelector('.checkbox-group');
        
        if (selectedValue && selectedValue !== '') {
            // Find the selected contact
            const contacts = appState.getContacts();
            this.selectedNostrContact = contacts.find(contact => contact.pubkey === selectedValue);
            
            if (this.selectedNostrContact) {
                console.log('[JS] Selected Nostr contact:', this.selectedNostrContact.name);
                // Auto-fill the email address
                domManager.setValue('toAddress', this.selectedNostrContact.email);
                // Save the contact selection for later restoration
                this.saveContactSelection();
                // Update DM checkbox visibility based on encryption state
                this.updateDmCheckboxVisibility();
            }
        } else {
            this.selectedNostrContact = null;
            // Clear saved contact selection when none is selected
            this.clearSavedContactSelection();
            // Hide the send matching DM checkbox when no Nostr contact is selected
            if (sendMatchingDmGroup) {
                sendMatchingDmGroup.style.display = 'none';
            }
        }
    }

    // Update DM checkbox visibility based on encryption state
    updateDmCheckboxVisibility() {
        const sendMatchingDmGroup = document.querySelector('.checkbox-group');
        const encryptBtn = domManager.get('encryptBtn');
        
        if (!sendMatchingDmGroup || !encryptBtn) return;
        
        const isEncrypted = encryptBtn.dataset.encrypted === 'true';
        
        if (this.selectedNostrContact && isEncrypted) {
            // Show DM checkbox only when a Nostr contact is selected AND message is encrypted
            sendMatchingDmGroup.style.display = 'block';
            console.log('[JS] Showing DM checkbox - contact selected and message encrypted');
        } else {
            // Hide DM checkbox when no contact selected or message not encrypted
            sendMatchingDmGroup.style.display = 'none';
            console.log('[JS] Hiding DM checkbox - no contact or message not encrypted');
        }
    }

    // Hash a string using SHA-256 and return hex
    async hashStringSHA256(str) {
        const encoder = new TextEncoder();
        const data = encoder.encode(str);
        const hashBuffer = await window.crypto.subtle.digest('SHA-256', data);
        return Array.from(new Uint8Array(hashBuffer)).map(b => b.toString(16).padStart(2, '0')).join('');
    }

    // Clear current draft state (for new compose)
    clearCurrentDraft() {
        this.currentDraftId = null;
        this.currentDraftDbId = null;
        this.selectedNostrContact = null;
        this.plaintextSubject = '';
        this.plaintextBody = '';
        this.currentMessageId = null; // Reset message ID when clearing draft
        // Clear saved contact selection when clearing draft
        this.clearSavedContactSelection();
        // Hide DM checkbox when clearing draft
        this.updateDmCheckboxVisibility();
    }
    
    // Generate and store message ID for reuse
    generateAndStoreMessageId() {
        if (!this.currentMessageId) {
            this.currentMessageId = `<${this.generateUUID()}@nostr-mail>`;
            console.log('[JS] Generated new message ID for reuse:', this.currentMessageId);
        }
        return this.currentMessageId;
    }

    // Send email with optional NIP-04 encryption
    async sendEmail() {
        console.log('[JS] sendEmail function called');
        console.log('[JS] appState.settings:', appState.getSettings());
        
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
            return;
        }
        
        const toAddress = domManager.getValue('toAddress')?.trim() || '';
        const subject = domManager.getValue('subject')?.trim() || '';
        const body = domManager.getValue('messageBody')?.trim() || '';
        
        console.log('[JS] Form values:', { toAddress, subject, body });
        console.log('[JS] Selected Nostr contact:', this.selectedNostrContact);
        
        if (!toAddress || !subject || !body) {
            console.log('[JS] Form validation failed - missing fields');
            notificationService.showError('Please fill in all fields');
            return;
        }
        
        console.log('[JS] Form validation passed');
        
        // Use stored message ID or generate new one
        const messageId = this.generateAndStoreMessageId();
        console.log('[JS] Using message ID:', messageId);
        
        // Check if using Gmail and warn about App Password
        const settings = appState.getSettings();
        if (settings.smtp_host === 'smtp.gmail.com') {
            console.log('[JS] Gmail detected, checking for App Password warning');
            const isGmailAddress = settings.email_address?.includes('@gmail.com');
            if (isGmailAddress) {
                console.log('[JS] Showing Gmail App Password info message');
                notificationService.showSuccess('Gmail detected: Make sure you\'re using an App Password, not your regular password. If you haven\'t set up an App Password, go to Google Account > Security > 2-Step Verification > App passwords.');
            }
        }
        
        console.log('[JS] About to enter try block');
        
        try {
            domManager.disable('sendBtn');
            domManager.setHTML('sendBtn', '<span class="loading"></span> Sending...');
            
            // Determine TLS setting - automatically enable for Gmail if not set
            let useTls = settings.use_tls;
            if (settings.smtp_host === 'smtp.gmail.com' && !useTls) {
                console.log('[JS] Auto-enabling TLS for Gmail (was disabled)');
                useTls = true;
            }
            
            console.log('[JS] Email config debug:', {
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                use_tls_setting: settings.use_tls,
                use_tls_final: useTls,
                email: settings.email_address
            });
            
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port,
                use_tls: useTls,
                private_key: appState.getKeypair() ? appState.getKeypair().private_key : null
            };
            
            // If a Nostr contact is selected, send encrypted email
            if (this.selectedNostrContact) {
                console.log('[JS] Sending encrypted email to Nostr contact:', this.selectedNostrContact.name);
                
                // Check if we have a keypair and active relays
                if (!appState.hasKeypair()) {
                    notificationService.showError('No Nostr keypair available for encryption');
                    return;
                }
                
                const activeRelays = appState.getActiveRelays();
                if (activeRelays.length === 0) {
                    notificationService.showError('No active Nostr relays configured');
                    return;
                }
                
                // Send encrypted email using NIP-04
                await this.sendEncryptedEmail(emailConfig, this.selectedNostrContact, subject, body, messageId);
            } else {
                // Send regular email
                console.log('[JS] Sending regular email');
                await TauriService.sendEmail(emailConfig, toAddress, subject, body, null, messageId);
            }
            
            console.log('[JS] Email sent successfully');
            
            // Clear form
            domManager.clear('toAddress');
            domManager.clear('subject');
            domManager.clear('messageBody');
            domManager.setValue('nostrContactSelect', '');
            this.selectedNostrContact = null;
            
            // Clear current draft state since we sent the email
            this.clearCurrentDraft(); // This will reset currentMessageId
            
            // Update DM checkbox visibility after clearing form
            this.updateDmCheckboxVisibility();
            
            notificationService.showSuccess('Email sent successfully');
            
        } catch (error) {
            console.error('[JS] Error in sendEmail function:', error);
            console.error('[JS] Error stack:', error.stack);
            notificationService.showError('Failed to send email: ' + error);
        } finally {
            domManager.enable('sendBtn');
            domManager.setHTML('sendBtn', '<i class="fas fa-paper-plane"></i> Send');
        }
    }

    // Preview email headers without sending
    async previewEmailHeaders() {
        console.log('[JS] previewEmailHeaders function called');
        
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
            return;
        }
        
        const toAddress = domManager.getValue('toAddress')?.trim() || '';
        const subject = domManager.getValue('subject')?.trim() || '';
        const body = domManager.getValue('messageBody')?.trim() || '';
        
        console.log('[JS] Form values for header preview:', { toAddress, subject, body });
        
        if (!toAddress || !subject || !body) {
            console.log('[JS] Form validation failed - missing fields');
            notificationService.showError('Please fill in all fields to preview headers');
            return;
        }
        
        console.log('[JS] Form validation passed for header preview');
        
        // Use stored message ID or generate new one
        const messageId = this.generateAndStoreMessageId();
        console.log('[JS] Using message ID for preview:', messageId);
        
        try {
            const settings = appState.getSettings();
            
            // Determine TLS setting - automatically enable for Gmail if not set
            let useTls = settings.use_tls;
            if (settings.smtp_host === 'smtp.gmail.com' && !useTls) {
                console.log('[JS] Auto-enabling TLS for Gmail (was disabled)');
                useTls = true;
            }
            
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port,
                use_tls: useTls,
                private_key: appState.getKeypair() ? appState.getKeypair().private_key : null
            };
            
            // Construct headers (sender's pubkey will be extracted from private_key in backend)
            const headers = await TauriService.constructEmailHeaders(emailConfig, toAddress, subject, body, null, messageId);
            
            // Debug: Log what headers we actually received
            console.log('[JS] Headers received from backend:');
            console.log(headers);
            console.log('[JS] Headers length:', headers.length);
            console.log('[JS] Headers contains Message-ID:', headers.includes('Message-ID'));
            console.log('[JS] Headers contains Message-ID:', headers.toLowerCase().includes('message-id'));
            
            // Show headers in a modal
            this.showHeadersModal(headers);
            
        } catch (error) {
            console.error('[JS] Error in previewEmailHeaders function:', error);
            notificationService.showError('Failed to preview headers: ' + error);
        }
    }

    // Show headers in a modal
    showHeadersModal(headers) {
        console.log('[JS] showHeadersModal called with headers:');
        console.log(headers);
        console.log('[JS] Headers type:', typeof headers);
        console.log('[JS] Headers contains Message-ID in modal:', headers.includes('Message-ID'));
        
        // HTML escape the headers to prevent Message-ID from being interpreted as HTML tags
        const escapedHeaders = headers
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');
        
        console.log('[JS] Escaped headers:');
        console.log(escapedHeaders);
        
        const modalHtml = `
            <div class="modal-overlay" id="headersModal">
                <div class="modal-content">
                    <div class="modal-header">
                        <h3>Email Headers Preview</h3>
                        <button class="modal-close" onclick="document.getElementById('headersModal').remove()">&times;</button>
                    </div>
                    <div class="modal-body">
                        <div class="headers-preview">
                            <pre>${escapedHeaders}</pre>
                        </div>
                    </div>
                    <div class="modal-footer">
                        <button class="btn btn-secondary" onclick="document.getElementById('headersModal').remove()">Close</button>
                    </div>
                </div>
            </div>
        `;
        
        console.log('[JS] Modal HTML being inserted:');
        console.log(modalHtml);
        
        // Remove any existing modal
        const existingModal = document.getElementById('headersModal');
        if (existingModal) {
            existingModal.remove();
        }
        
        // Add modal to page
        document.body.insertAdjacentHTML('beforeend', modalHtml);
        
        // After insertion, check what's actually in the pre tag
        setTimeout(() => {
            const preElement = document.querySelector('#headersModal pre');
            if (preElement) {
                console.log('[JS] Content in pre element after insertion:');
                console.log(preElement.textContent);
                console.log('[JS] Pre element contains Message-ID:', preElement.textContent.includes('Message-ID'));
            }
        }, 100);
    }

    async sendEncryptedEmail(emailConfig, contact, subject, body, messageId) {
        console.log('[JS] sendEncryptedEmail called for contact:', contact.name);
        try {
            const keypair = appState.getKeypair();
            const activeRelays = appState.getActiveRelays();
            const encryptBtn = domManager.get('encryptBtn');
            
            // Check if user wants to send a matching DM
            const shouldSendDm = domManager.isChecked('sendMatchingDm');
            
            // Only send DM if checkbox is checked and encrypt button is in encrypted state
            if (shouldSendDm && encryptBtn && encryptBtn.dataset.encrypted === 'true') {
                console.log('[JS] Sending matching DM to contact:', contact.name);
                // Use the encrypted subject for the DM to match the email subject blob
                // Since we know the subject is already encrypted, we'll pass it as encrypted content
                const dmResult = await TauriService.sendEncryptedDirectMessage(
                    keypair.private_key,
                    contact.pubkey,
                    subject, // This is already encrypted content
                    activeRelays
                );
                console.log('[JS] DM sent successfully, event ID:', dmResult);
            } else if (shouldSendDm) {
                console.warn('[JS] Encrypt button not in encrypted state, DM will NOT be sent for security reasons.');
                notificationService.showInfo('No DM sent: Email is not encrypted.');
            } else {
                console.log('[JS] Send matching DM checkbox is unchecked, skipping DM.');
            }
            
            // Send encrypted email with the encrypted subject and body
            await TauriService.sendEmail(emailConfig, contact.email, subject, body, null, messageId);
            console.log('[JS] Encrypted email sent successfully');
        } catch (error) {
            console.error('[JS] Error sending encrypted email:', error);
            throw new Error(`Failed to send encrypted email: ${error}`);
        }
    }

    // Load emails
    async loadEmails(searchQuery = '') {
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
            return;
        }
        try {
            domManager.disable('refreshInbox');
            domManager.setHTML('refreshInbox', '<span class="loading"></span> Loading...');
            const settings = appState.getSettings();
            const keypair = appState.getKeypair();
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port,
                use_tls: settings.use_tls,
                private_key: keypair ? keypair.private_key : null
            };
            // Read filter dropdown
            const filterDropdown = document.getElementById('email-filter-dropdown');
            const onlyNostr = !filterDropdown || filterDropdown.value === 'nostr';
            // Always pass the user's email address for filtering (only as recipient)
            const userEmail = settings.email_address ? settings.email_address : null;
            console.log('[JS] getDbEmails userEmail:', userEmail);
            let emails;
            if (onlyNostr) {
                emails = await TauriService.getDbEmails(50, 0, true, userEmail);
            } else {
                emails = await TauriService.getDbEmails(50, 0, false, userEmail);
            }
            appState.setEmails(emails);
            this.renderEmails();
        } catch (error) {
            console.error('Failed to load emails:', error);
            notificationService.showError('Failed to load emails: ' + error);
        } finally {
            domManager.enable('refreshInbox');
            domManager.setHTML('refreshInbox', '<i class="fas fa-sync"></i> Refresh');
        }
    }

    // Load sent emails
    async loadSentEmails() {
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
            return;
        }
        try {
            domManager.disable('refreshSent');
            domManager.setHTML('refreshSent', '<span class="loading"></span> Loading...');
            const settings = appState.getSettings();
            const keypair = appState.getKeypair();
            const userEmail = settings.email_address ? settings.email_address : null;
            // Fetch sent emails (where user is sender)
            let emails = await TauriService.getDbSentEmails(50, 0, userEmail);
            appState.setSentEmails(emails);
            this.renderSentEmails();
        } catch (error) {
            console.error('Failed to load sent emails:', error);
            notificationService.showError('Failed to load sent emails: ' + error);
        } finally {
            domManager.enable('refreshSent');
            domManager.setHTML('refreshSent', '<i class="fas fa-sync"></i> Refresh');
        }
    }

    // Sync and reload emails
    async syncAndReloadEmails() {
        try {
            // Sync emails from network
            const newCount = await TauriService.syncNostrEmails();
            console.log(`[JS] Synced ${newCount} new emails from network`);
            
            // Reload emails from database
            await this.loadEmails();
        } catch (error) {
            console.error('[JS] Error in syncAndReloadEmails:', error);
            throw error;
        }
    }

    async syncInboxEmails() {
        try {
            const newCount = await TauriService.syncNostrEmails();
            console.log(`[JS] Synced ${newCount} new inbox emails from network`);
            return newCount;
        } catch (error) {
            console.error('[JS] Error in syncInboxEmails:', error);
            throw error;
        }
    }

    async syncSentEmails() {
        try {
            const newCount = await TauriService.syncSentEmails();
            console.log(`[JS] Synced ${newCount} new sent emails from network`);
            return newCount;
        } catch (error) {
            console.error('[JS] Error in syncSentEmails:', error);
            throw error;
        }
    }

    async syncAllEmails() {
        try {
            const [inboxCount, sentCount] = await TauriService.syncAllEmails();
            console.log(`[JS] Synced ${inboxCount} inbox emails and ${sentCount} sent emails from network`);
            return { inboxCount, sentCount };
        } catch (error) {
            console.error('[JS] Error in syncAllEmails:', error);
            throw error;
        }
    }

    // Fallback decryption method for Nostr messages
    async decryptNostrMessageWithFallback(email, encryptedContent, keypair) {
        console.log('[JS] Fallback decryption called for email:', email.from || email.from_address);
        // 1. Try the pubkey from the header/field first
        if (email.nostr_pubkey) {
            try {
                const decrypted = await TauriService.decryptDmContent(keypair.private_key, email.nostr_pubkey, encryptedContent);
                if (decrypted && !decrypted.startsWith('Unable to decrypt')) {
                    return decrypted;
                }
            } catch (e) {
                // continue to fallback
            }
        }

        // 2. Fallback: search DB for pubkeys matching sender email
        const senderEmail = email.from || email.from_address;
        if (!senderEmail) return "Unable to decrypt: sender address not found";

        let pubkeys = [];
        try {
            console.log('[JS] Calling db_find_pubkeys_by_email with:', senderEmail);
            pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email', { email: senderEmail });
        } catch (e) {
            return "Unable to decrypt: error searching contacts";
        }

        if (!pubkeys || pubkeys.length === 0) {
            return "Unable to decrypt: sender pubkey not found";
        }

        // 3. Try each pubkey
        for (const pubkey of pubkeys) {
            try {
                const decrypted = await TauriService.decryptDmContent(keypair.private_key, pubkey, encryptedContent);
                if (decrypted && !decrypted.startsWith('Unable to decrypt')) {
                    // Update the email's nostr_pubkey in the DB for future use
                    try {
                        await window.__TAURI__.core.invoke('db_update_email_nostr_pubkey_by_id', {
                            id: Number(email.id),
                            nostrPubkey: pubkey
                        });
                        email.nostr_pubkey = pubkey; // <-- Add this line
                    } catch (err) {
                        console.warn('Failed to update nostr_pubkey in DB:', err);
                    }
                    return decrypted;
                }
            } catch (e) {
                // try next
            }
        }

        return "Unable to decrypt: tried all candidate pubkeys";
    }

    // Decrypt Nostr message for sent emails (using recipient pubkey from contacts DB)
    async decryptNostrSentMessageWithFallback(email, encryptedContent, keypair) {
        // Always look up the recipient's pubkey using the to address
        const recipientEmail = email.to || email.to_address;
        if (!recipientEmail) return "Unable to decrypt: recipient address not found";
        let pubkeys = [];
        try {
            pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email', { email: recipientEmail });
        } catch (e) {
            return "Unable to decrypt: error searching contacts";
        }
        if (!pubkeys || pubkeys.length === 0) {
            return "Unable to decrypt: recipient pubkey not found";
        }
        for (const pubkey of pubkeys) {
            try {
                const decrypted = await TauriService.decryptDmContent(keypair.private_key, pubkey, encryptedContent);
                if (decrypted && !decrypted.startsWith('Unable to decrypt')) {
                    return decrypted;
                }
            } catch (e) {
                // try next
            }
        }
        return "Unable to decrypt: tried all candidate pubkeys";
    }

    async renderEmails() {
        const emailList = domManager.get('emailList');
        if (!emailList) return;
        try {
            emailList.innerHTML = '';
            const emails = appState.getEmails();
            if (emails.length === 0) {
                emailList.innerHTML = '<div class="text-center text-muted">No emails found</div>';
                return;
            }
            for (const email of emails) {
                console.log('Inbox preview nostr_pubkey for email', email.id, ':', email.nostr_pubkey);
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
                // Determine preview text
                let previewText = 'Could not decrypt';
                let showSubject = false;
                let previewSubject = email.subject;

                // Detect any NOSTR NIP-X ENCRYPTED MESSAGE
                const armorRegex = /-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/;
                if (email.body && armorRegex.test(email.body)) {
                    const keypair = appState.getKeypair();
                    if (!keypair) {
                        previewText = 'Unable to decrypt: no keypair';
                    } else {
                        // Decrypt subject if it looks encrypted
                        if (Utils.isLikelyEncryptedContent(email.subject)) {
                            try {
                                previewSubject = await this.decryptNostrMessageWithFallback(email, email.subject, keypair);
                            } catch (e) {
                                previewSubject = 'Could not decrypt';
                            }
                        }
                        // Decrypt body as before
                        const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/);
                        if (encryptedBodyMatch) {
                            const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                            try {
                                const decrypted = await this.decryptNostrMessageWithFallback(email, encryptedContent, keypair);
                                previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                showSubject = true;
                            } catch (e) {
                                previewText = 'Could not decrypt';
                            }
                        }
                    }
                } else {
                    previewText = Utils.escapeHtml(email.body ? email.body.substring(0, 100) : '');
                    if (email.body && email.body.length > 100) previewText += '...';
                    showSubject = true;
                }

                emailElement.innerHTML = `
                    <div class="email-header">
                        <div class="email-sender email-list-strong">${Utils.escapeHtml(email.from)}</div>
                        <div class="email-date">${dateDisplay}</div>
                    </div>
                    ${showSubject ? `<div class="email-subject email-list-strong">${Utils.escapeHtml(previewSubject)}</div>` : ''}
                    <div class="email-preview">${previewText}</div>
                `;
                emailElement.addEventListener('click', () => this.showEmailDetail(email.id));
                emailList.appendChild(emailElement);
            }
        } catch (error) {
            console.error('Error rendering emails:', error);
        }
    }

    // Filter emails with debouncing
    filterEmails() {
        const searchQuery = domManager.getValue('emailSearch')?.trim() || '';
        // Clear existing timeout
        if (this.searchTimeout) {
            clearTimeout(this.searchTimeout);
        }
        // Set a new timeout to debounce the search
        this.searchTimeout = setTimeout(async () => {
            try {
                await this.loadEmails(searchQuery);
            } catch (error) {
                console.error('Error filtering emails:', error);
            }
        }, 1500); // 1.5 second delay
    }

    // Show email detail
    showEmailDetail(emailId) {
        try {
            const email = appState.getEmails().find(e => e.id === emailId);
            if (!email) return;
            const emailList = domManager.get('emailList');
            const emailDetailView = document.getElementById('email-detail-view');
            const inboxActions = document.getElementById('inbox-actions');
            const inboxTitle = document.getElementById('inbox-title');
            if (emailList) emailList.style.display = 'none';
            if (emailDetailView) emailDetailView.style.display = 'flex';
            if (inboxActions) inboxActions.style.display = 'none';
            if (inboxTitle) inboxTitle.textContent = 'Email Detail';
            const emailDetailContent = document.getElementById('email-detail-content');
            if (emailDetailContent) {
                const cleanedBody = email.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('ENCRYPTED MESSAGE')).join('\n').trim();
                const nostrPubkey = email.nostr_pubkey;
                console.log('Nostr pubkey for email:', nostrPubkey);
                const isEncryptedSubject = Utils.isLikelyEncryptedContent(email.subject);
                const encryptedBodyMatch = cleanedBody.match(/-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/);
                let decryptedSubject = email.subject;
                let decryptedBody = cleanedBody;
                let originalSubject = email.subject;
                let decryptionAttempted = false;
                const keypair = appState.getKeypair();
                if ((nostrPubkey || (isEncryptedSubject || encryptedBodyMatch)) && keypair) {
                    (async () => {
                        try {
                            if (isEncryptedSubject) {
                                decryptedSubject = await this.decryptNostrMessageWithFallback(email, email.subject, keypair);
                            }
                            if (encryptedBodyMatch) {
                                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                                decryptedBody = await this.decryptNostrMessageWithFallback(email, encryptedContent, keypair);
                            }
                            updateDetail(decryptedSubject, decryptedBody);
                        } catch (err) {
                            updateDetail('Could not decrypt', 'Could not decrypt');
                        }
                    })();
                } else {
                    updateDetail(decryptedSubject, decryptedBody);
                }
                function updateDetail(subject, body) {
                    emailDetailContent.innerHTML =
                        `<div class="email-detail">
<div class="email-detail-header vertical" id="email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
<div class="email-header-row"><span class="email-header-label">Subject:</span> <span class="email-header-value">${Utils.escapeHtml(subject)}</span></div>
</div>
<pre id="raw-header-info" style="display:none; background:#222b3a; color:#fff; padding:10px; border-radius:6px; margin-bottom:10px; max-height:300px; overflow:auto;">${Utils.escapeHtml(email.raw_headers || '')}</pre>
<div class="email-detail-body" id="email-body-info">${Utils.escapeHtml(body).replace(/\n/g, '<br>')}</div>
<button id="toggle-raw-btn" class="btn btn-secondary" style="margin: 18px 0 0 0;">Show Raw Content</button>
<pre id="raw-body-info" style="display:none; background:#222b3a; color:#fff; padding:10px; border-radius:6px; margin-top:10px; max-height:400px; overflow:auto; white-space:pre-wrap;">${Utils.escapeHtml(email.raw_body)}</pre>
</div>`;
                    const toggleRawBtn = document.getElementById('toggle-raw-btn');
                    const headerInfo = document.getElementById('email-header-info');
                    const rawHeaderInfo = document.getElementById('raw-header-info');
                    const bodyInfo = document.getElementById('email-body-info');
                    const rawBodyInfo = document.getElementById('raw-body-info');
                    let showingRaw = false;
                    toggleRawBtn.addEventListener('click', () => {
                        showingRaw = !showingRaw;
                        if (showingRaw) {
                            headerInfo.classList.add('hidden-header');
                            rawHeaderInfo.style.display = 'block';
                            bodyInfo.style.display = 'none';
                            rawBodyInfo.style.display = 'block';
                            toggleRawBtn.textContent = 'Show Display Content';
                        } else {
                            headerInfo.classList.remove('hidden-header');
                            rawHeaderInfo.style.display = 'none';
                            bodyInfo.style.display = 'block';
                            rawBodyInfo.style.display = 'none';
                            toggleRawBtn.textContent = 'Show Raw Content';
                        }
                    });
                }
            }
        } catch (error) {
            console.error('Error showing email detail:', error);
        }
    }

    // Show email list
    showEmailList() {
        try {
            // Show the email list and hide the detail view
            const emailList = domManager.get('emailList');
            const emailDetailView = document.getElementById('email-detail-view');
            const inboxActions = document.getElementById('inbox-actions');
            const inboxTitle = document.getElementById('inbox-title');
            
            if (emailList) emailList.style.display = 'block';
            if (emailDetailView) emailDetailView.style.display = 'none';
            if (inboxActions) inboxActions.style.display = 'flex';
            if (inboxTitle) inboxTitle.textContent = 'Inbox';
            
        } catch (error) {
            console.error('Error showing email list:', error);
        }
    }

    async saveDraft() {
        console.log('[JS] saveDraft called');
        
        const toAddress = domManager.getValue('toAddress');
        const subject = domManager.getValue('subject');
        const body = domManager.getValue('messageBody');
        
        console.log('[JS] Form values:', { 
            toAddress: toAddress, 
            subject: subject, 
            body: body,
            toAddressType: typeof toAddress,
            subjectType: typeof subject,
            bodyType: typeof body,
            toAddressLength: toAddress ? toAddress.length : 0,
            subjectLength: subject ? subject.length : 0,
            bodyLength: body ? body.length : 0,
            currentDraftId: this.currentDraftId
        });
        
        // Also check if the elements exist
        const toAddressElement = document.getElementById('to-address');
        const subjectElement = document.getElementById('subject');
        const bodyElement = document.getElementById('message-body');
        
        console.log('[JS] Elements found:', {
            toAddressElement: !!toAddressElement,
            subjectElement: !!subjectElement,
            bodyElement: !!bodyElement
        });
        
        if (toAddressElement) {
            console.log('[JS] to-address element value:', toAddressElement.value);
        }
        if (subjectElement) {
            console.log('[JS] subject element value:', subjectElement.value);
        }
        if (bodyElement) {
            console.log('[JS] message-body element value:', bodyElement.value);
        }
        
        if (!toAddress || !subject || !body) {
            console.log('Draft not saved: missing required fields');
            notificationService.showError('Please fill in all required fields (To, Subject, and Message)');
            return;
        }

        // Check if we have email settings
        if (!appState.hasSettings()) {
            console.log('Draft not saved: no email settings');
            notificationService.showError('Please configure your email settings first');
            return;
        }

        const settings = appState.getSettings();
        if (!settings.email_address) {
            console.log('Draft not saved: no email address in settings');
            notificationService.showError('Please configure your email address in settings');
            return;
        }

        // Use existing draft ID if we're editing a draft, otherwise generate a new one
        const draftId = this.currentDraftId || this.generateUUID();
        
        const draft = {
            id: this.currentDraftDbId, // Use the database ID if editing an existing draft
            message_id: draftId,
            from_address: settings.email_address,
            to_address: toAddress,
            subject: subject,
            body: body,
            body_plain: null,
            body_html: null,
            received_at: new Date().toISOString(),
            is_nostr_encrypted: this.selectedNostrContact ? true : false,
            nostr_pubkey: this.selectedNostrContact ? this.selectedNostrContact.pubkey : null,
            raw_headers: null,
            is_draft: true,
            is_read: false,
            updated_at: null,
            created_at: new Date().toISOString()
        };

        console.log('[JS] Draft object:', draft);

        try {
            const result = await TauriService.saveDraft(draft);
            console.log('Draft saved to database:', result);
            
            // Clear form after successful save
            domManager.setValue('toAddress', '');
            domManager.setValue('subject', '');
            domManager.setValue('messageBody', '');
            this.selectedNostrContact = null;
            
            // Clear the current draft ID since we've saved it
            const wasUpdating = this.currentDraftId !== null;
            this.currentDraftId = null;
            this.currentDraftDbId = null;
            
            // Update UI to show draft saved
            const message = wasUpdating ? 'Draft updated successfully!' : 'Draft saved successfully!';
            notificationService.showSuccess(message);
        } catch (error) {
            console.error('Error saving draft:', error);
            notificationService.showError('Failed to save draft: ' + error);
        }
    }

    async loadDrafts() {
        try {
            const userEmail = appState.getSettings().email_address;
            const drafts = await TauriService.getDrafts(userEmail);
            console.log('Loaded drafts:', drafts);
            return drafts;
        } catch (error) {
            console.error('Error loading drafts:', error);
            return [];
        }
    }

    async deleteDraft(messageId) {
        try {
            await TauriService.deleteDraft(messageId);
            console.log('Draft deleted:', messageId);
            return true;
        } catch (error) {
            console.error('Error deleting draft:', error);
            return false;
        }
    }

    async markAsRead(messageId) {
        try {
            await TauriService.markAsRead(messageId);
            console.log('Email marked as read:', messageId);
            return true;
        } catch (error) {
            console.error('Error marking as read:', error);
            return false;
        }
    }

    // Test email connection
    async testEmailConnection() {
        if (!appState.hasSettings()) {
            notificationService.showError('Please save your settings first');
            return;
        }
        
        // Validate that required settings are present
        const settings = appState.getSettings();
        if (!settings.email_address || !settings.email_address.trim()) {
            notificationService.showError('Email address is required. Please fill in your email address.');
            return;
        }
        
        if (!settings.password || !settings.password.trim()) {
            notificationService.showError('Password is required. Please fill in your email password.');
            return;
        }
        
        if (!settings.smtp_host || !settings.smtp_host.trim()) {
            notificationService.showError('SMTP host is required. Please fill in the SMTP host field.');
            return;
        }
        
        if (!settings.imap_host || !settings.imap_host.trim()) {
            notificationService.showError('IMAP host is required. Please fill in the IMAP host field.');
            return;
        }
        
        try {
            domManager.disable('testEmailConnectionBtn');
            domManager.setHTML('testEmailConnectionBtn', '<span class="loading"></span> Testing...');
            
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port,
                use_tls: settings.use_tls,
                private_key: appState.getKeypair() ? appState.getKeypair().private_key : null
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
                TauriService.testImapConnection(emailConfig),
                TauriService.testSmtpConnection(emailConfig)
            ]);
            
            const imapResult = results[0];
            const smtpResult = results[1];
            
            // Check results and provide comprehensive feedback
            if (imapResult.status === 'fulfilled' && smtpResult.status === 'fulfilled') {
                notificationService.showSuccess('✅ Email connection test successful!\n\n• IMAP: Connected and authenticated\n• SMTP: Connected and authenticated\n\nYour email settings are working correctly.');
            } else if (imapResult.status === 'fulfilled' && smtpResult.status === 'rejected') {
                notificationService.showError(`⚠️ Partial success:\n\n✅ IMAP: Connected and authenticated\n❌ SMTP: ${smtpResult.reason}\n\nYou can receive emails but may have issues sending them.`);
            } else if (imapResult.status === 'rejected' && smtpResult.status === 'fulfilled') {
                notificationService.showError(`⚠️ Partial success:\n\n❌ IMAP: ${imapResult.reason}\n✅ SMTP: Connected and authenticated\n\nYou can send emails but may have issues receiving them.`);
            } else {
                const imapError = imapResult.status === 'rejected' ? imapResult.reason : 'Unknown error';
                const smtpError = smtpResult.status === 'rejected' ? smtpResult.reason : 'Unknown error';
                notificationService.showError(`❌ Email connection test failed:\n\n• IMAP: ${imapError}\n• SMTP: ${smtpError}\n\nPlease check your email settings and try again.`);
            }
            
        } catch (error) {
            console.error('Email connection test failed:', error);
            notificationService.showError('Email connection test failed: ' + error);
        } finally {
            domManager.enable('testEmailConnectionBtn');
            domManager.setHTML('testEmailConnectionBtn', '<i class="fas fa-envelope"></i> Test Email Connection');
        }
    }

    // Handle email provider selection
    handleEmailProviderChange() {
        const provider = domManager.getValue('emailProvider') || '';
        
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
            domManager.setValue('smtpHost', settings.smtp_host);
            domManager.setValue('smtpPort', settings.smtp_port);
            domManager.setValue('imapHost', settings.imap_host);
            domManager.setValue('imapPort', settings.imap_port);
            domManager.get('useTls').checked = settings.use_tls;
            
            // Show a helpful message
            let message = `${provider.charAt(0).toUpperCase() + provider.slice(1)} settings applied.`;
            
            if (provider === 'gmail') {
                message += ' For Gmail, you must use an App Password instead of your regular password. Go to your Google Account settings > Security > 2-Step Verification > App passwords to generate one.';
            }
            
            // Add TLS info
            if (settings.use_tls) {
                message += ' TLS has been automatically enabled (required for secure connections).';
            }
            
            notificationService.showSuccess(message);
        }
    }

    // Encrypt subject and body fields using NIP-04
    async encryptEmailFields() {
        console.log('[JS] encryptEmailFields called');
        console.log('[JS] Selected Nostr contact:', this.selectedNostrContact);
        console.log('[JS] Available contacts:', appState.getContacts());
        console.log('[JS] Current dropdown value:', domManager.getValue('nostrContactSelect'));
        
        if (!this.selectedNostrContact) {
            console.log('[JS] No Nostr contact selected');
            console.log('[JS] Trying to restore saved contact selection...');
            
            // First try to restore from saved state
            if (this.restoreContactSelection()) {
                console.log('[JS] Successfully restored contact selection');
            } else {
                console.log('[JS] No saved contact selection, trying dropdown...');
                // Try to re-select the contact from the dropdown
                const select = domManager.get('nostrContactSelect');
                const selectedValue = select.value;
                if (selectedValue && selectedValue !== '') {
                    const contacts = appState.getContacts();
                    this.selectedNostrContact = contacts.find(contact => contact.pubkey === selectedValue);
                    console.log('[JS] Re-selected contact from dropdown:', this.selectedNostrContact);
                }
            }
            
            if (!this.selectedNostrContact) {
                notificationService.showError('Select a Nostr contact to encrypt for');
                return false;
            }
        }
        if (!appState.hasKeypair()) {
            console.log('[JS] No keypair available');
            notificationService.showError('No Nostr keypair available for encryption');
            return false;
        }
        const subject = domManager.getValue('subject') || '';
        const body = domManager.getValue('messageBody') || '';
        // Store plaintext versions for later use
        this.plaintextSubject = subject;
        this.plaintextBody = body;
        console.log('[JS] Subject:', subject);
        console.log('[JS] Body:', body);
        if (!body && !subject) {
            console.log('[JS] Nothing to encrypt');
            notificationService.showError('Nothing to encrypt');
            return false;
        }
        const privkey = appState.getKeypair().private_key;
        const pubkey = this.selectedNostrContact.pubkey;
        console.log('[JS] Using privkey:', privkey.substring(0, 20) + '...');
        console.log('[JS] Using pubkey:', pubkey);
        
        // Get the selected encryption algorithm
        const settings = appState.getSettings();
        const encryptionAlgorithm = settings?.encryption_algorithm || 'nip44';
        console.log('[JS] Using encryption algorithm:', encryptionAlgorithm);
        
        const encryptBtn = domManager.get('encryptBtn');
        let iconSpan, labelSpan;
        if (encryptBtn) {
            iconSpan = encryptBtn.querySelector('.encrypt-btn-icon i');
            labelSpan = encryptBtn.querySelector('.encrypt-btn-label');
            encryptBtn.disabled = true;
            // No loading spinner or text, since encryption is instant
        }
        try {
            // Encrypt subject
            let encryptedSubject = subject;
            if (subject) {
                console.log('[JS] Encrypting subject...');
                encryptedSubject = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, subject, encryptionAlgorithm);
                console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
                domManager.setValue('subject', encryptedSubject.trim());
            }
            // Encrypt body
            let encryptedBody = body;
            if (body) {
                console.log('[JS] Encrypting body...');
                encryptedBody = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, body, encryptionAlgorithm);
                console.log('[JS] Body encrypted:', encryptedBody.substring(0, 50) + '...');
                // Add ASCII armor around the encrypted body, matching the algorithm
                const armorType = encryptionAlgorithm === 'nip04' ? 'NIP-04' : 'NIP-44';
                const armoredBody = [
                    `-----BEGIN NOSTR ${armorType} ENCRYPTED MESSAGE-----`,
                    encryptedBody.trim(),
                    `-----END NOSTR ${armorType} ENCRYPTED MESSAGE-----`
                ].join('\n');
                domManager.setValue('messageBody', armoredBody.trim());
            }
            notificationService.showSuccess(`Subject and body encrypted using ${encryptionAlgorithm.toUpperCase()}`);
            return true;
        } catch (error) {
            console.error('[JS] Encryption error:', error);
            notificationService.showError('Failed to encrypt: ' + error);
            return false;
        } finally {
            if (encryptBtn) {
                encryptBtn.disabled = false;
                // Do not reset icon/label here; let the toggle handler manage Encrypt/Decrypt state
            }
        }
    }

    // Generate a UUID for message IDs
    generateUUID() {
        return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
            const r = Math.random() * 16 | 0;
            const v = c === 'x' ? r : (r & 0x3 | 0x8);
            return v.toString(16);
        });
    }

    // Generate a unique message ID
    generateMessageId() {
        return `<${this.generateUUID()}@nostr-mail>`;
    }

    // Find related emails by message ID
    async findRelatedEmails(messageId) {
        try {
            console.log('[JS] Finding related emails for message ID:', messageId);
            const relatedEmails = await TauriService.findEmailsByMessageId(messageId);
            console.log('[JS] Found related emails:', relatedEmails.length);
            return relatedEmails;
        } catch (error) {
            console.error('[JS] Error finding related emails:', error);
            notificationService.showError('Failed to find related emails: ' + error);
            return [];
        }
    }

    // Render sent emails
    async renderSentEmails() {
        const sentList = domManager.get('sentList');
        if (!sentList) return;
        try {
            sentList.innerHTML = '';
            const emails = appState.getSentEmails();
            if (!emails || emails.length === 0) {
                sentList.innerHTML = '<div class="text-center text-muted">No sent emails found</div>';
                return;
            }
            for (const email of emails) {
                const emailElement = document.createElement('div');
                emailElement.className = 'email-item';
                emailElement.dataset.emailId = email.id;
                // Format the date
                const emailDate = new Date(email.date);
                const now = new Date();
                const diffInHours = (now - emailDate) / (1000 * 60 * 60);
                let dateDisplay;
                if (diffInHours < 24) {
                    dateDisplay = emailDate.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
                } else if (diffInHours < 168) {
                    dateDisplay = emailDate.toLocaleDateString([], { weekday: 'short' });
                } else {
                    dateDisplay = emailDate.toLocaleDateString();
                }
                let previewText = email.body ? Utils.escapeHtml(email.body.substring(0, 100)) : '';
                let showSubject = true;
                let previewSubject = email.subject;
                
                // Detect any NOSTR NIP-X ENCRYPTED MESSAGE (same as inbox)
                const armorRegex = /-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/;
                if (email.body && armorRegex.test(email.body)) {
                    const keypair = appState.getKeypair();
                    if (!keypair) {
                        previewText = 'Unable to decrypt: no keypair';
                    } else {
                        // Decrypt subject if it looks encrypted (ASCII armor not required)
                        if (Utils.isLikelyEncryptedContent(email.subject)) {
                            try {
                                previewSubject = await this.decryptNostrSentMessageWithFallback(email, email.subject, keypair);
                            } catch (e) {
                                previewSubject = 'Could not decrypt';
                            }
                        }
                        // Decrypt body using generic NIP-X regex (same as inbox)
                        const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/);
                        if (encryptedBodyMatch) {
                            const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                            try {
                                const decrypted = await this.decryptNostrSentMessageWithFallback(email, encryptedContent, keypair);
                                previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                showSubject = true;
                            } catch (e) {
                                previewText = 'Could not decrypt';
                            }
                        }
                    }
                } else {
                    // If subject looks like encrypted base64, try to decrypt (no ASCII armor required)
                    if (Utils.isLikelyEncryptedContent(email.subject)) {
                        try {
                            previewSubject = await this.decryptNostrSentMessageWithFallback(email, email.subject, keypair);
                        } catch (e) {
                            previewSubject = 'Could not decrypt';
                        }
                    }
                    previewText = Utils.escapeHtml(email.body ? email.body.substring(0, 100) : '');
                    if (email.body && email.body.length > 100) previewText += '...';
                    showSubject = true;
                }
                emailElement.innerHTML = `
                    <div class="email-header">
                        <div class="email-sender email-list-strong">To: ${Utils.escapeHtml(email.to)}</div>
                        <div class="email-date">${dateDisplay}</div>
                    </div>
                    ${showSubject ? `<div class="email-subject email-list-strong">${Utils.escapeHtml(previewSubject)}</div>` : ''}
                    <div class="email-preview">${previewText}</div>
                `;
                emailElement.addEventListener('click', () => this.showSentDetail(email.id));
                sentList.appendChild(emailElement);
            }
        } catch (error) {
            console.error('Error rendering sent emails:', error);
        }
    }

    // Show sent email detail
    showSentDetail(emailId) {
        try {
            const email = appState.getSentEmails().find(e => e.id === emailId);
            if (!email) return;
            const sentList = domManager.get('sentList');
            const sentDetailView = domManager.get('sentDetailView');
            const sentActions = domManager.get('sentActions');
            const sentTitle = domManager.get('sentTitle');
            if (sentList) sentList.style.display = 'none';
            if (sentDetailView) sentDetailView.style.display = 'flex';
            if (sentActions) sentActions.style.display = 'none';
            if (sentTitle) sentTitle.textContent = 'Sent Email Detail';
            const sentDetailContent = domManager.get('sentDetailContent');
            if (sentDetailContent) {
                const cleanedBody = email.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('ENCRYPTED MESSAGE')).join('\n').trim();
                const nostrPubkey = email.nostr_pubkey;
                const isEncryptedSubject = Utils.isLikelyEncryptedContent(email.subject);
                // Use generic NIP-X regex (same as inbox)
                const encryptedBodyMatch = cleanedBody.match(/-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/);
                let decryptedSubject = email.subject;
                let decryptedBody = cleanedBody;
                const keypair = appState.getKeypair();
                if ((nostrPubkey || (isEncryptedSubject || encryptedBodyMatch)) && keypair) {
                    (async () => {
                        try {
                            // Decrypt subject if it looks encrypted (ASCII armor not required)
                            if (isEncryptedSubject) {
                                decryptedSubject = await this.decryptNostrSentMessageWithFallback(email, email.subject, keypair);
                            }
                            if (encryptedBodyMatch) {
                                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                                decryptedBody = await this.decryptNostrSentMessageWithFallback(email, encryptedContent, keypair);
                            }
                            updateDetail(decryptedSubject, decryptedBody);
                        } catch (err) {
                            updateDetail('Could not decrypt', 'Could not decrypt');
                        }
                    })();
                } else {
                    updateDetail(decryptedSubject, decryptedBody);
                }
                function updateDetail(subject, body) {
                    sentDetailContent.innerHTML =
                        `<div class="email-detail">
<div class="email-detail-header vertical" id="email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
<div class="email-header-row"><span class="email-header-label">Subject:</span> <span class="email-header-value">${Utils.escapeHtml(subject)}</span></div>
</div>
<pre id="raw-header-info" style="display:none; background:#222b3a; color:#fff; padding:10px; border-radius:6px; margin-bottom:10px; max-height:300px; overflow:auto;">${Utils.escapeHtml(email.raw_headers || '')}</pre>
<div class="email-detail-body" id="email-body-info">${Utils.escapeHtml(body).replace(/\n/g, '<br>')}</div>
<button id="sent-toggle-raw-btn" class="btn btn-secondary" style="margin: 18px 0 0 0;">Show Raw Content</button>
<pre id="raw-body-info" style="display:none; background:#222b3a; color:#fff; padding:10px; border-radius:6px; margin-top:10px; max-height:400px; overflow:auto; white-space:pre-wrap;">${Utils.escapeHtml(email.raw_body)}</pre>
</div>`;
                    const toggleRawBtn = document.getElementById('sent-toggle-raw-btn');
                    const headerInfo = document.getElementById('email-header-info');
                    const rawHeaderInfo = document.getElementById('raw-header-info');
                    const bodyInfo = document.getElementById('email-body-info');
                    const rawBodyInfo = document.getElementById('raw-body-info');
                    let showingRaw = false;
                    toggleRawBtn.addEventListener('click', () => {
                        showingRaw = !showingRaw;
                        if (showingRaw) {
                            headerInfo.classList.add('hidden-header');
                            rawHeaderInfo.style.display = 'block';
                            bodyInfo.style.display = 'none';
                            rawBodyInfo.style.display = 'block';
                            toggleRawBtn.textContent = 'Show Display Content';
                            rawBodyInfo.parentNode.insertBefore(toggleRawBtn, rawBodyInfo.nextSibling);
                        } else {
                            headerInfo.classList.remove('hidden-header');
                            rawHeaderInfo.style.display = 'none';
                            bodyInfo.style.display = 'block';
                            rawBodyInfo.style.display = 'none';
                            toggleRawBtn.textContent = 'Show Raw Content';
                            bodyInfo.parentNode.insertBefore(toggleRawBtn, bodyInfo.nextSibling);
                        }
                    });
                }
            }
        } catch (error) {
            console.error('Error showing sent email detail:', error);
        }
    }

    // Show sent email list
    showSentList() {
        try {
            const sentList = domManager.get('sentList');
            const sentDetailView = domManager.get('sentDetailView');
            const sentActions = domManager.get('sentActions');
            const sentTitle = domManager.get('sentTitle');
            if (sentList) sentList.style.display = 'block';
            if (sentDetailView) sentDetailView.style.display = 'none';
            if (sentActions) sentActions.style.display = 'flex';
            if (sentTitle) sentTitle.textContent = 'Sent';
        } catch (error) {
            console.error('Error showing sent email list:', error);
        }
    }

    // Load drafts
    async loadDrafts() {
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
            return;
        }
        try {
            domManager.disable('refreshDrafts');
            domManager.setHTML('refreshDrafts', '<span class="loading"></span> Loading...');
            const settings = appState.getSettings();
            const userEmail = settings.email_address ? settings.email_address : null;
            console.log('[JS] loadDrafts userEmail:', userEmail);
            
            const drafts = await TauriService.getDrafts(userEmail);
            appState.setDrafts(drafts);
            this.renderDrafts();
        } catch (error) {
            console.error('Failed to load drafts:', error);
            notificationService.showError('Failed to load drafts: ' + error);
        } finally {
            domManager.enable('refreshDrafts');
            domManager.setHTML('refreshDrafts', '<i class="fas fa-sync"></i> Refresh');
        }
    }

    // Render drafts
    async renderDrafts() {
        const draftsList = domManager.get('draftsList');
        if (!draftsList) return;
        try {
            draftsList.innerHTML = '';
            const drafts = appState.getDrafts();
            if (!drafts || drafts.length === 0) {
                draftsList.innerHTML = '<div class="text-center text-muted">No drafts found</div>';
                return;
            }
            for (const draft of drafts) {
                const draftElement = document.createElement('div');
                draftElement.className = 'email-item';
                draftElement.dataset.draftId = draft.message_id;
                
                // Format the date
                const draftDate = new Date(draft.created_at);
                const now = new Date();
                const diffInHours = (now - draftDate) / (1000 * 60 * 60);
                let dateDisplay;
                if (diffInHours < 24) {
                    dateDisplay = draftDate.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
                } else if (diffInHours < 168) {
                    dateDisplay = draftDate.toLocaleDateString([], { weekday: 'short' });
                } else {
                    dateDisplay = draftDate.toLocaleDateString();
                }
                
                let previewText = draft.body ? Utils.escapeHtml(draft.body.substring(0, 100)) : '';
                let previewSubject = draft.subject;
                
                // Handle encrypted content
                if (draft.body && draft.body.includes('BEGIN NOSTR NIP-')) {
                    const keypair = appState.getKeypair();
                    if (!keypair) {
                        previewText = 'Unable to decrypt: no keypair';
                    } else {
                        // Try to decrypt subject and body
                        try {
                            if (Utils.isLikelyEncryptedContent(draft.subject)) {
                                previewSubject = await this.decryptNostrMessageWithFallback(draft, draft.subject, keypair);
                            }
                            const encryptedBodyMatch = draft.body.match(/-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/);
                            if (encryptedBodyMatch) {
                                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                                const decrypted = await this.decryptNostrMessageWithFallback(draft, encryptedContent, keypair);
                                previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                            }
                        } catch (e) {
                            previewText = 'Could not decrypt';
                        }
                    }
                } else {
                    if (draft.body && draft.body.length > 100) previewText += '...';
                }
                
                draftElement.innerHTML = `
                    <div class="email-header">
                        <div class="email-sender email-list-strong">To: ${Utils.escapeHtml(draft.to_address)}</div>
                        <div class="email-date">${dateDisplay}</div>
                    </div>
                    <div class="email-subject email-list-strong">${Utils.escapeHtml(previewSubject)}</div>
                    <div class="email-preview">${previewText}</div>
                    <div class="draft-actions" style="margin-top: 8px;">
                        <button class="btn btn-primary btn-small" onclick="event.stopPropagation(); emailService.loadDraftToCompose(${JSON.stringify(draft).replace(/"/g, '&quot;')})">
                            <i class="fas fa-edit"></i> Edit
                        </button>
                        <button class="btn btn-danger btn-small" onclick="event.stopPropagation(); emailService.deleteDraftFromList('${draft.message_id}')">
                            <i class="fas fa-trash"></i> Delete
                        </button>
                    </div>
                `;
                
                // Add click handler to load draft into compose form
                draftElement.addEventListener('click', () => this.loadDraftToCompose(draft));
                draftsList.appendChild(draftElement);
            }
        } catch (error) {
            console.error('Error rendering drafts:', error);
        }
    }

    // Load draft into compose form
    async loadDraftToCompose(draft) {
        try {
            console.log('[JS] Loading draft to compose:', draft);
            
            // Set the current draft ID so we know we're editing an existing draft
            this.currentDraftId = draft.message_id;
            this.currentDraftDbId = draft.id; // Store the database ID
            
            // Switch to compose tab
            const composeTab = document.querySelector('[data-tab="compose"]');
            if (composeTab) {
                composeTab.click();
            }
            
            // Wait a moment for the tab to switch
            setTimeout(() => {
                // Fill in the form fields
                domManager.setValue('toAddress', draft.to_address || '');
                domManager.setValue('subject', draft.subject || '');
                domManager.setValue('messageBody', draft.body || '');
                
                // If this was an encrypted draft, restore the Nostr contact selection
                if (draft.is_nostr_encrypted && draft.nostr_pubkey) {
                    const contacts = appState.getContacts();
                    const contact = contacts.find(c => c.pubkey === draft.nostr_pubkey);
                    if (contact) {
                        this.selectedNostrContact = contact;
                        // Update the Nostr contact dropdown
                        const dropdown = domManager.get('nostrContactSelect');
                        if (dropdown) {
                            dropdown.value = contact.pubkey;
                        }
                        // Update the UI to show it's an encrypted email
                        const toAddressInput = domManager.get('toAddress');
                        if (toAddressInput) {
                            toAddressInput.style.borderColor = '#667eea';
                            toAddressInput.style.backgroundColor = '#f8f9ff';
                            toAddressInput.classList.add('hidden');
                        }
                    }
                }
                
                notificationService.showSuccess('Draft loaded successfully!');
            }, 100);
            
        } catch (error) {
            console.error('Error loading draft to compose:', error);
            notificationService.showError('Failed to load draft: ' + error);
        }
    }

    // Delete draft from the drafts list
    async deleteDraftFromList(messageId) {
        try {
            if (confirm('Are you sure you want to delete this draft?')) {
                await this.deleteDraft(messageId);
                notificationService.showSuccess('Draft deleted successfully!');
                
                // If we deleted the draft that was currently being edited, clear the compose form
                if (this.currentDraftId === messageId) {
                    this.clearCurrentDraft();
                    // Clear the compose form
                    domManager.setValue('toAddress', '');
                    domManager.setValue('subject', '');
                    domManager.setValue('messageBody', '');
                    domManager.setValue('nostrContactSelect', '');
                    this.selectedNostrContact = null;
                    
                    // Reset the UI for Nostr contact selection
                    const toAddressInput = domManager.get('toAddress');
                    if (toAddressInput) {
                        toAddressInput.style.borderColor = '';
                        toAddressInput.style.backgroundColor = '';
                        toAddressInput.classList.remove('hidden');
                    }
                    const helperText = document.getElementById('to-helper-text');
                    if (helperText) helperText.classList.remove('hidden');
                }
                
                // Reload the drafts list to reflect the deletion
                await this.loadDrafts();
            }
        } catch (error) {
            console.error('Error deleting draft:', error);
            notificationService.showError('Failed to delete draft: ' + error);
        }
    }

    // Show draft detail
    showDraftDetail(draftId) {
        try {
            const draft = appState.getDrafts().find(d => d.message_id === draftId);
            if (!draft) return;
            
            const draftsList = domManager.get('draftsList');
            const draftsDetailView = domManager.get('draftsDetailView');
            const draftsActions = domManager.get('draftsActions');
            const draftsTitle = domManager.get('draftsTitle');
            
            if (draftsList) draftsList.style.display = 'none';
            if (draftsDetailView) draftsDetailView.style.display = 'flex';
            if (draftsActions) draftsActions.style.display = 'none';
            if (draftsTitle) draftsTitle.textContent = 'Draft Detail';
            
            const draftsDetailContent = domManager.get('draftsDetailContent');
            if (draftsDetailContent) {
                const cleanedBody = draft.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('ENCRYPTED MESSAGE')).join('\n').trim();
                const nostrPubkey = draft.nostr_pubkey;
                const isEncryptedSubject = Utils.isLikelyEncryptedContent(draft.subject);
                const encryptedBodyMatch = cleanedBody.match(/-----BEGIN NOSTR NIP-\d+ ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n?]+)\s*-----END NOSTR NIP-\d+ ENCRYPTED MESSAGE-----/);
                let decryptedSubject = draft.subject;
                let decryptedBody = cleanedBody;
                const keypair = appState.getKeypair();
                
                if ((nostrPubkey || (isEncryptedSubject || encryptedBodyMatch)) && keypair) {
                    (async () => {
                        try {
                            if (isEncryptedSubject) {
                                decryptedSubject = await this.decryptNostrMessageWithFallback(draft, draft.subject, keypair);
                            }
                            if (encryptedBodyMatch) {
                                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                                decryptedBody = await this.decryptNostrMessageWithFallback(draft, encryptedContent, keypair);
                            }
                            updateDetail(decryptedSubject, decryptedBody);
                        } catch (err) {
                            updateDetail('Could not decrypt', 'Could not decrypt');
                        }
                    })();
                } else {
                    updateDetail(decryptedSubject, decryptedBody);
                }
                
                function updateDetail(subject, body) {
                    draftsDetailContent.innerHTML = `
                        <div class="email-detail">
                            <div class="email-detail-header vertical" id="draft-header-info">
                                <div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(draft.from_address)}</span></div>
                                <div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(draft.to_address)}</span></div>
                                <div class="email-header-row"><span class="email-header-label">Created:</span> <span class="email-header-value">${new Date(draft.created_at).toLocaleString()}</span></div>
                                <div class="email-header-row"><span class="email-header-label">Subject:</span> <span class="email-header-value">${Utils.escapeHtml(subject)}</span></div>
                            </div>
                            <div class="email-detail-body" id="draft-body-info">${Utils.escapeHtml(body).replace(/\n/g, '<br>')}</div>
                            <div class="draft-actions" style="margin-top: 20px;">
                                <button id="edit-draft-btn" class="btn btn-primary" style="margin-right: 10px;">
                                    <i class="fas fa-edit"></i> Edit Draft
                                </button>
                                <button id="delete-draft-btn" class="btn btn-danger">
                                    <i class="fas fa-trash"></i> Delete Draft
                                </button>
                            </div>
                        </div>`;
                    
                    // Add event listeners for edit and delete buttons
                    const editBtn = document.getElementById('edit-draft-btn');
                    const deleteBtn = document.getElementById('delete-draft-btn');
                    
                    if (editBtn) {
                        editBtn.addEventListener('click', () => this.loadDraftToCompose(draft));
                    }
                    
                    if (deleteBtn) {
                        deleteBtn.addEventListener('click', async () => {
                            if (confirm('Are you sure you want to delete this draft?')) {
                                try {
                                    await this.deleteDraft(draft.message_id);
                                    notificationService.showSuccess('Draft deleted successfully!');
                                    
                                    // If we deleted the draft that was currently being edited, clear the compose form
                                    if (this.currentDraftId === draft.message_id) {
                                        this.clearCurrentDraft();
                                        // Clear the compose form
                                        domManager.setValue('toAddress', '');
                                        domManager.setValue('subject', '');
                                        domManager.setValue('messageBody', '');
                                        domManager.setValue('nostrContactSelect', '');
                                        this.selectedNostrContact = null;
                                        
                                        // Reset the UI for Nostr contact selection
                                        const toAddressInput = domManager.get('toAddress');
                                        if (toAddressInput) {
                                            toAddressInput.style.borderColor = '';
                                            toAddressInput.style.backgroundColor = '';
                                            toAddressInput.classList.remove('hidden');
                                        }
                                        const helperText = document.getElementById('to-helper-text');
                                        if (helperText) helperText.classList.remove('hidden');
                                    }
                                    
                                    this.showDraftsList();
                                    await this.loadDrafts();
                                } catch (error) {
                                    notificationService.showError('Failed to delete draft: ' + error);
                                }
                            }
                        });
                    }
                }
            }
        } catch (error) {
            console.error('Error showing draft detail:', error);
        }
    }

    // Show drafts list
    showDraftsList() {
        try {
            const draftsList = domManager.get('draftsList');
            const draftsDetailView = domManager.get('draftsDetailView');
            const draftsActions = domManager.get('draftsActions');
            const draftsTitle = domManager.get('draftsTitle');
            if (draftsList) draftsList.style.display = 'block';
            if (draftsDetailView) draftsDetailView.style.display = 'none';
            if (draftsActions) draftsActions.style.display = 'flex';
            if (draftsTitle) draftsTitle.textContent = 'Drafts';
        } catch (error) {
            console.error('Error showing drafts list:', error);
        }
    }

    // Save contact selection state
    saveContactSelection() {
        if (this.selectedNostrContact) {
            localStorage.setItem('nostrMailSelectedContact', JSON.stringify({
                pubkey: this.selectedNostrContact.pubkey,
                name: this.selectedNostrContact.name,
                email: this.selectedNostrContact.email
            }));
            console.log('[JS] Saved contact selection:', this.selectedNostrContact.name);
        }
    }
    
    // Restore contact selection state
    restoreContactSelection() {
        try {
            const savedContact = localStorage.getItem('nostrMailSelectedContact');
            if (savedContact) {
                const contactData = JSON.parse(savedContact);
                const contacts = appState.getContacts();
                const contact = contacts.find(c => c.pubkey === contactData.pubkey);
                if (contact) {
                    this.selectedNostrContact = contact;
                    // Update the dropdown to reflect the restored selection
                    const dropdown = domManager.get('nostrContactSelect');
                    if (dropdown) {
                        dropdown.value = contact.pubkey;
                    }
                    console.log('[JS] Restored contact selection:', contact.name);
                    return true;
                }
            }
        } catch (error) {
            console.error('[JS] Error restoring contact selection:', error);
        }
        return false;
    }
    
    // Clear saved contact selection
    clearSavedContactSelection() {
        localStorage.removeItem('nostrMailSelectedContact');
        console.log('[JS] Cleared saved contact selection');
    }
}

// Create and export a singleton instance
window.EmailService = EmailService;
window.emailService = new EmailService();