// Email Service
// Handles all email-related functionality including sending, fetching, and management

import { appState } from './app-state.js';
import { domManager } from './dom-manager.js';
import { TauriService } from './tauri-service.js';
import { notificationService } from './notification-service.js';
import { Utils } from './utils.js';

export class EmailService {
    constructor() {
        this.searchTimeout = null;
        this.selectedNostrContact = null;
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
        const dropdown = domManager.get('nostrContactSelect');
        if (!dropdown) return;

        const selectedPubkey = dropdown.value;
        const selectedOption = dropdown.options[dropdown.selectedIndex];
        const toAddressInput = domManager.get('toAddress');
        const helperText = document.getElementById('to-helper-text');

        if (selectedPubkey) {
            // Get contact details
            const contact = appState.getContacts().find(c => c.pubkey === selectedPubkey);
            if (contact && contact.email) {
                this.selectedNostrContact = contact;
                // Auto-fill the email address
                domManager.setValue('toAddress', contact.email);
                // Add visual indicator that this will be an encrypted email
                if (toAddressInput) {
                    toAddressInput.style.borderColor = '#667eea';
                    toAddressInput.style.backgroundColor = '#f8f9ff';
                    toAddressInput.classList.add('hidden'); // Hide the input
                }
                if (helperText) helperText.classList.add('hidden'); // Hide helper text
                console.log(`[JS] Selected Nostr contact: ${contact.name} (${contact.email})`);
                notificationService.showSuccess(`Selected ${contact.name} for encrypted email`);
            }
        } else {
            // Clear selection
            this.selectedNostrContact = null;
            // Reset email input styling
            if (toAddressInput) {
                toAddressInput.style.borderColor = '';
                toAddressInput.style.backgroundColor = '';
                toAddressInput.classList.remove('hidden'); // Show the input
            }
            if (helperText) helperText.classList.remove('hidden'); // Show helper text
        }
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
                await this.sendEncryptedEmail(emailConfig, this.selectedNostrContact, subject, body);
            } else {
                // Send regular email
                console.log('[JS] Sending regular email');
                await TauriService.sendEmail(emailConfig, toAddress, subject, body);
            }
            
            console.log('[JS] Email sent successfully');
            
            // Clear form
            domManager.clear('toAddress');
            domManager.clear('subject');
            domManager.clear('messageBody');
            domManager.setValue('nostrContactSelect', '');
            this.selectedNostrContact = null;
            
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

    // Send encrypted email using NIP-04
    async sendEncryptedEmail(emailConfig, contact, subject, body) {
        console.log('[JS] sendEncryptedEmail called for contact:', contact.name);
        try {
            const keypair = appState.getKeypair();
            const activeRelays = appState.getActiveRelays();
            // Send DM with encrypted subject
            const dmResult = await TauriService.sendDirectMessage(
                keypair.private_key,
                contact.pubkey,
                subject, // The subject becomes the DM content
                activeRelays
            );
            console.log('[JS] DM sent successfully, event ID:', dmResult);
            // Send encrypted email with the same subject and include npub header
            await TauriService.sendEmail(emailConfig, contact.email, subject, body, keypair.public_key);
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
            // Pass the search query and filter to the backend
            const searchParam = searchQuery.trim() || null;
            let emails;
            if (onlyNostr) {
                emails = await TauriService.fetchNostrEmailsLast24h(emailConfig);
            } else {
                emails = await TauriService.fetchEmails(emailConfig, 10, searchParam, onlyNostr);
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

    // Render emails
    renderEmails() {
        const emailList = domManager.get('emailList');
        if (!emailList) return;
        
        try {
            emailList.innerHTML = '';
            
            const emails = appState.getEmails();
            if (emails.length === 0) {
                emailList.innerHTML = '<div class="text-center text-muted">No emails found</div>';
                return;
            }
            
            emails.forEach(email => {
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
                        <div class="email-sender">${Utils.escapeHtml(email.from)}</div>
                        <div class="email-date">${dateDisplay}</div>
                    </div>
                    <div class="email-subject">${Utils.escapeHtml(email.subject)}</div>
                    <div class="email-preview">${Utils.escapeHtml(email.body.substring(0, 100))}${email.body.length > 100 ? '...' : ''}</div>
                `;
                
                emailElement.addEventListener('click', () => this.showEmailDetail(email.id));
                emailList.appendChild(emailElement);
            });
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
            
            // Hide the email list and show the detail view
            const emailList = domManager.get('emailList');
            const emailDetailView = document.getElementById('email-detail-view');
            const inboxActions = document.getElementById('inbox-actions');
            const inboxTitle = document.getElementById('inbox-title');
            
            if (emailList) emailList.style.display = 'none';
            if (emailDetailView) emailDetailView.style.display = 'flex';
            if (inboxActions) inboxActions.style.display = 'none';
            if (inboxTitle) inboxTitle.textContent = 'Email Detail';
            
            // Populate the email detail content
            const emailDetailContent = document.getElementById('email-detail-content');
            if (emailDetailContent) {
                // Clean up body: remove leading/trailing blank lines and collapse multiple blank lines except for header/footer
                const cleanedBody = email.body
                    .split('\n')
                    .filter(line => line.trim() !== '' || line.includes('ENCRYPTED MESSAGE'))
                    .join('\n')
                    .trim();
                // Try to extract Nostr pubkey from headers
                const nostrPubkey = Utils.extractNostrPubkeyFromHeaders(email.raw_headers);
                // Check if subject/body are likely encrypted
                const isEncryptedSubject = Utils.isLikelyEncryptedContent(email.subject);
                const encryptedBodyMatch = cleanedBody.match(/-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----\s*([A-Za-z0-9+/=\n]+)\s*-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----/);
                // Prepare decrypted fields
                let decryptedSubject = email.subject;
                let decryptedBody = cleanedBody;
                let originalSubject = email.subject;
                let isDecrypted = false;
                // Add decrypt button if possible
                if (nostrPubkey && (isEncryptedSubject || encryptedBodyMatch)) {
                    const decryptBtn = document.createElement('button');
                    decryptBtn.textContent = 'Decrypt Nostr Message';
                    decryptBtn.className = 'btn btn-primary';
                    decryptBtn.style.marginTop = '10px';
                    let showingOriginal = false;
                    decryptBtn.addEventListener('click', async () => {
                        if (!isDecrypted) {
                            try {
                                const keypair = appState.getKeypair();
                                if (!keypair) {
                                    notificationService.showError('No Nostr keypair available for decryption');
                                    return;
                                }
                                // Decrypt subject
                                if (isEncryptedSubject) {
                                    decryptedSubject = await TauriService.decryptDmContent(keypair.private_key, nostrPubkey, email.subject);
                                }
                                // Decrypt body
                                if (encryptedBodyMatch) {
                                    const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                                    decryptedBody = await TauriService.decryptDmContent(keypair.private_key, nostrPubkey, encryptedContent);
                                }
                                isDecrypted = true;
                                // Update UI to show decrypted
                                updateDetail(decryptedSubject, decryptedBody);
                                decryptBtn.textContent = 'Show Original Encrypted';
                                notificationService.showSuccess('Decryption successful!');
                            } catch (err) {
                                notificationService.showError('Failed to decrypt: ' + err);
                            }
                        } else {
                            showingOriginal = !showingOriginal;
                            if (showingOriginal) {
                                updateDetail(originalSubject, cleanedBody);
                                decryptBtn.textContent = 'Show Decrypted';
                            } else {
                                updateDetail(decryptedSubject, decryptedBody);
                                decryptBtn.textContent = 'Show Original Encrypted';
                            }
                        }
                    });
                    emailDetailContent.appendChild(decryptBtn);
                }
                // Helper to update the detail UI
                function updateDetail(subject, body) {
                    emailDetailContent.innerHTML = `
                        <div class="email-detail">
                            <button id="toggle-raw-btn" class="btn btn-secondary" style="margin-bottom: 10px;">Show Raw Content</button>
                            <div class="email-detail-header vertical" id="email-header-info">
                                <div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
                                <div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
                                <div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
                                <div class="email-header-row"><span class="email-header-label">Subject:</span> <span class="email-header-value">${Utils.escapeHtml(subject)}</span></div>
                            </div>
                            <pre id="raw-header-info" style="display:none; background:#222b3a; color:#fff; padding:10px; border-radius:6px; margin-bottom:10px; max-height:300px; overflow:auto;">${Utils.escapeHtml(email.raw_headers || '')}</pre>
                            <div class="email-detail-body" id="email-body-info">
                                ${Utils.escapeHtml(body).replace(/\n/g, '<br>')}
                            </div>
                            <pre id="raw-body-info" style="display:none; background:#222b3a; color:#fff; padding:10px; border-radius:6px; margin-top:10px; max-height:400px; overflow:auto; white-space:pre-wrap;">${Utils.escapeHtml(email.raw_body)}</pre>
                        </div>
                    `;
                    // Toggle between display content and raw content
                    const toggleRawBtn = document.getElementById('toggle-raw-btn');
                    const headerInfo = document.getElementById('email-header-info');
                    const rawHeaderInfo = document.getElementById('raw-header-info');
                    const bodyInfo = document.getElementById('email-body-info');
                    const rawBodyInfo = document.getElementById('raw-body-info');
                    let showingRaw = false;
                    toggleRawBtn.addEventListener('click', () => {
                        showingRaw = !showingRaw;
                        if (showingRaw) {
                            headerInfo.style.display = 'none';
                            rawHeaderInfo.style.display = 'block';
                            bodyInfo.style.display = 'none';
                            rawBodyInfo.style.display = 'block';
                            toggleRawBtn.textContent = 'Show Display Content';
                        } else {
                            headerInfo.style.display = 'flex';
                            rawHeaderInfo.style.display = 'none';
                            bodyInfo.style.display = 'block';
                            rawBodyInfo.style.display = 'none';
                            toggleRawBtn.textContent = 'Show Raw Content';
                        }
                    });
                }
                // Initial render (decrypted if possible, otherwise cleaned)
                updateDetail(originalSubject, cleanedBody);
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

    // Save draft
    saveDraft() {
        try {
            const draft = {
                to: domManager.getValue('toAddress') || '',
                subject: domManager.getValue('subject') || '',
                body: domManager.getValue('messageBody') || '',
                timestamp: Date.now()
            };
            
            localStorage.setItem('email_draft', JSON.stringify(draft));
            notificationService.showSuccess('Draft saved');
        } catch (error) {
            console.error('Error saving draft:', error);
            notificationService.showError('Failed to save draft');
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
        
        if (!this.selectedNostrContact) {
            console.log('[JS] No Nostr contact selected');
            notificationService.showError('Select a Nostr contact to encrypt for');
            return;
        }
        if (!appState.hasKeypair()) {
            console.log('[JS] No keypair available');
            notificationService.showError('No Nostr keypair available for encryption');
            return;
        }
        const subject = domManager.getValue('subject') || '';
        const body = domManager.getValue('messageBody') || '';
        console.log('[JS] Subject:', subject);
        console.log('[JS] Body:', body);
        if (!subject && !body) {
            console.log('[JS] Nothing to encrypt');
            notificationService.showError('Nothing to encrypt');
            return;
        }
        const privkey = appState.getKeypair().private_key;
        const pubkey = this.selectedNostrContact.pubkey;
        console.log('[JS] Using privkey:', privkey.substring(0, 20) + '...');
        console.log('[JS] Using pubkey:', pubkey);
        try {
            domManager.disable('encryptBtn');
            domManager.setHTML('encryptBtn', '<span class="loading"></span> Encrypting...');
            // Encrypt subject
            console.log('[JS] Encrypting subject...');
            const encryptedSubject = await TauriService.encryptNip04Message(privkey, pubkey, subject);
            console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
            // Encrypt body
            console.log('[JS] Encrypting body...');
            const encryptedBody = await TauriService.encryptNip04Message(privkey, pubkey, body);
            console.log('[JS] Body encrypted:', encryptedBody.substring(0, 50) + '...');
            
            // Add ASCII armor around the encrypted body, matching Gmail's rendering
            const armoredBody = [
                "-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----",
                encryptedBody.trim(),
                "-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----"
            ].join('\n');
            domManager.setValue('subject', encryptedSubject);
            domManager.setValue('messageBody', armoredBody.trim());
            notificationService.showSuccess('Subject and body encrypted');
        } catch (error) {
            console.error('[JS] Encryption error:', error);
            notificationService.showError('Failed to encrypt: ' + error);
        } finally {
            domManager.enable('encryptBtn');
            domManager.setHTML('encryptBtn', '<i class="fas fa-lock"></i> Encrypt');
        }
    }
}

// Create and export a singleton instance
export const emailService = new EmailService();

// Make it available globally for other services to access
window.emailService = emailService; 