// Tauri command parameter naming:
// Tauri automatically converts camelCase keys in JS to snake_case for Rust command parameters.
// For example, passing { userEmail: ... } from JS will be received as user_email in Rust.
// You can use camelCase in JS and it will map to the expected snake_case Rust parameter.
// See: https://tauri.app/v1/guides/features/command/#naming-conventions
// Email Service
// Handles all email-related functionality including sending, fetching, and management

class EmailService {
    // ── Armor format constants and helpers ──

    // Regex patterns for armor block detection (matches both new and legacy formats)
    // New format: BEGIN NOSTR NIP-XX ENCRYPTED BODY, BEGIN NOSTR SIGNED BODY
    // Legacy: BEGIN NOSTR NIP-XX ENCRYPTED MESSAGE, BEGIN NOSTR SIGNED MESSAGE
    static ARMOR_BEGIN_ENCRYPTED = /(-{3,})\s*BEGIN NOSTR NIP-\d+ ENCRYPTED (?:MESSAGE|BODY)\s*\1/;
    static ARMOR_BEGIN_ANY = /(-{3,})\s*BEGIN NOSTR (?:SIGNED (?:MESSAGE|BODY)|NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*\1/;

    constructor() {
        this.searchTimeout = null;
        this.searchUnlisten = null;
        this.searchResults = [];
        this.searchInProgress = false;
        this.sentSearchTimeout = null;
        this.sentSearchUnlisten = null;
        this.sentSearchResults = [];
        this.sentSearchInProgress = false;
        this.selectedNostrContact = null;
        this.plaintextSubject = ''; // Store plaintext subject
        this.plaintextBody = ''; // Store plaintext body
        this._subjectCiphertext = null; // Raw NIP ciphertext for DM sending
        this.currentDraftId = null; // Track the current draft being edited
        this.currentDraftDbId = null; // Track the database ID of the current draft
        this.currentMessageId = null; // Store the UUID for reuse
        this.attachments = []; // Store attachment objects with encryption state
        this.inboxOffset = 0; // Track pagination offset for inbox emails
        this.inboxPageSize = 50; // Number of emails per page (will be updated from settings)
        this.sentOffset = 0; // Track pagination offset for sent emails
        this.draftsOffset = 0; // Track pagination offset for drafts
        this.searchOffset = 0; // Track pagination offset for search results
        this.sentSearchOffset = 0; // Track pagination offset for sent search results
        this.searchHasMore = false; // Track if there are more search results
        this.sentSearchHasMore = false; // Track if there are more sent search results
        this._htmlBody = null; // HTML alternative body for multipart emails
        this._plainBody = null; // BIP39-armored plaintext body for text/plain MIME part
        this._quotedOriginalArmor = null; // Original message armor to append as quote in replies
        this._replyToMessageId = null; // Message-ID of email being replied to (In-Reply-To header)
        this._replyReferences = null; // References chain for email threading
        this._previewCache = new Map(); // Cache decrypted preview data by email ID to avoid re-decrypting on re-render
    }

    // Build indexed contact maps for O(1) lookups during list rendering.
    // Call once per render pass; the maps are stored on `this` for use by per-item methods.
    _buildContactIndex() {
        const contacts = appState.getContacts();
        this._contactsByPubkey = new Map();
        this._contactsByEmail = new Map();
        for (const c of contacts) {
            if (c.pubkey) {
                this._contactsByPubkey.set(c.pubkey, c);
            }
            if (c.email) {
                const lower = c.email.trim().toLowerCase();
                this._contactsByEmail.set(lower, c);
                // Also index the Gmail-normalized form
                if (lower.includes('@gmail.com')) {
                    const [local, domain] = lower.split('@');
                    const normalized = `${local.split('+')[0]}@${domain}`;
                    if (normalized !== lower) {
                        this._contactsByEmail.set(normalized, c);
                    }
                }
            }
        }
    }

    // Look up a contact by pubkey, then fall back to email address.
    _findContact(pubkey, emailAddr) {
        let contact = pubkey ? this._contactsByPubkey.get(pubkey) || null : null;
        if (!contact && emailAddr) {
            const lower = emailAddr.trim().toLowerCase();
            contact = this._contactsByEmail.get(lower) || null;
            if (!contact && lower.includes('@gmail.com')) {
                const [local, domain] = lower.split('@');
                const normalized = `${local.split('+')[0]}@${domain}`;
                contact = this._contactsByEmail.get(normalized) || null;
            }
        }
        return contact;
    }

    // Get appropriate background color for Nostr contact input based on dark mode
    getNostrContactInputBackgroundColor() {
        const isDarkMode = document.body.classList.contains('dark-mode');
        return isDarkMode ? '#1a1f3a' : '#f8f9ff';
    }

    /**
     * Get the recipient pubkey from the input field, falling back to selectedNostrContact.
     * Returns null if neither is set.
     */
    getRecipientPubkey() {
        const input = document.getElementById('recipient-pubkey-value');
        const val = input ? input.value.trim() : '';
        if (val) return val;
        return this.selectedNostrContact ? this.selectedNostrContact.pubkey : null;
    }

    // Populate Nostr contact dropdown with contacts that have email addresses
    populateNostrContactDropdown() {
        const dropdown = domManager.get('nostrContactSelect');
        if (!dropdown) return;

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
            // Just show the username since email and pubkey are displayed separately
            option.value = contact.pubkey;
            option.textContent = contact.name || 'Unknown';
            option.dataset.email = contact.email;
            option.dataset.name = contact.name;
            dropdown.appendChild(option);
        });

        // Reset dropdown to empty selection (don't restore previous selection)
        dropdown.value = '';

        console.log(`[JS] Populated Nostr contact dropdown with ${contactsWithEmail.length} contacts`);
    }

    // Attachment Management Methods
    
    // Initialize attachment event listeners
    initializeAttachmentListeners() {
        const addAttachmentBtn = document.getElementById('add-attachment-btn');
        const attachmentInput = document.getElementById('attachment-input');
        
        if (addAttachmentBtn) {
            addAttachmentBtn.addEventListener('click', () => {
                attachmentInput.click();
            });
        }
        
        if (attachmentInput) {
            attachmentInput.addEventListener('change', (e) => {
                this.handleAttachmentSelection(e.target.files);
            });
        }
    }
    
    // Handle file selection
    async handleAttachmentSelection(files) {
        if (!files || files.length === 0) return;
        
        for (const file of files) {
            // Check file size (limit to 10MB)
            if (file.size > 10 * 1024 * 1024) {
                window.notificationService.showError(`File "${file.name}" is too large. Maximum size is 10MB.`);
                continue;
            }
            
            const attachment = {
                id: this.generateAttachmentId(),
                file: file,
                name: file.name,
                size: file.size,
                type: file.type,
                data: null, // Will store base64 data
                encryptedData: null,
                isEncrypted: false,
                isEncrypting: false
            };
            
            // Read file data
            try {
                attachment.data = await this.readFileAsBase64(file);
                this.attachments.push(attachment);
                this.renderAttachmentList();
                console.log(`[JS] Added attachment: ${file.name} (${this.formatFileSize(file.size)})`);
            } catch (error) {
                console.error('Failed to read file:', error);
                window.notificationService.showError(`Failed to read file "${file.name}"`);
            }
        }
        
        // Clear the input
        document.getElementById('attachment-input').value = '';
    }
    
    // Generate unique attachment ID
    generateAttachmentId() {
        return 'att_' + Math.random().toString(36).substr(2, 9) + '_' + Date.now();
    }
    
    // Read file as base64
    readFileAsBase64(file) {
        return new Promise((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => {
                const base64 = reader.result.split(',')[1]; // Remove data:type;base64, prefix
                resolve(base64);
            };
            reader.onerror = reject;
            reader.readAsDataURL(file);
        });
    }
    
    // Format file size
    formatFileSize(bytes) {
        if (bytes === 0) return '0 Bytes';
        const k = 1024;
        const sizes = ['Bytes', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    }
    
    // Get file icon based on type
    getFileIcon(type) {
        if (type.startsWith('image/')) return 'fa-image';
        if (type.startsWith('video/')) return 'fa-video';
        if (type.startsWith('audio/')) return 'fa-music';
        if (type.includes('pdf')) return 'fa-file-pdf';
        if (type.includes('word') || type.includes('document')) return 'fa-file-word';
        if (type.includes('excel') || type.includes('spreadsheet')) return 'fa-file-excel';
        if (type.includes('powerpoint') || type.includes('presentation')) return 'fa-file-powerpoint';
        if (type.includes('zip') || type.includes('rar') || type.includes('tar')) return 'fa-file-archive';
        if (type.includes('text')) return 'fa-file-text';
        return 'fa-file';
    }
    
    // Render attachment list
    renderAttachmentList() {
        const attachmentList = document.getElementById('attachment-list');
        if (!attachmentList) return;
        
        if (this.attachments.length === 0) {
            attachmentList.innerHTML = '';
            return;
        }
        
        attachmentList.innerHTML = this.attachments.map(attachment => {
            const icon = this.getFileIcon(attachment.type);
            const statusClass = attachment.isEncrypted ? 'encrypted' : (attachment.isEncrypting ? 'encrypting' : '');
            
            // Show opaque filename when encrypted
            let displayName = attachment.name;
            let statusText = '📄 Plain';
            
            if (attachment.isEncrypting) {
                statusText = '🔄 Encrypting...';
            } else if (attachment.isEncrypted && attachment.encryptedData) {
                if (attachment.encryptedData.method === 'manifest_aes') {
                    displayName = `${attachment.encryptedData.opaque_id}.dat`;
                    statusText = `🔒 Manifest Encrypted`;
                } else {
                    statusText = '🔒 Hybrid Encrypted (AES+NIP)';
                }
            }
            
            return `
                <div class="attachment-item ${attachment.isEncrypted ? 'encrypted' : ''}" data-attachment-id="${attachment.id}">
                    <div class="attachment-info">
                        <i class="fas ${icon} attachment-icon"></i>
                        <div class="attachment-details">
                            <div class="attachment-name">${displayName}</div>
                            <div class="attachment-size">${this.formatFileSize(attachment.size)}</div>
                            <div class="attachment-status ${statusClass}">${statusText}</div>
                        </div>
                    </div>
                    <div class="attachment-actions">
                        <button type="button" class="btn btn-danger" onclick="window.emailService.removeAttachment('${attachment.id}')">
                            <i class="fas fa-trash"></i>
                        </button>
                    </div>
                </div>
            `;
        }).join('');
    }
    
    // Encrypt individual attachment using hybrid encryption
    async encryptAttachment(attachmentId) {
        const attachment = this.attachments.find(a => a.id === attachmentId);
        if (!attachment || attachment.isEncrypted || attachment.isEncrypting) return;
        
        if (!this.selectedNostrContact) {
            window.notificationService.showError('Please select a Nostr contact first');
            return;
        }
        
        const pubkey = this.selectedNostrContact.pubkey;

        if (!appState.hasKeypair() || !pubkey) {
            window.notificationService.showError('Missing encryption keys');
            return;
        }

        try {
            attachment.isEncrypting = true;
            this.renderAttachmentList();

            console.log(`[JS] Encrypting attachment using hybrid encryption: ${attachment.name}`);

            // Get encryption algorithm from settings (for NIP-44/04 key encryption)
            const settings = appState.getSettings();
            const encryptionAlgorithm = settings?.encryption_algorithm || 'nip44';
            
            // Always use hybrid encryption for consistent security architecture
            console.log(`[JS] Using hybrid encryption for attachment: ${attachment.name} (${attachment.size} bytes)`);
            
            // Step 1: Generate a random AES-256 symmetric key
            const symmetricKey = await this.generateSymmetricKey();
            
            // Step 2: Encrypt the file data with AES-256
            const encryptedFileData = await this.encryptWithAES(attachment.data, symmetricKey);
            
            // Step 3: Create key package with metadata
            const keyPackage = JSON.stringify({
                symmetric_key: symmetricKey,
                filename: attachment.name,
                size: attachment.size,
                type: attachment.type,
                encryption_method: 'hybrid_aes256'
            });
            
            // Step 4: Encrypt the key package with NIP-44/04
            const encryptedKeyPackage = await window.TauriService.encryptMessageWithAlgorithm(
                pubkey,
                keyPackage,
                encryptionAlgorithm
            );
            
            // Step 5: Store both encrypted key and encrypted data
            attachment.encryptedData = {
                method: 'hybrid',
                encrypted_key: encryptedKeyPackage,
                encrypted_file: encryptedFileData,
                algorithm: encryptionAlgorithm
            };
            
            console.log(`[JS] Hybrid encryption complete - Key: ${encryptedKeyPackage.length} bytes, File: ${encryptedFileData.length} bytes`);
            
            attachment.isEncrypted = true;
            attachment.isEncrypting = false;
            
            this.renderAttachmentList();
            console.log(`[JS] Successfully encrypted attachment: ${attachment.name}`);
            window.notificationService.showSuccess(`Encrypted attachment: ${attachment.name}`);
            
        } catch (error) {
            console.error('Failed to encrypt attachment:', error);
            attachment.isEncrypting = false;
            this.renderAttachmentList();
            window.notificationService.showError(`Failed to encrypt attachment: ${attachment.name}`);
        }
    }
    
    // Remove attachment
    removeAttachment(attachmentId) {
        const index = this.attachments.findIndex(a => a.id === attachmentId);
        if (index !== -1) {
            const attachment = this.attachments[index];
            this.attachments.splice(index, 1);
            this.renderAttachmentList();
            console.log(`[JS] Removed attachment: ${attachment.name}`);
        }
    }
    
    // Clear all attachments
    clearAttachments() {
        this.attachments = [];
        this.renderAttachmentList();
    }
    
    // Encrypt all attachments
    async encryptAllAttachments() {
        const unencryptedAttachments = this.attachments.filter(a => !a.isEncrypted && !a.isEncrypting);
        
        if (unencryptedAttachments.length === 0) return;
        
        console.log(`[JS] Encrypting ${unencryptedAttachments.length} attachments...`);
        
        for (const attachment of unencryptedAttachments) {
            await this.encryptAttachment(attachment.id);
        }
    }
    
    // Generate a random AES-256 symmetric key
    async generateSymmetricKey() {
        const key = await window.crypto.subtle.generateKey(
            {
                name: 'AES-GCM',
                length: 256
            },
            true, // extractable
            ['encrypt', 'decrypt']
        );
        
        // Export the key as raw bytes and convert to base64
        const keyBuffer = await window.crypto.subtle.exportKey('raw', key);
        const keyArray = new Uint8Array(keyBuffer);
        // Convert Uint8Array to base64 without using spread operator (avoids stack overflow)
        // Use chunked approach for large arrays
        const CHUNK_SIZE = 8192; // Process in 8KB chunks
        let binaryString = '';
        for (let i = 0; i < keyArray.length; i += CHUNK_SIZE) {
            const chunk = keyArray.slice(i, i + CHUNK_SIZE);
            binaryString += String.fromCharCode.apply(null, chunk);
        }
        return btoa(binaryString);
    }
    
    // Calculate SHA256 hash of data
    async calculateSHA256(data) {
        const encoder = new TextEncoder();
        const dataBytes = encoder.encode(data);
        const hashBuffer = await crypto.subtle.digest('SHA-256', dataBytes);
        const hashArray = new Uint8Array(hashBuffer);
        return Array.from(hashArray).map(b => b.toString(16).padStart(2, '0')).join('');
    }

    // Encrypt data with AES-256-GCM
    async encryptWithAES(base64Data, base64Key, shouldPad = false) {
        try {
            // Convert base64 key back to CryptoKey
            const keyBytes = Uint8Array.from(atob(base64Key), c => c.charCodeAt(0));
            const cryptoKey = await window.crypto.subtle.importKey(
                'raw',
                keyBytes,
                { name: 'AES-GCM' },
                false,
                ['encrypt']
            );
            
            // Convert base64 data to bytes
            let dataBytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));
            
            // Apply padding for attachments (pad to 64 KiB multiples)
            if (shouldPad) {
                const PADDING_SIZE = 64 * 1024; // 64 KiB
                const currentSize = dataBytes.length;
                const paddedSize = Math.ceil(currentSize / PADDING_SIZE) * PADDING_SIZE;
                
                if (paddedSize > currentSize) {
                    console.log(`[JS] Padding attachment from ${currentSize} to ${paddedSize} bytes`);
                    const paddedData = new Uint8Array(paddedSize);
                    paddedData.set(dataBytes);
                    
                    // Fill padding with random bytes to avoid patterns
                    const paddingBytes = window.crypto.getRandomValues(new Uint8Array(paddedSize - currentSize));
                    paddedData.set(paddingBytes, currentSize);
                    
                    // Store original size in first 4 bytes (little endian)
                    const sizeBytes = new Uint8Array(4);
                    const dataView = new DataView(sizeBytes.buffer);
                    dataView.setUint32(0, currentSize, true); // true = little endian
                    
                    // Prepend size to data
                    const finalData = new Uint8Array(4 + paddedSize);
                    finalData.set(sizeBytes);
                    finalData.set(paddedData, 4);
                    
                    dataBytes = finalData;
                }
            }
            
            // Generate a random IV (12 bytes for GCM)
            const iv = window.crypto.getRandomValues(new Uint8Array(12));
            
            // Encrypt the data
            const encryptedBuffer = await window.crypto.subtle.encrypt(
                {
                    name: 'AES-GCM',
                    iv: iv
                },
                cryptoKey,
                dataBytes
            );
            
            // Combine IV + encrypted data
            const combined = new Uint8Array(iv.length + encryptedBuffer.byteLength);
            combined.set(iv, 0);
            combined.set(new Uint8Array(encryptedBuffer), iv.length);
            
            // Return as base64
            // Convert Uint8Array to base64 without using spread operator (avoids stack overflow for large files)
            // Use chunked approach for large arrays
            const CHUNK_SIZE = 8192; // Process in 8KB chunks
            let binaryString = '';
            for (let i = 0; i < combined.length; i += CHUNK_SIZE) {
                const chunk = combined.slice(i, i + CHUNK_SIZE);
                binaryString += String.fromCharCode.apply(null, chunk);
            }
            return btoa(binaryString);
            
        } catch (error) {
            console.error('AES encryption failed:', error);
            throw new Error('Failed to encrypt with AES: ' + error.message);
        }
    }
    
    // Decrypt data with AES-256-GCM (handles padding for attachments)
    async decryptWithAES(encryptedBase64Data, base64Key, wasPadded = false) {
        try {
            // Convert base64 key back to CryptoKey
            const keyBytes = Uint8Array.from(atob(base64Key), c => c.charCodeAt(0));
            const cryptoKey = await window.crypto.subtle.importKey(
                'raw',
                keyBytes,
                { name: 'AES-GCM' },
                false,
                ['decrypt']
            );
            
            // Convert base64 encrypted data to bytes
            const encryptedBytes = Uint8Array.from(atob(encryptedBase64Data), c => c.charCodeAt(0));
            
            // Extract IV (first 12 bytes) and encrypted data
            const iv = encryptedBytes.slice(0, 12);
            const encryptedData = encryptedBytes.slice(12);
            
            // Decrypt the data
            const decryptedBuffer = await window.crypto.subtle.decrypt(
                {
                    name: 'AES-GCM',
                    iv: iv
                },
                cryptoKey,
                encryptedData
            );
            
            let decryptedArray = new Uint8Array(decryptedBuffer);
            
            // Remove padding if it was applied
            if (wasPadded) {
                // Extract original size from first 4 bytes (little endian)
                const sizeBytes = decryptedArray.slice(0, 4);
                const dataView = new DataView(sizeBytes.buffer);
                const originalSize = dataView.getUint32(0, true); // true = little endian
                
                console.log(`[JS] Removing padding: ${decryptedArray.length - 4} bytes -> ${originalSize} bytes`);
                
                // Extract original data (skip size bytes and padding)
                decryptedArray = decryptedArray.slice(4, 4 + originalSize);
            }
            
            // Return as base64
            // Convert Uint8Array to base64 without using spread operator (avoids stack overflow for large files)
            // Use chunked approach for large arrays
            const CHUNK_SIZE = 8192; // Process in 8KB chunks
            let binaryString = '';
            for (let i = 0; i < decryptedArray.length; i += CHUNK_SIZE) {
                const chunk = decryptedArray.slice(i, i + CHUNK_SIZE);
                binaryString += String.fromCharCode.apply(null, chunk);
            }
            return btoa(binaryString);
            
        } catch (error) {
            console.error('AES decryption failed:', error);
            throw new Error('Failed to decrypt with AES: ' + error.message);
        }
    }
    
    // Decrypt all attachments
    async decryptAllAttachments() {
        const encryptedAttachments = this.attachments.filter(a => a.isEncrypted);
        
        if (encryptedAttachments.length === 0) return;
        
        console.log(`[JS] Decrypting ${encryptedAttachments.length} attachments...`);
        
        for (const attachment of encryptedAttachments) {
            await this.decryptAttachment(attachment.id);
        }
    }

    // Decrypt the body content in compose view
    async decryptBodyContent() {
        const body = domManager.getValue('messageBody');
        if (!body || !body.includes('BEGIN NOSTR')) {
            console.log('[JS] Body is not encrypted');
            return false;
        }

        if (!this.selectedNostrContact) {
            window.notificationService.showError('Please select a Nostr contact first');
            return false;
        }

        const pubkey = this.selectedNostrContact.pubkey;

        if (!appState.hasKeypair() || !pubkey) {
            window.notificationService.showError('Missing decryption keys');
            return false;
        }

        try {
            // Extract quoted armor (reply chain) to preserve after decrypt
            const encParts = await this.parseArmorComponentsRust(body);
            let trailingQuoted = '';
            if (encParts && encParts.quotedArmor) {
                trailingQuoted = '\n\n' + encParts.quotedArmor;
            }

            const result = await TauriService.decryptEmailBody(body, '', pubkey, null);

            if (result.success) {
                domManager.setValue('messageBody', result.body + trailingQuoted);
                this.clearSignature();
                return true;
            } else {
                const errorMsg = result.error || 'Failed to decrypt body content';
                window.notificationService.showError(errorMsg);
                return false;
            }
        } catch (error) {
            console.error('[JS] Failed to decrypt body:', error);
            window.notificationService.showError('Failed to decrypt body content');
            return false;
        }
    }
    
    // Decrypt individual attachment using manifest-based encryption
    async decryptAttachment(attachmentId) {
        const attachment = this.attachments.find(a => a.id === attachmentId);
        if (!attachment || !attachment.isEncrypted || !attachment.encryptedData) return;
        
        // Only support manifest-based encryption
        if (attachment.encryptedData.method !== 'manifest_aes') {
            window.notificationService.showError('Unsupported attachment encryption method');
            return;
        }
        
        try {
            console.log(`[JS] Decrypting manifest attachment: ${attachment.name}`);
            console.log('[JS] Attachment opaque_id:', attachment.encryptedData.opaque_id);
            
            // For manifest-based attachments, we need to get the manifest from the current email
            // The manifest should have been decrypted when viewing the email
            // We need to find the attachment metadata in the manifest
            
            // This is a simplified approach - in a real implementation, we'd need to:
            // 1. Get the current email being viewed
            // 2. Decrypt its manifest to get attachment metadata
            // 3. Use the AES key from the manifest to decrypt the attachment
            
            // For now, let's extract the key from the attachment's encrypted data
            // This assumes the manifest has already been processed and the key is available
            
            if (!attachment.encryptedData.aes_key) {
                window.notificationService.showError('Attachment AES key not available. Please decrypt the email first.');
                return;
            }
            
            // Verify hash if present
            if (attachment.encryptedData.cipher_sha256) {
                const actualHash = await this.calculateSHA256(attachment.encryptedData.encrypted_file);
                if (actualHash !== attachment.encryptedData.cipher_sha256) {
                    console.warn(`[JS] Attachment ${attachmentId} hash mismatch!`);
                }
            }
            
            // Decrypt the file data with AES key from manifest (with padding removal)
            const decryptedFileData = await this.decryptWithAES(
                attachment.encryptedData.encrypted_file, 
                attachment.encryptedData.aes_key,
                true // wasPadded = true for attachments
            );
            
            // Restore original attachment data
            attachment.data = decryptedFileData;
            attachment.name = attachment.encryptedData.original_filename || attachment.name;
            attachment.size = attachment.encryptedData.original_size || decryptedFileData.length;
            attachment.type = attachment.encryptedData.original_type || attachment.type;
            attachment.encryptedData = null;
            attachment.isEncrypted = false;
            
            this.renderAttachmentList();
            console.log(`[JS] Successfully decrypted attachment: ${attachment.name}`);
            window.notificationService.showSuccess(`Decrypted attachment: ${attachment.name}`);
            
        } catch (error) {
            console.error('Failed to decrypt attachment:', error);
            window.notificationService.showError(`Failed to decrypt attachment: ${attachment.name}`);
        }
    }
    
    // Prepare attachments for email sending
    prepareAttachmentsForEmail() {
        if (this.attachments.length === 0) {
            return null;
        }
        
        return this.attachments.map(attachment => {
            if (attachment.isEncrypted && attachment.encryptedData) {
                if (attachment.encryptedData.method === 'manifest_aes') {
                    // New manifest-based encryption: use opaque filename
                    return {
                        filename: attachment.encryptedData.opaque_id + '.dat',
                        content_type: 'application/octet-stream',
                        data: attachment.encryptedData.encrypted_file,
                        size: attachment.encryptedData.encrypted_file.length,
                        is_encrypted: true,
                        encryption_method: 'manifest_aes',
                        algorithm: null,
                        original_filename: attachment.name,
                        original_type: attachment.type,
                        original_size: attachment.size
                    };
                } else {
                    // Legacy hybrid encryption (backward compatibility if needed)
                    return [
                        {
                            filename: attachment.name + '.key',
                            content_type: 'application/octet-stream',
                            data: attachment.encryptedData.encrypted_key,
                            size: attachment.encryptedData.encrypted_key.length,
                            is_encrypted: true,
                            encryption_method: 'hybrid_key',
                            algorithm: attachment.encryptedData.algorithm,
                            original_filename: attachment.name,
                            original_type: attachment.type,
                            original_size: attachment.size
                        },
                        {
                            filename: attachment.name + '.data',
                            content_type: 'application/octet-stream',
                            data: attachment.encryptedData.encrypted_file,
                            size: attachment.encryptedData.encrypted_file.length,
                            is_encrypted: true,
                            encryption_method: 'hybrid_data',
                            original_filename: attachment.name,
                            original_type: attachment.type,
                            original_size: attachment.size
                        }
                    ];
                }
            } else {
                // Send plain attachment data
                return {
                    filename: attachment.name,
                    content_type: attachment.type,
                    data: attachment.data,
                    size: attachment.size,
                    is_encrypted: false,
                    encryption_method: null,
                    algorithm: null,
                    original_filename: null,
                    original_type: null,
                    original_size: null
                };
            }
        }).flat(); // Flatten array since legacy hybrid encryption creates 2 attachments per file
    }

    // Handle Nostr contact selection
    handleNostrContactSelection() {
        const select = domManager.get('nostrContactSelect');
        const selectedValue = select.value;
        const sendMatchingDmGroup = document.querySelector('.checkbox-group');
        const pubkeyDisplay = document.getElementById('selected-recipient-pubkey');
        const pubkeyValue = document.getElementById('recipient-pubkey-value');
        
        if (selectedValue && selectedValue !== '') {
            // Find the selected contact
            const contacts = appState.getContacts();
            this.selectedNostrContact = contacts.find(contact => contact.pubkey === selectedValue);
            
            if (this.selectedNostrContact) {
                console.log('[JS] Selected Nostr contact:', this.selectedNostrContact.name);
                // Auto-fill the email address
                domManager.setValue('toAddress', this.selectedNostrContact.email);
                // Fill the recipient pubkey input
                if (pubkeyValue) {
                    pubkeyValue.value = this.selectedNostrContact.pubkey;
                }
                // Style the toAddress input for Nostr encryption
                const toAddressInput = domManager.get('toAddress');
                if (toAddressInput) {
                    toAddressInput.style.borderColor = '#667eea';
                    toAddressInput.style.backgroundColor = this.getNostrContactInputBackgroundColor();
                }
                // Save the contact selection for later restoration
                this.saveContactSelection();
                // Update DM checkbox visibility based on encryption state
                this.updateDmCheckboxVisibility();
            }
        } else {
            this.selectedNostrContact = null;
            // Clear the pubkey input (but keep it visible)
            if (pubkeyValue) {
                pubkeyValue.value = '';
            }
            // Reset the toAddress input styling
            const toAddressInput = domManager.get('toAddress');
            if (toAddressInput) {
                toAddressInput.style.borderColor = '';
                toAddressInput.style.backgroundColor = '';
            }
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
        this._subjectCiphertext = null;
        this.currentMessageId = null; // Reset message ID when clearing draft
        this._quotedOriginalArmor = null; // Clear quoted reply armor
        this._replyToMessageId = null;
        this._replyReferences = null;
        this.clearAttachments(); // Clear all attachments
        // Clear saved contact selection when clearing draft
        this.clearSavedContactSelection();
        // Hide DM checkbox when clearing draft
        this.updateDmCheckboxVisibility();
        // Reset encrypt button state
        this.resetEncryptButtonState();
        // Hide pubkey display
        const pubkeyDisplay = document.getElementById('selected-recipient-pubkey');
        if (pubkeyDisplay) {
            // pubkey field is always visible
        }
    }
    
    // Clear signature when body state changes (encrypt/decrypt)
    clearSignature() {
        const signBtn = document.getElementById('sign-btn');
        if (signBtn) {
            const iconSpan = signBtn.querySelector('.sign-btn-icon i');
            const labelSpan = signBtn.querySelector('.sign-btn-label');
            
            signBtn.dataset.signed = 'false';
            delete signBtn.dataset.signature;
            
            if (iconSpan) iconSpan.className = 'fas fa-pen';
            if (labelSpan) labelSpan.textContent = 'Sign';
            signBtn.classList.remove('signed');

            console.log('[JS] Signature cleared due to body state change');
        }
        this._htmlBody = null;
        this._plainBody = null;
        // Note: _quotedOriginalArmor is NOT cleared here — it survives
        // encrypt/sign cycles and is only cleared on form reset
    }

    // Auto-sign the current DOM body for NIP-04 messages (spec section 4.1).
    // NIP-04 lacks authenticated encryption, so the Schnorr signature serves as
    // the MAC — preventing padding oracle and bit-flipping attacks.
    async _autoSignNip04Body() {
        const body = domManager.getValue('messageBody') || '';

        // Extract body text and quoted armor for backend byte extraction
        const parts = await this.parseArmorComponents(body);
        const bodyText = (parts && parts.bodyText) ? parts.bodyText : body;
        // Use quoted armor from DOM body (if already nested) or from stored reply context.
        // During reply compose, the DOM only has the outermost body; the nested quoted
        // levels are in _quotedOriginalArmor and must be included so the signature covers
        // the full conversation chain per spec section 3.5.0.
        const quotedArmor = (parts && parts.quotedArmor) ? parts.quotedArmor : this._quotedOriginalArmor;

        // Extract signable bytes via backend (handles glossia decode + nested concatenation)
        const rawBytes = await TauriService.extractSignableBytes(bodyText, true, quotedArmor, null);
        const dataBytes = new Uint8Array(rawBytes);
        if (!dataBytes || dataBytes.length === 0) {
            throw new Error('NIP-04 auto-sign failed: could not decode body bytes');
        }

        // Sign via Rust backend
        const signature = await TauriService.signDataBytes(dataBytes);
        if (!signature) {
            throw new Error('NIP-04 auto-sign failed: signing returned no signature');
        }

        // Update sign button state so _plainBody rebuild includes SIGNATURE block
        const signBtn = document.getElementById('sign-btn');
        if (signBtn) {
            signBtn.dataset.signed = 'true';
            signBtn.dataset.signature = signature;
            const iconSpan = signBtn.querySelector('.sign-btn-icon i');
            const labelSpan = signBtn.querySelector('.sign-btn-label');
            if (iconSpan) iconSpan.className = 'fas fa-check';
            if (labelSpan) labelSpan.textContent = 'Signed';
            signBtn.classList.add('signed');
        }

        console.log('[JS] NIP-04 auto-sign complete, signature:', signature.substring(0, 16) + '...');
    }

    // Inject a signature verification badge into an HTML email body.
    // Finds the signature <h4> header and inserts a badge span after the header text.
    injectHtmlSigBadge(htmlString, inlineSigResult) {
        // Accept a single result or an array of results
        const results = Array.isArray(inlineSigResult) ? inlineSigResult : [inlineSigResult];
        const validResults = results.filter(r => r != null);
        if (validResults.length === 0 || !htmlString) return htmlString;
        try {
            const parser = new DOMParser();
            const doc = parser.parseFromString(htmlString, 'text/html');
            // Find all h4 elements — the signature header is the one before the border-left div
            const h4s = doc.querySelectorAll('h4');
            let resultIdx = 0;
            for (const h4 of h4s) {
                if (resultIdx >= validResults.length) break;
                const next = h4.nextElementSibling;
                // Signature div has border-left style with italic text (not the seal blockquote)
                if (next && next.tagName === 'DIV' && next.style.borderLeft && next.style.fontStyle === 'italic') {
                    const result = validResults[resultIdx];
                    const badge = doc.createElement('span');
                    if (result.isValid) {
                        badge.style.cssText = 'display:inline-block;margin-left:8px;padding:2px 8px;border-radius:4px;font-size:0.8em;font-weight:600;background:#d4edda;color:#155724;border:1px solid #c3e6cb;';
                        badge.textContent = '\u2713 Verified';
                    } else {
                        badge.style.cssText = 'display:inline-block;margin-left:8px;padding:2px 8px;border-radius:4px;font-size:0.8em;font-weight:600;background:#f8d7da;color:#721c24;border:1px solid #f5c6cb;';
                        badge.textContent = '\u2717 Invalid';
                    }
                    badge.className = 'sig-inline-badge';
                    h4.appendChild(badge);
                    resultIdx++;
                }
            }
            return doc.documentElement.outerHTML;
        } catch (e) {
            console.warn('[JS] injectHtmlSigBadge failed:', e);
            return htmlString;
        }
    }

    // Build HTML alternative body from plaintext components
    buildHtmlAlt(bodyText, encodedSig, encodedPubkey, profileName, displayName, metaSig, metaPubkey, encodedSigPubkey, quotedHtml) {
        const escHtml = (str) => str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
        const bodyHtml = escHtml(bodyText).replace(/\n/g, '<br>');

        let sigHtml = '';
        const sigContent = encodedSigPubkey || encodedSig;
        if (sigContent) {
            const sigLabel = profileName ? escHtml(profileName) : 'Signature';
            // Include npub below the sig when using separate encoding (default mode)
            const pubkeyLine = (encodedPubkey && encodedSig && !encodedSigPubkey)
                ? `\n    <br><code style="overflow-wrap:break-word;font-size:0.85em;">${escHtml(encodedPubkey)}</code>`
                : '';
            sigHtml = `
  <hr style="border:none;border-top:1px solid #ccc;margin:1.5em 0;">
  <h4 style="margin:0 0 0.5em;color:#666;font-size:0.9em;">${sigLabel}</h4>
  <div class="sig-content" style="border-left:2px solid #ccc;padding-left:1em;color:#888;font-style:italic;overflow-wrap:break-word;">
    ${escHtml(sigContent)}${pubkeyLine}
  </div>`;
        }

        let sealHtml = '';
        // Only show separate seal block for unsigned messages (no sig at all)
        if (encodedPubkey && !encodedSigPubkey && !encodedSig) {
            const nameHtml = displayName ? `<p style="margin:0 0 0.5em;"><strong>@${escHtml(displayName)}</strong></p>` : '';
            sealHtml = `
  <blockquote class="seal-block" style="border-left:4px solid #4a90d9;background:#f0f4f8;padding:1em;margin:1.5em 0 0;border-radius:4px;">
    ${nameHtml}
    <code style="overflow-wrap:break-word;font-size:0.85em;">${escHtml(encodedPubkey)}</code>
  </blockquote>`;
        }

        const quotedBlock = quotedHtml
            ? `\n  <blockquote style="border-left:2px solid #ccc;margin:1em 0;padding:0 1em;">${quotedHtml}</blockquote>`
            : '';

        return `<div style="font-family:sans-serif;line-height:1.6;">
  <div>${bodyHtml}</div>${quotedBlock}${sigHtml}${sealHtml}
</div>`;
    }

    // Recursively build HTML for nested quoted armor at all nesting levels.
    // plaintextFallback: plaintext extracted from parent's prefixText for when glossia decode fails.
    async buildRecursiveQuotedHtml(armorText, plaintextFallback) {
        if (!armorText) return null;
        const qParts = await this.parseArmorComponents(armorText);
        if (!qParts) return null;

        // Determine plaintext fallback for this level and deeper levels.
        // prefixText contains email-quoted plaintext with > prefixes from reply nesting.
        // Strip one > level and split into this level's text vs deeper quoted text.
        let thisLevelFallback = null;
        let deeperFallback = null;
        const rawFallback = plaintextFallback != null ? plaintextFallback : qParts.prefixText;
        if (rawFallback) {
            const stripped = rawFallback.split('\n').map(l => {
                if (l.startsWith('> ')) return l.substring(2);
                if (l === '>') return '';
                return l;
            }).join('\n');
            const lines = stripped.split('\n');
            const ownLines = [];
            const deepLines = [];
            let inDeep = false;
            for (const line of lines) {
                if (line.startsWith('> ') || line === '>') {
                    inDeep = true;
                    deepLines.push(line);
                } else if (inDeep && line.trim() === '') {
                    deepLines.push(line);
                } else {
                    if (inDeep) inDeep = false;
                    ownLines.push(line);
                }
            }
            thisLevelFallback = ownLines.join('\n').trim() || null;
            deeperFallback = deepLines.length > 0 ? deepLines.join('\n').trim() : null;
        }

        // Recurse into deeper quoted content first
        let deeperHtml = null;
        if (qParts.quotedArmor) {
            deeperHtml = await this.buildRecursiveQuotedHtml(qParts.quotedArmor, deeperFallback);
        }
        // Per spec 6.2.2:
        // - Signed plaintext: decode glossia to show readable plaintext
        // - Encrypted: show glossia-encoded ciphertext as-is (can't decode to readable text)
        let decodedBody = null;
        if (qParts.bodyText) {
            if (qParts.isEncryptedBody) {
                // Encrypted: show the glossia prose directly (decoding would produce raw ciphertext binary)
                decodedBody = qParts.bodyText;
            } else {
                // Signed plaintext: decode glossia to plaintext for display
                const gs = window.GlossiaService;
                if (gs) {
                    try {
                        const bytes = await gs.transcodeToBytes(qParts.bodyText);
                        if (bytes) decodedBody = new TextDecoder().decode(bytes);
                    } catch (_) {}
                }
                if (!decodedBody) {
                    console.warn('[JS] buildRecursiveQuotedHtml: glossia decode failed for body, using plaintext fallback');
                    decodedBody = thisLevelFallback || qParts.bodyText;
                }
            }
        }
        // Only decoded body in HTML blockquote (spec 6.2.3)
        let qBodyText = decodedBody || '';
        return this.buildHtmlAlt(
            qBodyText, qParts.sigContent, qParts.sealContent,
            qParts.profileName, qParts.displayName,
            null, null, qParts.rawSigPubkey, deeperHtml
        );
    }

    // Parse an ASCII armor block into its structural components (body, signature, pubkey).
    // Works with both encrypted (NIP-XX) and signed armor formats.
    // New format: SIGNATURE block contains sig (64 bytes) then pubkey (32 bytes).
    // Legacy format: separate SIGNATURE and SEAL blocks are also accepted.
    // Separate body text from any inner "> " quoted blocks.
    // Returns { bodyOnly, quotedArmor } where quotedArmor is the stripped (one level) quoted content.
    _splitBodyAndQuoted(rawBody) {
        // New format: nested armor blocks appear inline without > prefixes.
        // Split at the first nested BEGIN NOSTR delimiter.
        const nestedStart = rawBody.search(/\n-{3,}\s*BEGIN NOSTR /);
        if (nestedStart >= 0) {
            const bodyOnly = rawBody.substring(0, nestedStart).trim();
            const quotedArmor = rawBody.substring(nestedStart + 1).trim(); // +1 to skip \n
            return { bodyOnly, quotedArmor };
        }

        // Backwards compat: detect > prefixed lines (from forwarded emails or old format)
        const lines = rawBody.split('\n');
        const bodyLines = [];
        const quotedLines = [];
        let inQuoted = false;
        for (const line of lines) {
            if (line.startsWith('> ') || line === '>') {
                inQuoted = true;
                quotedLines.push(line);
            } else if (inQuoted && line.trim() === '') {
                quotedLines.push(line);
            } else {
                inQuoted = false;
                bodyLines.push(line);
            }
        }
        const bodyOnly = bodyLines.join('\n').trim();
        const quotedArmor = quotedLines.length > 0
            ? this._stripQuotePrefixes(quotedLines.join('\n'))
            : null;
        return { bodyOnly, quotedArmor };
    }

    async parseArmorComponents(armorText) {
        if (!armorText) return null;
        const normalized = armorText.replace(/\r\n/g, '\n');

        // Extract any plaintext that appears before the first armor delimiter
        const armorStart = normalized.search(/-{3,}\s*BEGIN NOSTR /);
        const prefixText = armorStart > 0 ? normalized.substring(0, armorStart).trim() : null;
        if (armorStart < 0) return null;

        const lines = normalized.substring(armorStart).split('\n');
        const isBeginBody = (l) => /-{3,}\s*BEGIN NOSTR (?:(?:NIP-\d+ ENCRYPTED|SIGNED) (?:MESSAGE|BODY))\s*-{3,}/.test(l.trim());
        const isBeginSig = (l) => /-{3,}\s*BEGIN NOSTR SIGNATURE\s*-{3,}/.test(l.trim());
        const isBeginSeal = (l) => /-{3,}\s*BEGIN NOSTR SEAL\s*-{3,}/.test(l.trim());
        const isEnd = (l) => /-{3,}\s*END NOSTR (?:(?:NIP-\d+ ENCRYPTED )?MESSAGE|SIGNATURE|SEAL)\s*-{3,}/.test(l.trim());

        let depth = 0;
        let state = 'before'; // before | body | quoted | sig | seal | done
        let isEncryptedBody = false;
        const bodyLines = [];
        const quotedArmorLines = [];
        const sigLines = [];
        const sealLines = [];

        for (const line of lines) {
            if (state === 'before') {
                if (isBeginBody(line)) {
                    depth = 1; state = 'body';
                    isEncryptedBody = /ENCRYPTED/i.test(line);
                }
                continue;
            }
            if (state === 'body') {
                if (isBeginBody(line)) {
                    depth++; state = 'quoted'; quotedArmorLines.push(line); continue;
                }
                if (isBeginSig(line) && depth === 1) { state = 'sig'; continue; }
                if (isBeginSeal(line) && depth === 1) { state = 'seal'; continue; }
                if (isEnd(line) && depth === 1) { state = 'done'; continue; }
                bodyLines.push(line);
                continue;
            }
            if (state === 'quoted') {
                quotedArmorLines.push(line);
                if (isBeginBody(line)) { depth++; continue; }
                if (isEnd(line)) {
                    depth--;
                    if (depth === 1) { state = 'body'; }
                }
                continue;
            }
            if (state === 'sig') {
                if (isEnd(line)) { state = 'done'; continue; }
                if (isBeginSeal(line)) { state = 'seal'; continue; } // Legacy: separate SEAL after SIGNATURE
                sigLines.push(line);
                continue;
            }
            if (state === 'seal') {
                if (isEnd(line)) { state = 'done'; continue; }
                sealLines.push(line);
                continue;
            }
        }

        if (state === 'before') return null;

        const bodyText = bodyLines.join('\n').trim();
        // Strip one level of "> " prefix from quoted armor lines (backwards compat for
        // old-format messages or forwarded emails where mail clients added quote prefixes)
        const strippedQuotedLines = quotedArmorLines.map(l => {
            if (l.startsWith('> ')) return l.substring(2);
            if (l === '>') return '';
            return l;
        });
        const quotedArmor = strippedQuotedLines.length > 0 ? strippedQuotedLines.join('\n').trim() : null;

        // Parse signature content
        let sigContent = null, sealContent = null, profileName = null, displayName = null, rawSigPubkey = null;
        if (sigLines.length > 0) {
            const nameLine = sigLines.find(l => l.trim().startsWith('@'));
            if (nameLine) profileName = nameLine.trim().replace(/^@/, '');
            const contentLines = sigLines.filter(l => !l.trim().startsWith('@'));
            const allContent = contentLines.join('\n').trim();
            rawSigPubkey = allContent;
            const split = await this._splitSigPubkey(allContent);
            if (split) {
                sigContent = split.sigHex;
                sealContent = split.pubkeyHex;
            } else {
                sigContent = allContent;
            }
        }
        if (sealLines.length > 0) {
            const sealNameLine = sealLines.find(l => l.trim().startsWith('@'));
            displayName = sealNameLine ? sealNameLine.trim().replace(/^@/, '') : null;
            sealContent = sealLines.filter(l => !l.trim().startsWith('@')).join('\n').trim();
        }
        if (!displayName) displayName = profileName;

        return { bodyText, sigContent, sealContent, profileName, displayName, rawSigPubkey, prefixText, quotedArmor, isEncryptedBody };
    }

    // Backend-powered armor parser with capnp validation. Returns the same shape as
    // parseArmorComponents() for compatibility, plus extra typed fields from the Rust parser.
    // Falls back to the JS parser on error.
    async parseArmorComponentsRust(armorText) {
        if (!armorText) return null;
        try {
            const result = await TauriService.parseArmorMessage(armorText);
            if (!result) return null;
            // Map Rust ParsedArmorMessage (camelCase) to the existing JS shape
            return {
                bodyText: result.bodyText,
                sigContent: result.signatureHex || result.rawSigPubkey,
                sealContent: result.sealPubkeyHex || result.sigPubkeyHex,
                profileName: result.profileName,
                displayName: result.displayName,
                rawSigPubkey: result.rawSigPubkey,
                prefixText: result.prefixText,
                quotedArmor: result.quotedArmorText,
                isEncryptedBody: result.bodyType === 'encrypted',
                // New typed fields from capnp-validated Rust parser
                _rustParsed: true,
                _bodyType: result.bodyType,
                _encryptionNip: result.encryptionNip,
                _bodyBytesB64: result.bodyBytesB64,
                _signatureHex: result.signatureHex,
                _sigPubkeyHex: result.sigPubkeyHex,
                _quoted: result.quoted,
            };
        } catch (e) {
            console.error('[JS] parseArmorComponentsRust failed:', e);
            return null;
        }
    }

    // Split combined signature block content into sig (64 bytes) and pubkey (32 bytes).
    // Decodes the full block as 96 bytes, splits at byte boundary.
    // Returns { sigHex, pubkeyHex } with pre-decoded hex strings, or null if decode fails.
    async _splitSigPubkey(content) {
        if (!content) return null;

        // Phase 1: Combined 96-byte decode (backward compat)
        const combined = await this._tryCombined96(content);
        if (combined) return combined;

        // Phase 2: Last-line heuristic (npub or hex pubkey on last line)
        const lines = content.split('\n').map(l => l.trim()).filter(l => l);
        if (lines.length >= 2) {
            const last = lines[lines.length - 1];
            const lastStripped = last.replace(/\s+/g, '');
            if (lastStripped.startsWith('npub1') || /^[0-9a-fA-F]{64}$/.test(lastStripped)) {
                const sig = await this._tryDecodeSig(lines.slice(0, -1).join('\n'));
                const pk = await this._tryDecodePubkey(last);
                if (sig && pk) return { sigHex: sig, pubkeyHex: pk };
            }
        }

        return null;
    }

    // Phase 1 helper: try to decode as combined 96-byte sig+pubkey (glossia or hex)
    async _tryCombined96(content) {
        const gs = window.GlossiaService;
        if (gs) {
            try {
                const detections = await gs.detectDialect(content);
                const dialect = (Array.isArray(detections) && detections.length > 0) ? detections[0].language : null;
                if (dialect) {
                    const hex = await gs.decodeRawBaseN(content, dialect, 96);
                    if (hex && hex.length === 192) {
                        return { sigHex: hex.substring(0, 128), pubkeyHex: hex.substring(128) };
                    }
                }
            } catch (_) {}
        }
        const stripped = content.replace(/\s+/g, '');
        if (/^[0-9a-fA-F]{192}$/.test(stripped)) {
            return { sigHex: stripped.substring(0, 128), pubkeyHex: stripped.substring(128) };
        }
        return null;
    }

    // Try to decode text as a 64-byte Schnorr signature (glossia or hex)
    async _tryDecodeSig(text) {
        if (!text) return null;
        const gs = window.GlossiaService;
        if (gs) {
            try {
                const detections = await gs.detectDialect(text);
                const dialect = (Array.isArray(detections) && detections.length > 0) ? detections[0].language : null;
                if (dialect) {
                    const hex = await gs.decodeRawBaseN(text, dialect, 64);
                    if (hex && hex.length === 128) return hex;
                }
            } catch (_) {}
        }
        const stripped = text.replace(/\s+/g, '');
        if (/^[0-9a-fA-F]{128}$/.test(stripped)) return stripped;
        return null;
    }

    // Try to decode text as a 32-byte pubkey (glossia, npub, or hex)
    async _tryDecodePubkey(text) {
        if (!text) return null;
        const stripped = text.replace(/\s+/g, '');
        // npub bech32
        if (stripped.startsWith('npub1')) {
            try {
                return window.CryptoService._npubToHex(stripped);
            } catch (_) {}
        }
        // hex (64 chars = 32 bytes)
        if (/^[0-9a-fA-F]{64}$/.test(stripped)) return stripped;
        // glossia
        const gs = window.GlossiaService;
        if (gs) {
            try {
                const detections = await gs.detectDialect(text);
                const dialect = (Array.isArray(detections) && detections.length > 0) ? detections[0].language : null;
                if (dialect) {
                    const hex = await gs.decodeRawBaseN(text, dialect, 32);
                    if (hex && hex.length === 64) return hex;
                }
            } catch (_) {}
        }
        return null;
    }

    // Build ASCII-armored plaintext body for text/plain MIME part.
    // Same glossia-encoded content as buildHtmlAlt, but with ASCII armor instead of HTML formatting.
    buildPlainBody(bodyText, encodedSig, encodedPubkey, profileName, displayName, isEncrypted, encryptionAlgorithm, originalPlaintext, encodedSigPubkey, quotedArmor) {
        const parts = [];

        // Word-wrap text to ~72 chars per line
        const wordWrap = (text, width = 72) => {
            const words = text.split(/\s+/);
            const lines = [];
            let line = '';
            for (const word of words) {
                if (line && (line.length + 1 + word.length) > width) {
                    lines.push(line);
                    line = word;
                } else {
                    line = line ? line + ' ' + word : word;
                }
            }
            if (line) lines.push(line);
            return lines.join('\n');
        };

        // Signed emails (encrypted or plaintext): combined signature block with sig+pubkey
        if (bodyText && (encodedSigPubkey || encodedSig)) {
            let beginTag;
            if (isEncrypted) {
                const armorType = encryptionAlgorithm === 'nip04' ? 'NIP-04' : 'NIP-44';
                beginTag = `----- BEGIN NOSTR ${armorType} ENCRYPTED BODY -----`;
            } else {
                beginTag = '----- BEGIN NOSTR SIGNED BODY -----';
                // For signed plaintext, show original plaintext above the armor block
                if (originalPlaintext) {
                    parts.push(originalPlaintext);
                }
            }
            const lines = [beginTag];
            lines.push(wordWrap(bodyText));
            // Insert quoted armor inside the armor block, before SIGNATURE
            if (quotedArmor) {
                lines.push(quotedArmor);
            }
            lines.push('----- BEGIN NOSTR SIGNATURE -----');
            if (profileName) lines.push(`@${profileName}`);
            if (encodedSigPubkey) {
                // Legacy combined 96-byte payload (for backward compat only)
                lines.push(wordWrap(encodedSigPubkey));
            } else {
                // New format: separate sig + pubkey
                lines.push(wordWrap(encodedSig));
                if (encodedPubkey) {
                    lines.push(wordWrap(encodedPubkey));
                }
            }
            lines.push('----- END NOSTR MESSAGE -----');
            parts.push(lines.join('\n'));
            return parts.join('\n\n');
        }

        // Unsigned encrypted: body + seal for sender identification (needed for decryption)
        if (isEncrypted && bodyText) {
            const armorType = encryptionAlgorithm === 'nip04' ? 'NIP-04' : 'NIP-44';
            const lines = [`----- BEGIN NOSTR ${armorType} ENCRYPTED BODY -----`];
            lines.push(wordWrap(bodyText));
            if (quotedArmor) {
                lines.push(quotedArmor);
            }
            if (encodedPubkey) {
                lines.push('----- BEGIN NOSTR SEAL -----');
                if (displayName) lines.push(`@${displayName}`);
                lines.push(wordWrap(encodedPubkey));
            }
            lines.push('----- END NOSTR MESSAGE -----');
            parts.push(lines.join('\n'));
            return parts.join('\n\n');
        }

        if (bodyText) {
            parts.push(bodyText);
        }

        // Unsigned plaintext with pubkey-only seal
        if (encodedPubkey) {
            const sealLines = ['----- BEGIN NOSTR SEAL -----'];
            if (displayName) sealLines.push(`@${displayName}`);
            sealLines.push(wordWrap(encodedPubkey));
            sealLines.push('----- END NOSTR SEAL -----');
            parts.push(sealLines.join('\n'));
        }

        return parts.join('\n\n');
    }

    // Reset encrypt button state
    resetEncryptButtonState() {
        const encryptBtn = document.getElementById('encrypt-btn');
        if (encryptBtn) {
            const iconSpan = encryptBtn.querySelector('.encrypt-btn-icon i');
            const labelSpan = encryptBtn.querySelector('.encrypt-btn-label');
            
            if (iconSpan) iconSpan.className = 'fas fa-lock';
            if (labelSpan) labelSpan.textContent = 'Encrypt';
            encryptBtn.dataset.encrypted = 'false';
            
            // Re-enable editing
            const subjectInput = document.getElementById('subject');
            const messageBodyInput = document.getElementById('message-body');
            if (subjectInput) subjectInput.disabled = false;
            if (messageBodyInput) messageBodyInput.disabled = false;
        }
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
        
        // Get settings early - needed for autoencrypt check
        const settings = appState.getSettings();
        
        // Check if content is encrypted but no recipient pubkey is selected
        const encryptBtn = domManager.get('encryptBtn');
        const isEncrypted = encryptBtn && encryptBtn.dataset.encrypted === 'true';
        const isEncryptedContent = Utils.isLikelyEncryptedContent(subject) || Utils.isLikelyEncryptedContent(body) || 
                                   (body && body.includes('BEGIN NOSTR'));
        
        const recipientPubkey = this.getRecipientPubkey();

        if ((isEncrypted || isEncryptedContent) && !recipientPubkey) {
            notificationService.showError('Encrypted content requires a recipient pubkey. Please select a Nostr contact or enter a pubkey.');
            return;
        }

        // Check if autoencrypt is enabled - if so, require recipient pubkey
        const autoEncrypt = settings && settings.automatically_encrypt !== false; // Default to true
        if (autoEncrypt && !recipientPubkey) {
            // Try to find contact by email address
            const contacts = appState.getContacts();
            const contactByEmail = contacts.find(c => c.email && c.email.toLowerCase() === toAddress.toLowerCase());

            if (contactByEmail) {
                // Found contact by email, set it as selected
                this.selectedNostrContact = contactByEmail;
                const dropdown = domManager.get('nostrContactSelect');
                if (dropdown) {
                    dropdown.value = contactByEmail.pubkey;
                }
                // Fill the recipient pubkey input
                const pubkeyValue = document.getElementById('recipient-pubkey-value');
                if (pubkeyValue) {
                    pubkeyValue.value = contactByEmail.pubkey;
                }
                console.log('[JS] Auto-found contact by email for autoencrypt:', contactByEmail.name);
            } else {
                // No contact found - block sending when autoencrypt is enabled
                notificationService.showError('Autoencrypt is enabled but no recipient pubkey found. Please select a Nostr contact, enter a pubkey, or disable autoencrypt to send unencrypted emails.');
                return;
            }
        }
        
        // Warn about attachments not being supported yet
        if (this.attachments && this.attachments.length > 0) {
            notificationService.showWarning(`Warning: ${this.attachments.length} attachment(s) will not be sent. Attachment support is coming soon.`);
        }
        
        console.log('[JS] Form validation passed');
        
        // Use stored message ID or generate new one
        const messageId = this.generateAndStoreMessageId();
        console.log('[JS] Using message ID:', messageId);
        
        // Check if using Gmail or Yahoo and warn about App Password
        // (settings already retrieved above)
        if (settings.smtp_host === 'smtp.gmail.com') {
            console.log('[JS] Gmail detected, checking for App Password warning');
            const isGmailAddress = settings.email_address?.includes('@gmail.com');
            if (isGmailAddress) {
                console.log('[JS] Showing Gmail App Password info message');
                notificationService.showSuccess('Gmail detected: Make sure you\'re using an App Password, not your regular password. If you haven\'t set up an App Password, go to Google Account > Security > 2-Step Verification > App passwords.');
            }
        } else if (settings.smtp_host === 'smtp.mail.yahoo.com') {
            console.log('[JS] Yahoo detected, checking for App Password warning');
            const isYahooAddress = settings.email_address?.includes('@yahoo.com') || settings.email_address?.includes('@ymail.com');
            if (isYahooAddress) {
                console.log('[JS] Showing Yahoo App Password info message');
                notificationService.showSuccess('Yahoo detected: Make sure you\'re using an App Password, not your regular password. If you haven\'t set up an App Password, go to your Yahoo Account Security settings > Generate app password.');
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
            
            // Check if we should sign the email
            // Sign if auto-sign is enabled OR if user manually signed
            const autoSign = settings && settings.automatically_sign !== false; // Default to true
            const signBtn = domManager.get('signBtn');
            const isManuallySigned = signBtn && signBtn.dataset.signed === 'true';
            const shouldSign = autoSign || isManuallySigned;
            
            console.log('[JS] Signing check:', { autoSign, isManuallySigned, shouldSign });
            
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port,
                use_tls: useTls
            };

            // If a Nostr contact is selected or a pubkey is manually entered, send encrypted email
            const sendPubkey = this.getRecipientPubkey();
            if (this.selectedNostrContact || sendPubkey) {
                // Build an ad-hoc contact if only the pubkey input was filled
                const contact = this.selectedNostrContact || { pubkey: sendPubkey, email: toAddress, name: toAddress };
                console.log('[JS] Sending encrypted email to:', contact.name || contact.pubkey);

                // Check if we have a keypair and active relays
                if (!appState.hasKeypair()) {
                    return;
                }

                const activeRelays = appState.getActiveRelays();
                if (activeRelays.length === 0) {
                    notificationService.showError('No active Nostr relays configured');
                    return;
                }

                // Safety check: NIP-04 messages must contain a SIGNATURE block (spec section 4.1)
                const encAlgo = settings?.encryption_algorithm || 'nip44';
                const currentBody = this._plainBody || domManager.getValue('messageBody') || '';
                if (encAlgo === 'nip04' &&
                    currentBody.includes('NIP-04 ENCRYPTED') &&
                    !currentBody.includes('BEGIN NOSTR SIGNATURE')) {
                    notificationService.showError(
                        'NIP-04 messages require a signature for security. ' +
                        'Signing failed — please try again or switch to NIP-44.'
                    );
                    return;
                }

                // Send encrypted email
                await this.sendEncryptedEmail(emailConfig, contact, subject, body, messageId, toAddress);
            } else {
                // Send regular email
                console.log('[JS] Sending regular email');
                
                // Send email with attachments
                const attachmentData = this.prepareAttachmentsForEmail();
                console.log('[JS] Sending email with attachments:', attachmentData);
                
                const plainBody = this._plainBody || body;
                await TauriService.sendEmail(emailConfig, toAddress, subject, plainBody, null, messageId, attachmentData, this._htmlBody, this._replyToMessageId, this._replyReferences);
            }

            console.log('[JS] Email sent successfully');

            // Trigger a sync of sent emails in the background (non-blocking)
            // Note: The email is not saved to DB immediately - it will be fetched from the server via IMAP sync
            // Run sync in background without blocking UI - use setTimeout to defer it
            setTimeout(async () => {
                try {
                    // Add timeout wrapper to prevent hanging
                    const syncPromise = this.syncSentEmails();
                    const timeoutPromise = new Promise((_, reject) => 
                        setTimeout(() => reject(new Error('Sync timeout after 30 seconds')), 30000)
                    );
                    await Promise.race([syncPromise, timeoutPromise]);
                    console.log('[JS] Synced sent emails after sending');
                } catch (syncError) {
                    console.warn('[JS] Failed to sync sent emails after sending (non-critical):', syncError);
                    // Don't show error to user - sync will happen on next refresh anyway
                }
            }, 100); // Defer by 100ms to let UI update first

            // Clear form
            domManager.clear('toAddress');
            domManager.clear('subject');
            domManager.clear('messageBody');
            domManager.setValue('nostrContactSelect', '');
            this.selectedNostrContact = null;
            
            // Hide pubkey display
            const pubkeyDisplay = document.getElementById('selected-recipient-pubkey');
            if (pubkeyDisplay) {
                // pubkey field is always visible
            }
            
            // Clear attachments
            this.clearAttachments();
            
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
        
        const missing = [!toAddress && 'To', !subject && 'Subject', !body && 'Body'].filter(Boolean);
        if (missing.length) {
            notificationService.showError('Missing required fields: ' + missing.join(', '));
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
            
            // Check if autoencrypt is enabled - if so, encrypt before previewing headers
            const autoEncrypt = settings && settings.automatically_encrypt !== false; // Default to true
            let previewSubject = subject;
            let previewBody = body;
            let nostrNpub = null;
            
            // Try to find contact if autoencrypt is enabled
            let contactForEncryption = this.selectedNostrContact;
            if (autoEncrypt && !contactForEncryption) {
                const contacts = appState.getContacts();
                const contactByEmail = contacts.find(c => c.email && c.email.toLowerCase() === toAddress.toLowerCase());
                if (contactByEmail) {
                    contactForEncryption = contactByEmail;
                }
            }
            
            // Encrypt in memory if autoencrypt is enabled and contact is found
            // This does NOT modify the DOM fields - only encrypts for preview
            if (autoEncrypt && contactForEncryption) {
                console.log('[JS] Auto-encrypt enabled for header preview, encrypting in memory...');
                const encrypted = await this.encryptEmailFieldsInMemory(subject, body, contactForEncryption);
                previewSubject = encrypted.encryptedSubject;
                previewBody = encrypted.encryptedBody;
                nostrNpub = contactForEncryption.pubkey;
                console.log('[JS] Encrypted for header preview (in memory only)');
            }
            
            // Check if we should sign the email (for preview)
            // Sign if auto-sign is enabled OR if user manually signed
            const autoSign = settings && settings.automatically_sign !== false; // Default to true
            const signBtn = domManager.get('signBtn');
            const isManuallySigned = signBtn && signBtn.dataset.signed === 'true';
            const shouldSign = autoSign || isManuallySigned;
            
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port,
                use_tls: useTls
            };

            // Construct headers (sender's pubkey will be derived from keychain in backend)
            const headers = await TauriService.constructEmailHeaders(emailConfig, toAddress, previewSubject, previewBody, nostrNpub, messageId, null, this._htmlBody, this._replyToMessageId, this._replyReferences);
            
            // Debug: Log what headers we actually received
            console.log('[JS] Headers received from backend:');
            console.log(headers);
            console.log('[JS] Headers length:', headers.length);
            console.log('[JS] Headers contains Message-ID:', headers.includes('Message-ID'));
            console.log('[JS] Headers contains Message-ID:', headers.toLowerCase().includes('message-id'));
            
            // Verify header signature (X-Nostr-Sig) if present
            let headerSigResult = null;
            const pubkeyMatch = headers.match(/X-Nostr-Pubkey:\s*(\S+)/i);
            const sigMatch = headers.match(/X-Nostr-Sig:\s*(\S+)/i);
            if (pubkeyMatch && sigMatch) {
                try {
                    // Extract signable bytes via backend (handles armor decode + nested concatenation)
                    const rawBytes = await TauriService.extractSignableBytes(previewBody, true, null, null);
                    const dataBytes = new Uint8Array(rawBytes);
                    console.log('[JS] Header sig verify: dataBytes length=', dataBytes.length);
                    const isValid = await TauriService.verifySignature(
                        pubkeyMatch[1], sigMatch[1], dataBytes
                    );
                    headerSigResult = { isValid, pubkey: pubkeyMatch[1], signature: sigMatch[1], type: 'header' };
                    console.log('[JS] Preview header signature:', isValid ? 'VALID' : 'INVALID');
                } catch (e) {
                    console.warn('[JS] Preview header signature verification failed:', e);
                    headerSigResult = { isValid: false, error: e.message, type: 'header' };
                }
            }

            // Show headers in a modal (inline sig verified after DOM render)
            this.showHeadersModal(headers, headerSigResult, body);

        } catch (error) {
            console.error('[JS] Error in previewEmailHeaders function:', error);
            notificationService.showError('Failed to preview headers: ' + error);
        }
    }

    // Show headers and content preview in a tabbed modal
    showHeadersModal(headers, headerSigResult = null, plainBody = null) {
        console.log('[JS] showHeadersModal called with headers');

        // HTML escape the headers to prevent Message-ID from being interpreted as HTML tags
        const escapedHeaders = headers
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');

        // Envelope fields
        const escHtml = (s) => (s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
        const settings = appState.getSettings() || {};
        const fromAddr = escHtml(settings.email_address || '');
        const toAddr = escHtml(domManager.getValue('toAddress') || '');
        const subject = escHtml(domManager.getValue('subject') || '');

        // Header signature badge
        let headerBadge = '';
        if (headerSigResult) {
            if (headerSigResult.isValid) {
                headerBadge = `<span class="signature-indicator verified"><i class="fas fa-check-circle"></i> Header Sig Verified</span>`;
            } else {
                headerBadge = `<span class="signature-indicator invalid"><i class="fas fa-times-circle"></i> Header Sig Invalid</span>`;
            }
        }

        const envelopeHtml = `
            <div style="margin-bottom:1em;font-size:0.95em;line-height:1.8;">
                <div><strong>From:</strong> ${fromAddr} ${headerBadge}</div>
                <div><strong>To:</strong> ${toAddr}</div>
                <div><strong>Subject:</strong> ${subject}</div>
            </div>
            <hr style="border:none;border-top:1px solid #ccc;margin:0 0 1em;">`;

        // Content tab: show HTML body if available, otherwise the plaintext body
        let bodyHtml;
        const hasHtml = !!this._htmlBody;
        if (hasHtml) {
            bodyHtml = '<div id="preview-html-body" class="email-detail-body"></div>';
        } else {
            const body = (domManager.getValue('messageBody') || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, '<br>');
            bodyHtml = `<div style="font-family:sans-serif;line-height:1.6;">${body}</div>`;
        }
        const contentHtml = envelopeHtml + bodyHtml;

        // Raw source: headers + plaintext body + raw HTML source
        const escBody = escHtml(this._plainBody || domManager.getValue('messageBody') || '');
        const rawHtmlSource = this._htmlBody ? escHtml(this._htmlBody) : '';
        let rawSource = escapedHeaders + '\n\n' + escBody;
        if (rawHtmlSource) {
            rawSource += '\n\n--- text/html ---\n\n' + rawHtmlSource;
        }

        const modalHtml = `
            <div class="modal-overlay" id="headersModal">
                <div class="modal-content modal">
                    <div class="modal-header">
                        <h3>Preview</h3>
                        <div class="preview-tabs">
                            <button class="preview-tab active" data-tab="content">Content</button>
                            <button class="preview-tab" data-tab="raw">Raw</button>
                        </div>
                        <button class="modal-close" onclick="document.getElementById('headersModal').remove()">&times;</button>
                    </div>
                    <div class="modal-body">
                        <div class="preview-tab-panel active" data-panel="content">
                            ${contentHtml}
                        </div>
                        <div class="preview-tab-panel" data-panel="raw">
                            <div class="headers-preview">
                                <pre>${rawSource}</pre>
                            </div>
                        </div>
                    </div>
                    <div class="modal-footer">
                        <button class="btn btn-primary" id="preview-send-btn" onclick="document.getElementById('headersModal').remove(); document.getElementById('send-btn').click();"><i class="fas fa-paper-plane"></i> Send</button>
                        <button class="btn btn-secondary" onclick="document.getElementById('headersModal').remove()">Close</button>
                    </div>
                </div>
            </div>
        `;

        // Remove any existing modal
        const existingModal = document.getElementById('headersModal');
        if (existingModal) {
            existingModal.remove();
        }

        // Add modal to page
        document.body.insertAdjacentHTML('beforeend', modalHtml);

        // Verify inline signature and inject badge into HTML body before rendering
        if (hasHtml && plainBody) {
            const renderHtmlWithSigBadge = async () => {
                let htmlToRender = this._htmlBody;
                try {
                    const sigResults = await TauriService.verifyAllSignatures(plainBody);
                    if (sigResults.length > 0) {
                        htmlToRender = this.injectHtmlSigBadge(htmlToRender, sigResults);
                    }
                } catch (e) {
                    console.warn('[JS] Preview inline signature verification failed:', e);
                }
                // Pre-decrypt from plaintext armor for lock toggle
                let decryptResults = null;
                try {
                    const pubkey = this.selectedNostrContact?.pubkey;
                    if (pubkey && appState.hasKeypair()) {
                        const result = await TauriService.decryptEmailBody(plainBody, '', pubkey, null);
                        if (result.blockResults && result.blockResults.length > 0) {
                            decryptResults = result.blockResults.map(b => {
                                if (!b.wasEncrypted) return null;
                                if (b.decryptedText != null) return { decryptedText: b.decryptedText };
                                if (b.error) return { error: b.error };
                                return null;
                            });
                        }
                    }
                } catch (e) {
                    console.warn('[JS] Preview pre-decrypt failed:', e);
                }
                Utils.renderHtmlBodyInIframe('preview-html-body', htmlToRender, { decryptedTexts: decryptResults });
            };
            renderHtmlWithSigBadge();
        } else if (hasHtml) {
            Utils.renderHtmlBodyInIframe('preview-html-body', this._htmlBody, {});
        }

        // Wire up tab switching
        const modal = document.getElementById('headersModal');
        modal.querySelectorAll('.preview-tab').forEach(tab => {
            tab.addEventListener('click', () => {
                modal.querySelectorAll('.preview-tab').forEach(t => t.classList.remove('active'));
                modal.querySelectorAll('.preview-tab-panel').forEach(p => p.classList.remove('active'));
                tab.classList.add('active');
                modal.querySelector(`.preview-tab-panel[data-panel="${tab.dataset.tab}"]`).classList.add('active');
            });
        });
    }

    async sendEncryptedEmail(emailConfig, contact, subject, body, messageId, toAddress) {
        console.log('[JS] sendEncryptedEmail called for contact:', contact.name);
        try {
            const keypair = appState.getKeypair();
            const activeRelays = appState.getActiveRelays();
            
            // Check if user wants to send a matching DM (from settings)
            const settings = appState.getSettings();
            const shouldSendDm = settings && settings.send_matching_dm !== false; // Default to true
            const autoEncrypt = settings && settings.automatically_encrypt !== false; // Default to true
            
            // Check if email is actually encrypted by examining the content
            // Subject should be encrypted (base64-like) or body should have NIP encryption markers
            let isSubjectEncrypted = window.Utils && window.Utils.isLikelyEncryptedContent(subject);
            let isBodyEncrypted = body.includes('BEGIN NOSTR') || (window.Utils && window.Utils.isLikelyEncryptedContent(body));
            let isEmailEncrypted = isSubjectEncrypted || isBodyEncrypted;
            
            // Auto-encrypt if enabled and email is not already encrypted
            if (autoEncrypt && !isEmailEncrypted) {
                console.log('[JS] Auto-encrypt enabled, encrypting email before sending...');
                // Temporarily set selected contact for encryption
                const originalContact = this.selectedNostrContact;
                this.selectedNostrContact = contact;
                
                // Encrypt the email fields
                const didEncrypt = await this.encryptEmailFields();
                if (didEncrypt) {
                    // Get the encrypted values from DOM (may be glossia-encoded after auto-encode)
                    subject = domManager.getValue('subject') || subject;
                    body = domManager.getValue('messageBody') || body;
                    isSubjectEncrypted = true; // encryptEmailFields succeeded, content is encrypted (possibly glossia-encoded)
                    isBodyEncrypted = true;
                    isEmailEncrypted = true;
                    console.log('[JS] Auto-encryption completed (may include glossia encoding)');
                } else {
                    console.error('[JS] Auto-encryption failed, not sending email');
                    notificationService.showError('Failed to auto-encrypt email. Please encrypt manually or check your settings.');
                    // Restore original contact selection
                    this.selectedNostrContact = originalContact;
                    return; // Don't send unencrypted email
                }
                
                // Restore original contact selection
                this.selectedNostrContact = originalContact;
            }
            
            console.log('[JS] Email encryption check:', { 
                isSubjectEncrypted, 
                isBodyEncrypted, 
                isEmailEncrypted,
                shouldSendDm,
                subjectPreview: subject.substring(0, 50),
                bodyPreview: body.substring(0, 50)
            });
            
            // Only send DM if setting is enabled and email is actually encrypted
            if (shouldSendDm && isEmailEncrypted) {
                console.log('[JS] Sending matching DM to contact:', contact.name);
                // Send the NIP ciphertext (base64) directly as the DM content.
                // This is already NIP-encrypted, so other Nostr clients can decrypt it.
                const dmCiphertext = this._subjectCiphertext;
                if (dmCiphertext) {
                    try {
                        const dmResult = await TauriService.sendEncryptedDirectMessage(
                            contact.pubkey,
                            dmCiphertext,
                            activeRelays
                        );
                        console.log('[JS] DM sent successfully, event ID:', dmResult);
                        notificationService.showSuccess(`DM sent successfully (event ID: ${dmResult.substring(0, 16)}...)`);
                    } catch (dmError) {
                        console.error('[JS] Failed to send DM:', dmError);
                        notificationService.showError('Email sent but DM failed: ' + dmError);
                    }
                } else {
                    console.warn('[JS] No subject ciphertext available for DM, skipping');
                }
            } else if (shouldSendDm && !isEmailEncrypted) {
                console.warn('[JS] Email is not encrypted, DM will NOT be sent for security reasons.');
                notificationService.showInfo('No DM sent: Email is not encrypted.');
            } else if (!shouldSendDm) {
                console.log('[JS] Send matching DM setting is disabled, skipping DM.');
            }
            
            // Send encrypted email with the encrypted subject and body
            const attachmentData = this.prepareAttachmentsForEmail();
            console.log('[JS] Sending encrypted email with attachments:', attachmentData);
            const plainBody = this._plainBody || body;
            const recipientEmail = toAddress || contact.email;
            if (!recipientEmail) {
                throw new Error('No email address available for this contact. Please enter one in the To field.');
            }
            await TauriService.sendEmail(emailConfig, recipientEmail, subject, plainBody, null, messageId, attachmentData, this._htmlBody, this._replyToMessageId, this._replyReferences);
            console.log('[JS] Encrypted email sent successfully');

            // Persist a minimal record of this sent email to the local DB immediately,
            // with subject_hash set to SHA-256(NIP ciphertext). The matching DM's
            // content_hash hashes the same bytes, so DM↔email linking works the
            // moment the DM lands — no IMAP-sync round-trip required. A later sync
            // of the Sent folder updates this row (save_email uses COALESCE so the
            // correct subject_hash survives).
            if (messageId) {
                try {
                    const hash = this._subjectCiphertext
                        ? await this.hashStringSHA256(this._subjectCiphertext)
                        : null;
                    await window.__TAURI__.core.invoke('db_save_sent_email_stub', {
                        messageId,
                        fromAddress: emailConfig.email_address,
                        toAddress: recipientEmail,
                        subject,
                        body: plainBody,
                        bodyHtml: this._htmlBody || null,
                        senderPubkey: keypair?.public_key || null,
                        recipientPubkey: contact?.pubkey || null,
                        subjectHash: hash,
                        inReplyTo: this._replyToMessageId || null,
                        references: this._replyReferences || null,
                    });
                    console.log('[JS] Saved sent-email stub for message_id:', messageId);
                } catch (e) {
                    console.warn('[JS] Failed to save sent-email stub:', e);
                }
            }
        } catch (error) {
            console.error('[JS] Error sending encrypted email:', error);
            throw new Error(`Failed to send encrypted email: ${error}`);
        }
    }

    // Load emails
    async loadEmails(searchQuery = '', append = false) {
        if (!appState.hasEmailSettingsConfigured()) {
            console.log('[JS] No email settings configured, skipping inbox load');
            appState.setEmails([]);
            const emailList = domManager.get('emailList');
            if (emailList) emailList.innerHTML = '<div class="text-center text-muted">Configure email settings to view inbox</div>';
            return;
        }
        try {
            if (!append) {
                // Reset offset when loading fresh (not appending)
                this.inboxOffset = 0;
                domManager.disable('refreshInbox');
                domManager.setHTML('refreshInbox', '<span class="loading"></span> Loading...');
            } else {
                // Show loading state on Load More button
                const loadMoreBtn = document.getElementById('load-more-emails');
                if (loadMoreBtn) {
                    loadMoreBtn.disabled = true;
                    loadMoreBtn.innerHTML = '<span class="loading"></span> Loading...';
                }
            }
            const settings = appState.getSettings();
            const keypair = appState.getKeypair();
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host,
                smtp_port: settings.smtp_port,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port,
                use_tls: settings.use_tls
            };
            // Read filter preference from settings
            const emailFilter = settings.email_filter || 'nostr';
            const nostrOnly = emailFilter === 'nostr' ? true : null;
            // Get page size from settings (default to 50)
            const pageSize = settings.emails_per_page || 50;
            // Always pass the user's email address for filtering (only as recipient)
            const userEmail = settings.email_address ? settings.email_address : null;
            // Pass user pubkey for contact-based filtering
            const userPubkey = keypair ? keypair.public_key : null;
            console.log('[JS] getDbEmails userEmail:', userEmail, 'userPubkey:', userPubkey);
            console.log('[JS] Email filter preference:', emailFilter, 'nostrOnly:', nostrOnly);
            console.log('[JS] Loading emails with offset:', this.inboxOffset, 'pageSize:', pageSize);
            const emails = await TauriService.getDbEmailThreads(pageSize, this.inboxOffset, nostrOnly, userEmail, userPubkey);
            
            // Load attachments for each email in parallel with timeout
            console.log(`[JS] Loading attachments for ${emails.length} inbox emails`);
            const attachmentPromises = emails.map(async (email) => {
                try {
                    // Convert email.id to integer - it might be a string
                    const emailIdInt = parseInt(email.id);
                    if (isNaN(emailIdInt)) {
                        console.warn(`[JS] Email ${email.id} has non-numeric ID, trying to get email by message_id to load attachments`);
                        // Try to get email by message_id to get the real database ID
                        if (email.message_id) {
                            try {
                                const emailData = await TauriService.getDbEmail(email.message_id);
                                if (emailData && emailData.id) {
                                    const realId = parseInt(emailData.id);
                                    if (!isNaN(realId)) {
                                        email.attachments = await TauriService.getAttachmentsForEmail(realId);
                                        console.log(`[JS] Email ${email.id} (message_id: ${email.message_id}, DB ID: ${realId}) has ${email.attachments.length} attachments`);
                                        return;
                                    }
                                }
                            } catch (e) {
                                console.error(`[JS] Failed to get email by message_id ${email.message_id}:`, e);
                            }
                        }
                        console.warn(`[JS] Email ${email.id} has non-numeric ID and couldn't resolve, cannot load attachments`);
                        email.attachments = [];
                        return;
                    }
                    
                    const timeoutPromise = new Promise((_, reject) => 
                        setTimeout(() => reject(new Error('Attachment load timeout')), 5000)
                    );
                    email.attachments = await Promise.race([
                        TauriService.getAttachmentsForEmail(emailIdInt),
                        timeoutPromise
                    ]);
                    console.log(`[JS] Email ${email.id} (DB ID: ${emailIdInt}) has ${email.attachments.length} attachments:`, email.attachments);
                } catch (error) {
                    console.error(`[JS] Failed to load attachments for email ${email.id}:`, error);
                    email.attachments = [];
                }
            });
            
            await Promise.all(attachmentPromises);
            
            let appendFrom = 0;
            if (append) {
                // Append new emails to existing ones
                const existingEmails = appState.getEmails();
                appendFrom = existingEmails.length;
                appState.setEmails([...existingEmails, ...emails]);
            } else {
                // Replace emails (fresh load)
                appState.setEmails(emails);
            }

            // Update offset for next load
            this.inboxOffset += emails.length;

            // Show Load More button if we got a full page of results
            this.renderEmails(emails.length === pageSize, appendFrom);
        } catch (error) {
            console.error('Failed to load emails:', error);
            notificationService.showError('Failed to load emails: ' + error);
        } finally {
            if (!append) {
                domManager.enable('refreshInbox');
                domManager.setHTML('refreshInbox', '<i class="fas fa-sync"></i> Refresh');
            } else {
                const loadMoreBtn = document.getElementById('load-more-emails');
                if (loadMoreBtn) {
                    loadMoreBtn.disabled = false;
                    loadMoreBtn.innerHTML = '<i class="fas fa-chevron-down"></i> Load More';
                }
            }
        }
    }

    // Load more emails (pagination)
    async loadMoreEmails() {
        const searchQuery = domManager.getValue('emailSearch')?.trim() || '';
        if (searchQuery) {
            // Load more search results
            await this.filterEmails(true);
        } else {
            // Load more regular emails
            await this.loadEmails('', true);
        }
    }
    
    async loadMoreSentEmails() {
        const searchQuery = domManager.getValue('sentSearch')?.trim() || '';
        if (searchQuery) {
            // Load more sent search results
            await this.filterSentEmails(true);
        } else {
            // Load more regular sent emails
            await this.loadSentEmails('', true);
        }
    }

    // Load sent emails
    async loadSentEmails(searchQuery = '', append = false) {
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
            return;
        }
        try {
            if (!append) {
                // Reset offset when loading fresh (not appending)
                this.sentOffset = 0;
                domManager.disable('refreshSent');
                domManager.setHTML('refreshSent', '<span class="loading"></span> Loading...');
            } else {
                // Show loading state on Load More button
                const loadMoreBtn = document.getElementById('load-more-sent-emails');
                if (loadMoreBtn) {
                    loadMoreBtn.disabled = true;
                    loadMoreBtn.innerHTML = '<span class="loading"></span> Loading...';
                }
            }
            const settings = appState.getSettings();
            const keypair = appState.getKeypair();
            const userEmail = settings.email_address ? settings.email_address : null;
            // Get page size from settings (default to 50)
            const pageSize = settings.emails_per_page || 50;
            console.log('[JS] Loading sent emails with offset:', this.sentOffset, 'pageSize:', pageSize);
            // Fetch sent emails (where user is sender)
            const userPubkey = keypair ? keypair.public_key : null;
            let emails = await TauriService.getDbSentEmailThreads(pageSize, this.sentOffset, userEmail, userPubkey);
            
            // Load attachments for each email in parallel with timeout
            console.log(`[JS] Loading attachments for ${emails.length} sent emails`);
            const attachmentPromises = emails.map(async (email) => {
                try {
                    // Convert email.id to integer - it might be a string
                    const emailIdInt = parseInt(email.id);
                    if (isNaN(emailIdInt)) {
                        console.warn(`[JS] Email ${email.id} has non-numeric ID, trying to get email by message_id to load attachments`);
                        // Try to get email by message_id to get the real database ID
                        if (email.message_id) {
                            try {
                                const emailData = await TauriService.getDbEmail(email.message_id);
                                if (emailData && emailData.id) {
                                    const realId = parseInt(emailData.id);
                                    if (!isNaN(realId)) {
                                        email.attachments = await TauriService.getAttachmentsForEmail(realId);
                                        console.log(`[JS] Email ${email.id} (message_id: ${email.message_id}, DB ID: ${realId}) has ${email.attachments.length} attachments`);
                                        return;
                                    }
                                }
                            } catch (e) {
                                console.error(`[JS] Failed to get email by message_id ${email.message_id}:`, e);
                            }
                        }
                        console.warn(`[JS] Email ${email.id} has non-numeric ID and couldn't resolve, cannot load attachments`);
                        email.attachments = [];
                        return;
                    }
                    
                    const timeoutPromise = new Promise((_, reject) => 
                        setTimeout(() => reject(new Error('Attachment load timeout')), 5000)
                    );
                    email.attachments = await Promise.race([
                        TauriService.getAttachmentsForEmail(emailIdInt),
                        timeoutPromise
                    ]);
                    console.log(`[JS] Email ${email.id} (DB ID: ${emailIdInt}) has ${email.attachments.length} attachments:`, email.attachments);
                } catch (error) {
                    console.error(`[JS] Failed to load attachments for email ${email.id}:`, error);
                    email.attachments = [];
                }
            });
            await Promise.allSettled(attachmentPromises);
            
            let appendFrom = 0;
            if (append) {
                // Append new emails to existing ones
                const existingEmails = appState.getSentEmails();
                appendFrom = existingEmails.length;
                appState.setSentEmails([...existingEmails, ...emails]);
            } else {
                // Replace emails (fresh load)
                appState.setSentEmails(emails);
            }

            // Update offset for next load
            this.sentOffset += emails.length;

            // Show Load More button if we got a full page of results
            this.renderSentEmails(emails.length === pageSize, appendFrom);
        } catch (error) {
            console.error('Failed to load sent emails:', error);
            notificationService.showError('Failed to load sent emails: ' + error);
        } finally {
            if (!append) {
                domManager.enable('refreshSent');
                domManager.setHTML('refreshSent', '<i class="fas fa-sync"></i> Refresh');
            } else {
                const loadMoreBtn = document.getElementById('load-more-sent-emails');
                if (loadMoreBtn) {
                    loadMoreBtn.disabled = false;
                    loadMoreBtn.innerHTML = '<i class="fas fa-chevron-down"></i> Load More';
                }
            }
        }
    }

    // Load more sent emails (pagination)
    async loadMoreSentEmails() {
        await this.loadSentEmails('', true);
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
            // Get selected folder from dropdown
            const folderSelect = document.getElementById('imap-folder-select');
            const selectedFolder = folderSelect && folderSelect.value ? folderSelect.value : null;
            console.log(`[JS] Syncing inbox emails from folder: ${selectedFolder || 'All Folders'}`);
            
            const newCount = await TauriService.syncNostrEmails(selectedFolder);
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
            console.error('[JS] Error details:', {
                message: error?.message,
                toString: error?.toString(),
                string: String(error),
                type: typeof error,
                error: error
            });
            // Ensure we throw an Error object with a proper message
            const errorMessage = error?.message || error?.toString() || String(error) || 'Unknown error';
            throw new Error(errorMessage);
        }
    }

    async filterSentEmails(append = false) {
        const searchQuery = domManager.getValue('sentSearch')?.trim() || '';
        
        // If search query is empty, load all sent emails normally
        if (!searchQuery) {
            // Clean up any existing search listeners
            if (this.sentSearchUnlisten) {
                this.sentSearchUnlisten();
                this.sentSearchUnlisten = null;
            }
            await this.loadSentEmails('');
            return;
        }
        
        // Clear existing timeout
        if (this.sentSearchTimeout) {
            clearTimeout(this.sentSearchTimeout);
        }
        
        // Clean up any existing search listeners
        if (this.sentSearchUnlisten) {
            this.sentSearchUnlisten();
            this.sentSearchUnlisten = null;
        }
        
        // Set a new timeout to debounce the search
        this.sentSearchTimeout = setTimeout(async () => {
            try {
                const settings = appState.getSettings();
                const userEmail = settings.email_address ? settings.email_address : null;

                // Initialize search results accumulator (or keep existing if appending)
                if (!append) {
                    this.sentSearchResults = [];
                    this.sentSearchOffset = 0;
                }
                this.sentSearchInProgress = true;

                // Show searching state
                const sentList = domManager.get('sentList');
                if (sentList) {
                    sentList.innerHTML = '<div class="text-center text-muted" style="padding: 20px;"><i class="fas fa-spinner fa-spin"></i> Searching...</div>';
                }
                
                // Set up event listeners for incremental search results
                this.sentSearchUnlisten = await window.__TAURI__.event.listen('sent-search-result', (event) => {
                    const email = event.payload;
                    this.sentSearchResults.push(email);
                    console.log(`[JS] Sent search result received: ${this.sentSearchResults.length} emails found so far`);
                    
                    // Update UI incrementally
                    this.updateSentSearchResults();
                });
                
                // Listen for progress updates
                const progressUnlisten = await window.__TAURI__.event.listen('sent-search-progress', (event) => {
                    const progress = event.payload;
                    const sentList = domManager.get('sentList');
                    if (sentList && this.sentSearchInProgress) {
                        const percent = Math.round((progress.processed / progress.total) * 100);
                        sentList.innerHTML = `<div class="text-center text-muted" style="padding: 20px;">
                            <i class="fas fa-spinner fa-spin"></i> Searching... ${percent}% (${progress.processed}/${progress.total} emails processed)
                            ${this.sentSearchResults.length > 0 ? `<br><small>Found ${this.sentSearchResults.length} match${this.sentSearchResults.length !== 1 ? 'es' : ''} so far</small>` : ''}
                        </div>`;
                    }
                });
                
                // Listen for search completion
                const completedUnlisten = await window.__TAURI__.event.listen('sent-search-completed', async (event) => {
                    const completion = event.payload;
                    console.log(`[JS] Sent search completed: ${completion.total_found} total matches, has_more: ${completion.has_more}`);
                    
                    // Update has_more flag
                    if (completion.has_more !== undefined) {
                        this.sentSearchHasMore = completion.has_more;
                    }
                    
                    this.sentSearchInProgress = false;
                    
                    // Clean up listeners
                    if (this.sentSearchUnlisten) {
                        this.sentSearchUnlisten();
                        this.sentSearchUnlisten = null;
                    }
                    progressUnlisten();
                    completedUnlisten();
                    
                    // Load attachments for all search results (same logic as normal sent email loading)
                    console.log(`[JS] Loading attachments for ${this.sentSearchResults.length} sent search result emails`);
                    const attachmentPromises = this.sentSearchResults.map(async (email) => {
                        try {
                            // Convert email.id to integer - it might be a string
                            const emailIdInt = parseInt(email.id);
                            if (isNaN(emailIdInt)) {
                                console.warn(`[JS] Email ${email.id} has non-numeric ID, trying to get email by message_id to load attachments`);
                                // Try to get email by message_id to get the real database ID
                                if (email.message_id) {
                                    try {
                                        const emailData = await TauriService.getDbEmail(email.message_id);
                                        if (emailData && emailData.id) {
                                            const realId = parseInt(emailData.id);
                                            if (!isNaN(realId)) {
                                                email.attachments = await TauriService.getAttachmentsForEmail(realId);
                                                console.log(`[JS] Email ${email.id} (message_id: ${email.message_id}, DB ID: ${realId}) has ${email.attachments.length} attachments`);
                                                return;
                                            }
                                        }
                                    } catch (e) {
                                        console.error(`[JS] Failed to get email by message_id ${email.message_id}:`, e);
                                    }
                                }
                                console.warn(`[JS] Email ${email.id} has non-numeric ID and couldn't resolve, cannot load attachments`);
                                email.attachments = [];
                                return;
                            }
                            
                            const timeoutPromise = new Promise((_, reject) => 
                                setTimeout(() => reject(new Error('Attachment load timeout')), 5000)
                            );
                            email.attachments = await Promise.race([
                                TauriService.getAttachmentsForEmail(emailIdInt),
                                timeoutPromise
                            ]);
                            console.log(`[JS] Email ${email.id} (DB ID: ${emailIdInt}) has ${email.attachments.length} attachments:`, email.attachments);
                        } catch (error) {
                            console.error(`[JS] Failed to load attachments for sent email ${email.id}:`, error);
                            email.attachments = [];
                        }
                    });
                    
                    await Promise.all(attachmentPromises);
                    
                    appState.setSentEmails(this.sentSearchResults);
                    await this.renderSentEmails(this.sentSearchHasMore);
                });
                
                // Reset search offset for new search
                if (!append) {
                    this.sentSearchOffset = 0;
                    this.sentSearchResults = [];
                }
                
                // Get page size from settings (already retrieved above)
                const pageSize = settings.emails_per_page || 50;
                
                console.log('[JS] Starting sent search with query:', searchQuery, 'limit:', pageSize, 'offset:', this.sentSearchOffset);
                // Start the search (this will emit events as results are found)
                const result = await TauriService.searchSentEmails(searchQuery, userEmail, pageSize, this.sentSearchOffset);
                console.log('[JS] Sent search result:', result);
                
                // Update offset for next page
                if (result && Array.isArray(result) && result.length === 2) {
                    const [count, hasMore] = result;
                    this.sentSearchHasMore = hasMore;
                    this.sentSearchOffset += count;
                }
                
            } catch (error) {
                console.error('Error searching sent emails:', error);
                notificationService.showError('Search failed: ' + error.message);
                
                // Clean up listeners on error
                if (this.sentSearchUnlisten) {
                    this.sentSearchUnlisten();
                    this.sentSearchUnlisten = null;
                }
                this.sentSearchInProgress = false;
                
                // Restore sent email list on error
                await this.loadSentEmails('');
            }
        }, 500); // 0.5 second delay for search
    }
    
    async updateSentSearchResults() {
        // Update UI with current sent search results incrementally
        if (this.sentSearchResults.length > 0 && this.sentSearchInProgress) {
            // Set emails and render incrementally
            appState.setSentEmails(this.sentSearchResults);
            await this.renderSentEmails();
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

    // ── Legacy JS decrypt functions removed — all decrypt now goes through backend ──
    // Removed: decryptManifestMessage, decryptManifestAttachment,
    //          decryptNostrMessageWithFallback, decryptSentManifestMessage,
    //          decryptNostrSentMessageWithFallback, decryptAllEncryptedBlocks,
    //          _decryptFromArmorParts, decodeGlossiaArmoredBody

    async renderEmails(showLoadMore = false, appendFrom = 0) {
        const emailList = domManager.get('emailList');
        if (!emailList) return;
        try {
            // Remove existing Load More button if it exists
            const existingLoadMoreBtn = document.getElementById('load-more-emails');
            if (existingLoadMoreBtn) {
                existingLoadMoreBtn.remove();
            }

            // Get all emails from state
            const allEmails = appState.getEmails();

            if (allEmails.length === 0) {
                emailList.innerHTML = '<div class="text-center text-muted">No emails found</div>';
                return;
            }

            // Check if we should hide unverified messages
            const settings = appState.getSettings();
            const hideUnverified = settings && settings.hide_unsigned_messages === true;

            // Only clear DOM on full re-render (not when appending)
            if (appendFrom <= 0) {
                emailList.innerHTML = '';
            }

            // Only render new emails when appending, all emails otherwise
            const emails = appendFrom > 0 ? allEmails.slice(appendFrom) : allEmails;

            console.log(`[JS] renderEmails: Rendering ${emails.length} inbox emails in parallel (appendFrom=${appendFrom})`);

            // Build contact index once for O(1) lookups across all emails
            this._buildContactIndex();

            // Filter emails for rendering
            const filteredEmails = emails.filter(email => {
                if (hideUnverified && email.signature_valid !== true) {
                    return false;
                }
                return true;
            });

            // Batch-decrypt uncached encrypted emails in a single IPC call
            const uncachedEncrypted = filteredEmails.filter(email => {
                if (this._previewCache.has(`inbox-${email.id}`)) return false;
                const firstBeginMatch = email.body && email.body.match(/-{3,}\s*BEGIN NOSTR ((?:NIP-\d+ ENCRYPTED|SIGNED) (?:MESSAGE|BODY))\s*-{3,}/);
                return firstBeginMatch && /NIP-\d+ ENCRYPTED/.test(firstBeginMatch[1]);
            });

            if (uncachedEncrypted.length > 0 && appState.getKeypair()) {
                try {
                    const batchInput = uncachedEncrypted.map(email => ({
                        id: String(email.id),
                        armorText: email.body,
                        subject: email.subject,
                        senderPubkey: email.sender_pubkey || email.nostr_pubkey || null,
                        recipientPubkey: null,
                    }));
                    console.log(`[JS] Batch decrypting ${batchInput.length} inbox emails in one IPC call`);
                    const batchResults = await TauriService.decryptEmailBodiesBatch(batchInput);

                    // Build a lookup from email ID to email object for side effects
                    const emailById = new Map(uncachedEncrypted.map(e => [String(e.id), e]));

                    for (const item of batchResults) {
                        const email = emailById.get(item.id);
                        if (item.result && item.result.success) {
                            let previewText = Utils.escapeHtml(item.result.body.substring(0, 100));
                            if (item.result.body.length > 100) previewText += '...';
                            this._previewCache.set(`inbox-${item.id}`, {
                                previewText,
                                previewSubject: item.result.subject,
                                showSubject: true,
                            });
                            // Fire-and-forget side effects: subject hash + pubkey backfill
                            if (item.result.subjectCiphertext && email && email.message_id) {
                                this.hashStringSHA256(item.result.subjectCiphertext).then(hash => {
                                    window.__TAURI__.core.invoke('db_update_email_subject_hash', {
                                        messageId: email.message_id,
                                        subjectHash: hash,
                                    }).catch(e => console.warn('[JS] Failed to update subject_hash:', e));
                                });
                            }
                            if (item.result.senderPubkey && email && !email.sender_pubkey && email.id) {
                                email.sender_pubkey = item.result.senderPubkey;
                                window.__TAURI__.core.invoke('db_update_email_sender_pubkey_by_id', {
                                    id: typeof email.id === 'number' ? email.id : parseInt(email.id, 10),
                                    senderPubkey: item.result.senderPubkey,
                                }).catch(e => console.warn('[JS] Failed to backfill sender_pubkey:', e));
                            }
                        } else {
                            // Cache the failure too so we don't re-attempt
                            this._previewCache.set(`inbox-${item.id}`, {
                                previewText: 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.',
                                previewSubject: email ? email.subject : '',
                                showSubject: false,
                            });
                        }
                    }
                } catch (e) {
                    console.error('[JS] Batch decryption failed, falling back to per-email:', e);
                    // Per-item renderers will decrypt individually as fallback
                }
            }

            // Process emails in parallel (decryption will hit cache from batch above)
            const emailPromises = filteredEmails
                .map(async (email) => {
                    try {
                        // Add timeout to prevent hanging on decryption
                        const timeoutPromise = new Promise((_, reject) =>
                            setTimeout(() => reject(new Error('Decryption timeout')), 5000)
                        );
                        const renderPromise = this.renderInboxEmailItem(email);
                        return await Promise.race([renderPromise, timeoutPromise]);
                    } catch (error) {
                        console.error(`[JS] Error rendering inbox email ${email.id}:`, error);
                        // Return a basic email item even if rendering fails
                        return this.renderInboxEmailItemBasic(email);
                    }
                });

            // Wait for all emails to be rendered (with timeout protection)
            const renderedItems = await Promise.allSettled(emailPromises);

            // Add all successfully rendered items to the list (filter out null results)
            let renderedCount = 0;
            for (const result of renderedItems) {
                if (result.status === 'fulfilled' && result.value) {
                    emailList.appendChild(result.value);
                    renderedCount++;
                }
            }

            // Show message if no emails were rendered (only on full render, not append)
            if (renderedCount === 0 && appendFrom <= 0) {
                const settings = appState.getSettings();
                const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
                const hideUnverified = settings && settings.hide_unsigned_messages === true;
                if (hideUndecryptable && allEmails.length > 0) {
                    emailList.innerHTML = '<div class="text-center text-muted">No decryptable emails found. All emails are encrypted for a different keypair.</div>';
                } else if (hideUnverified && allEmails.length > 0) {
                    emailList.innerHTML = '<div class="text-center text-muted">No verified emails found. All emails have missing or invalid signatures.</div>';
                } else {
                    emailList.innerHTML = '<div class="text-center text-muted">No emails found</div>';
                }
                return;
            }

            // Add Load More button if there might be more emails
            if (showLoadMore) {
                const loadMoreBtn = document.createElement('button');
                loadMoreBtn.id = 'load-more-emails';
                loadMoreBtn.className = 'btn btn-secondary';
                loadMoreBtn.style.cssText = 'width: 100%; margin-top: 15px; padding: 12px;';
                loadMoreBtn.innerHTML = '<i class="fas fa-chevron-down"></i> Load More';
                loadMoreBtn.addEventListener('click', () => this.loadMoreEmails());
                emailList.appendChild(loadMoreBtn);
            }

            console.log(`[JS] renderEmails: Successfully rendered ${renderedItems.filter(r => r.status === 'fulfilled').length} emails`);
        } catch (error) {
            console.error('Error rendering emails:', error);
        }
    }

    // Render a single inbox email item (with decryption, used in parallel rendering)
    async renderInboxEmailItem(email) {
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
        // Determine preview text — check cache first to avoid re-decrypting
        let previewText, showSubject, previewSubject;
        const cacheKey = `inbox-${email.id}`;
        const cached = this._previewCache.get(cacheKey);

        if (cached) {
            previewText = cached.previewText;
            showSubject = cached.showSubject;
            previewSubject = cached.previewSubject;
        } else {
            previewText = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
            showSubject = false;
            previewSubject = email.subject;

            // Check the FIRST/outermost BEGIN NOSTR block to determine message type.
            // Nested blocks (quoted replies) may be a different type — only the
            // outermost block determines whether this is an encrypted or signed message.
            const firstBeginMatch = email.body && email.body.match(/-{3,}\s*BEGIN NOSTR ((?:NIP-\d+ ENCRYPTED|SIGNED) (?:MESSAGE|BODY))\s*-{3,}/);
            const outerIsEncrypted = firstBeginMatch && /NIP-\d+ ENCRYPTED/.test(firstBeginMatch[1]);

            if (outerIsEncrypted) {
                const keypair = appState.getKeypair();
                if (!keypair) {
                    previewText = 'Unable to decrypt: no keypair';
                } else {
                    try {
                        const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                        const result = await TauriService.decryptEmailBody(
                            email.body, email.subject,
                            senderPubkey, null
                        );
                        if (result.success) {
                            previewSubject = result.subject;
                            previewText = Utils.escapeHtml(result.body.substring(0, 100));
                            if (result.body.length > 100) previewText += '...';
                            showSubject = true;
                        } else {
                            previewText = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                        }
                        // Update subject_hash for DM↔email matching
                        if (result.subjectCiphertext && email.message_id) {
                            this.hashStringSHA256(result.subjectCiphertext).then(hash => {
                                window.__TAURI__.core.invoke('db_update_email_subject_hash', {
                                    messageId: email.message_id,
                                    subjectHash: hash,
                                }).catch(e => console.warn('[JS] Failed to update subject_hash:', e));
                            });
                        }
                        // Backfill sender_pubkey from armor signature if header was missing
                        if (result.senderPubkey && !email.sender_pubkey && email.id) {
                            console.log('[JS] Backfilling sender_pubkey from armor:', result.senderPubkey.substring(0, 20) + '...');
                            email.sender_pubkey = result.senderPubkey;
                            window.__TAURI__.core.invoke('db_update_email_sender_pubkey_by_id', {
                                id: typeof email.id === 'number' ? email.id : parseInt(email.id, 10),
                                senderPubkey: result.senderPubkey,
                            }).catch(e => console.warn('[JS] Failed to backfill sender_pubkey:', e));
                        }
                    } catch (e) {
                        console.error('[JS] Backend decrypt failed for inbox preview:', e);
                        previewText = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                    }
                }
            } else {
                // Decode glossia signed message body for preview
                let previewBody = email.body || '';
                const signedMsg = await this.decodeGlossiaSignedMessage(previewBody);
                if (signedMsg && signedMsg.plaintextBody) {
                    previewBody = signedMsg.plaintextBody;
                }
                previewText = Utils.escapeHtml(previewBody ? previewBody.substring(0, 100) : '');
                if (previewBody && previewBody.length > 100) previewText += '...';
                showSubject = true;
            }

            // Cache the result for future re-renders
            this._previewCache.set(cacheKey, { previewText, previewSubject, showSubject });
        }

        // Check if email is decryptable (for filtering)
        // Only hide if the body cannot be decrypted (subject decryption failure is less critical)
        const isDecryptable = !previewText.includes('Unable to decrypt') &&
                            !previewText.includes('Your private key could not decrypt this message') &&
                            !previewText.includes('could not decrypt') &&
                            !previewText.includes('Could not decrypt') &&
                            previewText !== 'Unable to decrypt: no keypair';

        // Return null if we should hide undecryptable emails and this email can't be decrypted
        const settings = appState.getSettings();
        const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
        if (hideUndecryptable && !isDecryptable) {
            return null;
        }

        // Add attachment indicator (same style as sent emails)
        const attachmentCount = email.attachments ? email.attachments.length : 0;
        const attachmentIndicator = attachmentCount > 0 ?
            `<span class="attachment-indicator" title="${attachmentCount} attachment${attachmentCount > 1 ? 's' : ''}">📎 ${attachmentCount}</span>` : '';

        // Add signature verification indicator
        let signatureIndicator = '';
        const sigSource = email.signature_source ? ` (${email.signature_source})` : '';
        if (email.signature_valid === true) {
            signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature${sigSource}"><i class="fas fa-pen"></i> Signature Verified</span>`;
        } else if (email.signature_valid === false) {
            signatureIndicator = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Invalid Nostr signature"><i class="fas fa-pen"></i> Signature Invalid</span>`;
        }

        // Add transport authentication indicator
        let transportAuthIndicator = '';
        if (email.transport_auth_verified === true) {
            transportAuthIndicator = `<span class="transport-auth-indicator verified" title="Email transport authentication verified (DMARC/DKIM/SPF)"><i class="fas fa-envelope"></i> Email Verified</span>`;
        } else if (email.transport_auth_verified === false) {
            transportAuthIndicator = `<span class="transport-auth-indicator invalid" title="Email transport authentication failed"><i class="fas fa-envelope"></i> Email Unverified</span>`;
        }

        // Get sender contact for avatar (O(1) lookup via pre-built index)
        const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
        const senderContact = this._findContact(senderPubkey, email.from || email.from_address);

        // Avatar fallback logic (copied from dm-service.js)
        const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
        let avatarSrc = defaultAvatar;
        let avatarClass = 'contact-avatar';
        const isValidDataUrl = senderContact && senderContact.picture_data_url && senderContact.picture_data_url.startsWith('data:image') && senderContact.picture_data_url !== 'data:application/octet-stream;base64,';
        if (senderContact && senderContact.picture_loading) {
            avatarClass += ' loading';
        } else if (isValidDataUrl) {
            avatarSrc = senderContact.picture_data_url;
        } else if (senderContact && senderContact.picture_data_url && !isValidDataUrl && senderContact.picture) {
            avatarSrc = senderContact.picture;
        } else if (senderContact && senderContact.picture) {
            avatarSrc = senderContact.picture;
        }

        emailElement.innerHTML = `
            <img class="${avatarClass}" src="${avatarSrc}" alt="${Utils.escapeHtml(email.from)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
            <div class="email-content">
                <div class="email-header">
                    <div class="email-sender email-list-strong">${Utils.escapeHtml(email.from)} ${attachmentIndicator} ${signatureIndicator} ${transportAuthIndicator}</div>
                    <div class="email-date">${dateDisplay}${email.message_count > 1 ? `<span class="thread-badge" title="${email.message_count} messages">${email.message_count}</span>` : ''}</div>
                </div>
                ${showSubject ? `<div class="email-subject email-list-strong">${Utils.escapeHtml(previewSubject)}</div>` : ''}
                <div class="email-preview">${previewText}</div>
            </div>
            <div class="email-actions">
                <button class="btn btn-danger btn-small" onclick="event.stopPropagation(); emailService.deleteInboxEmailFromList('${Utils.escapeHtml(email.message_id || email.id)}')">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;

        // Add hover and click handlers for invalid signature indicator
        if (email.signature_valid === false) {
            const sigIndicator = emailElement.querySelector('.signature-indicator.invalid');
            if (sigIndicator) {
                const originalText = sigIndicator.textContent;
                sigIndicator.addEventListener('mouseenter', () => {
                    sigIndicator.textContent = 'recheck signature?';
                });
                sigIndicator.addEventListener('mouseleave', () => {
                    sigIndicator.textContent = originalText;
                });
                sigIndicator.addEventListener('click', async (e) => {
                    e.stopPropagation(); // Prevent email detail from opening
                    const messageId = sigIndicator.dataset.messageId;
                    if (messageId) {
                        sigIndicator.textContent = 'checking...';
                        sigIndicator.style.opacity = '0.7';
                        try {
                            const result = await TauriService.recheckEmailSignature(messageId);
                            if (result === true) {
                                // Update the email object
                                email.signature_valid = true;
                                // Re-render this email item
                                sigIndicator.className = 'signature-indicator verified';
                                sigIndicator.innerHTML = '<i class="fas fa-pen"></i> Signature Verified';
                                sigIndicator.title = 'Verified Nostr signature';
                                sigIndicator.removeAttribute('data-message-id');
                                // Remove hover handlers
                                sigIndicator.replaceWith(sigIndicator.cloneNode(true));
                                notificationService.showSuccess('Signature verified successfully!');
                            } else if (result === false) {
                                sigIndicator.textContent = originalText;
                                notificationService.showError('Signature is still invalid');
                            } else {
                                sigIndicator.textContent = originalText;
                                notificationService.showWarning('Could not verify signature (missing pubkey or signature)');
                            }
                        } catch (error) {
                            console.error('[JS] Failed to recheck signature:', error);
                            sigIndicator.textContent = originalText;
                            notificationService.showError('Failed to recheck signature: ' + error);
                        } finally {
                            sigIndicator.style.opacity = '1';
                        }
                    }
                });
            }
        }

        emailElement.addEventListener('click', () => {
            if (email.message_count > 1) {
                this.showThreadDetail(email.thread_id, 'inbox');
            } else {
                this.showEmailDetail(email.id);
            }
        });
        return emailElement;
    }

    // Render a basic inbox email item (without decryption, used as fallback on timeout)
    renderInboxEmailItemBasic(email) {
        const emailElement = document.createElement('div');
        emailElement.className = `email-item ${!email.is_read ? 'unread' : ''}`;
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

        const attachmentCount = email.attachments ? email.attachments.length : 0;
        const attachmentIndicator = attachmentCount > 0 ?
            `<span class="attachment-indicator" title="${attachmentCount} attachment${attachmentCount > 1 ? 's' : ''}">📎 ${attachmentCount}</span>` : '';

        // Get sender contact for avatar (O(1) lookup via pre-built index)
        const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
        const senderContact = this._findContact(senderPubkey, email.from || email.from_address);

        const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
        let avatarSrc = defaultAvatar;
        let avatarClass = 'contact-avatar';
        const isValidDataUrl = senderContact && senderContact.picture_data_url && senderContact.picture_data_url.startsWith('data:image') && senderContact.picture_data_url !== 'data:application/octet-stream;base64,';
        if (senderContact && senderContact.picture_loading) {
            avatarClass += ' loading';
        } else if (isValidDataUrl) {
            avatarSrc = senderContact.picture_data_url;
        } else if (senderContact && senderContact.picture_data_url && !isValidDataUrl && senderContact.picture) {
            avatarSrc = senderContact.picture;
        } else if (senderContact && senderContact.picture) {
            avatarSrc = senderContact.picture;
        }

        emailElement.innerHTML = `
            <img class="${avatarClass}" src="${avatarSrc}" alt="${Utils.escapeHtml(email.from)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
            <div class="email-content">
                <div class="email-header">
                    <div class="email-sender email-list-strong">${Utils.escapeHtml(email.from)} ${attachmentIndicator}</div>
                    <div class="email-date">${dateDisplay}</div>
                </div>
                <div class="email-subject email-list-strong">${Utils.escapeHtml(email.subject)}</div>
                <div class="email-preview">Loading...</div>
            </div>
            <div class="email-actions">
                <button class="btn btn-danger btn-small" onclick="event.stopPropagation(); emailService.deleteInboxEmailFromList('${Utils.escapeHtml(email.message_id || email.id)}')">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;

        emailElement.addEventListener('click', () => {
            if (email.message_count > 1) {
                this.showThreadDetail(email.thread_id, 'inbox');
            } else {
                this.showEmailDetail(email.id);
            }
        });
        return emailElement;
    }

    // Filter emails with debouncing
    async filterEmails(append = false) {
        const searchQuery = domManager.getValue('emailSearch')?.trim() || '';
        
        // If search query is empty, load all emails normally
        if (!searchQuery) {
            // Clean up any existing search listeners
            if (this.searchUnlisten) {
                this.searchUnlisten();
                this.searchUnlisten = null;
            }
            await this.loadEmails('');
            return;
        }
        
        // Clear existing timeout
        if (this.searchTimeout) {
            clearTimeout(this.searchTimeout);
        }
        
        // Clean up any existing search listeners
        if (this.searchUnlisten) {
            this.searchUnlisten();
            this.searchUnlisten = null;
        }
        
        // Set a new timeout to debounce the search
        this.searchTimeout = setTimeout(async () => {
            try {
                const settings = appState.getSettings();
                const userEmail = settings.email_address ? settings.email_address : null;

                // Initialize search results accumulator (or keep existing if appending)
                if (!append) {
                    this.searchResults = [];
                    this.searchOffset = 0;
                }
                this.searchInProgress = true;

                // Show searching state
                const emailList = domManager.get('emailList');
                if (emailList) {
                    emailList.innerHTML = '<div class="text-center text-muted" style="padding: 20px;"><i class="fas fa-spinner fa-spin"></i> Searching...</div>';
                }
                
                // Set up event listeners for incremental search results
                this.searchUnlisten = await window.__TAURI__.event.listen('email-search-result', (event) => {
                    const email = event.payload;
                    this.searchResults.push(email);
                    console.log(`[JS] Search result received: ${this.searchResults.length} emails found so far`);
                    
                    // Update UI incrementally
                    this.updateSearchResults();
                });
                
                // Listen for progress updates
                const progressUnlisten = await window.__TAURI__.event.listen('email-search-progress', (event) => {
                    const progress = event.payload;
                    const emailList = domManager.get('emailList');
                    if (emailList && this.searchInProgress) {
                        const percent = Math.round((progress.processed / progress.total) * 100);
                        emailList.innerHTML = `<div class="text-center text-muted" style="padding: 20px;">
                            <i class="fas fa-spinner fa-spin"></i> Searching... ${percent}% (${progress.processed}/${progress.total} emails processed)
                            ${this.searchResults.length > 0 ? `<br><small>Found ${this.searchResults.length} match${this.searchResults.length !== 1 ? 'es' : ''} so far</small>` : ''}
                        </div>`;
                    }
                });
                
                // Listen for search completion
                const completedUnlisten = await window.__TAURI__.event.listen('email-search-completed', async (event) => {
                    const completion = event.payload;
                    console.log(`[JS] Search completed: ${completion.total_found} total matches, has_more: ${completion.has_more}`);
                    
                    // Update has_more flag
                    if (completion.has_more !== undefined) {
                        this.searchHasMore = completion.has_more;
                    }
                    
                    this.searchInProgress = false;
                    
                    // Clean up listeners
                    if (this.searchUnlisten) {
                        this.searchUnlisten();
                        this.searchUnlisten = null;
                    }
                    progressUnlisten();
                    completedUnlisten();
                    
                    // Load attachments for all search results (same logic as normal email loading)
                    console.log(`[JS] Loading attachments for ${this.searchResults.length} search result emails`);
                    const attachmentPromises = this.searchResults.map(async (email) => {
                        try {
                            // Convert email.id to integer - it might be a string
                            const emailIdInt = parseInt(email.id);
                            if (isNaN(emailIdInt)) {
                                console.warn(`[JS] Email ${email.id} has non-numeric ID, trying to get email by message_id to load attachments`);
                                // Try to get email by message_id to get the real database ID
                                if (email.message_id) {
                                    try {
                                        const emailData = await TauriService.getDbEmail(email.message_id);
                                        if (emailData && emailData.id) {
                                            const realId = parseInt(emailData.id);
                                            if (!isNaN(realId)) {
                                                email.attachments = await TauriService.getAttachmentsForEmail(realId);
                                                console.log(`[JS] Email ${email.id} (message_id: ${email.message_id}, DB ID: ${realId}) has ${email.attachments.length} attachments`);
                                                return;
                                            }
                                        }
                                    } catch (e) {
                                        console.error(`[JS] Failed to get email by message_id ${email.message_id}:`, e);
                                    }
                                }
                                console.warn(`[JS] Email ${email.id} has non-numeric ID and couldn't resolve, cannot load attachments`);
                                email.attachments = [];
                                return;
                            }
                            
                            const timeoutPromise = new Promise((_, reject) => 
                                setTimeout(() => reject(new Error('Attachment load timeout')), 5000)
                            );
                            email.attachments = await Promise.race([
                                TauriService.getAttachmentsForEmail(emailIdInt),
                                timeoutPromise
                            ]);
                            console.log(`[JS] Email ${email.id} (DB ID: ${emailIdInt}) has ${email.attachments.length} attachments:`, email.attachments);
                        } catch (error) {
                            console.error(`[JS] Failed to load attachments for email ${email.id}:`, error);
                            email.attachments = [];
                        }
                    });
                    
                    await Promise.all(attachmentPromises);
                    
                    appState.setEmails(this.searchResults);
                    await this.renderEmails(this.searchHasMore);
                });
                
                // Reset search offset for new search
                if (!append) {
                    this.searchOffset = 0;
                    this.searchResults = [];
                }
                
                // Get page size from settings (already retrieved above)
                const pageSize = settings.emails_per_page || 50;
                
                console.log('[JS] Starting search with query:', searchQuery, 'limit:', pageSize, 'offset:', this.searchOffset);
                // Start the search (this will emit events as results are found)
                const result = await TauriService.searchEmails(searchQuery, userEmail, pageSize, this.searchOffset);
                console.log('[JS] Search result:', result);
                
                // Update offset for next page
                if (result && Array.isArray(result) && result.length === 2) {
                    const [count, hasMore] = result;
                    this.searchHasMore = hasMore;
                    this.searchOffset += count;
                }
                
            } catch (error) {
                console.error('Error searching emails:', error);
                notificationService.showError('Search failed: ' + error.message);
                
                // Clean up listeners on error
                if (this.searchUnlisten) {
                    this.searchUnlisten();
                    this.searchUnlisten = null;
                }
                this.searchInProgress = false;
                
                // Restore email list on error
                await this.loadEmails('');
            }
        }, 500); // 0.5 second delay for search
    }
    
    async updateSearchResults() {
        // Update UI with current search results incrementally
        if (this.searchResults.length > 0 && this.searchInProgress) {
            // Set emails and render incrementally
            appState.setEmails(this.searchResults);
            await this.renderEmails();
        }
    }

    // Show email detail
    showEmailDetail(emailId) {
        try {
            const email = appState.getEmails().find(e => e.id === emailId);
            if (!email) return;
            
            // Mark email as read if it's unread
            if (!email.is_read && email.message_id) {
                (async () => {
                    try {
                        await this.markAsRead(email.message_id);
                        // Update the email object in appState
                        email.is_read = true;
                        // Update the UI to remove unread indicator
                        const emailElement = document.querySelector(`[data-email-id="${emailId}"]`);
                        if (emailElement) {
                            emailElement.classList.remove('unread');
                        }
                    } catch (error) {
                        console.error('[JS] Failed to mark email as read:', error);
                    }
                })();
            }
            
            const emailList = domManager.get('emailList');
            const emailDetailView = document.getElementById('email-detail-view');
            const emailDetailHeader = emailDetailView ? emailDetailView.querySelector('.email-detail-header') : null;
            const inboxActions = document.getElementById('inbox-actions');
            const inboxTitle = document.getElementById('inbox-title');
            if (emailList) emailList.style.display = 'none';
            if (emailDetailView) emailDetailView.style.display = 'flex';
            if (inboxActions) inboxActions.style.display = 'none';
            if (inboxTitle) inboxTitle.style.display = 'none';
            const emailDetailContent = document.getElementById('email-detail-content');
            if (emailDetailContent) {
                // Use email.body directly instead of cleanedBody to preserve encrypted message format
                const emailBody = email.body || '';
                const senderPubkey = email.sender_pubkey || email.nostr_pubkey; // Fallback for backward compatibility
                console.log('Sender pubkey for email:', senderPubkey);
                // Detect encrypted body via armor regex
                const encryptedBodyMatch = emailBody.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}/);
                let decryptedSubject = email.subject;
                let decryptedBody = emailBody;
                const keypair = appState.getKeypair();

                if (encryptedBodyMatch && keypair) {
                    // ── Backend decrypt path ──
                    (async () => {
                        try {
                            // Run decryption and signature verification in parallel (they're independent)
                            console.log('[JS] Calling backend decrypt_email_body + verifyAllSignatures in parallel...');
                            const [result, allSigs] = await Promise.all([
                                TauriService.decryptEmailBody(emailBody, email.subject, senderPubkey, null),
                                TauriService.verifyAllSignatures(emailBody).catch(e => {
                                    console.warn('[JS] Signature verification error:', e);
                                    return [];
                                })
                            ]);
                            const sigResults = allSigs.length > 0 ? allSigs : null;
                            console.log('[JS] Backend decrypt result: success=', result.success, 'isManifest=', result.isManifest, 'blocks=', result.blockResults?.length);

                            decryptedSubject = result.subject;
                            decryptedBody = result.success ? result.body : emailBody;

                            if (!result.success) {
                                decryptedSubject = 'Unable to decrypt';
                                decryptedBody = result.error || 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                            }

                            // Map backend manifest attachments to the format updateDetail expects
                            let manifestResult = null;
                            if (result.isManifest && result.attachments && result.attachments.length > 0) {
                                manifestResult = {
                                    type: 'manifest',
                                    manifest: {
                                        attachments: result.attachments.map(a => ({
                                            id: a.id,
                                            orig_filename: a.origFilename,
                                            orig_mime: a.origMime,
                                            key_wrap: a.keyWrapB64,
                                            cipher_sha256: a.cipherSha256Hex,
                                            cipher_size: a.cipherSize,
                                        }))
                                    }
                                };
                            }

                            // Map block results for lock/unlock icons (same shape as decryptAllEncryptedBlocks)
                            let decryptResults = null;
                            if (result.blockResults && result.blockResults.length > 0) {
                                decryptResults = result.blockResults.map(b => {
                                    if (!b.wasEncrypted) return null;
                                    if (b.decryptedText != null) return { decryptedText: b.decryptedText };
                                    if (b.error) return { error: b.error };
                                    return null;
                                });
                            }

                            // Backfill sender_pubkey from armor signature if header was missing
                            if (result.senderPubkey && !email.sender_pubkey && email.id) {
                                console.log('[JS] Backfilling sender_pubkey from armor (detail):', result.senderPubkey.substring(0, 20) + '...');
                                email.sender_pubkey = result.senderPubkey;
                                window.__TAURI__.core.invoke('db_update_email_sender_pubkey_by_id', {
                                    id: typeof email.id === 'number' ? email.id : parseInt(email.id, 10),
                                    senderPubkey: result.senderPubkey,
                                }).catch(e => console.warn('[JS] Failed to backfill sender_pubkey:', e));
                            }

                            await updateDetail(decryptedSubject, decryptedBody, manifestResult, result.success, sigResults, decryptResults);
                        } catch (err) {
                            console.error('[JS] Backend decrypt_email_body error:', err);
                            await updateDetail('Unable to decrypt', 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.', null, true);
                        }
                    })();
                } else {
                    (async () => {
                        // Run signature verification and glossia decode in parallel (they're independent)
                        const [allSigs, signedMsg] = await Promise.all([
                            TauriService.verifyAllSignatures(emailBody).catch(e => {
                                console.warn('[JS] Signature verification error:', e);
                                return [];
                            }),
                            this.decodeGlossiaSignedMessage(emailBody)
                        ]);
                        const sigResults = allSigs.length > 0 ? allSigs : null;
                        let displayBody = decryptedBody;
                        if (signedMsg && signedMsg.plaintextBody) {
                            displayBody = signedMsg.plaintextBody;
                        }
                        await updateDetail(decryptedSubject, displayBody, null, false, sigResults);
                    })();
                }
                const updateDetail = async (subject, body, cachedManifestResult, wasDecrypted = false, inlineSigResult = null, decryptResults = null) => {
                    // Render attachments - decrypt metadata for display
                    console.log(`[JS] Rendering detail for inbox email ${email.id}, attachments:`, email.attachments);
                    
                    // Ensure attachments array exists
                    if (!email.attachments || !Array.isArray(email.attachments)) {
                        console.warn(`[JS] Email ${email.id} has no attachments array, initializing empty array`);
                        email.attachments = [];
                    }
                    
                    let attachmentsHtml = '';
                    if (email.attachments && email.attachments.length > 0) {
                        // For manifest-encrypted emails, we need to decrypt the manifest to get original metadata
                        let attachmentDisplayData = [];
                        
                        const hasManifestAttachments = email.attachments.some(att => att.encryption_method === 'manifest_aes');
                        const hasValidManifest = cachedManifestResult && 
                                               cachedManifestResult.type === 'manifest' && 
                                               cachedManifestResult.manifest && 
                                               cachedManifestResult.manifest.attachments &&
                                               cachedManifestResult.manifest.attachments.length > 0;
                        
                        if (hasManifestAttachments || hasValidManifest) {
                            try {
                                let manifestResult = cachedManifestResult;
                                
                                if (!manifestResult || manifestResult.type !== 'manifest') {
                                    // Re-decrypt via backend to extract manifest
                                    const keypair = appState.getKeypair();
                                    if (keypair && email.body && email.body.includes('BEGIN NOSTR')) {
                                        const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                                        const decryptResult = await TauriService.decryptEmailBody(
                                            email.body, email.subject || '',
                                            senderPubkey, null
                                        );
                                        if (decryptResult.isManifest && decryptResult.attachments && decryptResult.attachments.length > 0) {
                                            manifestResult = {
                                                type: 'manifest',
                                                manifest: {
                                                    attachments: decryptResult.attachments.map(a => ({
                                                        id: a.id,
                                                        orig_filename: a.origFilename,
                                                        orig_mime: a.origMime,
                                                        key_wrap: a.keyWrapB64,
                                                        cipher_sha256: a.cipherSha256Hex,
                                                        cipher_size: a.cipherSize,
                                                    }))
                                                }
                                            };
                                        }
                                    }
                                }

                                if (manifestResult && manifestResult.type === 'manifest' && manifestResult.manifest && manifestResult.manifest.attachments) {
                                    attachmentDisplayData = email.attachments.map(dbAttachment => {
                                        const opaqueId = dbAttachment.filename.replace(/\.dat$/, '');
                                        const manifestAttachment = manifestResult.manifest.attachments.find(ma => ma.id === opaqueId);
                                        
                                        if (manifestAttachment) {
                                            return {
                                                ...dbAttachment,
                                                displayName: manifestAttachment.orig_filename,
                                                encryptedFilename: dbAttachment.filename,
                                                displaySize: manifestAttachment.orig_size || dbAttachment.size,
                                                encryptedSize: manifestAttachment.cipher_size || dbAttachment.size,
                                                displayMime: manifestAttachment.orig_mime || dbAttachment.content_type || dbAttachment.mime_type
                                            };
                                        } else {
                                            return {
                                                ...dbAttachment,
                                                displayName: dbAttachment.filename,
                                                encryptedFilename: dbAttachment.filename,
                                                displaySize: dbAttachment.size,
                                                encryptedSize: dbAttachment.size,
                                                displayMime: dbAttachment.content_type || dbAttachment.mime_type
                                            };
                                        }
                                    });
                                } else {
                                    attachmentDisplayData = email.attachments.map(att => ({
                                        ...att,
                                        displayName: att.filename,
                                        encryptedFilename: att.filename,
                                        displaySize: att.size,
                                        encryptedSize: att.size,
                                        displayMime: att.mime_type
                                    }));
                                }
                            } catch (error) {
                                console.error('Failed to decrypt manifest for attachment display:', error);
                                attachmentDisplayData = email.attachments.map(att => ({
                                    ...att,
                                    displayName: att.filename,
                                    encryptedFilename: att.filename,
                                    displaySize: att.size,
                                    encryptedSize: att.size,
                                    displayMime: att.mime_type
                                }));
                            }
                        } else {
                            attachmentDisplayData = email.attachments.map(att => ({
                                ...att,
                                displayName: att.filename,
                                encryptedFilename: att.filename,
                                displaySize: att.size,
                                encryptedSize: att.size,
                                displayMime: att.mime_type
                            }));
                        }
                        
                        attachmentsHtml = `
                        <div class="email-attachments" id="inbox-email-attachments" style="margin: 15px 0;">
                            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;">
                                <h4>Attachments (${attachmentDisplayData.length})</h4>
                                <button class="btn btn-sm btn-outline-success" onclick="window.emailService.downloadAllInboxAttachments(${email.id})" title="Download all attachments as ZIP">
                                    <i class="fas fa-download"></i> Download All
                                </button>
                            </div>
                            <div class="attachment-list">
                                ${attachmentDisplayData.map(attachment => {
                                    const sizeFormatted = (attachment.displaySize / 1024).toFixed(2) + ' KB';
                                    const isEncrypted = attachment.encryption_method === 'manifest_aes';
                                    // Default to display mode (decrypted/unlocked)
                                    const statusIcon = isEncrypted ? '🔓' : '📄';
                                    const statusText = isEncrypted ? 'Decrypted' : 'Plain';
                                    const encryptedFilename = attachment.encryptedFilename || attachment.filename;
                                    const decryptedFilename = attachment.displayName;
                                    
                                    return `
                                    <div class="attachment-item" style="display: flex; justify-content: space-between; align-items: center; padding: 10px; border: 1px solid #ddd; border-radius: 4px; margin: 5px 0;" 
                                         data-encrypted-filename="${Utils.escapeHtml(encryptedFilename)}" 
                                         data-decrypted-filename="${Utils.escapeHtml(decryptedFilename)}"
                                         data-encrypted-size="${attachment.encryptedSize || attachment.displaySize}"
                                         data-decrypted-size="${attachment.displaySize}"
                                         data-is-encrypted="${isEncrypted}">
                                        <div class="attachment-info" style="display: flex; align-items: center;">
                                            <i class="fas fa-file" style="margin-right: 10px;"></i>
                                            <div class="attachment-details">
                                                <div class="attachment-name" style="font-weight: bold;" data-display-name="${Utils.escapeHtml(decryptedFilename)}">${Utils.escapeHtml(decryptedFilename)}</div>
                                                <div class="attachment-meta" style="font-size: 0.9em; color: #666;">
                                                    <span class="attachment-size">${sizeFormatted}</span> • <span class="attachment-status-icon">${statusIcon}</span> <span class="attachment-status-text">${statusText}</span>
                                                </div>
                                            </div>
                                        </div>
                                        <div class="attachment-actions">
                                            <button class="btn btn-sm btn-outline-primary" onclick="window.emailService.downloadInboxAttachment(${email.id}, ${attachment.id}, '${Utils.escapeHtml(encryptedFilename)}')">
                                                <i class="fas fa-download"></i> Download
                                            </button>
                                        </div>
                                    </div>`;
                                }).join('')}
                            </div>
                        </div>`;
                    }
                    
                    // Signature indicator — icon only in header, full text in details panel
                    const outerSigResult = Array.isArray(inlineSigResult) ? inlineSigResult[inlineSigResult.length - 1] : inlineSigResult;
                    let signatureIcon = '';
                    let securityRows = '';
                    if (outerSigResult && outerSigResult.isValid === true) {
                        signatureIcon = `<span class="signature-indicator verified" title="Signature Verified"><i class="fas fa-check-circle"></i></span>`;
                        securityRows += `<div class="security-row verified"><i class="fas fa-check-circle"></i> Signature Verified</div>`;
                    } else if (outerSigResult && outerSigResult.isValid === false) {
                        signatureIcon = `<span class="signature-indicator invalid" title="Signature Invalid"><i class="fas fa-times-circle"></i></span>`;
                        securityRows += `<div class="security-row invalid"><i class="fas fa-times-circle"></i> Signature Invalid</div>`;
                    } else if (email.signature_valid === true) {
                        signatureIcon = `<span class="signature-indicator verified" title="Signature Verified"><i class="fas fa-check-circle"></i></span>`;
                        securityRows += `<div class="security-row verified"><i class="fas fa-check-circle"></i> Signature Verified</div>`;
                    } else if (email.signature_valid === false) {
                        signatureIcon = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Signature Invalid"><i class="fas fa-times-circle"></i></span>`;
                        securityRows += `<div class="security-row invalid"><i class="fas fa-times-circle"></i> Signature Invalid</div>`;
                    }
                    if (email.transport_auth_verified === true) {
                        securityRows += `<div class="security-row verified"><i class="fas fa-envelope"></i> Email Transport Verified</div>`;
                    }

                    // Get sender info for header (O(1) lookup via contact index)
                    const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                    if (!this._contactsByPubkey) this._buildContactIndex();
                    const senderContact = this._findContact(senderPubkey, email.from || email.from_address);
                    
                    // Avatar logic (same as dm-service.js)
                    const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
                    let avatarSrc = defaultAvatar;
                    const isValidDataUrl = senderContact && senderContact.picture_data_url && senderContact.picture_data_url.startsWith('data:image') && senderContact.picture_data_url !== 'data:application/octet-stream;base64,';
                    if (senderContact && !senderContact.picture_loading) {
                        if (isValidDataUrl) {
                            avatarSrc = senderContact.picture_data_url;
                        } else if (senderContact.picture) {
                            avatarSrc = senderContact.picture;
                        }
                    }
                    
                    const senderName = senderContact ? (senderContact.name || senderContact.display_name || email.from) : email.from;
                    const timeAgo = Utils.formatTimeAgo(new Date(email.date));
                    
                    // Update page header (back button only — reply moved to card)
                    if (emailDetailHeader) {
                        emailDetailHeader.innerHTML = `
                            <button id="back-to-inbox" class="btn btn-secondary">
                                <i class="fas fa-arrow-left"></i> Back to Inbox
                            </button>
                        `;
                        const backBtn = emailDetailHeader.querySelector('#back-to-inbox');
                        if (backBtn) {
                            backBtn.addEventListener('click', () => {
                                if (emailList) emailList.style.display = 'block';
                                if (emailDetailView) emailDetailView.style.display = 'none';
                                if (inboxActions) inboxActions.style.display = 'flex';
                                if (inboxTitle) {
                                    inboxTitle.textContent = 'Inbox';
                                    inboxTitle.style.display = '';
                                }
                            });
                        }
                    }
                    
                    emailDetailContent.innerHTML =
                        `<div class="email-detail">
<h2 class="email-detail-subject">${Utils.escapeHtml(subject)}</h2>
<div class="email-detail-card">
<div class="email-sender-header">
<div class="email-sender-row">
<img class="email-sender-avatar" src="${avatarSrc}" alt="${Utils.escapeHtml(senderName)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='email-sender-avatar';">
<div class="email-sender-info">
<div class="email-sender-name-row">
<div class="email-sender-name">${Utils.escapeHtml(senderName)}</div>
${signatureIcon}
<div class="email-sender-time">${Utils.escapeHtml(timeAgo)}</div>
</div>
<details class="email-metadata-details">
<summary class="email-metadata-summary">to ${Utils.escapeHtml(email.to)}</summary>
<div class="email-header-panel" id="inbox-email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
${securityRows ? `<hr><div class="email-security-info">${securityRows}</div>` : ''}
</div>
</details>
</div>
<div class="thread-card-actions">
<button class="thread-action-btn thread-reply-btn" title="Reply"><i class="fas fa-reply"></i></button>
<div class="thread-more-menu">
<button class="thread-action-btn thread-more-btn" title="More"><i class="fas fa-ellipsis-v"></i></button>
<div class="thread-more-dropdown">
<button class="thread-menu-item thread-raw-toggle">Show Raw</button>
</div>
</div>
</div>
</div>
</div>
<pre id="inbox-raw-header-info" class="email-raw-content">${Utils.escapeHtml(email.raw_headers || '')}</pre>
<div class="email-detail-body" id="inbox-email-body-info">${email.html_body ? '' : Utils.escapeHtml(body).replace(/\n/g, '<br>')}</div>
<pre id="inbox-raw-body-info" class="email-raw-content email-raw-body">${Utils.escapeHtml(email.raw_body)}${email.html_body ? '\n\n--- text/html ---\n\n' + Utils.escapeHtml(email.html_body) : ''}</pre>
${attachmentsHtml}
</div>
</div>`;
                    if (email.html_body) {
                        let htmlToRender = email.html_body;
                        if (inlineSigResult) htmlToRender = this.injectHtmlSigBadge(htmlToRender, inlineSigResult);
                        Utils.renderHtmlBodyInIframe('inbox-email-body-info', htmlToRender, { decryptedTexts: decryptResults, startDecrypted: true });
                    }
                    // Decorate and verify inline signature blocks in the body
                    if (!email.html_body) {
                        Utils.decorateArmorBlocks('inbox-email-body-info');
                        this.verifyAndAnnotateSignatureBlocks(body, 'inbox-email-body-info');
                    }
                    // Wire up reply and three-dot menu in card header
                    const inboxReplyBtn = emailDetailContent.querySelector('.thread-reply-btn');
                    if (inboxReplyBtn) {
                        inboxReplyBtn.addEventListener('click', () => {
                            this.replyToEmail(email, subject, body);
                        });
                    }
                    const inboxMoreBtn = emailDetailContent.querySelector('.thread-more-btn');
                    const inboxMoreDropdown = emailDetailContent.querySelector('.thread-more-dropdown');
                    if (inboxMoreBtn && inboxMoreDropdown) {
                        inboxMoreBtn.addEventListener('click', (e) => {
                            e.stopPropagation();
                            inboxMoreDropdown.classList.toggle('open');
                        });
                        document.addEventListener('click', () => inboxMoreDropdown.classList.remove('open'));
                    }

                    const headerInfo = document.getElementById('inbox-email-header-info');
                    const rawHeaderInfo = document.getElementById('inbox-raw-header-info');
                    const bodyInfo = document.getElementById('inbox-email-body-info');
                    const rawBodyInfo = document.getElementById('inbox-raw-body-info');
                    const attachmentsInfo = document.getElementById('inbox-email-attachments');

                    const inboxRawToggle = emailDetailContent.querySelector('.thread-raw-toggle');
                    if (inboxRawToggle && rawHeaderInfo && rawBodyInfo && bodyInfo) {
                        let showingRaw = false;
                        inboxRawToggle.addEventListener('click', () => {
                            inboxMoreDropdown.classList.remove('open');
                            showingRaw = !showingRaw;
                            if (showingRaw) {
                                rawHeaderInfo.style.display = 'block';
                                rawBodyInfo.style.display = 'block';
                                bodyInfo.style.display = 'none';
                                inboxRawToggle.textContent = 'Hide Raw';
                                if (attachmentsInfo) {
                                    attachmentsInfo.querySelectorAll('.attachment-item').forEach(item => {
                                        const ef = item.getAttribute('data-encrypted-filename');
                                        const es = item.getAttribute('data-encrypted-size');
                                        const enc = item.getAttribute('data-is-encrypted') === 'true';
                                        if (ef) item.querySelector('.attachment-name').textContent = ef;
                                        if (es) item.querySelector('.attachment-size').textContent = (parseFloat(es) / 1024).toFixed(2) + ' KB';
                                        if (enc) { item.querySelector('.attachment-status-icon').textContent = '\u{1F512}'; item.querySelector('.attachment-status-text').textContent = 'Encrypted'; }
                                    });
                                }
                            } else {
                                rawHeaderInfo.style.display = 'none';
                                rawBodyInfo.style.display = 'none';
                                bodyInfo.style.display = '';
                                inboxRawToggle.textContent = 'Show Raw';
                                if (attachmentsInfo) {
                                    attachmentsInfo.querySelectorAll('.attachment-item').forEach(item => {
                                        const df = item.getAttribute('data-decrypted-filename');
                                        const ds = item.getAttribute('data-decrypted-size');
                                        const enc = item.getAttribute('data-is-encrypted') === 'true';
                                        if (df) item.querySelector('.attachment-name').textContent = df;
                                        if (ds) item.querySelector('.attachment-size').textContent = (parseFloat(ds) / 1024).toFixed(2) + ' KB';
                                        if (enc) { item.querySelector('.attachment-status-icon').textContent = '\u{1F513}'; item.querySelector('.attachment-status-text').textContent = 'Decrypted'; }
                                    });
                                }
                            }
                        });
                    }
                    
                    // Add event listeners for invalid signature indicator in sender header
                    if (email.signature_valid === false) {
                        const sigIndicator = emailDetailContent.querySelector('.email-sender-header .signature-indicator.invalid');
                        if (sigIndicator) {
                            const originalText = sigIndicator.textContent;
                            sigIndicator.addEventListener('mouseenter', () => {
                                sigIndicator.textContent = 'recheck signature?';
                            });
                            sigIndicator.addEventListener('mouseleave', () => {
                                sigIndicator.textContent = originalText;
                            });
                            sigIndicator.addEventListener('click', async (e) => {
                                e.stopPropagation();
                                const messageId = sigIndicator.dataset.messageId;
                                if (messageId) {
                                    sigIndicator.textContent = 'checking...';
                                    sigIndicator.style.opacity = '0.7';
                                    try {
                                        const result = await TauriService.recheckEmailSignature(messageId);
                                        if (result === true) {
                                            email.signature_valid = true;
                                            sigIndicator.className = 'signature-indicator verified';
                                            sigIndicator.innerHTML = '<i class="fas fa-pen"></i> Signature Verified';
                                            sigIndicator.title = 'Verified Nostr signature';
                                            sigIndicator.removeAttribute('data-message-id');
                                            sigIndicator.replaceWith(sigIndicator.cloneNode(true));
                                            notificationService.showSuccess('Signature verified successfully!');
                                        } else if (result === false) {
                                            sigIndicator.textContent = originalText;
                                            notificationService.showError('Signature is still invalid.');
                                        } else {
                                            sigIndicator.textContent = originalText;
                                            notificationService.showError('Could not verify signature.');
                                        }
                                    } catch (error) {
                                        console.error('[JS] Failed to recheck signature:', error);
                                        sigIndicator.textContent = originalText;
                                        notificationService.showError('Failed to recheck signature: ' + error);
                                    } finally {
                                        sigIndicator.style.opacity = '1';
                                    }
                                }
                            });
                        }
                    }
                    
                }
            }
        } catch (error) {
            console.error('Error showing email detail:', error);
        }
    }

    // Reply to email - navigate to compose with pre-filled fields
    replyToEmail(email, decryptedSubject, decryptedBody) {
        try {
            // Prefer Reply-To header over From address
            let replyTo = email.from || '';
            if (email.raw_headers) {
                const match = email.raw_headers.match(/^Reply-To:\s*(.+)$/mi);
                if (match) {
                    // Extract bare email from "Name <email>" or plain "email" format
                    const angleMatch = match[1].trim().match(/<([^>]+)>/);
                    replyTo = angleMatch ? angleMatch[1] : match[1].trim();
                }
            }
            
            // Format the subject with "Re: " prefix (avoid duplicate "Re:")
            let replySubject = decryptedSubject || email.subject || '';
            if (!replySubject.startsWith('Re:') && !replySubject.startsWith('re:')) {
                replySubject = 'Re: ' + replySubject;
            }
            
            // Format the body with quoted original message
            // The quoted armor is visible in the textarea while composing.
            // On sign/encrypt, quoted armor is placed inside the outer armor block
            // before the SIGNATURE, and the signature covers both reply + quoted bytes.
            const originalBody = email.body || '';
            let replyBody = '';

            if (originalBody.trim()) {
                const armorIdx = originalBody.search(/-{3,}\s*BEGIN NOSTR /);
                if (armorIdx > 0) {
                    // Quote only the plaintext prefix; leave armor flat
                    const plainPrefix = originalBody.substring(0, armorIdx).trim();
                    const armorPart = originalBody.substring(armorIdx).trim();
                    const quotedPlain = plainPrefix.split('\n').map(l => '> ' + l).join('\n');
                    replyBody = '\n\n' + quotedPlain + '\n\n' + armorPart;
                } else if (armorIdx === 0) {
                    // All armor, no plaintext prefix
                    replyBody = '\n\n' + originalBody;
                } else {
                    // No armor — quote everything
                    const quotedBody = originalBody.split('\n').map(l => '> ' + l).join('\n');
                    replyBody = '\n\n' + quotedBody;
                }
            }
            
            // Navigate to compose tab
            if (window.app && window.app.switchTab) {
                window.app.switchTab('compose');
            } else {
                // Fallback: click the compose tab button
                const composeTab = document.querySelector('[data-tab="compose"]');
                if (composeTab) {
                    composeTab.click();
                }
            }
            
            // Wait a moment for the tab to switch, then fill in the form fields
            setTimeout(() => {
                // Reset compose state from any previous compose session
                this.resetEncryptButtonState();
                this.clearSignature();
                this._quotedOriginalArmor = null;

                // Capture threading headers for In-Reply-To and References
                this._replyToMessageId = null;
                this._replyReferences = null;
                if (email.message_id) {
                    const msgId = email.message_id.includes('@')
                        ? `<${email.message_id}>` : email.message_id;
                    this._replyToMessageId = msgId;
                    // Build References: original's References + original's Message-ID
                    let refs = '';
                    if (email.raw_headers) {
                        const refMatch = email.raw_headers.match(/^References:\s*(.+(?:\n\s+.+)*)$/mi);
                        if (refMatch) refs = refMatch[1].replace(/\n\s+/g, ' ').trim();
                    }
                    this._replyReferences = refs ? `${refs} ${msgId}` : msgId;
                }

                domManager.setValue('toAddress', replyTo);
                domManager.setValue('subject', replySubject);
                domManager.setValue('messageBody', replyBody);
                
                // Get the sender's pubkey (recipient when replying)
                let senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                const pubkeyDisplay = document.getElementById('selected-recipient-pubkey');
                const pubkeyValue = document.getElementById('recipient-pubkey-value');
                
                // If no pubkey in email, try to look it up by email address
                if (!senderPubkey && replyTo) {
                    try {
                        if (!this._contactsByPubkey) this._buildContactIndex();
                        const senderContact = this._findContact(null, replyTo);
                        if (senderContact && senderContact.pubkey) {
                            senderPubkey = senderContact.pubkey;
                        }
                    } catch (e) {
                        console.log('[JS] Could not look up pubkey by email:', e);
                    }
                }

                if (senderPubkey) {
                    // Try to find the contact by pubkey
                    if (!this._contactsByPubkey) this._buildContactIndex();
                    const senderContact = this._findContact(senderPubkey, null);
                    
                    if (senderContact) {
                        // Set as selected Nostr contact
                        this.selectedNostrContact = senderContact;
                        domManager.setValue('nostrContactSelect', senderPubkey);
                        
                        // Display the recipient pubkey
                        if (pubkeyDisplay && pubkeyValue) {
                            pubkeyValue.value = senderPubkey;
                            // pubkey field is always visible
                        }
                        
                        // Style the toAddress input for Nostr encryption
                        const toAddressInput = domManager.get('toAddress');
                        if (toAddressInput) {
                            toAddressInput.style.borderColor = '#667eea';
                            toAddressInput.style.backgroundColor = this.getNostrContactInputBackgroundColor();
                        }
                    } else {
                        // Pubkey exists but contact not found - still display it
                        this.selectedNostrContact = null;
                        domManager.setValue('nostrContactSelect', '');
                        
                        // Display the recipient pubkey even if contact not found
                        if (pubkeyDisplay && pubkeyValue) {
                            pubkeyValue.value = senderPubkey;
                            // pubkey field is always visible
                        }
                        
                        // Style the toAddress input for Nostr encryption
                        const toAddressInput = domManager.get('toAddress');
                        if (toAddressInput) {
                            toAddressInput.style.borderColor = '#667eea';
                            toAddressInput.style.backgroundColor = this.getNostrContactInputBackgroundColor();
                        }
                    }
                } else {
                    // No pubkey available - clear any selected Nostr contact
                    domManager.setValue('nostrContactSelect', '');
                    this.selectedNostrContact = null;
                    
                    // Hide the pubkey display
                    if (pubkeyDisplay) {
                        // pubkey field is always visible
                    }
                    
                    // Reset the toAddress input styling
                    const toAddressInput = domManager.get('toAddress');
                    if (toAddressInput) {
                        toAddressInput.style.borderColor = '';
                        toAddressInput.style.backgroundColor = '';
                        toAddressInput.classList.remove('hidden');
                    }
                }
            }, 100);
        } catch (error) {
            console.error('Error replying to email:', error);
            if (window.notificationService) {
                window.notificationService.showError('Failed to open reply: ' + error.message);
            }
        }
    }

    // Show email list
    showEmailList() {
        try {
            // Show the email list and hide detail views (single email + thread)
            const emailList = domManager.get('emailList');
            const emailDetailView = document.getElementById('email-detail-view');
            const inboxThreadDetailView = document.getElementById('inbox-thread-detail-view');
            const inboxActions = document.getElementById('inbox-actions');
            const inboxTitle = document.getElementById('inbox-title');

            if (emailList) emailList.style.display = 'block';
            if (emailDetailView) emailDetailView.style.display = 'none';
            if (inboxThreadDetailView) inboxThreadDetailView.style.display = 'none';
            if (inboxActions) inboxActions.style.display = 'flex';

            // Re-render emails to update unread indicators
            this.renderEmails();
            if (inboxTitle) {
                inboxTitle.textContent = 'Inbox';
                inboxTitle.style.display = '';
            }
            appState.clearCurrentThread();

        } catch (error) {
            console.error('Error showing email list:', error);
        }
    }

    // Show thread detail view with all messages in a conversation
    async showThreadDetail(threadId, source = 'inbox') {
        try {
            const isInbox = source === 'inbox';
            const prefix = isInbox ? 'inbox' : 'sent';

            // Toggle UI: hide list, show thread detail
            const emailList = isInbox ? domManager.get('emailList') : domManager.get('sentList');
            const actions = document.getElementById(isInbox ? 'inbox-actions' : 'sent-actions');
            const title = document.getElementById(isInbox ? 'inbox-title' : 'sent-title');
            const threadDetailView = document.getElementById(`${prefix}-thread-detail-view`);
            const threadContent = document.getElementById(`${prefix}-thread-detail-content`);
            const threadSubject = document.getElementById(`${prefix}-thread-subject`);

            if (emailList) emailList.style.display = 'none';
            if (actions) actions.style.display = 'none';
            if (title) title.style.display = 'none';
            if (threadDetailView) threadDetailView.style.display = 'flex';

            // Show loading state
            if (threadContent) {
                threadContent.innerHTML = '<div class="thread-loading"><i class="fas fa-spinner fa-spin"></i> Loading conversation...</div>';
            }

            // Fetch all emails in thread
            const threadEmails = await TauriService.getThreadEmails(threadId);
            if (!threadEmails || threadEmails.length === 0) {
                if (threadContent) threadContent.innerHTML = '<div class="thread-loading">No messages found.</div>';
                return;
            }

            appState.setCurrentThread(threadId, threadEmails);

            // Classify each email as sent or received
            const settings = appState.getSettings();
            const userEmail = (settings?.email_address || '').trim().toLowerCase();
            const userPubkey = appState.getKeypair()?.public_key;

            for (const email of threadEmails) {
                const fromAddr = (email.from || '').trim().toLowerCase();
                email._isSentByUser = (fromAddr === userEmail) ||
                    (userPubkey && email.sender_pubkey === userPubkey);
            }

            // Build contact index if needed
            if (!this._contactsByPubkey) this._buildContactIndex();

            // Render messages using same per-email decrypt as single email views
            const keypair = appState.getKeypair();
            const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
            let threadSubjectText = threadEmails[0].subject;
            let lastDecryptedSubject = null;
            let lastDecryptedBody = null;

            if (threadContent) {
                threadContent.innerHTML = '';
                for (const email of threadEmails) {
                    const isSent = email._isSentByUser;
                    const emailBody = email.body || '';
                    const encryptedMatch = emailBody.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}/);

                    let displayBody = emailBody;
                    let displaySubject = email.subject;
                    let sigResults = null;
                    let blockDecryptResults = null;

                    if (encryptedMatch && keypair) {
                        // Determine correct pubkey: sender for received, recipient for sent
                        const senderPubkey = isSent ? null : (email.sender_pubkey || email.nostr_pubkey || null);
                        let recipientPubkey = null;
                        if (isSent) {
                            recipientPubkey = email.recipient_pubkey || null;
                            if (!recipientPubkey) {
                                const recipientEmail = email.to || email.to_address;
                                const contact = recipientEmail ? this._findContact(null, recipientEmail) : null;
                                if (contact && contact.pubkey) recipientPubkey = contact.pubkey;
                            }
                        }

                        try {
                            const [result, allSigs] = await Promise.all([
                                TauriService.decryptEmailBody(emailBody, email.subject, senderPubkey, recipientPubkey),
                                TauriService.verifyAllSignatures(emailBody).catch(e => {
                                    console.warn('[JS] Thread sig verify error:', e);
                                    return [];
                                })
                            ]);
                            sigResults = allSigs.length > 0 ? allSigs : null;
                            displaySubject = result.subject || email.subject;
                            displayBody = result.success ? result.body : emailBody;

                            if (result.blockResults && result.blockResults.length > 0) {
                                blockDecryptResults = result.blockResults.map(b => {
                                    if (!b.wasEncrypted) return null;
                                    if (b.decryptedText != null) return { decryptedText: b.decryptedText };
                                    if (b.error) return { error: b.error };
                                    return null;
                                });
                            }

                            // Backfill sender_pubkey from armor if missing
                            if (result.senderPubkey && !email.sender_pubkey && email.id) {
                                email.sender_pubkey = result.senderPubkey;
                            }
                        } catch (err) {
                            console.error(`[JS] Thread email ${email.id} decrypt error:`, err);
                        }
                    } else {
                        // Non-encrypted: verify sigs + glossia decode (same as single view)
                        try {
                            const [allSigs, signedMsg] = await Promise.all([
                                TauriService.verifyAllSignatures(emailBody).catch(e => {
                                    console.warn('[JS] Thread sig verify error:', e);
                                    return [];
                                }),
                                this.decodeGlossiaSignedMessage(emailBody)
                            ]);
                            sigResults = allSigs.length > 0 ? allSigs : null;
                            if (signedMsg && signedMsg.plaintextBody) {
                                displayBody = signedMsg.plaintextBody;
                            }
                        } catch (err) {
                            console.error(`[JS] Thread email ${email.id} sig/glossia error:`, err);
                        }
                    }

                    // Track thread subject from first successful decrypt
                    if (displaySubject && displaySubject !== email.subject && threadSubjectText === threadEmails[0].subject) {
                        threadSubjectText = displaySubject;
                    }

                    // Resolve contact for avatar
                    const contactPubkey = isSent ? userPubkey : email.sender_pubkey;
                    const contactEmail = isSent ? userEmail : (email.from || '');
                    const contact = this._findContact(contactPubkey, contactEmail);

                    let avatarSrc = defaultAvatar;
                    if (contact) {
                        const isValidDataUrl = contact.picture_data_url && contact.picture_data_url.startsWith('data:image') && contact.picture_data_url !== 'data:application/octet-stream;base64,';
                        if (isValidDataUrl) {
                            avatarSrc = contact.picture_data_url;
                        } else if (contact.picture) {
                            avatarSrc = contact.picture;
                        }
                    }
                    const senderName = contact?.name || contact?.display_name || email.from || 'Unknown';
                    const timeAgo = Utils.formatTimeAgo(new Date(email.date));

                    // Signature indicator — icon only in header, full text in details panel
                    const outerSigResult = Array.isArray(sigResults) ? sigResults[sigResults.length - 1] : sigResults;
                    let signatureIcon = '';
                    let securityRows = '';
                    if (outerSigResult && outerSigResult.isValid === true) {
                        signatureIcon = `<span class="signature-indicator verified" title="Signature Verified"><i class="fas fa-check-circle"></i></span>`;
                        securityRows += `<div class="security-row verified"><i class="fas fa-check-circle"></i> Signature Verified</div>`;
                    } else if (outerSigResult && outerSigResult.isValid === false) {
                        signatureIcon = `<span class="signature-indicator invalid" title="Signature Invalid"><i class="fas fa-times-circle"></i></span>`;
                        securityRows += `<div class="security-row invalid"><i class="fas fa-times-circle"></i> Signature Invalid</div>`;
                    } else if (email.signature_valid === true) {
                        signatureIcon = `<span class="signature-indicator verified" title="Signature Verified"><i class="fas fa-check-circle"></i></span>`;
                        securityRows += `<div class="security-row verified"><i class="fas fa-check-circle"></i> Signature Verified</div>`;
                    } else if (email.signature_valid === false) {
                        signatureIcon = `<span class="signature-indicator invalid" title="Signature Invalid"><i class="fas fa-times-circle"></i></span>`;
                        securityRows += `<div class="security-row invalid"><i class="fas fa-times-circle"></i> Signature Invalid</div>`;
                    }

                    // Transport auth indicator
                    let transportAuthIcon = '';
                    if (email.transport_auth_verified === true) {
                        transportAuthIcon = `<span class="transport-auth-indicator verified" title="Email Transport Verified"><i class="fas fa-envelope"></i></span>`;
                        securityRows += `<div class="security-row verified"><i class="fas fa-envelope"></i> Email Transport Verified</div>`;
                    }

                    // Generate body snippet for collapsed view (use decrypted displayBody, not raw html_body)
                    const snippetPlain = (displayBody || '').replace(/<[^>]+>/g, '').replace(/\s+/g, ' ').trim();
                    const snippet = Utils.escapeHtml(snippetPlain.substring(0, 120));

                    const bodyId = `thread-body-${email.id}`;
                    const cardDiv = document.createElement('div');
                    cardDiv.className = 'email-detail-card';
                    cardDiv.innerHTML = `
<div class="email-sender-header">
<div class="email-sender-row">
<img class="email-sender-avatar" src="${avatarSrc}" alt="${Utils.escapeHtml(senderName)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='email-sender-avatar';">
<div class="email-sender-info">
<div class="email-sender-name-row">
<div class="email-sender-name">${Utils.escapeHtml(senderName)}</div>
${signatureIcon}
${transportAuthIcon}
<span class="email-body-snippet">${snippet}</span>
<div class="email-sender-time">${Utils.escapeHtml(timeAgo)}</div>
</div>
<details class="email-metadata-details">
<summary class="email-metadata-summary">to ${Utils.escapeHtml(email.to)}</summary>
<div class="email-header-panel">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
${securityRows ? `<hr><div class="email-security-info">${securityRows}</div>` : ''}
</div>
</details>
</div>
<div class="thread-card-actions">
<button class="thread-action-btn thread-reply-btn" title="Reply"><i class="fas fa-reply"></i></button>
<div class="thread-more-menu">
<button class="thread-action-btn thread-more-btn" title="More"><i class="fas fa-ellipsis-v"></i></button>
<div class="thread-more-dropdown">
<button class="thread-menu-item thread-raw-toggle">Show Raw</button>
</div>
</div>
</div>
</div>
</div>
<pre class="email-raw-content" style="display:none">${Utils.escapeHtml(email.raw_headers || '')}</pre>
<div class="email-detail-body" id="${bodyId}">${email.html_body ? '' : Utils.escapeHtml(displayBody).replace(/\n/g, '<br>')}</div>
<pre class="email-raw-content email-raw-body" style="display:none">${Utils.escapeHtml(email.raw_body || '')}${email.html_body ? '\n\n--- text/html ---\n\n' + Utils.escapeHtml(email.html_body) : ''}</pre>
                    `;
                    threadContent.appendChild(cardDiv);

                    // Collapse all cards except the last (most recent)
                    const isLastEmail = (email === threadEmails[threadEmails.length - 1]);
                    if (!isLastEmail) {
                        cardDiv.classList.add('collapsed');
                    }

                    // Click to toggle collapse/expand.
                    // When collapsed: anywhere on the card (including its padding) expands.
                    // When expanded: only the sender header collapses — clicks in the body
                    // or on action buttons don't accidentally collapse.
                    const senderHeader = cardDiv.querySelector('.email-sender-header');
                    senderHeader.style.cursor = 'pointer';
                    cardDiv.addEventListener('click', (e) => {
                        if (e.target.closest('.thread-card-actions, .email-metadata-details, a')) return;
                        if (cardDiv.classList.contains('collapsed')) {
                            cardDiv.classList.remove('collapsed');
                        } else if (e.target.closest('.email-sender-header')) {
                            cardDiv.classList.add('collapsed');
                        }
                    });

                    // Wire up per-card action buttons
                    const replyCardBtn = cardDiv.querySelector('.thread-reply-btn');
                    if (replyCardBtn) {
                        replyCardBtn.addEventListener('click', () => {
                            this.replyToEmail(email, displaySubject, displayBody);
                        });
                    }
                    const moreBtn = cardDiv.querySelector('.thread-more-btn');
                    const moreDropdown = cardDiv.querySelector('.thread-more-dropdown');
                    if (moreBtn && moreDropdown) {
                        moreBtn.addEventListener('click', (e) => {
                            e.stopPropagation();
                            // Close any other open menus
                            document.querySelectorAll('.thread-more-dropdown.open').forEach(d => {
                                if (d !== moreDropdown) d.classList.remove('open');
                            });
                            moreDropdown.classList.toggle('open');
                        });
                    }
                    const rawToggle = cardDiv.querySelector('.thread-raw-toggle');
                    if (rawToggle) {
                        rawToggle.addEventListener('click', () => {
                            moreDropdown.classList.remove('open');
                            const bodyDiv = cardDiv.querySelector('.email-detail-body');
                            const rawHeader = cardDiv.querySelector('.email-raw-content:not(.email-raw-body)');
                            const rawBody = cardDiv.querySelector('.email-raw-content.email-raw-body');
                            const showingRaw = rawBody && rawBody.style.display !== 'none';
                            if (showingRaw) {
                                if (rawHeader) rawHeader.style.display = 'none';
                                if (rawBody) rawBody.style.display = 'none';
                                if (bodyDiv) bodyDiv.style.display = '';
                                rawToggle.textContent = 'Show Raw';
                            } else {
                                if (rawHeader) rawHeader.style.display = 'block';
                                if (rawBody) rawBody.style.display = 'block';
                                if (bodyDiv) bodyDiv.style.display = 'none';
                                rawToggle.textContent = 'Hide Raw';
                            }
                        });
                    }

                    // Post-render: same steps as single email detail view
                    if (email.html_body) {
                        let htmlToRender = email.html_body;
                        if (sigResults) htmlToRender = this.injectHtmlSigBadge(htmlToRender, sigResults);
                        Utils.renderHtmlBodyInIframe(bodyId, htmlToRender, {
                            decryptedTexts: blockDecryptResults,
                            startDecrypted: true,
                            startCollapsed: true
                        });
                    }
                    if (!email.html_body) {
                        Utils.decorateArmorBlocks(bodyId);
                        this.verifyAndAnnotateSignatureBlocks(displayBody, bodyId);
                    }

                    // Track last email's decrypted content for reply button
                    lastDecryptedSubject = displaySubject;
                    lastDecryptedBody = displayBody;
                }

                // Close menus on outside click
                document.addEventListener('click', () => {
                    document.querySelectorAll('.thread-more-dropdown.open').forEach(d => d.classList.remove('open'));
                }, { once: false });

                // Scroll to bottom (most recent message)
                threadContent.scrollTop = threadContent.scrollHeight;
            }

            if (threadSubject) {
                threadSubject.textContent = threadSubjectText;
            }

            // Mark unread emails as read (fire-and-forget)
            for (const email of threadEmails) {
                if (!email.is_read && (email.message_id || email.id)) {
                    this.markAsRead(email.message_id || email.id).catch(() => {});
                }
            }

            // Set up back button
            const backBtn = document.getElementById(`back-to-${isInbox ? 'inbox' : 'sent'}-from-thread`);
            if (backBtn) {
                backBtn.onclick = () => {
                    if (threadDetailView) threadDetailView.style.display = 'none';
                    appState.clearCurrentThread();
                    if (isInbox) {
                        this.showEmailList();
                    } else {
                        this.showSentList();
                    }
                };
            }

            // Set up reply button (reply to most recent email)
            const replyBtn = document.getElementById(`reply-to-${prefix}-thread`);
            if (replyBtn) {
                const lastEmail = threadEmails[threadEmails.length - 1];
                replyBtn.onclick = () => {
                    this.replyToEmail(
                        lastEmail,
                        lastDecryptedSubject || lastEmail.subject,
                        lastDecryptedBody || lastEmail.body
                    );
                };
            }

        } catch (error) {
            console.error('[JS] Error showing thread detail:', error);
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
            sender_pubkey: null, // Will be set when sending
            recipient_pubkey: this.selectedNostrContact ? this.selectedNostrContact.pubkey : null,
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

    async deleteSentEmail(messageId, deleteFromServer) {
        try {
            const settings = appState.getSettings();
            const userEmail = settings?.email_address || null;
            await TauriService.deleteSentEmail(messageId, deleteFromServer, userEmail);
        } catch (error) {
            console.error('Error deleting sent email:', error);
            throw error;
        }
    }

    // Delete sent email from the sent list
    async deleteSentEmailFromList(messageId) {
        try {
            const choice = await notificationService.showDeleteOptions(
                'Delete Sent Email',
                'Delete locally (will re-fetch on next sync) or delete everywhere (removes from email server too)?'
            );
            if (!choice) return; // cancelled

            const deleteFromServer = choice === 'everywhere';
            await this.deleteSentEmail(messageId, deleteFromServer);
            notificationService.showSuccess(deleteFromServer ? 'Email deleted from server and local database.' : 'Email deleted locally.');
            await this.loadSentEmails();
        } catch (error) {
            console.error('Error deleting sent email:', error);
            notificationService.showError('Failed to delete sent email: ' + error);
        }
    }

    async deleteInboxEmailFromList(messageId) {
        try {
            const choice = await notificationService.showDeleteOptions(
                'Delete Email',
                'Delete locally (will re-fetch on next sync) or delete everywhere (removes from email server too)?'
            );
            if (!choice) return; // cancelled

            const settings = appState.getSettings();
            const userEmail = settings?.email_address || null;
            const deleteFromServer = choice === 'everywhere';

            await TauriService.deleteInboxEmail(messageId, deleteFromServer, userEmail);
            notificationService.showSuccess(deleteFromServer ? 'Email deleted from server and local database.' : 'Email deleted locally.');
            await this.loadEmails();
        } catch (error) {
            console.error('Error deleting inbox email:', error);
            notificationService.showError('Failed to delete email: ' + error);
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
                use_tls: settings.use_tls
            };
            
            console.log('[JS] Testing email connections with config:', {
                smtp_host: emailConfig.smtp_host,
                smtp_port: emailConfig.smtp_port,
                imap_host: emailConfig.imap_host,
                imap_port: emailConfig.imap_port,
                use_tls: emailConfig.use_tls,
                email: emailConfig.email_address
            });
            
            // Helper function to add timeout to a promise
            const withTimeout = (promise, timeoutMs, name) => {
                return Promise.race([
                    promise,
                    new Promise((_, reject) => 
                        setTimeout(() => reject(new Error(`${name} test timed out after ${timeoutMs/1000} seconds`)), timeoutMs)
                    )
                ]);
            };
            
            // Test both IMAP and SMTP connections with 30-second timeout each
            const results = await Promise.allSettled([
                withTimeout(TauriService.testImapConnection(emailConfig), 30000, 'IMAP'),
                withTimeout(TauriService.testSmtpConnection(emailConfig), 30000, 'SMTP')
            ]);
            
            const imapResult = results[0];
            const smtpResult = results[1];
            
            // Extract error messages, handling both Error objects and strings
            const getErrorMessage = (result) => {
                if (result.status === 'rejected') {
                    if (result.reason instanceof Error) {
                        return result.reason.message || result.reason.toString();
                    }
                    return result.reason || 'Unknown error';
                }
                return null;
            };
            
            const imapError = getErrorMessage(imapResult);
            const smtpError = getErrorMessage(smtpResult);
            
            // Check results and provide comprehensive feedback
            if (imapResult.status === 'fulfilled' && smtpResult.status === 'fulfilled') {
                notificationService.showSuccess('✅ Email connection test successful!\n\n• IMAP: Connected and authenticated\n• SMTP: Connected and authenticated\n\nYour email settings are working correctly.');
            } else if (imapResult.status === 'fulfilled' && smtpResult.status === 'rejected') {
                notificationService.showError(`⚠️ Partial success:\n\n✅ IMAP: Connected and authenticated\n❌ SMTP: ${smtpError}\n\nYou can receive emails but may have issues sending them.`);
            } else if (imapResult.status === 'rejected' && smtpResult.status === 'fulfilled') {
                notificationService.showError(`⚠️ Partial success:\n\n❌ IMAP: ${imapError}\n✅ SMTP: Connected and authenticated\n\nYou can send emails but may have issues receiving them.`);
            } else {
                notificationService.showError(`❌ Email connection test failed:\n\n• IMAP: ${imapError || 'Unknown error'}\n• SMTP: ${smtpError || 'Unknown error'}\n\nPlease check your email settings and try again.`);
            }
            
        } catch (error) {
            console.error('Email connection test failed:', error);
            notificationService.showError('Email connection test failed: ' + error);
        } finally {
            domManager.enable('testEmailConnectionBtn');
            domManager.setHTML('testEmailConnectionBtn', '<i class="fas fa-envelope"></i> Test Email Connection');
        }
    }

    // List IMAP folders
    async listImapFolders() {
        console.log('[EMAIL-SERVICE] listImapFolders called');
        if (!appState.hasSettings()) {
            notificationService.showError('Please save your settings first');
            return;
        }
        
        // Validate that required settings are present
        const settings = appState.getSettings();
        console.log('[EMAIL-SERVICE] Settings:', { 
            hasEmail: !!settings.email_address, 
            hasPassword: !!settings.password, 
            hasImapHost: !!settings.imap_host 
        });
        
        if (!settings.email_address || !settings.email_address.trim()) {
            notificationService.showError('Email address is required.');
            return;
        }
        
        if (!settings.password || !settings.password.trim()) {
            notificationService.showError('Password is required.');
            return;
        }
        
        if (!settings.imap_host || !settings.imap_host.trim()) {
            notificationService.showError('IMAP host is required.');
            return;
        }
        
        try {
            const selectElement = document.getElementById('imap-folder-select');
            console.log('[EMAIL-SERVICE] Elements found:', { 
                selectElement: !!selectElement
            });
            
            if (selectElement) {
                selectElement.disabled = true;
                selectElement.innerHTML = '<option disabled>Loading folders...</option>';
            }
            
            const emailConfig = {
                email_address: settings.email_address,
                password: settings.password,
                smtp_host: settings.smtp_host || '',
                smtp_port: settings.smtp_port || 587,
                imap_host: settings.imap_host,
                imap_port: settings.imap_port || 993,
                use_tls: settings.use_tls !== false,
            };
            
            console.log('[JS] Listing IMAP folders with config:', {
                imap_host: emailConfig.imap_host,
                imap_port: emailConfig.imap_port,
                use_tls: emailConfig.use_tls,
                email: emailConfig.email_address
            });
            
            const folders = await TauriService.listImapFolders(emailConfig);
            
            if (selectElement) {
                selectElement.innerHTML = '';
                if (folders && folders.length > 0) {
                    // Add a default "All Folders" option
                    const allOption = document.createElement('option');
                    allOption.value = '';
                    allOption.textContent = 'All Folders';
                    selectElement.appendChild(allOption);
                    
                    // Filter out "Sent" folder (case-insensitive)
                    const filteredFolders = folders.filter(folder => 
                        folder.toLowerCase() !== 'sent'
                    );
                    
                    filteredFolders.forEach(folder => {
                        const option = document.createElement('option');
                        option.value = folder;
                        option.textContent = folder;
                        selectElement.appendChild(option);
                    });
                    selectElement.disabled = false;
                } else {
                    const option = document.createElement('option');
                    option.disabled = true;
                    option.textContent = 'No folders found';
                    selectElement.appendChild(option);
                }
            }
            
            console.log(`[EMAIL-SERVICE] Loaded ${folders?.length || 0} folders`);
            
        } catch (error) {
            console.error('Failed to list IMAP folders:', error);
            notificationService.showError('Failed to list IMAP folders: ' + error);
            
            const selectElement = document.getElementById('imap-folder-select');
            if (selectElement) {
                selectElement.disabled = true;
                selectElement.innerHTML = '<option disabled>Failed to load folders</option>';
            }
        }
    }

    getSelectedFolder() {
        const selectElement = document.getElementById('imap-folder-select');
        if (!selectElement) return null;
        
        return selectElement.value || null;
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
            } else if (provider === 'yahoo') {
                message += ' For Yahoo, you must use an App Password instead of your regular password. Go to your Yahoo Account Security settings > Generate app password to create one.';
            }
            
            // Add TLS info
            if (settings.use_tls) {
                message += ' TLS has been automatically enabled (required for secure connections).';
            }
            
            notificationService.showSuccess(message);
        }
    }

    // Get glossia meta encoding keyword for body/ciphertext from settings
    getGlossiaEncoding() {
        const settings = window.appState?.getSettings();
        return settings?.glossia_encoding_body ?? 'latin';
    }

    // Get glossia meta encoding keyword for signature from settings
    getGlossiaEncodingSignature() {
        const settings = window.appState?.getSettings();
        return settings?.glossia_encoding_signature ?? 'latin';
    }

    // Get glossia meta encoding keyword for pubkey from settings
    getGlossiaEncodingPubkey() {
        const settings = window.appState?.getSettings();
        return settings?.glossia_encoding_pubkey ?? '';
    }

    /**
     * Parse a BEGIN NOSTR (?:SIGNED (?:MESSAGE|BODY)) armor block.
     * Extracts glossia-encoded body, signature content, and seal content.
     * Decodes the glossia body back to the original UTF-8 plaintext.
     * Returns { plaintextBody, glossiaBody, sigContent, sealContent, profileName, displayName } or null.
     */
    async decodeGlossiaSignedMessage(plainBody) {
        if (!plainBody) { console.log('[JS] decodeGlossiaSignedMessage: no body'); return null; }

        // Delegate parsing to the depth-counting parseArmorComponents
        const parts = await this.parseArmorComponents(plainBody);
        if (!parts) { console.log('[JS] decodeGlossiaSignedMessage: parseArmorComponents returned null, body preview:', plainBody.substring(0, 120)); return null; }

        const glossiaBody = parts.bodyText;
        const { sigContent, sealContent, profileName, displayName, quotedArmor } = parts;
        console.log('[JS] decodeGlossiaSignedMessage: parsed armor, bodyText len=', glossiaBody.length, 'isEncrypted=', parts.isEncryptedBody, 'preview:', glossiaBody.substring(0, 80));

        // Decode glossia body → original UTF-8 plaintext
        // Use transcodeToBytes (full pipeline with header word), not decodeToBytes (raw base_n)
        let plaintextBody = null;
        const gs = window.GlossiaService;
        if (gs) {
            try {
                const bytes = await gs.transcodeToBytes(glossiaBody);
                console.log('[JS] decodeGlossiaSignedMessage: transcodeToBytes returned', bytes ? `${bytes.length} bytes` : 'null');
                if (bytes) {
                    plaintextBody = new TextDecoder().decode(bytes);
                    console.log('[JS] decodeGlossiaSignedMessage: decoded plaintext:', plaintextBody.substring(0, 80));
                }
            } catch (e) {
                console.warn('[JS] decodeGlossiaSignedMessage: transcodeToBytes failed:', e);
            }
        } else {
            console.warn('[JS] decodeGlossiaSignedMessage: GlossiaService not available');
        }

        return { plaintextBody, glossiaBody, sigContent, sealContent, profileName, displayName, quotedArmor, isEncryptedBody: parts.isEncryptedBody };
    }

    /**
     * Decode a glossia-encoded subject line.
     * Subjects use payload_only encoding (bare words, no grammar) via "raw" mode.
     * Returns the decrypted ciphertext string, or null if not glossia-encoded.
     */
    async decodeGlossiaSubject(subject) {
        if (!subject || typeof subject !== 'string') return null;

        // If already base64-like, not glossia
        if (Utils.isLikelyEncryptedContent(subject)) return null;

        const gs = window.GlossiaService;
        if (!gs) return null;

        try {
            const detections = await gs.detectDialect(subject);
            console.log('[JS] decodeGlossiaSubject: detected dialects:', detections);
            if (!Array.isArray(detections) || detections.length === 0) return null;
            if (detections[0].hit_rate < 0.8) return null;
            const dialect = detections[0].language;
            if (!dialect) return null;

            const result = await gs.transcode(subject, `decode from ${dialect} raw`);
            let decoded = result.output;

            if (gs._isHex(decoded)) {
                decoded = gs._hexToBase64(decoded);
            }

            decoded = gs._autoUnpack(decoded);
            return decoded;
        } catch (e) {
            console.error('[JS] decodeGlossiaSubject: glossia decode failed:', e);
            return null;
        }
    }

    // Parse raw sig+pubkey blocks from body (Hex/no-glossia mode).
    // Strips Seal block (with npub) from end, then Signature block if present.
    async parseRawSignedBody(fullBody) {
        let body = fullBody;
        let pubkeyHex = null;
        let signatureHex = null;

        // New format: Seal\nnpub1... or DisplayName\nnpub1... (or legacy @name line)
        const sealMatch = body.match(/\n\n[^\n]+\n\n?`?(npub1[a-z0-9]+)`?\s*$/);
        // Legacy: **Seal**\n@name\n`npub1...`
        const legacySealMatch = !sealMatch && body.match(/\n\n\*\*Seal\*\*\n(?:@[^\n]*\n)?`?(npub1[a-z0-9]+)`?\s*$/);
        // Old format: bare npub as last paragraph
        const npubMatch = sealMatch || legacySealMatch || body.match(/\n\n`?(npub1[a-z0-9]+)`?\s*$/);
        if (npubMatch) {
            try {
                pubkeyHex = window.CryptoService._npubToHex(npubMatch[1]);
                body = body.substring(0, npubMatch.index);
            } catch (_) { /* not a valid npub */ }
        }

        // Strip sig_bip39 armored block from end (multi-line > quoted)
        if (pubkeyHex) {
            const gs = window.GlossiaService;
            if (gs) {
                // Match multi-line > quoted block or bare armor block
                const sigArmorRegex = /\n\n((?:>\s*[^\n]*\n?)*-{3,}[^\n]*\n[\s\S]*?-{3,}[^\n]*)\s*$/;
                const sigMatch = body.match(sigArmorRegex);
                if (sigMatch) {
                    try {
                        // Strip > prefixes from each line
                        const sigText = sigMatch[1].split('\n').map(l => l.replace(/^>\s*/, '')).join('\n').trim();
                        const sigResult = await gs.transcode(sigText, `decode from sig nostr`);
                        let sigDecoded = sigResult.output;
                        if (!gs._isHex(sigDecoded)) {
                            sigDecoded = gs._base64ToHex(sigDecoded);
                        }
                        if (sigDecoded.length === 128) {
                            signatureHex = sigDecoded;
                            body = body.substring(0, sigMatch.index);
                        }
                    } catch (_) { /* not a sig_nostr block */ }
                }
            }

            // Strip Signature header if present
            const sigHeaderMatch = body.match(/\n\n(?:\*\*Signature\*\*|Signature)\s*$/);
            if (sigHeaderMatch) {
                body = body.substring(0, sigHeaderMatch.index);
            }
        }

        return { body, pubkeyHex, signatureHex };
    }


    /**
     * Build a JSON proof blob for third-party signature verification
     * and return an HTML copy button that writes it to the clipboard.
     */
    _buildProofButton(pubkeyHex, signatureHex, dataBytes) {
        const npub = window.CryptoService._nip19.npubEncode(pubkeyHex);
        // Convert dataBytes to base64
        let dataBase64 = '';
        if (dataBytes instanceof Uint8Array) {
            let binary = '';
            for (let i = 0; i < dataBytes.length; i++) binary += String.fromCharCode(dataBytes[i]);
            dataBase64 = btoa(binary);
        }
        const proof = {
            nostr_mail_proof: {
                version: 1,
                pubkey_hex: pubkeyHex,
                npub: npub,
                signature_hex: signatureHex,
                data_base64: dataBase64,
                algorithm: 'schnorr-secp256k1',
                hash: 'sha256',
                note: 'Verify: SHA-256(base64decode(data_base64)) then schnorr.verify(signature_hex, hash, pubkey_hex)'
            }
        };
        const json = JSON.stringify(proof, null, 2);
        const btn = document.createElement('button');
        btn.className = 'proof-copy-btn';
        btn.title = 'Copy signature proof JSON for third-party verification';
        btn.innerHTML = '<i class="fas fa-copy"></i>';
        btn.addEventListener('click', async (e) => {
            e.stopPropagation();
            await Utils.copyToClipboard(json);
            window.notificationService.showSuccess('Signature proof copied to clipboard');
        });
        return btn;
    }

    /**
     * Find all signature+seal block pairs in the plain body text,
     * verify each, and update the corresponding DOM indicators
     * created by Utils.decorateArmorBlocks().
     */
    async verifyAndAnnotateSignatureBlocks(bodyText, containerId) {
        if (!bodyText) return;

        // Use backend to verify all signatures recursively
        let results = [];
        try {
            results = await TauriService.verifyAllSignatures(bodyText);
        } catch (e) {
            console.error('[JS] verifyAndAnnotateSignatureBlocks: backend verification error:', e);
            return;
        }

        if (!results || results.length === 0) return;

        // Annotate DOM elements with verification results (innermost-first order)
        // IDs are scoped to containerId to avoid collisions in thread view
        const idPrefix = `${containerId}-sig-block`;
        for (let i = 0; i < results.length; i++) {
            const result = results[i];
            const el = document.getElementById(`${idPrefix}-${i}`);
            if (!el) continue;
            const indicator = el.querySelector('.inline-sig-indicator');
            if (!indicator) continue;

            const pubkeyHex = result.pubkeyHex;
            const signatureHex = result.signatureHex;

            // Toggle raw armor content on indicator click
            const sigContent = el.querySelector('.inline-sig-content');
            if (sigContent) {
                indicator.addEventListener('click', () => {
                    sigContent.style.display = sigContent.style.display === 'block' ? 'none' : 'block';
                });
            }

            if (!pubkeyHex || !signatureHex) {
                indicator.className = 'inline-sig-indicator invalid';
                indicator.innerHTML = '<i class="fas fa-question-circle"></i> Cannot decode';
                continue;
            }

            try {
                const npub = window.CryptoService._nip19.npubEncode(pubkeyHex);
                const shortNpub = npub.substring(0, 12) + '...' + npub.substring(npub.length - 6);
                if (result.isValid) {
                    indicator.className = 'inline-sig-indicator verified';
                    indicator.innerHTML = `<i class="fas fa-check-circle"></i> Signed by: ${shortNpub}`;
                    indicator.title = `Verified signature from ${npub}`;
                    el.classList.add('verified');
                    indicator.after(this._buildProofButton(pubkeyHex, signatureHex, null));
                } else {
                    indicator.className = 'inline-sig-indicator invalid';
                    indicator.innerHTML = '<i class="fas fa-times-circle"></i> Signature Invalid';
                    el.classList.add('invalid');
                }
            } catch (e) {
                console.error('[JS] verifyAndAnnotateSignatureBlocks: annotation error:', e);
                indicator.className = 'inline-sig-indicator invalid';
                indicator.innerHTML = '<i class="fas fa-times-circle"></i> Verification Error';
                el.classList.add('invalid');
            }
        }
    }

    // Strip one level of "> " quote prefixes from text (for verifying quoted armor)
    _stripQuotePrefixes(text) {
        if (!text) return null;
        const lines = text.split('\n').map(l => {
            if (l.startsWith('> ')) return l.substring(2);
            if (l === '>') return '';
            return l;
        });
        return lines.join('\n').trim() || null;
    }

    // Returns { replyText, quotedOriginal } where the split point is the first body-level
    // BEGIN NOSTR armor tag. Everything above (including any "> " quoted plaintext) is the
    // reply text that will be encrypted/signed; the armor block and below is preserved as
    // quotedOriginal (not re-encrypted).
    splitReplyAndQuoted(body) {
        if (!body) return { replyText: '', quotedOriginal: '' };

        // Only match body-level tags (ENCRYPTED BODY/MESSAGE or SIGNED BODY/MESSAGE), NOT
        // structural tags like SEAL or SIGNATURE which are part of the current message's armor.
        const armorIdx = body.search(/\n-{3,}\s*BEGIN NOSTR (?:NIP-(?:04|44) ENCRYPTED |SIGNED )/);
        if (armorIdx >= 0) {
            const replyText = body.substring(0, armorIdx).trimEnd();
            const quotedOriginal = body.substring(armorIdx + 1).trim();
            return { replyText, quotedOriginal };
        }

        // No armor boundary — the entire body is the reply (including any "> " quoted plaintext).
        return { replyText: body, quotedOriginal: '' };
    }

    // Wrap raw ciphertext in ASCII armor
    armorCiphertext(ciphertext, encryptionAlgorithm) {
        const armorType = encryptionAlgorithm === 'nip04' ? 'NIP-04' : 'NIP-44';
        return [
            `-----BEGIN NOSTR ${armorType} ENCRYPTED BODY-----`,
            ciphertext.trim(),
            '-----END NOSTR MESSAGE-----'
        ].join('\n');
    }

    // ── Glossia Email Dialect Helpers ──────────────────────────────────

    // Gzip compress a string to Uint8Array using browser CompressionStream API
    async gzipCompress(input) {
        const blob = new Blob([new TextEncoder().encode(input)]);
        const cs = new CompressionStream('gzip');
        const stream = blob.stream().pipeThrough(cs);
        const compressedBlob = await new Response(stream).blob();
        return new Uint8Array(await compressedBlob.arrayBuffer());
    }

    // Gzip decompress Uint8Array back to string using browser DecompressionStream API
    async gzipDecompress(compressedBytes) {
        const blob = new Blob([compressedBytes]);
        const ds = new DecompressionStream('gzip');
        const stream = blob.stream().pipeThrough(ds);
        const decompressedBlob = await new Response(stream).blob();
        return await decompressedBlob.text();
    }

    // Build outer payload: [flags:1][pubkey:32][signature?:64][ciphertext:N]
    // flags: bit 0 = has_signature, bits 1-2 = algo (00=nip44, 01=nip04)
    buildOuterPayload(pubkeyHex, signature, ciphertextBase64, encryptionAlgorithm) {
        const pubkeyBytes = new Uint8Array(pubkeyHex.match(/.{2}/g).map(b => parseInt(b, 16)));
        const ciphertextBytes = Uint8Array.from(atob(ciphertextBase64), c => c.charCodeAt(0));

        let flags = 0;
        if (encryptionAlgorithm === 'nip04') flags |= 0x02; // bits 1-2 = 01
        const hasSig = signature && signature.length > 0;
        if (hasSig) flags |= 0x01; // bit 0

        const sigBytes = hasSig
            ? new Uint8Array(signature.match(/.{2}/g).map(b => parseInt(b, 16)))
            : new Uint8Array(0);

        const total = 1 + 32 + sigBytes.length + ciphertextBytes.length;
        const payload = new Uint8Array(total);
        payload[0] = flags;
        payload.set(pubkeyBytes, 1);
        payload.set(sigBytes, 33);
        payload.set(ciphertextBytes, 33 + sigBytes.length);

        return btoa(String.fromCharCode(...payload));
    }

    // Parse outer payload back into components
    parseOuterPayload(base64) {
        const bytes = Uint8Array.from(atob(base64), c => c.charCodeAt(0));
        if (bytes.length < 33) throw new Error('Outer payload too short');

        const flags = bytes[0];
        const hasSig = (flags & 0x01) !== 0;
        const algoFlags = (flags >> 1) & 0x03;
        const encryptionAlgorithm = algoFlags === 1 ? 'nip04' : 'nip44';

        const pubkeyBytes = bytes.slice(1, 33);
        const pubkeyHex = Array.from(pubkeyBytes).map(b => b.toString(16).padStart(2, '0')).join('');

        const sigOffset = hasSig ? 97 : 33; // 33 + 64 = 97
        let signature = null;
        if (hasSig) {
            const sigBytes = bytes.slice(33, 97);
            signature = Array.from(sigBytes).map(b => b.toString(16).padStart(2, '0')).join('');
        }

        const ciphertextBytes = bytes.slice(sigOffset);
        const ciphertextBase64 = btoa(String.fromCharCode(...ciphertextBytes));

        return { pubkeyHex, signature, ciphertextBase64, encryptionAlgorithm };
    }

    // Parse glossia RFC 5322-shaped output into { subject, body }
    parseGlossiaEmailOutput(glossiaText) {
        const lines = glossiaText.split(/\r?\n/);
        let subject = '';
        let bodyStart = -1;

        for (let i = 0; i < lines.length; i++) {
            if (lines[i].toLowerCase().startsWith('subject:')) {
                subject = lines[i].substring(lines[i].indexOf(':') + 1).trim();
            }
            // Blank line after headers marks start of body
            if (lines[i].trim() === '' && bodyStart === -1 && i > 0) {
                bodyStart = i + 1;
            }
        }

        const body = bodyStart >= 0 ? lines.slice(bodyStart).join('\n').trim() : '';
        return { subject, body };
    }

    // Check if a dialect string is an email dialect
    isEmailDialect(dialect) {
        return ['email', 'email_alt', 'email_mime'].includes(dialect);
    }

    // Heuristic: detect if received email was encoded with glossia email dialect
    isGlossiaEmailDialect(subject, body) {
        if (!body) return false;
        // Body contains RFC 5322-like structural tokens alongside prose
        const hasContentType = body.includes('Content-Type:');
        const hasMimeBoundary = body.includes('--glossia');
        if (hasContentType || hasMimeBoundary) return true;
        return false;
    }

    // Reconstruct glossia email format from received subject + body for decoding
    reconstructGlossiaEmail(subject, body) {
        return `From: x\r\nSubject: ${subject}\r\nContent-Type: text/plain; charset="UTF-8"\r\n\r\n${body}`;
    }

    // Full encode pipeline: JSON → gzip → encrypt → outer payload → glossia email
    async encryptAndEncodeAsEmail(dialect) {
        if (!this.selectedNostrContact) {
            notificationService.showError('Select a Nostr contact to encrypt for');
            return false;
        }
        if (!appState.hasKeypair()) {
            notificationService.showError('No keypair available');
            return false;
        }

        const subject = domManager.getValue('subject') || '';
        const body = domManager.getValue('messageBody') || '';
        if (!subject && !body) {
            notificationService.showError('Nothing to encrypt');
            return false;
        }

        // Store plaintext for later use
        this.plaintextSubject = subject;
        this.plaintextBody = body;

        const pubkey = this.selectedNostrContact.pubkey;
        const settings = appState.getSettings();
        const encryptionAlgorithm = settings?.encryption_algorithm || 'nip44';

        try {
            const gs = window.GlossiaService;
            if (!gs) {
                notificationService.showError('Glossia service not available');
                return false;
            }

            // 1. Pack plaintext into JSON struct
            const emailPayload = JSON.stringify({ version: 1, subject, body });

            // 2. Gzip compress
            const compressed = await this.gzipCompress(emailPayload);

            // 3. Encrypt the compressed data
            const compressedBase64 = btoa(String.fromCharCode(...compressed));
            const ciphertext = await TauriService.encryptMessageWithAlgorithm(
                pubkey, compressedBase64, encryptionAlgorithm
            );

            // 4. Pack NIP-04 if needed, then build outer payload with pubkey
            const packedCiphertext = gs._packNip04(ciphertext);
            const senderNpub = appState.getKeypair().public_key;
            const senderHex = window.CryptoService._npubToHex(senderNpub);
            const outerPayload = this.buildOuterPayload(
                senderHex, null, packedCiphertext, encryptionAlgorithm
            );
            // 5. Encode using glossia email dialect via explicit pipeline
            const target = `english/bip39/${dialect}`;
            const result = gs.pipelineExecute(outerPayload, 'base64', target);

            // 6. Parse glossia output into subject + body
            const parsed = this.parseGlossiaEmailOutput(result.output);

            // 7. Set form fields
            domManager.setValue('subject', parsed.subject);
            domManager.setValue('messageBody', parsed.body);

            // Clear signature when encrypting (body state changed)
            this.clearSignature();

            notificationService.showSuccess('Encrypted and encoded as email with glossia');
            return true;
        } catch (error) {
            console.error('[JS] Email dialect encode error:', error);
            notificationService.showError('Failed to encode as email: ' + error);
            return false;
        }
    }

    // Full decode pipeline: glossia email → outer payload → decrypt → gunzip → JSON
    async decodeAndDecryptEmailDialect(dialect) {
        const currentSubject = domManager.getValue('subject') || '';
        const currentBody = domManager.getValue('messageBody') || '';

        if (!currentSubject && !currentBody) {
            notificationService.showError('Nothing to decode');
            return false;
        }
        if (!appState.hasKeypair()) {
            notificationService.showError('No keypair available');
            return false;
        }

        try {
            const gs = window.GlossiaService;
            if (!gs) {
                notificationService.showError('Glossia service not available');
                return false;
            }

            // 1. Reconstruct glossia email format
            const glossiaEmail = this.reconstructGlossiaEmail(currentSubject, currentBody);

            // 2. Decode via glossia (english/bip39/email → base64)
            const result = gs.pipelineExecute(glossiaEmail, 'english/bip39/email', 'base64');
            const decodedBase64 = result.output;

            // 3. Parse outer payload
            const outer = this.parseOuterPayload(decodedBase64);

            // 4. Reconstruct ciphertext format for NIP-04 if needed
            const ciphertext = gs._autoUnpack(outer.ciphertextBase64);

            // 5. Decrypt: try extracted sender pubkey first (recipient flow),
            //    then selected contact pubkey (sender self-test / sent-folder flow)
            const senderNpub = window.CryptoService._nip19.npubEncode(outer.pubkeyHex);
            let compressedBase64;
            try {
                compressedBase64 = await TauriService.decryptDmContent(
                    senderNpub, ciphertext
                );
            } catch (_) {
                // Sender pubkey didn't work — try the selected contact (other party)
                const contactPubkey = this.selectedNostrContact?.pubkey;
                if (!contactPubkey) throw new Error('Decryption failed and no contact selected to try');
                compressedBase64 = await TauriService.decryptDmContent(
                    contactPubkey, ciphertext
                );
            }

            // 6. Gunzip decompress
            const compressedBytes = Uint8Array.from(
                atob(compressedBase64), c => c.charCodeAt(0)
            );
            const jsonStr = await this.gzipDecompress(compressedBytes);

            // 7. Parse JSON and restore original fields
            const payload = JSON.parse(jsonStr);
            if (payload.version !== 1) {
                throw new Error('Unsupported email payload version: ' + payload.version);
            }

            domManager.setValue('subject', payload.subject || '');
            domManager.setValue('messageBody', payload.body || '');

            notificationService.showSuccess('Decoded and decrypted email from glossia');
            return true;
        } catch (error) {
            console.error('[JS] Email dialect decode error:', error);
            notificationService.showError('Failed to decode email: ' + error);
            return false;
        }
    }

    // ── End Glossia Email Dialect Helpers ───────────────────────────────

    // Encode already-encrypted email fields using glossia grammar steganography
    async encodeEmailFields() {
        const meta = this.getGlossiaEncoding();

        const currentSubject = domManager.getValue('subject') || '';
        const currentBody = domManager.getValue('messageBody') || '';

        if (!currentSubject && !currentBody) {
            notificationService.showError('Nothing to encode');
            return false;
        }

        // Empty meta means "no encoding" — leave raw base64 ciphertext as-is
        if (!meta) {
            console.log('[JS] Glossia encoding is empty (base64 mode), skipping encode');
            return true;
        }

        try {
            const gs = window.GlossiaService;
            if (!gs) {
                notificationService.showError('Glossia service not available');
                return false;
            }

            // Encode subject (bitpack NIP-04 ciphertext for compactness)
            if (currentSubject) {
                console.log('[JS] Encoding subject with transcode("encode into ' + meta + ' raw")...');
                const packed = gs._packNip04(currentSubject.trim());
                const result = await gs.transcode(packed, `encode into ${meta} raw`);
                domManager.setValue('subject', result.output);
            }

            // Encode body: strip ASCII armor to get raw ciphertext, then encode
            if (currentBody) {
                console.log('[JS] Encoding body with transcode("encode into ' + meta + '")...');
                const armorMatch = currentBody.match(
                    /-{3,}\s*BEGIN NOSTR (?:NIP-(?:04|44) ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-(?:04|44) ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/
                );
                const rawCipher = armorMatch ? armorMatch[1].replace(/\s+/g, '') : currentBody.trim();
                const packed = gs._packNip04(rawCipher);
                const result = await gs.transcode(packed, `encode into ${meta}`);
                domManager.setValue('messageBody', result.output);
            }

            notificationService.showSuccess('Encoded with glossia');
            return true;
        } catch (error) {
            console.error('[JS] Encode error:', error);
            notificationService.showError('Failed to encode: ' + error);
            return false;
        }
    }

    // Decode glossia-encoded email fields back to ASCII-armored ciphertext
    async decodeEmailFields() {
        const meta = this.getGlossiaEncoding();

        // Empty meta means "no encoding" was used locally, but incoming content
        // may use any dialect — fall back to glossia auto-detection
        const decodeMeta = meta || null;

        const currentSubject = domManager.getValue('subject') || '';
        const currentBody = domManager.getValue('messageBody') || '';

        if (!currentSubject && !currentBody) {
            notificationService.showError('Nothing to decode');
            return false;
        }

        try {
            const gs = window.GlossiaService;
            if (!gs) {
                notificationService.showError('Glossia service not available');
                return false;
            }

            if (currentSubject) {
                const subjectInstruction = decodeMeta ? `decode from ${decodeMeta} raw` : 'decode raw';
                console.log('[JS] Decoding subject with transcode("' + subjectInstruction + '")...');
                const result = await gs.transcode(currentSubject, subjectInstruction);
                // Base-N codec returns hex; convert to base64 for NIP decrypt
                const subjectOut = gs._isHex(result.output) ? gs._hexToBase64(result.output) : result.output;
                // Unpack NIP-04 binary format if the subject was bitpacked on encode
                domManager.setValue('subject', gs._autoUnpack(subjectOut));
            }

            if (currentBody) {
                const bodyInstruction = decodeMeta ? `decode from ${decodeMeta}` : 'decode';
                console.log('[JS] Decoding body with transcode("' + bodyInstruction + '")...');

                // Strip embedded pubkey/signature blocks before decoding body
                let bodyToDecode = currentBody;
                let extractedPubkey = null;
                let extractedSignature = null;
                try {
                    const metaPubkey = this.getGlossiaEncodingPubkey();
                    const metaSig = this.getGlossiaEncodingSignature();
                    const parsed = await gs.parseSignedBody(currentBody, metaPubkey, metaSig);
                    if (parsed.pubkeyHex) {
                        bodyToDecode = parsed.body;
                        extractedPubkey = parsed.pubkeyHex;
                        extractedSignature = parsed.signatureHex;
                        console.log('[JS] Extracted pubkey from body:', extractedPubkey.substring(0, 16) + '...');
                        if (extractedSignature) {
                            console.log('[JS] Extracted signature from body:', extractedSignature.substring(0, 32) + '...');
                        }
                    }
                } catch (parseErr) {
                    console.warn('[JS] Failed to parse signed body, decoding as-is:', parseErr);
                }

                const result = await gs.transcode(bodyToDecode, bodyInstruction);
                // Base-N codec returns hex; convert to base64 for NIP decrypt
                const decoded = gs._isHex(result.output) ? gs._hexToBase64(result.output) : result.output;

                // Verify signature against raw ciphertext if present
                if (extractedSignature && extractedPubkey) {
                    try {
                        const npub = window.CryptoService._nip19.npubEncode(extractedPubkey);
                        const isValid = await TauriService.verifySignature(npub, extractedSignature, decoded);
                        console.log('[JS] Glossia body signature verification:', isValid ? 'VALID' : 'INVALID');
                        this._lastBodySignatureValid = isValid;
                        this._lastBodySenderPubkey = extractedPubkey;
                        if (isValid) {
                            notificationService.showSuccess('Signature verified (from body)');
                        } else {
                            notificationService.showWarning('Signature in body is INVALID');
                        }
                    } catch (verifyErr) {
                        console.error('[JS] Signature verification failed:', verifyErr);
                        this._lastBodySignatureValid = false;
                        this._lastBodySenderPubkey = extractedPubkey;
                    }
                } else if (extractedPubkey) {
                    this._lastBodySignatureValid = null; // unsigned but pubkey present
                    this._lastBodySenderPubkey = extractedPubkey;
                }

                // Re-wrap in ASCII armor only if decoded content is actual ciphertext
                const detectedAlgo = Utils.detectEncryptionFormat(decoded);
                if (detectedAlgo === 'nip04' || detectedAlgo === 'nip44') {
                    const armored = this.armorCiphertext(decoded, detectedAlgo);
                    domManager.setValue('messageBody', armored);
                } else {
                    domManager.setValue('messageBody', decoded);
                }
            }

            notificationService.showSuccess('Decoded from glossia');
            return true;
        } catch (error) {
            console.error('[JS] Decode error:', error);
            notificationService.showError('Failed to decode: ' + error);
            return false;
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
                // Last resort: build a temporary contact from manually-entered npub + email
                const manualPubkey = this.getRecipientPubkey();
                const manualEmail = domManager.getValue('toAddress');
                if (manualPubkey) {
                    this.selectedNostrContact = { pubkey: manualPubkey, email: manualEmail || '', name: '' };
                    console.log('[JS] Created contact from manual pubkey input:', manualPubkey.substring(0, 20) + '...');
                } else {
                    notificationService.showError('Select a Nostr contact to encrypt for');
                    return false;
                }
            }
        }
        if (!appState.hasKeypair()) {
            console.log('[JS] No keypair available');
            return false;
        }
        const subject = domManager.getValue('subject') || '';
        const rawBody = domManager.getValue('messageBody') || '';
        // Split reply text from quoted original armor
        // Only the reply text gets encrypted; quoted armor is appended outside
        const { replyText, quotedOriginal } = this.splitReplyAndQuoted(rawBody);
        const body = quotedOriginal ? replyText : rawBody; // only use split result when quotes found
        if (quotedOriginal) {
            this._quotedOriginalArmor = quotedOriginal;
        }
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
        const pubkey = this.selectedNostrContact.pubkey;
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
            // Check if we should use manifest encryption
            const hasAttachments = this.attachments.length > 0;
            const bodySize = new TextEncoder().encode(body || '').length;
            const shouldUseManifest = hasAttachments || bodySize > 64 * 1024; // 64 KB threshold
            
            console.log(`[JS] Body size: ${bodySize} bytes (${(bodySize / 1024).toFixed(1)} KB)`);
            console.log(`[JS] Has attachments: ${hasAttachments}`);
            console.log(`[JS] Should use manifest: ${shouldUseManifest}`);

            // Capture raw encrypted body ciphertext for _plainBody generation
            let rawEncryptedBody = null;
            // Capture raw NIP ciphertext for the subject (base64, before glossia encoding)
            // so we can send it as the DM content directly
            this._subjectCiphertext = null;

            if (shouldUseManifest) {
                // Use manifest-based encryption when attachments are present or body is large
                const reason = hasAttachments ? 'has attachments' : 'large body (>64KB)';
                console.log(`[JS] Using manifest-based encryption (${reason})`);
                
                // Create the manifest structure
                const manifest = {
                    body: {},
                    attachments: []
                };
                
                // 1. Encrypt body with AES and store in manifest
                if (body) {
                    console.log('[JS] Encrypting body with AES...');
                    const bodyAesKey = await this.generateSymmetricKey();
                    // Convert UTF-8 string to base64 properly (handles multi-byte characters)
                    const bodyBase64 = btoa(unescape(encodeURIComponent(body)));
                    const encryptedBodyData = await this.encryptWithAES(bodyBase64, bodyAesKey);
                    const bodySha256 = await this.calculateSHA256(encryptedBodyData);
                    
                    manifest.body = {
                        ciphertext: encryptedBodyData,
                        cipher_sha256: bodySha256,
                        cipher_size: encryptedBodyData.length,
                        key_wrap: bodyAesKey // Unencrypted AES key (manifest will be encrypted)
                    };
                    console.log('[JS] Body encrypted with AES, size:', encryptedBodyData.length);
                }
                
                // 2. Encrypt attachments with AES and create manifest entries
                for (let i = 0; i < this.attachments.length; i++) {
                    const attachment = this.attachments[i];
                    const opaqueId = `a${i + 1}`;
                    
                    console.log(`[JS] Encrypting attachment ${opaqueId}: ${attachment.name}`);
                    
                    // Generate AES key for this attachment
                    const attachmentAesKey = await this.generateSymmetricKey();
                    
                    // Encrypt attachment data with AES (with padding)
                    const encryptedAttachmentData = await this.encryptWithAES(attachment.data, attachmentAesKey, true);
                    const attachmentSha256 = await this.calculateSHA256(encryptedAttachmentData);
                    
                    // Calculate padded size for display
                    const originalSize = attachment.size;
                    const PADDING_SIZE = 64 * 1024; // 64 KiB
                    const paddedSize = Math.ceil(originalSize / PADDING_SIZE) * PADDING_SIZE;
                    
                    // Update attachment with opaque filename and encrypted data
                    attachment.encryptedData = {
                        method: 'manifest_aes',
                        encrypted_file: encryptedAttachmentData,
                        opaque_id: opaqueId,
                        aes_key: attachmentAesKey, // Store AES key for decryption
                        cipher_sha256: attachmentSha256,
                        original_filename: attachment.name,
                        original_type: attachment.type,
                        original_size: originalSize
                    };
                    
                    // Update attachment size to show padded size
                    attachment.size = paddedSize;
                    attachment.isEncrypted = true;
                    
                    // Add to manifest
                    manifest.attachments.push({
                        id: opaqueId,
                        orig_filename: attachment.name,
                        orig_mime: attachment.type,
                        cipher_sha256: attachmentSha256,
                        cipher_size: encryptedAttachmentData.length,
                        key_wrap: attachmentAesKey // Unencrypted AES key (manifest will be encrypted)
                    });
                    
                    console.log(`[JS] Attachment ${opaqueId} encrypted, size: ${encryptedAttachmentData.length}`);
                }
                
                // 3. Encrypt subject (direct NIP encryption)
                let encryptedSubject = subject;
                if (subject) {
                    console.log('[JS] Encrypting subject with NIP...');
                    encryptedSubject = await TauriService.encryptMessageWithAlgorithm(pubkey, subject, encryptionAlgorithm);
                    console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
                    this._subjectCiphertext = encryptedSubject.trim();
                    domManager.setValue('subject', encryptedSubject.trim());
                }

                // 4. JSON.stringify(manifest) → encrypt entire manifest with NIP → ASCII armor
                console.log('[JS] Creating encrypted manifest...');
                const manifestJson = JSON.stringify(manifest);
                console.log('[JS] Manifest JSON size:', manifestJson.length);

                const encryptedManifest = await TauriService.encryptMessageWithAlgorithm(pubkey, manifestJson, encryptionAlgorithm);
                console.log('[JS] Manifest encrypted, size:', encryptedManifest.length);
                rawEncryptedBody = encryptedManifest;

                // Wrap in ASCII armor
                const armoredManifest = this.armorCiphertext(encryptedManifest, encryptionAlgorithm);
                domManager.setValue('messageBody', armoredManifest.trim());
                
                // NIP-44 clears any stale signature since body state changed.
                // NIP-04 signing happens after glossia encoding (below).
                if (encryptionAlgorithm !== 'nip04') {
                    this.clearSignature();
                }

                // Update attachment list display
                this.renderAttachmentList();

                notificationService.showSuccess(`Email encrypted using manifest format (${reason}) with ${encryptionAlgorithm.toUpperCase()}`);
                
            } else {
                // Use simplified encryption when no attachments
                console.log('[JS] Using simplified encryption (no attachments)');
                
                // Encrypt subject
                let encryptedSubject = subject;
                if (subject) {
                    console.log('[JS] Encrypting subject...');
                    encryptedSubject = await TauriService.encryptMessageWithAlgorithm(pubkey, subject, encryptionAlgorithm);
                    console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
                    this._subjectCiphertext = encryptedSubject.trim();
                    domManager.setValue('subject', encryptedSubject.trim());
                }

                // Encrypt body directly with NIP
                let encryptedBody = body;
                if (body) {
                    console.log('[JS] Encrypting body...');
                    encryptedBody = await TauriService.encryptMessageWithAlgorithm(pubkey, body, encryptionAlgorithm);
                    console.log('[JS] Body encrypted:', encryptedBody.substring(0, 50) + '...');
                    rawEncryptedBody = encryptedBody;

                    // Wrap in ASCII armor
                    const armoredBody = this.armorCiphertext(encryptedBody, encryptionAlgorithm);
                    domManager.setValue('messageBody', armoredBody.trim());
                }
                
                // NIP-04 signing happens after glossia encoding (below).

                notificationService.showSuccess(`Subject and body encrypted using ${encryptionAlgorithm.toUpperCase()}`);
            }

            // Auto-encode with glossia if an encoding is selected
            const glossiaMeta = this.getGlossiaEncoding();
            if (glossiaMeta) {
                console.log('[JS] Auto-encoding with glossia encoding:', glossiaMeta);
                const didEncode = await this.encodeEmailFields();
                if (didEncode) {
                    console.log('[JS] Auto-encode succeeded');
                } else {
                    console.warn('[JS] Auto-encode failed, sending as ASCII armor');
                }
            }

            // NIP-04 requires mandatory signing (spec section 4.1).
            // Sign AFTER glossia encoding so the signed bytes match what the
            // verifier recovers (glossia decode produces packed bytes; signing
            // those ensures consistency with verifyInlineSignature).
            if (encryptionAlgorithm === 'nip04') {
                await this._autoSignNip04Body();
            }

            // Rebuild _plainBody with ASCII-armored encrypted content
            // Use the glossia-encoded body from DOM (after auto-encode) with the same
            // encoded sig/pubkey that _htmlBody uses, just formatted as ASCII armor.
            if (rawEncryptedBody) {
                // The DOM body now has the glossia-encoded ciphertext (or ASCII armor if no glossia)
                const encodedBody = domManager.getValue('messageBody') || '';

                // Re-encode sig/pubkey with user's per-field settings (same as sign handler)
                const gs = window.GlossiaService;
                const signBtn = document.getElementById('sign-btn');
                const sigHex = signBtn?.dataset?.signature || null;
                const kp = appState.getKeypair();
                const pkHex = kp ? window.CryptoService._npubToHex(kp.public_key) : null;
                let encodedSig = null, encodedPubkey = null, encodedSigPubkey = null;
                if (gs) {
                    const metaSig = this.getGlossiaEncodingSignature();
                    const metaPubkey = this.getGlossiaEncodingPubkey();
                    if (sigHex && pkHex) {
                        // Encode sig+pubkey for SIGNATURE block (default: glossia sig + npub; masked: combined 96-byte)
                        const result = await gs.encodeSigPubkey(sigHex, pkHex, metaSig, metaPubkey);
                        if (result.combined) {
                            encodedSigPubkey = result.encodedSigPubkey;
                        } else {
                            encodedSig = result.encodedSig;
                            encodedPubkey = result.encodedPubkey;
                        }
                    }
                    // For unsigned messages, encode pubkey separately for SEAL block
                    if (!encodedPubkey && !encodedSigPubkey && pkHex) {
                        encodedPubkey = await gs.encodePubkey(pkHex, metaPubkey);
                    }
                }
                let profileName = null, senderDisplayName = null;
                try {
                    const cached = localStorage.getItem('nostr_mail_profiles');
                    if (cached) {
                        const profile = JSON.parse(cached)[kp?.public_key];
                        profileName = profile?.fields?.name || null;
                        senderDisplayName = profile?.fields?.display_name || null;
                    }
                } catch (_) {}
                // Build quoted HTML recursively for all nesting levels
                const quotedHtmlContent = await this.buildRecursiveQuotedHtml(this._quotedOriginalArmor);
                this._plainBody = this.buildPlainBody(
                    encodedBody, encodedSig, encodedPubkey, profileName, senderDisplayName,
                    true, encryptionAlgorithm, null, encodedSigPubkey,
                    this._quotedOriginalArmor
                );
                // Also rebuild _htmlBody with the encrypted content + sig/seal + quoted
                const metaSig = this.getGlossiaEncodingSignature();
                const metaPubkey = this.getGlossiaEncodingPubkey();
                this._htmlBody = this.buildHtmlAlt(
                    encodedBody, encodedSig, encodedPubkey, profileName, senderDisplayName,
                    metaSig, metaPubkey, encodedSigPubkey, quotedHtmlContent
                );
                console.log('[JS] encryptEmailFields: _htmlBody set, length=', this._htmlBody?.length, 'encodedPubkey=', !!encodedPubkey);

                // Show armored format in textarea so user sees the full armor block
                if (this._plainBody) {
                    domManager.setValue('messageBody', this._plainBody);
                }
            }

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

    // Encrypt email fields in memory without modifying DOM (for preview purposes)
    async encryptEmailFieldsInMemory(subject, body, contact) {
        console.log('[JS] encryptEmailFieldsInMemory called');
        
        if (!contact) {
            console.log('[JS] No contact provided for encryption');
            return { encryptedSubject: subject, encryptedBody: body };
        }
        
        if (!appState.hasKeypair()) {
            console.log('[JS] No keypair available');
            return { encryptedSubject: subject, encryptedBody: body };
        }
        
        const pubkey = contact.pubkey;
        console.log('[JS] Using pubkey:', pubkey);
        
        // Get the selected encryption algorithm
        const settings = appState.getSettings();
        const encryptionAlgorithm = settings?.encryption_algorithm || 'nip44';
        console.log('[JS] Using encryption algorithm:', encryptionAlgorithm);
        
        try {
            // Encrypt subject
            let encryptedSubject = subject;
            if (subject) {
                console.log('[JS] Encrypting subject in memory...');
                encryptedSubject = await TauriService.encryptMessageWithAlgorithm(pubkey, subject, encryptionAlgorithm);
                console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
            }

            // Encrypt body
            let encryptedBody = body;
            if (body) {
                console.log('[JS] Encrypting body in memory...');
                encryptedBody = await TauriService.encryptMessageWithAlgorithm(pubkey, body, encryptionAlgorithm);
                console.log('[JS] Body encrypted:', encryptedBody.substring(0, 50) + '...');
                encryptedBody = this.armorCiphertext(encryptedBody, encryptionAlgorithm).trim();
            }

            return { encryptedSubject, encryptedBody };
        } catch (error) {
            console.error('[JS] Encryption error in memory:', error);
            // Return original values on error
            return { encryptedSubject: subject, encryptedBody: body };
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
    async renderSentEmails(showLoadMore = false, appendFrom = 0) {
        const sentList = domManager.get('sentList');
        if (!sentList) {
            console.error('[JS] renderSentEmails: sentList element not found');
            return;
        }
        try {
            // Remove existing Load More button if it exists
            const existingLoadMoreBtn = document.getElementById('load-more-sent-emails');
            if (existingLoadMoreBtn) {
                existingLoadMoreBtn.remove();
            }

            // Get all emails from state
            const allEmails = appState.getSentEmails();

            if (!allEmails || allEmails.length === 0) {
                sentList.innerHTML = '<div class="text-center text-muted">No sent emails found</div>';
                return;
            }

            // Only clear DOM on full re-render (not when appending)
            if (appendFrom <= 0) {
                sentList.innerHTML = '';
            }

            // Only render new emails when appending, all emails otherwise
            const emails = appendFrom > 0 ? allEmails.slice(appendFrom) : allEmails;

            console.log(`[JS] renderSentEmails: Rendering ${emails.length} sent emails (appendFrom=${appendFrom})`);

            // Build contact index once for O(1) lookups across all emails
            this._buildContactIndex();

            // Check if we should hide unverified messages
            const settings = appState.getSettings();
            const hideUnverified = settings && settings.hide_unsigned_messages === true;

            // Filter emails for rendering
            const filteredEmails = emails.filter(email => {
                if (hideUnverified && email.signature_valid !== true) {
                    return false;
                }
                return true;
            });

            // Batch-decrypt uncached encrypted sent emails, resolving pubkeys from contact index
            const uncachedEncrypted = filteredEmails.filter(email => {
                if (this._previewCache.has(`sent-${email.id}`)) return false;
                const firstBeginMatch = email.body && email.body.match(/-{3,}\s*BEGIN NOSTR ((?:NIP-\d+ ENCRYPTED|SIGNED) (?:MESSAGE|BODY))\s*-{3,}/);
                return firstBeginMatch && /NIP-\d+ ENCRYPTED/.test(firstBeginMatch[1]);
            });

            if (uncachedEncrypted.length > 0 && appState.getKeypair()) {
                try {
                    const batchInput = uncachedEncrypted.map(email => {
                        // Resolve recipient pubkey: stored value > contact index lookup
                        let recipientPubkey = email.recipient_pubkey || email.nostr_pubkey || null;
                        if (!recipientPubkey) {
                            const recipientEmail = email.to || email.to_address;
                            const contact = recipientEmail ? this._findContact(null, recipientEmail) : null;
                            if (contact && contact.pubkey) {
                                recipientPubkey = contact.pubkey;
                                // Backfill on the email object so per-item renderer skips DB lookup too
                                email.recipient_pubkey = recipientPubkey;
                            }
                        }
                        return {
                            id: String(email.id),
                            armorText: email.body,
                            subject: email.subject,
                            senderPubkey: null,
                            recipientPubkey,
                        };
                    });
                    console.log(`[JS] Batch decrypting ${batchInput.length} sent emails in one IPC call`);
                    const batchResults = await TauriService.decryptEmailBodiesBatch(batchInput);

                    const emailById = new Map(uncachedEncrypted.map(e => [String(e.id), e]));

                    for (const item of batchResults) {
                        const email = emailById.get(item.id);
                        if (item.result && item.result.success) {
                            let previewText = Utils.escapeHtml(item.result.body.substring(0, 100));
                            if (item.result.body.length > 100) previewText += '...';
                            this._previewCache.set(`sent-${item.id}`, {
                                previewText,
                                previewSubject: item.result.subject,
                                showSubject: true,
                            });
                            // Fire-and-forget side effects
                            if (item.result.subjectCiphertext && email && email.message_id) {
                                this.hashStringSHA256(item.result.subjectCiphertext).then(hash => {
                                    window.__TAURI__.core.invoke('db_update_email_subject_hash', {
                                        messageId: email.message_id,
                                        subjectHash: hash,
                                    }).catch(e => console.warn('[JS] Failed to update subject_hash:', e));
                                });
                            }
                            // Backfill recipient_pubkey to DB if resolved from contact index
                            if (email && email.recipient_pubkey && email.id) {
                                this._saveRecipientPubkeyToDb(email, email.recipient_pubkey)
                                    .catch(e => console.warn('[JS] Failed to backfill recipient_pubkey:', e));
                            }
                        } else {
                            this._previewCache.set(`sent-${item.id}`, {
                                previewText: 'Could not decrypt',
                                previewSubject: email ? email.subject : '',
                                showSubject: true,
                            });
                        }
                    }
                } catch (e) {
                    console.error('[JS] Batch decryption failed for sent emails, falling back to per-email:', e);
                }
            }

            // Process emails in parallel (decryption will hit cache from batch above)
            const emailPromises = filteredEmails
                .map(async (email) => {
                try {
                    // Add timeout to prevent hanging on decryption
                    const timeoutPromise = new Promise((_, reject) =>
                        setTimeout(() => reject(new Error('Decryption timeout')), 5000)
                    );
                    const renderPromise = this.renderSentEmailItem(email);
                    return await Promise.race([renderPromise, timeoutPromise]);
                } catch (error) {
                    console.error(`[JS] Error rendering email ${email.id}:`, error);
                    // Return a basic email item even if rendering fails
                    return this.renderSentEmailItemBasic(email);
                }
            });
            
            // Wait for all emails to be rendered (with timeout protection)
            const renderedItems = await Promise.allSettled(emailPromises);
            
            // Add all successfully rendered items to the list (filter out null results)
            let renderedCount = 0;
            for (const result of renderedItems) {
                if (result.status === 'fulfilled' && result.value) {
                    sentList.appendChild(result.value);
                    renderedCount++;
                }
            }
            
            // Show message if no emails were rendered (only on full render, not append)
            if (renderedCount === 0 && appendFrom <= 0) {
                const settings = appState.getSettings();
                const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
                const hideUnverified = settings && settings.hide_unsigned_messages === true;
                if (hideUndecryptable && allEmails.length > 0) {
                    sentList.innerHTML = '<div class="text-center text-muted">No decryptable emails found. All emails are encrypted for a different keypair.</div>';
                } else if (hideUnverified && allEmails.length > 0) {
                    sentList.innerHTML = '<div class="text-center text-muted">No verified emails found. All emails have missing or invalid signatures.</div>';
                } else {
                    sentList.innerHTML = '<div class="text-center text-muted">No sent emails found</div>';
                }
                return;
            }
            
            // Add Load More button if there might be more emails
            if (showLoadMore) {
                const loadMoreBtn = document.createElement('button');
                loadMoreBtn.id = 'load-more-sent-emails';
                loadMoreBtn.className = 'btn btn-secondary';
                loadMoreBtn.style.cssText = 'width: 100%; margin-top: 15px; padding: 12px;';
                loadMoreBtn.innerHTML = '<i class="fas fa-chevron-down"></i> Load More';
                loadMoreBtn.addEventListener('click', () => this.loadMoreSentEmails());
                sentList.appendChild(loadMoreBtn);
            }
            
            console.log(`[JS] renderSentEmails: Successfully rendered ${renderedItems.filter(r => r.status === 'fulfilled').length} emails`);
        } catch (error) {
            console.error('[JS] Error in renderSentEmails:', error);
            sentList.innerHTML = '<div class="text-center text-muted">Error loading sent emails</div>';
        }
    }
    
    // Render a single sent email item (with decryption)
    async renderSentEmailItem(email) {
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
        
        // Determine preview text — check cache first to avoid re-decrypting
        let previewText, showSubject, previewSubject;
        const cacheKey = `sent-${email.id}`;
        const cached = this._previewCache.get(cacheKey);

        if (cached) {
            previewText = cached.previewText;
            showSubject = cached.showSubject;
            previewSubject = cached.previewSubject;
        } else {
            previewText = '';
            showSubject = true;
            previewSubject = email.subject;

            // Check the FIRST/outermost BEGIN NOSTR block to determine message type.
            const sentFirstBegin = email.body && email.body.match(/-{3,}\s*BEGIN NOSTR ((?:NIP-\d+ ENCRYPTED|SIGNED) (?:MESSAGE|BODY))\s*-{3,}/);
            const sentOuterIsEncrypted = sentFirstBegin && /NIP-\d+ ENCRYPTED/.test(sentFirstBegin[1]);

            if (sentOuterIsEncrypted) {
                const keypair = appState.getKeypair();
                if (!keypair) {
                    previewText = 'Unable to decrypt: no keypair';
                } else {
                    try {
                        let recipientPubkey = email.recipient_pubkey || email.nostr_pubkey;
                        // If no recipient pubkey, try DB lookup by email address (same as detail view)
                        if (!recipientPubkey) {
                            const recipientEmail = email.to || email.to_address;
                            if (recipientEmail) {
                                try {
                                    let pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email_including_dms', { email: recipientEmail });
                                    if (!pubkeys || pubkeys.length === 0) {
                                        pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email', { email: recipientEmail });
                                    }
                                    if (pubkeys && pubkeys.length > 0) {
                                        recipientPubkey = pubkeys[0];
                                    }
                                } catch (e) {
                                    console.warn('[JS] Sent preview: pubkey lookup failed for', recipientEmail, e);
                                }
                            }
                        }
                        const result = await TauriService.decryptEmailBody(
                            email.body, email.subject,
                            null, recipientPubkey
                        );
                        if (result.success) {
                            previewSubject = result.subject;
                            previewText = Utils.escapeHtml(result.body.substring(0, 100));
                            if (result.body.length > 100) previewText += '...';
                            showSubject = true;
                        } else {
                            previewText = 'Could not decrypt';
                        }
                        // Backfill recipient_pubkey to DB if discovered via lookup
                        if (result.success && recipientPubkey && !email.recipient_pubkey && email.id) {
                            email.recipient_pubkey = recipientPubkey;
                            this._saveRecipientPubkeyToDb(email, recipientPubkey)
                                .catch(e => console.warn('[JS] Failed to backfill recipient_pubkey:', e));
                        }
                        // Update subject_hash for DM↔email matching
                        if (result.subjectCiphertext && email.message_id) {
                            this.hashStringSHA256(result.subjectCiphertext).then(hash => {
                                window.__TAURI__.core.invoke('db_update_email_subject_hash', {
                                    messageId: email.message_id,
                                    subjectHash: hash,
                                }).catch(e => console.warn('[JS] Failed to update subject_hash:', e));
                            });
                        }
                    } catch (e) {
                        console.error('[JS] Backend decrypt failed for sent preview:', e);
                        previewText = 'Could not decrypt';
                    }
                }
            } else {
                // Decode glossia signed message body for preview
                let sentPreviewBody = email.body || '';
                const sentSignedMsg = await this.decodeGlossiaSignedMessage(sentPreviewBody);
                if (sentSignedMsg && sentSignedMsg.plaintextBody) {
                    sentPreviewBody = sentSignedMsg.plaintextBody;
                }
                previewText = Utils.escapeHtml(sentPreviewBody ? sentPreviewBody.substring(0, 100) : '');
                if (sentPreviewBody && sentPreviewBody.length > 100) previewText += '...';
                showSubject = true;
            }

            // Cache the result for future re-renders
            this._previewCache.set(cacheKey, { previewText, previewSubject, showSubject });
        }

        // Check if email is decryptable (for filtering)
        const settings = appState.getSettings();
        const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
        const isDecryptable = !previewText.includes('Unable to decrypt') && 
                            !previewText.includes('Your private key could not decrypt this message') &&
                            !previewText.includes('could not decrypt') &&
                            !previewText.includes('Could not decrypt') &&
                            previewText !== 'Unable to decrypt: no keypair' &&
                            previewSubject !== 'Could not decrypt';
        
        // Return null if we should hide undecryptable emails and this email can't be decrypted
        if (hideUndecryptable && !isDecryptable) {
            return null;
        }
        
        // Add attachment indicator
        const attachmentCount = email.attachments ? email.attachments.length : 0;
        const attachmentIndicator = attachmentCount > 0 ? 
            `<span class="attachment-indicator" title="${attachmentCount} attachment${attachmentCount > 1 ? 's' : ''}">📎 ${attachmentCount}</span>` : '';

        // Add signature verification indicator
        let signatureIndicator = '';
        const sentSigSource = email.signature_source ? ` (${email.signature_source})` : '';
        // Check signature_valid - handle both boolean and null/undefined cases
        if (email.signature_valid === true || email.signature_valid === 1) {
            signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature${sentSigSource}"><i class="fas fa-pen"></i> Signature Verified</span>`;
        } else if (email.signature_valid === false || email.signature_valid === 0) {
            signatureIndicator = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Invalid Nostr signature"><i class="fas fa-pen"></i> Signature Invalid</span>`;
        }

        // Add transport authentication indicator
        let transportAuthIndicator = '';
        if (email.transport_auth_verified === true || email.transport_auth_verified === 1) {
            transportAuthIndicator = `<span class="transport-auth-indicator verified" title="Email transport authentication verified (DMARC/DKIM/SPF)"><i class="fas fa-envelope"></i> Email Verified</span>`;
        } else if (email.transport_auth_verified === false || email.transport_auth_verified === 0) {
            transportAuthIndicator = `<span class="transport-auth-indicator invalid" title="Email transport authentication failed"><i class="fas fa-envelope"></i> Email Unverified</span>`;
        }

        // Get recipient contact for avatar (O(1) lookup via pre-built index)
        const recipientContact = this._findContact(null, email.to);

        // Avatar fallback logic (same as inbox)
        const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
        let avatarSrc = defaultAvatar;
        let avatarClass = 'contact-avatar';
        const isValidDataUrl = recipientContact && recipientContact.picture_data_url && recipientContact.picture_data_url.startsWith('data:image') && recipientContact.picture_data_url !== 'data:application/octet-stream;base64,';
        if (recipientContact && recipientContact.picture_loading) {
            avatarClass += ' loading';
        } else if (isValidDataUrl) {
            avatarSrc = recipientContact.picture_data_url;
        } else if (recipientContact && recipientContact.picture_data_url && !isValidDataUrl && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        } else if (recipientContact && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        }

        emailElement.innerHTML = `
            <img class="${avatarClass}" src="${avatarSrc}" alt="${Utils.escapeHtml(email.to)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
            <div class="email-content">
                <div class="email-header">
                    <div class="email-sender email-list-strong">To: ${Utils.escapeHtml(email.to)} ${attachmentIndicator} ${signatureIndicator} ${transportAuthIndicator}</div>
                    <div class="email-date">${dateDisplay}${email.message_count > 1 ? `<span class="thread-badge" title="${email.message_count} messages">${email.message_count}</span>` : ''}</div>
                </div>
                ${showSubject ? `<div class="email-subject email-list-strong">${Utils.escapeHtml(previewSubject)}</div>` : ''}
                <div class="email-preview">${previewText}</div>
            </div>
            <div class="email-actions">
                <button class="btn btn-danger btn-small" onclick="event.stopPropagation(); emailService.deleteSentEmailFromList('${email.message_id}')">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;
        
        // Add hover and click handlers for invalid signature indicator
        if (email.signature_valid === false) {
            const sigIndicator = emailElement.querySelector('.signature-indicator.invalid');
            if (sigIndicator) {
                const originalText = sigIndicator.textContent;
                sigIndicator.addEventListener('mouseenter', () => {
                    sigIndicator.textContent = 'recheck signature?';
                });
                sigIndicator.addEventListener('mouseleave', () => {
                    sigIndicator.textContent = originalText;
                });
                sigIndicator.addEventListener('click', async (e) => {
                    e.stopPropagation(); // Prevent email detail from opening
                    const messageId = sigIndicator.dataset.messageId;
                    if (messageId) {
                        sigIndicator.textContent = 'checking...';
                        sigIndicator.style.opacity = '0.7';
                        try {
                            const result = await TauriService.recheckEmailSignature(messageId);
                            if (result === true) {
                                // Update the email object
                                email.signature_valid = true;
                                // Re-render this email item
                                sigIndicator.className = 'signature-indicator verified';
                                sigIndicator.innerHTML = '<i class="fas fa-pen"></i> Signature Verified';
                                sigIndicator.title = 'Verified Nostr signature';
                                sigIndicator.removeAttribute('data-message-id');
                                // Remove hover handlers
                                sigIndicator.replaceWith(sigIndicator.cloneNode(true));
                                notificationService.showSuccess('Signature verified successfully!');
                            } else if (result === false) {
                                sigIndicator.textContent = originalText;
                                notificationService.showError('Signature is still invalid');
                            } else {
                                sigIndicator.textContent = originalText;
                                notificationService.showWarning('Could not verify signature (missing pubkey or signature)');
                            }
                        } catch (error) {
                            console.error('[JS] Failed to recheck signature:', error);
                            sigIndicator.textContent = originalText;
                            notificationService.showError('Failed to recheck signature: ' + error);
                        } finally {
                            sigIndicator.style.opacity = '1';
                        }
                    }
                });
            }
        }
        emailElement.addEventListener('click', () => {
            if (email.message_count > 1) {
                this.showThreadDetail(email.thread_id, 'sent');
            } else {
                this.showSentDetail(email.id);
            }
        });
        return emailElement;
    }

    // Render a basic sent email item (without decryption, used as fallback)
    renderSentEmailItemBasic(email) {
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
        
        const attachmentCount = email.attachments ? email.attachments.length : 0;
        const attachmentIndicator = attachmentCount > 0 ? 
            `<span class="attachment-indicator" title="${attachmentCount} attachment${attachmentCount > 1 ? 's' : ''}">📎 ${attachmentCount}</span>` : '';

        // Get recipient contact for avatar (O(1) lookup via pre-built index)
        const recipientContact = this._findContact(null, email.to);

        // Avatar fallback logic (same as inbox)
        const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
        let avatarSrc = defaultAvatar;
        let avatarClass = 'contact-avatar';
        const isValidDataUrl = recipientContact && recipientContact.picture_data_url && recipientContact.picture_data_url.startsWith('data:image') && recipientContact.picture_data_url !== 'data:application/octet-stream;base64,';
        if (recipientContact && recipientContact.picture_loading) {
            avatarClass += ' loading';
        } else if (isValidDataUrl) {
            avatarSrc = recipientContact.picture_data_url;
        } else if (recipientContact && recipientContact.picture_data_url && !isValidDataUrl && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        } else if (recipientContact && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        }

        emailElement.innerHTML = `
            <img class="${avatarClass}" src="${avatarSrc}" alt="${Utils.escapeHtml(email.to)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
            <div class="email-content">
                <div class="email-header">
                    <div class="email-sender email-list-strong">To: ${Utils.escapeHtml(email.to)} ${attachmentIndicator}</div>
                    <div class="email-date">${dateDisplay}</div>
                </div>
                <div class="email-subject email-list-strong">${Utils.escapeHtml(email.subject)}</div>
                <div class="email-preview">${Utils.escapeHtml(email.body ? email.body.substring(0, 100) : '')}</div>
            </div>
            <div class="email-actions">
                <button class="btn btn-danger btn-small" onclick="event.stopPropagation(); emailService.deleteSentEmailFromList('${email.message_id}')">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;
        emailElement.addEventListener('click', () => {
            if (email.message_count > 1) {
                this.showThreadDetail(email.thread_id, 'sent');
            } else {
                this.showSentDetail(email.id);
            }
        });
        return emailElement;
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
            if (sentTitle) sentTitle.style.display = 'none';
            const sentDetailContent = domManager.get('sentDetailContent');
            
            // Show loading state immediately to prevent freeze
            if (sentDetailContent) {
                sentDetailContent.innerHTML = '<div class="loading-spinner"><i class="fas fa-spinner fa-spin"></i> Loading email...</div>';
            }
            
            // Use setTimeout to allow UI to update before starting decryption
            setTimeout(() => {
                this._loadSentEmailDetail(email, sentDetailContent);
            }, 10);
        } catch (error) {
            console.error('Error showing sent email detail:', error);
        }
    }
    
    // Internal method to load sent email detail (separated for async handling)
    async _loadSentEmailDetail(email, sentDetailContent) {
        if (!sentDetailContent) return;
        
        // Refresh email from appState to ensure we have the latest recipient_pubkey
        const freshEmail = appState.getSentEmails().find(e => e.id === email.id || e.message_id === email.message_id);
        if (freshEmail) {
            // Update the email object with fresh data
            Object.assign(email, freshEmail);
        }
        
        // Check if recipient_pubkey is missing and email is encrypted
        // Note: We specifically check for recipient_pubkey, not nostr_pubkey (which might be sender's pubkey)
        const hasRecipientPubkey = !!(email.recipient_pubkey);
        const cleanedBody = email.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('BEGIN NOSTR')).join('\n').trim();
        // Permissive regex: matches both base64 and glossia word content between armor markers
        const encryptedBodyMatch = cleanedBody.match(/-{3,}\s*BEGIN NOSTR (?:(NIP-\d+) ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
        // Only check subject encryption if body has armor (encrypted body implies subject may also be encrypted)
        const isEncryptedSubject = encryptedBodyMatch && (Utils.isLikelyEncryptedContent(email.subject) || !!(await this.decodeGlossiaSubject(email.subject)));
        const isEncrypted = isEncryptedSubject || encryptedBodyMatch;

        console.log(`[JS] _loadSentEmailDetail: Email ${email.id}, encrypted: ${isEncrypted}, hasRecipientPubkey: ${hasRecipientPubkey}, recipient_pubkey: ${email.recipient_pubkey || 'none'}`);
        
        // If encrypted but no recipient_pubkey, try to find it via email address lookup first
        // (faster than brute-force contact search, and the normal rendering path handles decryption failures gracefully)
        if (isEncrypted && !hasRecipientPubkey && appState.getKeypair()) {
            console.log(`[JS] _loadSentEmailDetail: No recipient_pubkey for email ${email.id}, trying email-based lookup`);
            // Try quick email-based lookup (same as decryptSentManifestMessage uses internally)
            const recipientEmail = email.to || email.to_address;
            if (recipientEmail) {
                try {
                    let pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email_including_dms', { email: recipientEmail });
                    if (!pubkeys || pubkeys.length === 0) {
                        pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email', { email: recipientEmail });
                    }
                    if (pubkeys && pubkeys.length > 0) {
                        console.log(`[JS] _loadSentEmailDetail: Found ${pubkeys.length} pubkey(s) for ${recipientEmail}, trying decryption`);
                        for (const pubkey of pubkeys) {
                            const success = await this._tryDecryptWithRecipientPubkey(email, pubkey);
                            if (success) {
                                const updatedEmail = appState.getSentEmails().find(e => e.id === email.id || e.message_id === email.message_id);
                                if (updatedEmail) Object.assign(email, updatedEmail);
                                await this._loadSentEmailDetail(email, sentDetailContent);
                                return;
                            }
                        }
                        console.log(`[JS] _loadSentEmailDetail: Decryption failed with all ${pubkeys.length} pubkey(s) for ${recipientEmail}`);
                    } else {
                        console.log(`[JS] _loadSentEmailDetail: No pubkeys found for ${recipientEmail}, falling back to contact search`);
                    }
                } catch (e) {
                    console.warn(`[JS] _loadSentEmailDetail: Email-based lookup failed:`, e);
                }
            }

            // Fallback: brute-force search through all contacts
            console.log(`[JS] _loadSentEmailDetail: Starting contact search for email ${email.id}`);
            const foundPubkey = await this._searchContactsForRecipientPubkey(email);
            if (foundPubkey) {
                console.log(`[JS] _loadSentEmailDetail: Found pubkey for email ${email.id}, reloading detail`);
                // Refresh email from appState after saving (pubkey was saved in _searchContactsForRecipientPubkey)
                const updatedEmail = appState.getSentEmails().find(e => e.id === email.id || e.message_id === email.message_id);
                if (updatedEmail) {
                    Object.assign(email, updatedEmail);
                }
                // Reload the detail view with the updated pubkey
                await this._loadSentEmailDetail(email, sentDetailContent);
                return;
            } else {
                // No matching pubkey found, show error but still allow viewing raw content
                console.log(`[JS] _loadSentEmailDetail: No pubkey found for email ${email.id}`);
                const rawBody = email.raw_body || email.body || '';
                const rawHeaders = email.raw_headers || '';
                
                // Signature indicator — icon only in header, full text in details panel
                let signatureIcon = '';
                let securityRows = '';
                if (email.signature_valid === true || email.signature_valid === 1) {
                    signatureIcon = `<span class="signature-indicator verified" title="Signature Verified"><i class="fas fa-check-circle"></i></span>`;
                    securityRows += `<div class="security-row verified"><i class="fas fa-check-circle"></i> Signature Verified</div>`;
                } else if (email.signature_valid === false || email.signature_valid === 0) {
                    signatureIcon = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Signature Invalid"><i class="fas fa-times-circle"></i></span>`;
                    securityRows += `<div class="security-row invalid"><i class="fas fa-times-circle"></i> Signature Invalid</div>`;
                }
                if (email.transport_auth_verified === true) {
                    securityRows += `<div class="security-row verified"><i class="fas fa-envelope"></i> Email Transport Verified</div>`;
                }
                
                // Get sender info (for sent emails, sender is us)
                // IMPORTANT: For sent emails, we are the sender, so we must use OUR pubkey, not email.recipient_pubkey
                // Always use profile cache directly (same as loadProfile) to ensure we get our own profile
                const myKeypair = appState.getKeypair();
                const myPubkey = myKeypair ? myKeypair.public_key : null;
                let senderContact = null;
                
                // Debug: Log what pubkeys we have to ensure we're using the right one
                console.log(`[Sent Email Detail Error] Our pubkey: ${myPubkey ? myPubkey.substring(0, 16) + '...' : 'null'}`);
                console.log(`[Sent Email Detail Error] Email sender_pubkey: ${email.sender_pubkey || 'null'}`);
                console.log(`[Sent Email Detail Error] Email recipient_pubkey: ${email.recipient_pubkey || 'null'}`);
                
                if (myPubkey) {
                    // Look up our profile in contacts list (O(1) via contact index)
                    if (!this._contactsByPubkey) this._buildContactIndex();
                    senderContact = this._findContact(myPubkey, null);
                    if (senderContact) {
                        console.log(`[Sent Email Detail Error] Found our profile in contacts - name: ${senderContact.name}`);
                    } else {
                        // Fallback to database lookup
                        console.log(`[Sent Email Detail Error] Not found in contacts, trying database lookup`);
                        try {
                            const ourProfile = await DatabaseService.getContact(myPubkey);
                            if (ourProfile) {
                                console.log(`[Sent Email Detail Error] Found our profile in database - name: ${ourProfile.name}`);
                                senderContact = {
                                    pubkey: myPubkey,
                                    name: ourProfile.name || ourProfile.display_name,
                                    display_name: ourProfile.display_name,
                                    picture: ourProfile.picture_url || ourProfile.picture,
                                    picture_data_url: ourProfile.picture_data_url
                                };
                            }
                        } catch (e) {
                            console.error(`[Sent Email Detail Error] Error fetching our profile from database:`, e);
                        }
                    }
                } else {
                    console.log(`[Sent Email Detail Error] No pubkey available`);
                }
                
                // Avatar logic
                const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
                let avatarSrc = defaultAvatar;
                const isValidDataUrl = senderContact && senderContact.picture_data_url && senderContact.picture_data_url.startsWith('data:image') && senderContact.picture_data_url !== 'data:application/octet-stream;base64,';
                if (senderContact && !senderContact.picture_loading) {
                    if (isValidDataUrl) {
                        avatarSrc = senderContact.picture_data_url;
                    } else if (senderContact.picture) {
                        avatarSrc = senderContact.picture;
                    }
                }
                
                const senderName = senderContact ? (senderContact.name || senderContact.display_name || email.from) : email.from;
                const timeAgo = Utils.formatTimeAgo(new Date(email.date));
                
                // Update page header (just back button, no subject)
                const sentDetailView = domManager.get('sentDetailView');
                const sentDetailHeader = sentDetailView ? sentDetailView.querySelector('.email-detail-header') : null;
                if (sentDetailHeader) {
                    sentDetailHeader.innerHTML = `
                        <button id="back-to-sent" class="btn btn-secondary">
                            <i class="fas fa-arrow-left"></i> Back to Sent
                        </button>
                    `;
                    // Re-attach back button event listener
                    const backBtn = sentDetailHeader.querySelector('#back-to-sent');
                    if (backBtn) {
                        backBtn.addEventListener('click', () => {
                            const sentList = domManager.get('sentList');
                            const sentActions = domManager.get('sentActions');
                            const sentTitle = domManager.get('sentTitle');
                            if (sentList) sentList.style.display = 'block';
                            if (sentDetailView) sentDetailView.style.display = 'none';
                            if (sentActions) sentActions.style.display = 'flex';
                            if (sentTitle) {
                                sentTitle.textContent = 'Sent';
                                sentTitle.style.display = '';
                            }
                        });
                    }
                }
                
                sentDetailContent.innerHTML =
                    `<div class="email-detail">
<h2 class="email-detail-subject">${Utils.escapeHtml(email.subject)}</h2>
<div class="email-detail-card">
<div class="email-sender-header">
<div class="email-sender-row">
<img class="email-sender-avatar" src="${avatarSrc}" alt="${Utils.escapeHtml(senderName)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='email-sender-avatar';">
<div class="email-sender-info">
<div class="email-sender-name-row">
<div class="email-sender-name">${Utils.escapeHtml(senderName)}</div>
${signatureIcon}
<div class="email-sender-time">${Utils.escapeHtml(timeAgo)}</div>
</div>
<details class="email-metadata-details">
<summary class="email-metadata-summary">to ${Utils.escapeHtml(email.to)}</summary>
<div class="email-header-panel" id="sent-email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
${securityRows ? `<hr><div class="email-security-info">${securityRows}</div>` : ''}
</div>
</details>
</div>
<div class="thread-card-actions">
<div class="thread-more-menu">
<button class="thread-action-btn thread-more-btn" title="More"><i class="fas fa-ellipsis-v"></i></button>
<div class="thread-more-dropdown">
<button class="thread-menu-item thread-raw-toggle">Show Raw</button>
</div>
</div>
</div>
</div>
</div>
<div class="error" style="margin-bottom: 15px;">Cannot decrypt: Decryption failed with all known contact keys. The recipient's pubkey may have changed since this email was sent.</div>
<pre id="sent-raw-header-info" class="email-raw-content">${Utils.escapeHtml(rawHeaders)}</pre>
<div class="email-detail-body" id="sent-email-body-info">${email.html_body ? '' : Utils.escapeHtml(rawBody).replace(/\n/g, '<br>')}</div>
<pre id="sent-raw-body-info" class="email-raw-content email-raw-body">${Utils.escapeHtml(rawBody)}${email.html_body ? '\n\n--- text/html ---\n\n' + Utils.escapeHtml(email.html_body) : ''}</pre>
</div>
</div>`;
                // This is the error path (no recipient pubkey) - show raw HTML if available
                if (email.html_body) {
                    Utils.renderHtmlBodyInIframe('sent-email-body-info', email.html_body, {});
                }
                if (!email.html_body) {
                    Utils.decorateArmorBlocks('sent-email-body-info');
                    this.verifyAndAnnotateSignatureBlocks(rawBody, 'sent-email-body-info');
                }

                // Add event listeners for invalid signature indicator in sender header
                if (email.signature_valid === false) {
                    const sigIndicator = sentDetailContent.querySelector('.email-sender-header .signature-indicator.invalid');
                    if (sigIndicator) {
                        const originalText = sigIndicator.textContent;
                        sigIndicator.addEventListener('mouseenter', () => {
                            sigIndicator.textContent = 'recheck signature?';
                        });
                        sigIndicator.addEventListener('mouseleave', () => {
                            sigIndicator.textContent = originalText;
                        });
                        sigIndicator.addEventListener('click', async (e) => {
                            e.stopPropagation();
                            const messageId = sigIndicator.dataset.messageId;
                            if (messageId) {
                                sigIndicator.textContent = 'checking...';
                                sigIndicator.style.opacity = '0.7';
                                try {
                                    const result = await TauriService.recheckEmailSignature(messageId);
                                    if (result === true) {
                                        email.signature_valid = true;
                                        sigIndicator.className = 'signature-indicator verified';
                                        sigIndicator.innerHTML = '<i class="fas fa-pen"></i> Signature Verified';
                                        sigIndicator.title = 'Verified Nostr signature';
                                        sigIndicator.removeAttribute('data-message-id');
                                        sigIndicator.replaceWith(sigIndicator.cloneNode(true));
                                        notificationService.showSuccess('Signature verified successfully!');
                                    } else if (result === false) {
                                        sigIndicator.textContent = originalText;
                                        notificationService.showError('Signature is still invalid.');
                                    } else {
                                        sigIndicator.textContent = originalText;
                                        notificationService.showError('Could not verify signature.');
                                    }
                                } catch (error) {
                                    console.error('[JS] Failed to recheck signature:', error);
                                    sigIndicator.textContent = originalText;
                                    notificationService.showError('Failed to recheck signature: ' + error);
                                } finally {
                                    sigIndicator.style.opacity = '1';
                                }
                            }
                        });
                    }
                }
                
                // Wire up three-dot menu for sent email (error path)
                const sentErrMoreBtn = sentDetailContent.querySelector('.thread-more-btn');
                const sentErrMoreDropdown = sentDetailContent.querySelector('.thread-more-dropdown');
                if (sentErrMoreBtn && sentErrMoreDropdown) {
                    sentErrMoreBtn.addEventListener('click', (e) => {
                        e.stopPropagation();
                        sentErrMoreDropdown.classList.toggle('open');
                    });
                    document.addEventListener('click', () => sentErrMoreDropdown.classList.remove('open'));
                }
                const rawHeaderInfo = document.getElementById('sent-raw-header-info');
                const bodyInfo = document.getElementById('sent-email-body-info');
                const rawBodyInfo = document.getElementById('sent-raw-body-info');
                const sentErrRawToggle = sentDetailContent.querySelector('.thread-raw-toggle');
                if (sentErrRawToggle && rawHeaderInfo && rawBodyInfo && bodyInfo) {
                    let showingRaw = false;
                    sentErrRawToggle.addEventListener('click', () => {
                        sentErrMoreDropdown.classList.remove('open');
                        showingRaw = !showingRaw;
                        if (showingRaw) {
                            rawHeaderInfo.style.display = 'block';
                            rawBodyInfo.style.display = 'block';
                            bodyInfo.style.display = 'none';
                            sentErrRawToggle.textContent = 'Hide Raw';
                        } else {
                            rawHeaderInfo.style.display = 'none';
                            rawBodyInfo.style.display = 'none';
                            bodyInfo.style.display = '';
                            sentErrRawToggle.textContent = 'Show Raw';
                        }
                    });
                }
                return;
            }
        }
        
        // Define updateDetail function first (before it's called)
        const updateDetail = async (subject, body, cachedManifestResult, wasDecrypted = false, inlineSigResult = null, decryptResults = null) => {
            // Render attachments - decrypt metadata for display
            console.log(`[JS] Rendering detail for email ${email.id}, attachments:`, email.attachments);
            console.log(`[JS] Email object:`, email);
            console.log(`[JS] email.html_body present:`, !!email.html_body, `length:`, email.html_body ? email.html_body.length : 0);
            console.log(`[JS] Email.attachments type:`, typeof email.attachments, Array.isArray(email.attachments));
            
            // Ensure attachments array exists
            if (!email.attachments || !Array.isArray(email.attachments)) {
                console.warn(`[JS] Email ${email.id} has no attachments array, initializing empty array`);
                email.attachments = [];
            }
            
            // Log attachment details for debugging
            if (email.attachments && email.attachments.length > 0) {
                console.log('[JS] Attachment details:');
                email.attachments.forEach((att, idx) => {
                    console.log(`[JS]   Attachment ${idx}:`, JSON.stringify({
                        filename: att.filename,
                        encryption_method: att.encryption_method,
                        is_encrypted: att.is_encrypted,
                        content_type: att.content_type,
                        size: att.size
                    }, null, 2));
                    console.log(`[JS]   Attachment ${idx} all keys:`, Object.keys(att));
                });
            }
            
            let attachmentsHtml = '';
            if (email.attachments && email.attachments.length > 0) {
                // For manifest-encrypted emails, we need to decrypt the manifest to get original metadata
                let attachmentDisplayData = [];
                
                console.log('[JS] Checking if attachments are manifest-encrypted...');
                console.log('[JS] Attachment encryption methods:', email.attachments.map(a => a.encryption_method));
                const hasManifestAttachments = email.attachments.some(att => att.encryption_method === 'manifest_aes');
                console.log('[JS] Has manifest attachments:', hasManifestAttachments);
                console.log('[JS] Cached manifest result:', cachedManifestResult);
                console.log('[JS] Cached manifest result type:', cachedManifestResult ? cachedManifestResult.type : 'null');
                
                // Use manifest if:
                // 1. Attachments are marked as manifest_aes, OR
                // 2. We have a cached manifest result with attachments (fallback for attachments not marked correctly)
                const hasValidManifest = cachedManifestResult && 
                                       cachedManifestResult.type === 'manifest' && 
                                       cachedManifestResult.manifest && 
                                       cachedManifestResult.manifest.attachments &&
                                       cachedManifestResult.manifest.attachments.length > 0;
                
                if (hasManifestAttachments || hasValidManifest) {
                    console.log('[JS] Using manifest for attachment mapping (hasManifestAttachments:', hasManifestAttachments, ', hasValidManifest:', hasValidManifest, ')');
                    try {
                        // Use cached manifest result if available (already decrypted above)
                        let manifestResult = cachedManifestResult;
                        console.log('[JS] Initial manifestResult from cache:', manifestResult);
                        console.log('[JS] manifestResult type:', manifestResult ? manifestResult.type : 'null');
                        
                        // Only decrypt if we don't have a cached result
                        if (!manifestResult || manifestResult.type !== 'manifest') {
                            const keypair = appState.getKeypair();
                            if (keypair && email.body && email.body.includes('BEGIN NOSTR')) {
                                const recipientPubkey = email.recipient_pubkey || email.nostr_pubkey;
                                const decryptResult = await TauriService.decryptEmailBody(
                                    email.body, email.subject || '',
                                    null, recipientPubkey
                                );
                                if (decryptResult.isManifest && decryptResult.attachments && decryptResult.attachments.length > 0) {
                                    manifestResult = {
                                        type: 'manifest',
                                        manifest: {
                                            attachments: decryptResult.attachments.map(a => ({
                                                id: a.id,
                                                orig_filename: a.origFilename,
                                                orig_mime: a.origMime,
                                                key_wrap: a.keyWrapB64,
                                                cipher_sha256: a.cipherSha256Hex,
                                                cipher_size: a.cipherSize,
                                            }))
                                        }
                                    };
                                }
                            }
                        }
                        
                        console.log('[JS] Manifest result:', manifestResult);
                        console.log('[JS] Manifest type:', manifestResult ? manifestResult.type : 'null');
                        console.log('[JS] Manifest attachments:', manifestResult && manifestResult.manifest ? manifestResult.manifest.attachments : 'null');
                        
                        if (manifestResult && manifestResult.type === 'manifest' && manifestResult.manifest && manifestResult.manifest.attachments) {
                            console.log('[JS] Using manifest to map attachments, manifest has', manifestResult.manifest.attachments.length, 'attachments');
                            // Map database attachments to manifest metadata
                            attachmentDisplayData = email.attachments.map(dbAttachment => {
                                console.log('[JS] Processing attachment:', dbAttachment.filename, 'encryption_method:', dbAttachment.encryption_method);
                                
                                // Try to match attachment with manifest entry by opaque ID
                                // Extract opaque ID from filename (e.g., "a1.dat" -> "a1")
                                const opaqueId = dbAttachment.filename.replace(/\.dat$/, ''); // Remove .dat extension
                                console.log('[JS] Looking for manifest attachment with id:', opaqueId, 'from filename:', dbAttachment.filename);
                                console.log('[JS] Available manifest attachment IDs:', manifestResult.manifest.attachments.map(a => a.id));
                                
                                const manifestAttachment = manifestResult.manifest.attachments.find(ma => ma.id === opaqueId);
                                
                                if (manifestAttachment) {
                                    console.log('[JS] Found manifest attachment:', manifestAttachment);
                                    return {
                                        ...dbAttachment,
                                        displayName: manifestAttachment.orig_filename,
                                        encryptedFilename: dbAttachment.filename, // Store encrypted filename
                                        displaySize: manifestAttachment.orig_size || dbAttachment.size,
                                        encryptedSize: manifestAttachment.cipher_size || dbAttachment.size,
                                        displayMime: manifestAttachment.orig_mime || dbAttachment.content_type || dbAttachment.mime_type
                                    };
                                } else {
                                    console.warn('[JS] No manifest attachment found for id:', opaqueId, 'available ids:', manifestResult.manifest.attachments.map(a => a.id));
                                    // Fallback to database data
                                    return {
                                        ...dbAttachment,
                                        displayName: dbAttachment.filename,
                                        encryptedFilename: dbAttachment.filename, // Same for non-encrypted
                                        displaySize: dbAttachment.size,
                                        encryptedSize: dbAttachment.size,
                                        displayMime: dbAttachment.content_type || dbAttachment.mime_type
                                    };
                                }
                            });
                        } else {
                            // Fallback to database data
                            attachmentDisplayData = email.attachments.map(att => ({
                                ...att,
                                displayName: att.filename,
                                encryptedFilename: att.filename, // Same for non-encrypted
                                displaySize: att.size,
                                encryptedSize: att.size,
                                displayMime: att.mime_type
                            }));
                        }
                    } catch (error) {
                        console.error('Failed to decrypt manifest for attachment display:', error);
                        // Fallback to database data
                        attachmentDisplayData = email.attachments.map(att => ({
                            ...att,
                            displayName: att.filename,
                            encryptedFilename: att.filename, // Same for non-encrypted
                            displaySize: att.size,
                            encryptedSize: att.size,
                            displayMime: att.mime_type
                        }));
                    }
                } else {
                    // Plain attachments - use database data directly
                    attachmentDisplayData = email.attachments.map(att => ({
                        ...att,
                        displayName: att.filename,
                        encryptedFilename: att.filename, // Same for non-encrypted
                        displaySize: att.size,
                        encryptedSize: att.size,
                        displayMime: att.mime_type
                    }));
                }
                
                attachmentsHtml = `
                <div class="email-attachments" id="sent-email-attachments" style="margin: 15px 0;">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;">
                        <h4>Attachments (${attachmentDisplayData.length})</h4>
                        <button class="btn btn-sm btn-outline-success" onclick="window.emailService.downloadAllSentAttachments(${email.id})" title="Download all attachments as ZIP">
                            <i class="fas fa-download"></i> Download All
                        </button>
                    </div>
                    <div class="attachment-list">
                        ${attachmentDisplayData.map(attachment => {
                            const sizeFormatted = (attachment.displaySize / 1024).toFixed(2) + ' KB';
                            const isEncrypted = attachment.encryption_method === 'manifest_aes';
                            const statusIcon = isEncrypted ? '🔒' : '📄';
                            const statusText = isEncrypted ? 'Encrypted' : 'Plain';
                            // Store both encrypted and decrypted filenames as data attributes
                            const encryptedFilename = attachment.encryptedFilename || attachment.filename;
                            const decryptedFilename = attachment.displayName;
                            
                            return `
                            <div class="attachment-item" style="display: flex; justify-content: space-between; align-items: center; padding: 10px; border: 1px solid #ddd; border-radius: 4px; margin: 5px 0;" 
                                 data-encrypted-filename="${Utils.escapeHtml(encryptedFilename)}" 
                                 data-decrypted-filename="${Utils.escapeHtml(decryptedFilename)}"
                                 data-encrypted-size="${attachment.encryptedSize || attachment.displaySize}"
                                 data-decrypted-size="${attachment.displaySize}">
                                <div class="attachment-info" style="display: flex; align-items: center;">
                                    <i class="fas fa-file" style="margin-right: 10px;"></i>
                                    <div class="attachment-details">
                                        <div class="attachment-name" style="font-weight: bold;" data-display-name="${Utils.escapeHtml(decryptedFilename)}">${Utils.escapeHtml(decryptedFilename)}</div>
                                        <div class="attachment-meta" style="font-size: 0.9em; color: #666;">
                                            <span class="attachment-size">${sizeFormatted}</span> • ${statusIcon} ${statusText}
                                        </div>
                                    </div>
                                </div>
                                <div class="attachment-actions">
                                    <button class="btn btn-sm btn-outline-primary" onclick="window.emailService.downloadSentAttachment(${email.id}, ${attachment.id})">
                                        <i class="fas fa-download"></i> Download
                                    </button>
                                </div>
                            </div>`;
                        }).join('')}
                    </div>
                </div>`;
            }
            
            // Signature indicator — icon only in header, full text in details panel
            const outerSigResult = Array.isArray(inlineSigResult) ? inlineSigResult[inlineSigResult.length - 1] : inlineSigResult;
            let signatureIcon = '';
            let securityRows = '';
            if (outerSigResult && outerSigResult.isValid === true) {
                signatureIcon = `<span class="signature-indicator verified" title="Signature Verified"><i class="fas fa-check-circle"></i></span>`;
                securityRows += `<div class="security-row verified"><i class="fas fa-check-circle"></i> Signature Verified</div>`;
            } else if (outerSigResult && outerSigResult.isValid === false) {
                signatureIcon = `<span class="signature-indicator invalid" title="Signature Invalid"><i class="fas fa-times-circle"></i></span>`;
                securityRows += `<div class="security-row invalid"><i class="fas fa-times-circle"></i> Signature Invalid</div>`;
            } else if (email.signature_valid === true || email.signature_valid === 1) {
                signatureIcon = `<span class="signature-indicator verified" title="Signature Verified"><i class="fas fa-check-circle"></i></span>`;
                securityRows += `<div class="security-row verified"><i class="fas fa-check-circle"></i> Signature Verified</div>`;
            } else if (email.signature_valid === false || email.signature_valid === 0) {
                signatureIcon = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Signature Invalid"><i class="fas fa-times-circle"></i></span>`;
                securityRows += `<div class="security-row invalid"><i class="fas fa-times-circle"></i> Signature Invalid</div>`;
            }
            if (email.transport_auth_verified === true) {
                securityRows += `<div class="security-row verified"><i class="fas fa-envelope"></i> Email Transport Verified</div>`;
            }

            // Get sender info (for sent emails, sender is us)
            // IMPORTANT: For sent emails, we are the sender, so we must use OUR pubkey, not email.recipient_pubkey
            // Always use profile cache directly (same as loadProfile) to ensure we get our own profile
            const myKeypair = appState.getKeypair();
            const myPubkey = myKeypair ? myKeypair.public_key : null;
            let senderContact = null;
            
            // Debug: Log what pubkeys we have to ensure we're using the right one
            console.log(`[Sent Email Detail] Our pubkey: ${myPubkey ? myPubkey.substring(0, 16) + '...' : 'null'}`);
            console.log(`[Sent Email Detail] Email sender_pubkey: ${email.sender_pubkey || 'null'}`);
            console.log(`[Sent Email Detail] Email recipient_pubkey: ${email.recipient_pubkey || 'null'}`);
            
            // CRITICAL: Ensure we never accidentally use recipient_pubkey for avatar lookup
            if (myPubkey && email.recipient_pubkey && myPubkey === email.recipient_pubkey) {
                console.error(`[Sent Email Detail] ERROR: Our pubkey matches recipient_pubkey! This should not happen for sent emails.`);
            }
            
            if (myPubkey) {
                // First try to get from profile cache (same logic as loadProfile)
                const cachedProfiles = localStorage.getItem('nostr_mail_profiles');
                if (cachedProfiles) {
                    try {
                        const profileDict = JSON.parse(cachedProfiles);
                        const cachedProfile = profileDict[myPubkey];
                        if (cachedProfile) {
                            console.log(`[Sent Email Detail] Found cached profile for our pubkey`);
                            // Get picture data URL - check if it's for our pubkey
                            // Note: nostr_mail_profile_picture is a single value, so we need to verify it's for current profile
                            const cachedPictureDataUrl = localStorage.getItem('nostr_mail_profile_picture');
                            senderContact = {
                                pubkey: myPubkey,
                                name: cachedProfile.fields?.name || cachedProfile.fields?.display_name || cachedProfile.name,
                                display_name: cachedProfile.fields?.display_name || cachedProfile.display_name,
                                picture: cachedProfile.fields?.picture || cachedProfile.picture,
                                picture_data_url: cachedPictureDataUrl
                            };
                            console.log(`[Sent Email Detail] Using profile cache - name: ${senderContact.name}, picture: ${senderContact.picture ? 'yes' : 'no'}`);
                        } else {
                            console.log(`[Sent Email Detail] No cached profile found for our pubkey`);
                        }
                    } catch (e) {
                        console.error('Error parsing cached profiles:', e);
                    }
                }
                
                // Fallback: Look up our profile in contacts list (user's profile is now added privately)
                // If not found, fall back to database lookup
                if (!senderContact) {
                    console.log(`[Sent Email Detail] Falling back to contacts lookup`);
                    if (!this._contactsByPubkey) this._buildContactIndex();
                    senderContact = this._findContact(myPubkey, null);
                    if (senderContact) {
                        console.log(`[Sent Email Detail] Found our profile in contacts - name: ${senderContact.name}`);
                    } else {
                        console.log(`[Sent Email Detail] Not found in contacts, trying database lookup`);
                        try {
                            const ourProfile = await DatabaseService.getContact(myPubkey);
                            if (ourProfile) {
                                console.log(`[Sent Email Detail] Found our profile in database - name: ${ourProfile.name}`);
                                senderContact = {
                                    pubkey: myPubkey,
                                    name: ourProfile.name || ourProfile.display_name,
                                    display_name: ourProfile.display_name,
                                    picture: ourProfile.picture_url || ourProfile.picture,
                                    picture_data_url: ourProfile.picture_data_url
                                };
                            }
                        } catch (e) {
                            console.error(`[Sent Email Detail] Error fetching our profile from database:`, e);
                        }
                    }
                }
            } else {
                console.log(`[Sent Email Detail] No pubkey available`);
            }
            
            // Avatar logic
            const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
            let avatarSrc = defaultAvatar;
            const isValidDataUrl = senderContact && senderContact.picture_data_url && senderContact.picture_data_url.startsWith('data:image') && senderContact.picture_data_url !== 'data:application/octet-stream;base64,';
            if (senderContact && !senderContact.picture_loading) {
                if (isValidDataUrl) {
                    avatarSrc = senderContact.picture_data_url;
                } else if (senderContact.picture) {
                    avatarSrc = senderContact.picture;
                }
            }
            
            const senderName = senderContact ? (senderContact.name || senderContact.display_name || email.from) : email.from;
            const timeAgo = Utils.formatTimeAgo(new Date(email.date));
            
            sentDetailContent.innerHTML =
                `<div class="email-detail">
<h2 class="email-detail-subject">${Utils.escapeHtml(subject)}</h2>
<div class="email-detail-card">
<div class="email-sender-header">
<div class="email-sender-row">
<img class="email-sender-avatar" src="${avatarSrc}" alt="${Utils.escapeHtml(senderName)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='email-sender-avatar';">
<div class="email-sender-info">
<div class="email-sender-name-row">
<div class="email-sender-name">${Utils.escapeHtml(senderName)}</div>
${signatureIcon}
<div class="email-sender-time">${Utils.escapeHtml(timeAgo)}</div>
</div>
<details class="email-metadata-details">
<summary class="email-metadata-summary">to ${Utils.escapeHtml(email.to)}</summary>
<div class="email-header-panel" id="sent-email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
${securityRows ? `<hr><div class="email-security-info">${securityRows}</div>` : ''}
</div>
</details>
</div>
<div class="thread-card-actions">
<div class="thread-more-menu">
<button class="thread-action-btn thread-more-btn" title="More"><i class="fas fa-ellipsis-v"></i></button>
<div class="thread-more-dropdown">
<button class="thread-menu-item thread-raw-toggle">Show Raw</button>
</div>
</div>
</div>
</div>
</div>
<pre id="sent-raw-header-info" class="email-raw-content">${Utils.escapeHtml(email.raw_headers || '')}</pre>
<div class="email-detail-body" id="sent-email-body-info">${email.html_body ? '' : Utils.escapeHtml(body).replace(/\n/g, '<br>')}</div>
<pre id="sent-raw-body-info" class="email-raw-content email-raw-body">${Utils.escapeHtml(email.raw_body)}${email.html_body ? '\n\n--- text/html ---\n\n' + Utils.escapeHtml(email.html_body) : ''}</pre>
${attachmentsHtml}
</div>
</div>`;
            if (email.html_body) {
                let htmlToRender = email.html_body;
                if (inlineSigResult) htmlToRender = this.injectHtmlSigBadge(htmlToRender, inlineSigResult);
                Utils.renderHtmlBodyInIframe('sent-email-body-info', htmlToRender, { decryptedTexts: decryptResults, startDecrypted: true });
            }
            // Decorate and verify inline signature blocks in the sent body
            if (!email.html_body) {
                Utils.decorateArmorBlocks('sent-email-body-info');
                this.verifyAndAnnotateSignatureBlocks(body, 'sent-email-body-info');
            }

            // Add event listeners for invalid signature indicator in sender header
            if (email.signature_valid === false) {
                const sigIndicator = sentDetailContent.querySelector('.email-sender-header .signature-indicator.invalid');
                if (sigIndicator) {
                    const originalText = sigIndicator.textContent;
                    sigIndicator.addEventListener('mouseenter', () => {
                        sigIndicator.textContent = 'recheck signature?';
                    });
                    sigIndicator.addEventListener('mouseleave', () => {
                        sigIndicator.textContent = originalText;
                    });
                    sigIndicator.addEventListener('click', async (e) => {
                        e.stopPropagation();
                        const messageId = sigIndicator.dataset.messageId;
                        if (messageId) {
                            sigIndicator.textContent = 'checking...';
                            sigIndicator.style.opacity = '0.7';
                            try {
                                const result = await TauriService.recheckEmailSignature(messageId);
                                if (result === true) {
                                    email.signature_valid = true;
                                    sigIndicator.className = 'signature-indicator verified';
                                    sigIndicator.innerHTML = '<i class="fas fa-pen"></i> Signature Verified';
                                    sigIndicator.title = 'Verified Nostr signature';
                                    sigIndicator.removeAttribute('data-message-id');
                                    sigIndicator.replaceWith(sigIndicator.cloneNode(true));
                                    notificationService.showSuccess('Signature verified successfully!');
                                } else if (result === false) {
                                    sigIndicator.textContent = originalText;
                                    notificationService.showError('Signature is still invalid.');
                                } else {
                                    sigIndicator.textContent = originalText;
                                    notificationService.showError('Could not verify signature.');
                                }
                            } catch (error) {
                                console.error('[JS] Failed to recheck signature:', error);
                                sigIndicator.textContent = originalText;
                                notificationService.showError('Failed to recheck signature: ' + error);
                            } finally {
                                sigIndicator.style.opacity = '1';
                            }
                        }
                    });
                }
            }
            
            // Wire up three-dot menu for sent email
            const sentMoreBtn = sentDetailContent.querySelector('.thread-more-btn');
            const sentMoreDropdown = sentDetailContent.querySelector('.thread-more-dropdown');
            if (sentMoreBtn && sentMoreDropdown) {
                sentMoreBtn.addEventListener('click', (e) => {
                    e.stopPropagation();
                    sentMoreDropdown.classList.toggle('open');
                });
                document.addEventListener('click', () => sentMoreDropdown.classList.remove('open'));
            }
            const rawHeaderInfo = document.getElementById('sent-raw-header-info');
            const bodyInfo = document.getElementById('sent-email-body-info');
            const rawBodyInfo = document.getElementById('sent-raw-body-info');
            const sentRawToggle = sentDetailContent.querySelector('.thread-raw-toggle');
            if (sentRawToggle && rawHeaderInfo && rawBodyInfo && bodyInfo) {
                let showingRaw = false;
                sentRawToggle.addEventListener('click', () => {
                    sentMoreDropdown.classList.remove('open');
                    showingRaw = !showingRaw;
                    if (showingRaw) {
                        rawHeaderInfo.style.display = 'block';
                        rawBodyInfo.style.display = 'block';
                        bodyInfo.style.display = 'none';
                        sentRawToggle.textContent = 'Hide Raw';
                    } else {
                        rawHeaderInfo.style.display = 'none';
                        rawBodyInfo.style.display = 'none';
                        bodyInfo.style.display = '';
                        sentRawToggle.textContent = 'Show Raw';
                    }
                });
            }
        }; // End of updateDetail function
            
            // Now execute the decryption and update
            try {
                const cleanedBody = email.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('BEGIN NOSTR')).join('\n').trim();
                // For sent emails, use recipient_pubkey for decryption
                const recipientPubkey = email.recipient_pubkey || email.nostr_pubkey;
                const encryptedBodyMatch = cleanedBody.match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}/);

                let decryptedSubject = email.subject;
                let decryptedBody = cleanedBody;
                const keypair = appState.getKeypair();

                if (encryptedBodyMatch && keypair) {
                    try {
                        // Run decryption and signature verification in parallel (they're independent)
                        console.log('[JS] Calling backend decrypt_email_body + verifyAllSignatures for sent email in parallel...');
                        const [result, allSigs] = await Promise.all([
                            TauriService.decryptEmailBody(email.body, email.subject, null, recipientPubkey),
                            TauriService.verifyAllSignatures(email.body).catch(e => {
                                console.warn('[JS] Signature verification error:', e);
                                return [];
                            })
                        ]);
                        const sigResults = allSigs.length > 0 ? allSigs : null;
                        console.log('[JS] Backend sent decrypt result: success=', result.success, 'isManifest=', result.isManifest);

                        decryptedSubject = result.subject;
                        decryptedBody = result.success ? result.body : cleanedBody;

                        if (!result.success) {
                            decryptedBody = '[Decryption failed: ' + (result.error || 'unknown error') + ']';
                        }

                        // Map backend manifest attachments
                        let manifestResult = null;
                        if (result.isManifest && result.attachments && result.attachments.length > 0) {
                            manifestResult = {
                                type: 'manifest',
                                manifest: {
                                    attachments: result.attachments.map(a => ({
                                        id: a.id,
                                        orig_filename: a.origFilename,
                                        orig_mime: a.origMime,
                                        key_wrap: a.keyWrapB64,
                                        cipher_sha256: a.cipherSha256Hex,
                                        cipher_size: a.cipherSize,
                                    }))
                                }
                            };
                        }

                        // Map block results for lock/unlock icons
                        let decryptResults = null;
                        if (result.blockResults && result.blockResults.length > 0) {
                            decryptResults = result.blockResults.map(b => {
                                if (!b.wasEncrypted) return null;
                                if (b.decryptedText != null) return { decryptedText: b.decryptedText };
                                if (b.error) return { error: b.error };
                                return null;
                            });
                        }

                        await updateDetail(decryptedSubject, decryptedBody, manifestResult, result.success, sigResults, decryptResults);
                    } catch (err) {
                        console.error('[JS] Backend decrypt_email_body error for sent:', err);
                        await updateDetail('Could not decrypt', 'Could not decrypt: ' + err.message, null, true);
                    }
                } else {
                    // Non-encrypted sent email: run sig verification and glossia decode in parallel
                    const [allSigs, signedMsg] = await Promise.all([
                        TauriService.verifyAllSignatures(email.body).catch(e => {
                            console.warn('[JS] Signature verification error:', e);
                            return [];
                        }),
                        this.decodeGlossiaSignedMessage(email.body)
                    ]);
                    const sigResults = allSigs.length > 0 ? allSigs : null;
                    let displayBody = decryptedBody;
                    if (signedMsg && signedMsg.plaintextBody) {
                        displayBody = signedMsg.plaintextBody;
                    }
                    await updateDetail(decryptedSubject, displayBody, null, false, sigResults);
                }
            } catch (error) {
            console.error('Error loading sent email detail:', error);
            if (sentDetailContent) {
                sentDetailContent.innerHTML = '<div class="error">Error loading email: ' + error.message + '</div>';
            }
        }
    }
    
    // Show modal to enter recipient pubkey for sent email
    async _showRecipientPubkeyModal(email) {
        return new Promise((resolve) => {
            const modalContent = `
                <div style="padding: 20px;">
                    <p>This sent email is encrypted but the recipient's pubkey is not available.</p>
                    <p><strong>To:</strong> ${Utils.escapeHtml(email.to || email.to_address)}</p>
                    <p><strong>Subject:</strong> ${Utils.escapeHtml(email.subject)}</p>
                    <div class="form-group" style="margin-top: 20px;">
                        <label for="recipient-pubkey-input">Enter recipient's Nostr pubkey:</label>
                        <input type="text" id="recipient-pubkey-input" class="form-control" placeholder="npub1..." style="margin-top: 8px;">
                        <small class="form-text text-muted">The pubkey will be saved if decryption is successful.</small>
                    </div>
                    <div style="margin-top: 20px; display: flex; gap: 10px; justify-content: flex-end;">
                        <button id="cancel-recipient-pubkey" class="btn btn-secondary">Cancel</button>
                        <button id="submit-recipient-pubkey" class="btn btn-primary">Try Decryption</button>
                    </div>
                </div>
            `;
            
            window.app.showModal('Enter Recipient Pubkey', modalContent);
            
            const input = document.getElementById('recipient-pubkey-input');
            const submitBtn = document.getElementById('submit-recipient-pubkey');
            const cancelBtn = document.getElementById('cancel-recipient-pubkey');
            
            const cleanup = () => {
                submitBtn.removeEventListener('click', submitHandler);
                cancelBtn.removeEventListener('click', cancelHandler);
                if (input) input.removeEventListener('keypress', keyHandler);
            };
            
            const submitHandler = () => {
                const pubkey = input ? input.value.trim() : '';
                if (pubkey) {
                    cleanup();
                    window.app.hideModal();
                    resolve(pubkey);
                } else {
                    window.notificationService.showError('Please enter a pubkey');
                }
            };
            
            const cancelHandler = () => {
                cleanup();
                window.app.hideModal();
                resolve(null);
            };
            
            const keyHandler = (e) => {
                if (e.key === 'Enter') {
                    submitHandler();
                }
            };
            
            submitBtn.addEventListener('click', submitHandler);
            cancelBtn.addEventListener('click', cancelHandler);
            if (input) {
                input.addEventListener('keypress', keyHandler);
                input.focus();
            }
        });
    }
    
    // Try decryption with provided recipient pubkey
    async _tryDecryptWithRecipientPubkey(email, recipientPubkey) {
        try {
            const keypair = appState.getKeypair();
            if (!keypair) return false;

            const result = await TauriService.decryptEmailBody(
                email.body, email.subject || '',
                null, recipientPubkey
            );

            if (result.success) {
                await this._saveRecipientPubkeyToDb(email, recipientPubkey);
                window.notificationService.showSuccess('Decryption successful! Pubkey saved.');

                // Update subject_hash for DM↔email matching
                if (result.subjectCiphertext && email.message_id) {
                    this.hashStringSHA256(result.subjectCiphertext).then(hash => {
                        window.__TAURI__.core.invoke('db_update_email_subject_hash', {
                            messageId: email.message_id,
                            subjectHash: hash,
                        }).catch(e => console.warn('[JS] Failed to update subject_hash:', e));
                    });
                }
                return true;
            }

            return false;
        } catch (error) {
            console.error('[JS] Error trying decryption with recipient pubkey:', error);
            return false;
        }
    }
    
    // Search through all contacts to find the recipient pubkey that can decrypt this email
    async _searchContactsForRecipientPubkey(email) {
        const keypair = appState.getKeypair();
        if (!keypair) {
            return null;
        }
        
        // Show toast notification
        window.notificationService.showInfo('searching for recipient key...');
        
        // Get all contacts
        const contacts = appState.getContacts();
        if (!contacts || contacts.length === 0) {
            window.notificationService.showError('No contacts available to search');
            return null;
        }
        
        console.log(`[JS] Searching through ${contacts.length} contacts for recipient pubkey...`);
        
        // Try each contact's pubkey
        for (const contact of contacts) {
            if (!contact.pubkey) {
                continue;
            }
            
            try {
                console.log(`[JS] Trying contact pubkey: ${contact.pubkey.substring(0, 16)}...`);
                const success = await this._tryDecryptWithRecipientPubkey(email, contact.pubkey);
                if (success) {
                    console.log(`[JS] Successfully decrypted with contact pubkey: ${contact.pubkey.substring(0, 16)}...`);
                    // Ensure the email in appState is updated (should already be done in _saveRecipientPubkeyToDb)
                    const updatedEmail = appState.getSentEmails().find(e => e.id === email.id || e.message_id === email.message_id);
                    if (updatedEmail && updatedEmail.recipient_pubkey === contact.pubkey) {
                        console.log(`[JS] Verified recipient_pubkey saved to appState for email ${email.id}`);
                    }
                    window.notificationService.showSuccess(`Found recipient key! Decryption successful.`);
                    return contact.pubkey;
                }
            } catch (e) {
                // Continue to next contact
                console.log(`[JS] Failed to decrypt with contact pubkey ${contact.pubkey.substring(0, 16)}...:`, e);
            }
        }
        
        console.log('[JS] No matching recipient pubkey found in contacts');
        window.notificationService.showError('Could not decrypt sent email with any contact key');
        return null;
    }
    
    // Save recipient pubkey to database
    async _saveRecipientPubkeyToDb(email, recipientPubkey) {
        try {
            if (email.id) {
                // Try to update by ID first (more reliable)
                await TauriService.updateEmailRecipientPubkeyById(Number(email.id), recipientPubkey);
            } else if (email.message_id) {
                // Fallback to message_id
                await TauriService.updateEmailRecipientPubkey(email.message_id, recipientPubkey);
            }
            
            // Update the email in appState
            const sentEmails = appState.getSentEmails();
            const emailIndex = sentEmails.findIndex(e => e.id === email.id || e.message_id === email.message_id);
            if (emailIndex !== -1) {
                sentEmails[emailIndex].recipient_pubkey = recipientPubkey;
                appState.setSentEmails(sentEmails);
            }
        } catch (error) {
            console.error('[JS] Error saving recipient pubkey to DB:', error);
            throw error;
        }
    }

    // Download attachment from sent email
    async downloadSentAttachment(emailId, attachmentId) {
        try {
            console.log(`[JS] Downloading attachment ${attachmentId} from sent email ${emailId}`);
            console.log(`[JS] EmailId type: ${typeof emailId}, AttachmentId type: ${typeof attachmentId}`);
            
            const attachment = await TauriService.getAttachment(attachmentId);
            if (!attachment) {
                window.notificationService.showError('Attachment not found');
                return;
            }
            
            // Get the email to check for manifest
            const sentEmails = appState.getSentEmails();
            const email = sentEmails.find(e => e.id == emailId);
            if (!email) {
                window.notificationService.showError('Email not found');
                return;
            }
            
            // Try to decrypt using manifest if:
            // 1. Attachment is marked as manifest_aes, OR
            // 2. Email has encrypted content and we can decrypt the manifest
            const hasManifestEncryption = attachment.encryption_method === 'manifest_aes';
            const hasEncryptedContent = email.body && email.body.includes('BEGIN NOSTR');
            
            if (hasManifestEncryption || (hasEncryptedContent && attachment.filename.endsWith('.dat'))) {
                console.log(`[JS] Attempting to decrypt attachment using manifest (hasManifestEncryption: ${hasManifestEncryption}, hasEncryptedContent: ${hasEncryptedContent})`);
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    return;
                }

                // Use backend to decrypt manifest and get attachment keys
                const recipientPubkey = email.recipient_pubkey || email.nostr_pubkey;
                console.log('[JS] Using backend decrypt_email_body to extract manifest for sent attachment...');
                const decryptResult = await TauriService.decryptEmailBody(
                    email.body, email.subject || '',
                    null, recipientPubkey
                );

                if (!decryptResult.isManifest || !decryptResult.attachments || decryptResult.attachments.length === 0) {
                    window.notificationService.showError('Cannot decrypt attachment: no manifest found');
                    return;
                }

                // Find attachment metadata in manifest
                const opaqueId = attachment.filename.replace(/\.dat$/, '');
                console.log(`[JS] Looking for manifest attachment with id: ${opaqueId} from filename: ${attachment.filename}`);
                const attachmentMeta = decryptResult.attachments.find(a => a.id === opaqueId);

                if (!attachmentMeta) {
                    console.warn(`[JS] Attachment metadata not found in manifest for id: ${opaqueId}, available ids:`, decryptResult.attachments.map(a => a.id));
                    window.notificationService.showError('Attachment metadata not found in manifest');
                    return;
                }

                console.log(`[JS] Found attachment metadata:`, attachmentMeta);

                // Decrypt attachment via backend
                const decryptedAttachment = await TauriService.decryptManifestAttachment(
                    attachment.data,
                    attachmentMeta.keyWrapB64,
                    attachmentMeta.cipherSha256Hex,
                    attachmentMeta.origFilename,
                    attachmentMeta.origMime,
                    opaqueId
                );

                // Save decrypted attachment to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    decryptedAttachment.filename,
                    decryptedAttachment.dataB64,
                    decryptedAttachment.contentType || 'application/octet-stream'
                );

                console.log(`[JS] Downloaded decrypted attachment: ${decryptedAttachment.filename} to ${filePath}`);
                window.notificationService.showSuccess(`Attachment saved to: ${filePath}`);
                
            } else {
                // Plain attachment - save directly to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    attachment.filename, 
                    attachment.data, 
                    attachment.content_type || attachment.mime_type || 'application/octet-stream'
                );
                
                console.log(`[JS] Downloaded plain attachment: ${attachment.filename} to ${filePath}`);
                window.notificationService.showSuccess(`Attachment saved to: ${filePath}`);
            }
            
        } catch (error) {
            console.error('[JS] Failed to download attachment:', error);
            window.notificationService.showError('Failed to download attachment: ' + error.message);
        }
    }

    // Download all attachments from sent email as ZIP
    async downloadAllSentAttachments(emailId) {
        try {
            console.log(`[JS] Downloading all attachments from sent email ${emailId} as ZIP`);
            
            const sentEmails = appState.getSentEmails();
            const email = sentEmails.find(e => e.id == emailId);
            if (!email) {
                window.notificationService.showError('Email not found');
                return;
            }
            
            if (!email.attachments || email.attachments.length === 0) {
                window.notificationService.showError('No attachments to download');
                return;
            }
            
            console.log(`[JS] Processing ${email.attachments.length} attachments for ZIP`);
            
            // Prepare attachments for ZIP
            const attachmentsForZip = [];
            
            // Try to decrypt manifest if email has encrypted content
            // This allows us to decrypt attachments even if they're not marked as manifest_aes
            const hasManifestAttachments = email.attachments.some(att => att.encryption_method === 'manifest_aes');
            const hasEncryptedContent = email.body && email.body.includes('BEGIN NOSTR');
            let manifestResult = null;
            
            if (hasManifestAttachments || (hasEncryptedContent && email.attachments.some(att => att.filename.endsWith('.dat')))) {
                console.log(`[JS] Attempting to decrypt manifest for ZIP (hasManifestAttachments: ${hasManifestAttachments}, hasEncryptedContent: ${hasEncryptedContent})`);
                
                const keypair = appState.getKeypair();
                if (!keypair) return;

                const recipientPubkey = email.recipient_pubkey || email.nostr_pubkey;
                const decryptResult = await TauriService.decryptEmailBody(
                    email.body, email.subject || '',
                    null, recipientPubkey
                );

                if (!decryptResult.isManifest || !decryptResult.attachments || decryptResult.attachments.length === 0) {
                    window.notificationService.showError('Cannot decrypt attachments: no manifest found');
                    return;
                }

                manifestResult = {
                    type: 'manifest',
                    manifest: {
                        attachments: decryptResult.attachments.map(a => ({
                            id: a.id,
                            orig_filename: a.origFilename,
                            orig_mime: a.origMime,
                            key_wrap: a.keyWrapB64,
                            cipher_sha256: a.cipherSha256Hex,
                            cipher_size: a.cipherSize,
                        }))
                    }
                };
            }

            // Process each attachment - decrypt if we have a manifest
            for (const attachment of email.attachments) {
                const shouldDecrypt = attachment.encryption_method === 'manifest_aes' ||
                                    (manifestResult && attachment.filename.endsWith('.dat'));

                if (shouldDecrypt && manifestResult) {
                    const opaqueId = attachment.filename.replace(/\.dat$/, '');
                    const attachmentMeta = manifestResult.manifest.attachments.find(a => a.id === opaqueId);

                    if (!attachmentMeta) {
                        attachmentsForZip.push({ filename: attachment.filename, data: attachment.data });
                        continue;
                    }

                    const decryptedAttachment = await TauriService.decryptManifestAttachment(
                        attachment.data, attachmentMeta.key_wrap, attachmentMeta.cipher_sha256,
                        attachmentMeta.orig_filename, attachmentMeta.orig_mime, opaqueId
                    );

                    attachmentsForZip.push({
                        filename: decryptedAttachment.filename,
                        data: decryptedAttachment.dataB64
                    });
                } else {
                    // Plain attachment - decode base64 if needed
                    let attachmentData = attachment.data;
                    // If data is base64 string, it will be handled by saveAttachmentsAsZip
                    attachmentsForZip.push({
                        filename: attachment.filename,
                        data: attachmentData
                    });
                    
                    console.log(`[JS] Added plain attachment to ZIP: ${attachment.filename}`);
                }
            }
            
            if (attachmentsForZip.length === 0) {
                window.notificationService.showError('No attachments could be processed');
                return;
            }
            
            // Create ZIP filename based on email subject or date
            let emailSubject = email.subject || 'Email';
            
            // If subject looks like encrypted content, use a generic name with timestamp
            if (emailSubject.length > 50 || /^[A-Za-z0-9+/=]+$/.test(emailSubject)) {
                const date = new Date(email.date || Date.now());
                const dateStr = date.toISOString().slice(0, 10); // YYYY-MM-DD
                emailSubject = `Email Attachments ${dateStr}`;
            }
            
            const cleanSubject = emailSubject.replace(/[^a-zA-Z0-9\s\-_]/g, '').trim();
            const zipFilename = `${cleanSubject}.zip`;
            
            console.log(`[JS] Creating ZIP file: ${zipFilename} with ${attachmentsForZip.length} files`);
            
            // Save as ZIP
            const zipPath = await TauriService.saveAttachmentsAsZip(zipFilename, attachmentsForZip);
            
            console.log(`[JS] Successfully created ZIP file: ${zipPath}`);
            window.notificationService.showSuccess(`All attachments saved to: ${zipPath}`);
            
        } catch (error) {
            console.error('[JS] Failed to download all attachments:', error);
            window.notificationService.showError('Failed to download attachments: ' + error.message);
        }
    }

    // Download attachment from inbox email
    async downloadInboxAttachment(emailId, attachmentId, encryptedFilename = null) {
        try {
            console.log(`[JS] Downloading attachment ${attachmentId} from inbox email ${emailId}`);
            
            const attachment = await TauriService.getAttachment(attachmentId);
            if (!attachment) {
                window.notificationService.showError('Attachment not found');
                return;
            }
            
            // Check if raw content is currently being displayed
            const rawBodyInfo = document.getElementById('inbox-raw-body-info');
            const isRawMode = rawBodyInfo && rawBodyInfo.style.display === 'block';
            
            // If in raw mode and attachment is encrypted, download the raw encrypted data
            if (isRawMode && attachment.encryption_method === 'manifest_aes') {
                console.log(`[JS] Raw mode detected, downloading encrypted attachment: ${attachment.filename}`);
                const filePath = await TauriService.saveAttachmentToDisk(
                    encryptedFilename || attachment.filename,
                    attachment.data,
                    attachment.content_type || attachment.mime_type || 'application/octet-stream'
                );
                console.log(`[JS] Downloaded raw encrypted attachment: ${attachment.filename} to ${filePath}`);
                window.notificationService.showSuccess(`Raw attachment saved to: ${filePath}`);
                return;
            }
            
            // Get the email to check for manifest
            const emails = appState.getEmails();
            const email = emails.find(e => e.id == emailId);
            if (!email) {
                window.notificationService.showError('Email not found');
                return;
            }
            
            // Try to decrypt using manifest if:
            // 1. Attachment is marked as manifest_aes, OR
            // 2. Email has encrypted content and we can decrypt the manifest
            const hasManifestEncryption = attachment.encryption_method === 'manifest_aes';
            const hasEncryptedContent = email.body && email.body.includes('BEGIN NOSTR');
            
            if (hasManifestEncryption || (hasEncryptedContent && attachment.filename.endsWith('.dat'))) {
                console.log(`[JS] Attempting to decrypt attachment using manifest (hasManifestEncryption: ${hasManifestEncryption}, hasEncryptedContent: ${hasEncryptedContent})`);
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    return;
                }

                // Use backend to decrypt manifest and get attachment keys
                const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                console.log('[JS] Using backend decrypt_email_body to extract manifest for attachment...');
                const decryptResult = await TauriService.decryptEmailBody(
                    email.body, email.subject || '',
                    senderPubkey, null
                );

                if (!decryptResult.isManifest || !decryptResult.attachments || decryptResult.attachments.length === 0) {
                    window.notificationService.showError('Cannot decrypt attachment: no manifest found');
                    return;
                }

                // Find attachment metadata in manifest
                const opaqueId = attachment.filename.replace(/\.dat$/, '');
                console.log(`[JS] Looking for manifest attachment with id: ${opaqueId} from filename: ${attachment.filename}`);
                const attachmentMeta = decryptResult.attachments.find(a => a.id === opaqueId);

                if (!attachmentMeta) {
                    console.warn(`[JS] Attachment metadata not found in manifest for id: ${opaqueId}, available ids:`, decryptResult.attachments.map(a => a.id));
                    window.notificationService.showError('Attachment metadata not found in manifest');
                    return;
                }

                console.log(`[JS] Found attachment metadata:`, attachmentMeta);

                // Decrypt attachment via backend
                const decryptedAttachment = await TauriService.decryptManifestAttachment(
                    attachment.data,
                    attachmentMeta.keyWrapB64,
                    attachmentMeta.cipherSha256Hex,
                    attachmentMeta.origFilename,
                    attachmentMeta.origMime,
                    opaqueId
                );

                // Save decrypted attachment to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    decryptedAttachment.filename,
                    decryptedAttachment.dataB64,
                    decryptedAttachment.contentType || 'application/octet-stream'
                );
                
                console.log(`[JS] Downloaded decrypted attachment: ${attachmentMeta.orig_filename} to ${filePath}`);
                window.notificationService.showSuccess(`Attachment saved to: ${filePath}`);
                
            } else {
                // Plain attachment - save directly to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    attachment.filename, 
                    attachment.data, 
                    attachment.content_type || attachment.mime_type || 'application/octet-stream'
                );
                
                console.log(`[JS] Downloaded plain attachment: ${attachment.filename} to ${filePath}`);
                window.notificationService.showSuccess(`Attachment saved to: ${filePath}`);
            }
            
        } catch (error) {
            console.error('[JS] Failed to download attachment:', error);
            window.notificationService.showError('Failed to download attachment: ' + error.message);
        }
    }

    // Download all attachments from inbox email as ZIP
    async downloadAllInboxAttachments(emailId) {
        try {
            console.log(`[JS] Downloading all attachments from inbox email ${emailId} as ZIP`);
            
            const emails = appState.getEmails();
            const email = emails.find(e => e.id == emailId);
            if (!email) {
                window.notificationService.showError('Email not found');
                return;
            }
            
            if (!email.attachments || email.attachments.length === 0) {
                window.notificationService.showError('No attachments to download');
                return;
            }
            
            console.log(`[JS] Processing ${email.attachments.length} attachments for ZIP`);
            
            // Prepare attachments for ZIP
            const attachmentsForZip = [];
            
            // Try to decrypt manifest if email has encrypted content
            const hasManifestAttachments = email.attachments.some(att => att.encryption_method === 'manifest_aes');
            const hasEncryptedContent = email.body && email.body.includes('BEGIN NOSTR');
            let manifestResult = null;
            
            if (hasManifestAttachments || (hasEncryptedContent && email.attachments.some(att => att.filename.endsWith('.dat')))) {
                console.log(`[JS] Attempting to decrypt manifest for ZIP (hasManifestAttachments: ${hasManifestAttachments}, hasEncryptedContent: ${hasEncryptedContent})`);
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    return;
                }

                // Use backend to decrypt manifest and get attachment keys
                const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                console.log('[JS] Using backend decrypt_email_body to extract manifest for ZIP...');
                const decryptResult = await TauriService.decryptEmailBody(
                    email.body, email.subject || '',
                    senderPubkey, null
                );

                if (!decryptResult.isManifest || !decryptResult.attachments || decryptResult.attachments.length === 0) {
                    window.notificationService.showError('Cannot decrypt attachments: no manifest found');
                    return;
                }

                manifestResult = {
                    type: 'manifest',
                    manifest: {
                        attachments: decryptResult.attachments.map(a => ({
                            id: a.id,
                            orig_filename: a.origFilename,
                            orig_mime: a.origMime,
                            key_wrap: a.keyWrapB64,
                            cipher_sha256: a.cipherSha256Hex,
                            cipher_size: a.cipherSize,
                        }))
                    }
                };

                console.log(`[JS] Manifest decrypted successfully, has ${manifestResult.manifest.attachments.length} attachments`);
            }

            // Process each attachment - decrypt if we have a manifest
            for (const attachment of email.attachments) {
                const shouldDecrypt = attachment.encryption_method === 'manifest_aes' ||
                                    (manifestResult && attachment.filename.endsWith('.dat'));

                if (shouldDecrypt && manifestResult) {
                    const opaqueId = attachment.filename.replace(/\.dat$/, '');
                    console.log(`[JS] Looking for manifest attachment with id: ${opaqueId} from filename: ${attachment.filename}`);
                    const attachmentMeta = manifestResult.manifest.attachments.find(a => a.id === opaqueId);

                    if (!attachmentMeta) {
                        console.warn(`[JS] Skipping attachment ${attachment.filename}: metadata not found in manifest`);
                        attachmentsForZip.push({
                            filename: attachment.filename,
                            data: attachment.data
                        });
                        continue;
                    }

                    console.log(`[JS] Found attachment metadata:`, attachmentMeta);

                    // Decrypt attachment via backend
                    const decryptedAttachment = await TauriService.decryptManifestAttachment(
                        attachment.data,
                        attachmentMeta.key_wrap,
                        attachmentMeta.cipher_sha256,
                        attachmentMeta.orig_filename,
                        attachmentMeta.orig_mime,
                        opaqueId
                    );

                    attachmentsForZip.push({
                        filename: decryptedAttachment.filename,
                        data: decryptedAttachment.dataB64
                    });

                    console.log(`[JS] Added decrypted attachment to ZIP: ${decryptedAttachment.filename}`);

                } else {
                    // Plain attachment
                    attachmentsForZip.push({
                        filename: attachment.filename,
                        data: attachment.data
                    });
                    
                    console.log(`[JS] Added plain attachment to ZIP: ${attachment.filename}`);
                }
            }
            
            if (attachmentsForZip.length === 0) {
                window.notificationService.showError('No attachments could be processed');
                return;
            }
            
            // Create ZIP filename based on email subject or date
            let emailSubject = email.subject || 'Email';
            
            // If subject looks like encrypted content, use a generic name with timestamp
            if (emailSubject.length > 50 || /^[A-Za-z0-9+/=]+$/.test(emailSubject)) {
                const date = new Date(email.date || Date.now());
                const dateStr = date.toISOString().slice(0, 10); // YYYY-MM-DD
                emailSubject = `Email Attachments ${dateStr}`;
            }
            
            const cleanSubject = emailSubject.replace(/[^a-zA-Z0-9\s\-_]/g, '').trim();
            const zipFilename = `${cleanSubject}.zip`;
            
            console.log(`[JS] Creating ZIP file: ${zipFilename} with ${attachmentsForZip.length} files`);
            
            // Save as ZIP
            const zipPath = await TauriService.saveAttachmentsAsZip(zipFilename, attachmentsForZip);
            
            console.log(`[JS] Successfully created ZIP file: ${zipPath}`);
            window.notificationService.showSuccess(`All attachments saved to: ${zipPath}`);
            
        } catch (error) {
            console.error('[JS] Failed to download all attachments:', error);
            window.notificationService.showError('Failed to download attachments: ' + error.message);
        }
    }

    // Show sent email list
    showSentList() {
        try {
            const sentList = domManager.get('sentList');
            const sentDetailView = domManager.get('sentDetailView');
            const sentThreadDetailView = document.getElementById('sent-thread-detail-view');
            const sentActions = domManager.get('sentActions');
            const sentTitle = domManager.get('sentTitle');
            if (sentList) sentList.style.display = 'block';
            if (sentDetailView) sentDetailView.style.display = 'none';
            if (sentThreadDetailView) sentThreadDetailView.style.display = 'none';
            if (sentActions) sentActions.style.display = 'flex';
            if (sentTitle) {
                sentTitle.textContent = 'Sent';
                sentTitle.style.display = '';
            }
            appState.clearCurrentThread();
        } catch (error) {
            console.error('Error showing sent email list:', error);
        }
    }

    // Load drafts
    async loadDrafts(append = false) {
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
            return;
        }
        try {
            if (!append) {
                // Reset offset when loading fresh (not appending)
                this.draftsOffset = 0;
                domManager.disable('refreshDrafts');
                domManager.setHTML('refreshDrafts', '<span class="loading"></span> Loading...');
            } else {
                // Show loading state on Load More button
                const loadMoreBtn = document.getElementById('load-more-drafts');
                if (loadMoreBtn) {
                    loadMoreBtn.disabled = true;
                    loadMoreBtn.innerHTML = '<span class="loading"></span> Loading...';
                }
            }
            const settings = appState.getSettings();
            const userEmail = settings.email_address ? settings.email_address : null;
            // Get page size from settings (default to 50)
            const pageSize = settings.emails_per_page || 50;
            console.log('[JS] loadDrafts userEmail:', userEmail);
            console.log('[JS] Loading drafts with offset:', this.draftsOffset, 'pageSize:', pageSize);
            
            const drafts = await TauriService.getDrafts(pageSize, this.draftsOffset, userEmail);

            // Load attachments for each draft in parallel with timeout
            const attachmentPromises = drafts.map(async (draft) => {
                try {
                    const emailIdInt = parseInt(draft.id);
                    if (isNaN(emailIdInt)) {
                        if (draft.message_id) {
                            try {
                                const emailData = await TauriService.getDbEmail(draft.message_id);
                                if (emailData && emailData.id) {
                                    const realId = parseInt(emailData.id);
                                    if (!isNaN(realId)) {
                                        draft.attachments = await TauriService.getAttachmentsForEmail(realId);
                                        return;
                                    }
                                }
                            } catch (e) {
                                console.error(`[JS] Failed to get draft by message_id ${draft.message_id}:`, e);
                            }
                        }
                        draft.attachments = [];
                        return;
                    }
                    const timeoutPromise = new Promise((_, reject) =>
                        setTimeout(() => reject(new Error('Attachment load timeout')), 5000)
                    );
                    draft.attachments = await Promise.race([
                        TauriService.getAttachmentsForEmail(emailIdInt),
                        timeoutPromise
                    ]);
                } catch (error) {
                    draft.attachments = [];
                }
            });
            await Promise.allSettled(attachmentPromises);

            if (append) {
                // Append new drafts to existing ones
                const existingDrafts = appState.getDrafts();
                appState.setDrafts([...existingDrafts, ...drafts]);
            } else {
                // Replace drafts (fresh load)
                appState.setDrafts(drafts);
            }
            
            // Update offset for next load
            this.draftsOffset += drafts.length;
            
            // Show Load More button if we got a full page of results
            this.renderDrafts(drafts.length === pageSize);
        } catch (error) {
            console.error('Failed to load drafts:', error);
            notificationService.showError('Failed to load drafts: ' + error);
        } finally {
            if (!append) {
                domManager.enable('refreshDrafts');
                domManager.setHTML('refreshDrafts', '<i class="fas fa-sync"></i> Refresh');
            } else {
                const loadMoreBtn = document.getElementById('load-more-drafts');
                if (loadMoreBtn) {
                    loadMoreBtn.disabled = false;
                    loadMoreBtn.innerHTML = '<i class="fas fa-chevron-down"></i> Load More';
                }
            }
        }
    }

    // Load more drafts (pagination)
    async loadMoreDrafts() {
        await this.loadDrafts(true);
    }

    // Render drafts
    async renderDrafts(showLoadMore = false) {
        const draftsList = domManager.get('draftsList');
        if (!draftsList) return;
        try {
            // Remove existing Load More button if it exists
            const existingLoadMoreBtn = document.getElementById('load-more-drafts');
            if (existingLoadMoreBtn) {
                existingLoadMoreBtn.remove();
            }

            // Get all drafts from state
            const drafts = appState.getDrafts();

            if (!drafts || drafts.length === 0) {
                draftsList.innerHTML = '<div class="text-center text-muted">No drafts found</div>';
                return;
            }

            // Always re-render all drafts (simpler approach)
            draftsList.innerHTML = '';

            // Check if we should hide unverified messages
            const settings = appState.getSettings();
            const hideUnverified = settings && settings.hide_unsigned_messages === true;

            // Process drafts in parallel with timeout protection
            const draftPromises = drafts
                .filter(draft => {
                    if (hideUnverified && draft.signature_valid !== true) {
                        return false;
                    }
                    return true;
                })
                .map(async (draft) => {
                    try {
                        const timeoutPromise = new Promise((_, reject) =>
                            setTimeout(() => reject(new Error('Decryption timeout')), 5000)
                        );
                        const renderPromise = this.renderDraftItem(draft);
                        return await Promise.race([renderPromise, timeoutPromise]);
                    } catch (error) {
                        console.error(`[JS] Error rendering draft ${draft.message_id}:`, error);
                        return this.renderDraftItemBasic(draft);
                    }
                });

            const renderedItems = await Promise.allSettled(draftPromises);

            let renderedCount = 0;
            for (const result of renderedItems) {
                if (result.status === 'fulfilled' && result.value) {
                    draftsList.appendChild(result.value);
                    renderedCount++;
                }
            }

            // Show message if no drafts were rendered (possibly all filtered out)
            if (renderedCount === 0) {
                const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
                if (hideUndecryptable && drafts.length > 0) {
                    draftsList.innerHTML = '<div class="text-center text-muted">No decryptable drafts found. All drafts are encrypted for a different keypair.</div>';
                } else if (hideUnverified && drafts.length > 0) {
                    draftsList.innerHTML = '<div class="text-center text-muted">No verified drafts found. All drafts have missing or invalid signatures.</div>';
                } else {
                    draftsList.innerHTML = '<div class="text-center text-muted">No drafts found</div>';
                }
                return;
            }

            // Add Load More button if there might be more drafts
            if (showLoadMore) {
                const loadMoreBtn = document.createElement('button');
                loadMoreBtn.id = 'load-more-drafts';
                loadMoreBtn.className = 'btn btn-secondary';
                loadMoreBtn.style.cssText = 'width: 100%; margin-top: 15px; padding: 12px;';
                loadMoreBtn.innerHTML = '<i class="fas fa-chevron-down"></i> Load More';
                loadMoreBtn.addEventListener('click', () => this.loadMoreDrafts());
                draftsList.appendChild(loadMoreBtn);
            }
        } catch (error) {
            console.error('Error rendering drafts:', error);
            draftsList.innerHTML = '<div class="text-center text-muted">Error loading drafts</div>';
        }
    }

    // Render a single draft item (with decryption)
    async renderDraftItem(draft) {
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

        let previewText = '';
        let showSubject = true;
        let previewSubject = draft.subject;

        // Check the FIRST/outermost BEGIN NOSTR block to determine message type.
        const draftFirstBegin = draft.body && draft.body.match(/-{3,}\s*BEGIN NOSTR ((?:NIP-\d+ ENCRYPTED|SIGNED) (?:MESSAGE|BODY))\s*-{3,}/);
        const draftOuterIsEncrypted = draftFirstBegin && /NIP-\d+ ENCRYPTED/.test(draftFirstBegin[1]);

        if (draftOuterIsEncrypted) {
            const keypair = appState.getKeypair();
            if (!keypair) {
                previewText = 'Unable to decrypt: no keypair';
            } else {
                try {
                    const recipientPubkey = draft.recipient_pubkey || draft.nostr_pubkey || draft.sender_pubkey;
                    const result = await TauriService.decryptEmailBody(
                        draft.body, draft.subject || '',
                        recipientPubkey, null
                    );
                    if (result.success) {
                        previewSubject = result.subject || draft.subject;
                        previewText = Utils.escapeHtml(result.body.substring(0, 100));
                        if (result.body.length > 100) previewText += '...';
                        showSubject = true;
                    } else {
                        previewText = 'Could not decrypt';
                    }
                } catch (e) {
                    console.error('[JS] Backend decrypt failed for draft preview:', e);
                    previewText = 'Could not decrypt';
                }
            }
        } else {
            // Decode glossia signed message body for preview
            let draftPreviewBody = draft.body || '';
            const draftSignedMsg = await this.decodeGlossiaSignedMessage(draftPreviewBody);
            if (draftSignedMsg && draftSignedMsg.plaintextBody) {
                draftPreviewBody = draftSignedMsg.plaintextBody;
            }
            previewText = Utils.escapeHtml(draftPreviewBody ? draftPreviewBody.substring(0, 100) : '');
            if (draftPreviewBody && draftPreviewBody.length > 100) previewText += '...';
            showSubject = true;
        }

        // Check if draft is decryptable (for filtering)
        const settings = appState.getSettings();
        const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
        const isDecryptable = !previewText.includes('Unable to decrypt') &&
                            !previewText.includes('Your private key could not decrypt this message') &&
                            !previewText.includes('could not decrypt') &&
                            !previewText.includes('Could not decrypt') &&
                            previewText !== 'Unable to decrypt: no keypair' &&
                            previewSubject !== 'Could not decrypt';

        if (hideUndecryptable && !isDecryptable) {
            return null;
        }

        // Add attachment indicator
        const attachmentCount = draft.attachments ? draft.attachments.length : 0;
        const attachmentIndicator = attachmentCount > 0 ?
            `<span class="attachment-indicator" title="${attachmentCount} attachment${attachmentCount > 1 ? 's' : ''}">📎 ${attachmentCount}</span>` : '';

        // Add signature verification indicator
        let signatureIndicator = '';
        const draftSigSource = draft.signature_source ? ` (${draft.signature_source})` : '';
        if (draft.signature_valid === true || draft.signature_valid === 1) {
            signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature${draftSigSource}"><i class="fas fa-pen"></i> Signature Verified</span>`;
        } else if (draft.signature_valid === false || draft.signature_valid === 0) {
            signatureIndicator = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(draft.message_id || draft.id)}" title="Invalid Nostr signature"><i class="fas fa-pen"></i> Signature Invalid</span>`;
        }

        // Add transport authentication indicator
        let transportAuthIndicator = '';
        if (draft.transport_auth_verified === true || draft.transport_auth_verified === 1) {
            transportAuthIndicator = `<span class="transport-auth-indicator verified" title="Email transport authentication verified (DMARC/DKIM/SPF)"><i class="fas fa-envelope"></i> Email Verified</span>`;
        } else if (draft.transport_auth_verified === false || draft.transport_auth_verified === 0) {
            transportAuthIndicator = `<span class="transport-auth-indicator invalid" title="Email transport authentication failed"><i class="fas fa-envelope"></i> Email Unverified</span>`;
        }

        // Get recipient contact for avatar
        const recipientEmail = draft.to_address;
        const contacts = appState.getContacts();
        let recipientContact = null;

        if (recipientEmail) {
            const normalizeGmail = (email) => {
                const lower = email.trim().toLowerCase();
                if (lower.includes('@gmail.com')) {
                    const [local, domain] = lower.split('@');
                    const normalizedLocal = local.split('+')[0];
                    return `${normalizedLocal}@${domain}`;
                }
                return lower;
            };

            const normalizedEmail = normalizeGmail(recipientEmail);
            recipientContact = contacts.find(c => {
                if (!c.email) return false;
                const contactEmail = c.email.trim().toLowerCase();
                return contactEmail === recipientEmail.toLowerCase() || contactEmail === normalizedEmail;
            });
        }

        // Avatar fallback logic
        const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
        let avatarSrc = defaultAvatar;
        let avatarClass = 'contact-avatar';
        const isValidDataUrl = recipientContact && recipientContact.picture_data_url && recipientContact.picture_data_url.startsWith('data:image') && recipientContact.picture_data_url !== 'data:application/octet-stream;base64,';
        if (recipientContact && recipientContact.picture_loading) {
            avatarClass += ' loading';
        } else if (isValidDataUrl) {
            avatarSrc = recipientContact.picture_data_url;
        } else if (recipientContact && recipientContact.picture_data_url && !isValidDataUrl && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        } else if (recipientContact && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        }

        draftElement.innerHTML = `
            <img class="${avatarClass}" src="${avatarSrc}" alt="${Utils.escapeHtml(draft.to_address)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
            <div class="email-content">
                <div class="email-header">
                    <div class="email-sender email-list-strong">To: ${Utils.escapeHtml(draft.to_address)} ${attachmentIndicator} ${signatureIndicator} ${transportAuthIndicator}</div>
                    <div class="email-date">${dateDisplay}</div>
                </div>
                ${showSubject ? `<div class="email-subject email-list-strong">${Utils.escapeHtml(previewSubject)}</div>` : ''}
                <div class="email-preview">${previewText}</div>
            </div>
            <div class="email-actions">
                <button class="btn btn-primary btn-small draft-edit-btn">
                    <i class="fas fa-edit"></i>
                </button>
                <button class="btn btn-danger btn-small draft-delete-btn">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;

        // Add event listeners for action buttons (avoids inline onclick with serialized JSON)
        const editBtn = draftElement.querySelector('.draft-edit-btn');
        if (editBtn) {
            editBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                this.loadDraftToCompose(draft);
            });
        }
        const deleteBtn = draftElement.querySelector('.draft-delete-btn');
        if (deleteBtn) {
            deleteBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                this.deleteDraftFromList(draft.message_id);
            });
        }

        // Add hover and click handlers for invalid signature indicator
        if (draft.signature_valid === false || draft.signature_valid === 0) {
            const sigIndicator = draftElement.querySelector('.signature-indicator.invalid');
            if (sigIndicator) {
                const originalText = sigIndicator.textContent;
                sigIndicator.addEventListener('mouseenter', () => {
                    sigIndicator.textContent = 'recheck signature?';
                });
                sigIndicator.addEventListener('mouseleave', () => {
                    sigIndicator.textContent = originalText;
                });
                sigIndicator.addEventListener('click', async (e) => {
                    e.stopPropagation();
                    const messageId = sigIndicator.dataset.messageId;
                    if (messageId) {
                        sigIndicator.textContent = 'checking...';
                        sigIndicator.style.opacity = '0.7';
                        try {
                            const result = await TauriService.recheckEmailSignature(messageId);
                            if (result === true) {
                                draft.signature_valid = true;
                                sigIndicator.className = 'signature-indicator verified';
                                sigIndicator.innerHTML = '<i class="fas fa-pen"></i> Signature Verified';
                                sigIndicator.title = 'Verified Nostr signature';
                                sigIndicator.removeAttribute('data-message-id');
                                sigIndicator.replaceWith(sigIndicator.cloneNode(true));
                                notificationService.showSuccess('Signature verified successfully!');
                            } else if (result === false) {
                                sigIndicator.textContent = originalText;
                                notificationService.showError('Signature is still invalid');
                            } else {
                                sigIndicator.textContent = originalText;
                                notificationService.showWarning('Could not verify signature (missing pubkey or signature)');
                            }
                        } catch (error) {
                            console.error('[JS] Failed to recheck signature:', error);
                            sigIndicator.textContent = originalText;
                            notificationService.showError('Failed to recheck signature: ' + error);
                        } finally {
                            sigIndicator.style.opacity = '1';
                        }
                    }
                });
            }
        }

        draftElement.addEventListener('click', () => this.loadDraftToCompose(draft));
        return draftElement;
    }

    // Render a basic draft item (without decryption, used as fallback)
    renderDraftItemBasic(draft) {
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

        const attachmentCount = draft.attachments ? draft.attachments.length : 0;
        const attachmentIndicator = attachmentCount > 0 ?
            `<span class="attachment-indicator" title="${attachmentCount} attachment${attachmentCount > 1 ? 's' : ''}">📎 ${attachmentCount}</span>` : '';

        // Get recipient contact for avatar
        const recipientEmail = draft.to_address;
        const contacts = appState.getContacts();
        let recipientContact = null;

        if (recipientEmail) {
            const normalizeGmail = (email) => {
                const lower = email.trim().toLowerCase();
                if (lower.includes('@gmail.com')) {
                    const [local, domain] = lower.split('@');
                    const normalizedLocal = local.split('+')[0];
                    return `${normalizedLocal}@${domain}`;
                }
                return lower;
            };

            const normalizedEmail = normalizeGmail(recipientEmail);
            recipientContact = contacts.find(c => {
                if (!c.email) return false;
                const contactEmail = c.email.trim().toLowerCase();
                return contactEmail === recipientEmail.toLowerCase() || contactEmail === normalizedEmail;
            });
        }

        // Avatar fallback logic
        const defaultAvatar = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>');
        let avatarSrc = defaultAvatar;
        let avatarClass = 'contact-avatar';
        const isValidDataUrl = recipientContact && recipientContact.picture_data_url && recipientContact.picture_data_url.startsWith('data:image') && recipientContact.picture_data_url !== 'data:application/octet-stream;base64,';
        if (recipientContact && recipientContact.picture_loading) {
            avatarClass += ' loading';
        } else if (isValidDataUrl) {
            avatarSrc = recipientContact.picture_data_url;
        } else if (recipientContact && recipientContact.picture_data_url && !isValidDataUrl && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        } else if (recipientContact && recipientContact.picture) {
            avatarSrc = recipientContact.picture;
        }

        draftElement.innerHTML = `
            <img class="${avatarClass}" src="${avatarSrc}" alt="${Utils.escapeHtml(draft.to_address)}'s avatar" onerror="this.onerror=null;this.src='${defaultAvatar}';this.className='contact-avatar';">
            <div class="email-content">
                <div class="email-header">
                    <div class="email-sender email-list-strong">To: ${Utils.escapeHtml(draft.to_address)} ${attachmentIndicator}</div>
                    <div class="email-date">${dateDisplay}</div>
                </div>
                <div class="email-subject email-list-strong">${Utils.escapeHtml(draft.subject)}</div>
                <div class="email-preview">${Utils.escapeHtml(draft.body ? draft.body.substring(0, 100) : '')}</div>
            </div>
            <div class="email-actions">
                <button class="btn btn-primary btn-small draft-edit-btn">
                    <i class="fas fa-edit"></i>
                </button>
                <button class="btn btn-danger btn-small draft-delete-btn">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        `;

        const editBtn = draftElement.querySelector('.draft-edit-btn');
        if (editBtn) {
            editBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                this.loadDraftToCompose(draft);
            });
        }
        const deleteBtn = draftElement.querySelector('.draft-delete-btn');
        if (deleteBtn) {
            deleteBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                this.deleteDraftFromList(draft.message_id);
            });
        }

        draftElement.addEventListener('click', () => this.loadDraftToCompose(draft));
        return draftElement;
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
                const draftRecipientPubkey = draft.recipient_pubkey || draft.nostr_pubkey; // Fallback for backward compatibility
                if (draft.is_nostr_encrypted && draftRecipientPubkey) {
                    const contacts = appState.getContacts();
                    const contact = contacts.find(c => c.pubkey === draftRecipientPubkey);
                    if (contact) {
                        this.selectedNostrContact = contact;
                        // Update the Nostr contact dropdown
                        const dropdown = domManager.get('nostrContactSelect');
                        if (dropdown) {
                            dropdown.value = contact.pubkey;
                        }
                        // Display the recipient pubkey
                        const pubkeyDisplay = document.getElementById('selected-recipient-pubkey');
                        const pubkeyValue = document.getElementById('recipient-pubkey-value');
                        if (pubkeyDisplay && pubkeyValue) {
                            pubkeyValue.value = contact.pubkey;
                            // pubkey field is always visible
                        }
                        // Update the UI to show it's an encrypted email
                        const toAddressInput = domManager.get('toAddress');
                        if (toAddressInput) {
                            toAddressInput.style.borderColor = '#667eea';
                            toAddressInput.style.backgroundColor = this.getNostrContactInputBackgroundColor();
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
            if (await window.__TAURI__.dialog.confirm('Are you sure you want to delete this draft?', { title: 'Delete Draft', kind: 'warning' })) {
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
                    
                    // Hide pubkey display
                    const pubkeyDisplay = document.getElementById('selected-recipient-pubkey');
                    if (pubkeyDisplay) {
                        // pubkey field is always visible
                    }
                    
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
                const cleanedBody = draft.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('BEGIN NOSTR')).join('\n').trim();
                // For drafts, use recipient_pubkey for decryption (drafts are emails we're preparing to send)
                const draftRecipientPubkey = draft.recipient_pubkey || draft.nostr_pubkey; // Fallback for backward compatibility
                const isEncryptedSubject = Utils.isLikelyEncryptedContent(draft.subject);
                const encryptedBodyMatch = cleanedBody.match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                let decryptedSubject = draft.subject;
                let decryptedBody = cleanedBody;
                const keypair = appState.getKeypair();
                
                if ((isEncryptedSubject || encryptedBodyMatch) && keypair) {
                    (async () => {
                        try {
                            const recipientPubkey = draftRecipientPubkey;
                            const result = await TauriService.decryptEmailBody(
                                draft.body, draft.subject || '',
                                recipientPubkey, null
                            );
                            if (result.success) {
                                decryptedSubject = result.subject || draft.subject;
                                decryptedBody = result.body;
                            } else {
                                decryptedSubject = 'Could not decrypt';
                                decryptedBody = result.error || 'Could not decrypt';
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
                            if (await window.__TAURI__.dialog.confirm('Are you sure you want to delete this draft?', { title: 'Delete Draft', kind: 'warning' })) {
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