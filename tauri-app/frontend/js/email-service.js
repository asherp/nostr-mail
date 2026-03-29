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
        
        const privkey = appState.getKeypair()?.private_key;
        const pubkey = this.selectedNostrContact.pubkey;
        
        if (!privkey || !pubkey) {
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
                privkey, 
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
        
        if (!body) return;
        
        // Check if body contains encrypted content
        const encParts = this.parseArmorComponents(body);
        const isEncryptedBody = !!body.match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}/);
        if (!encParts || !isEncryptedBody) {
            console.log('[JS] Body is not encrypted');
            return;
        }

        if (!this.selectedNostrContact) {
            window.notificationService.showError('Please select a Nostr contact first');
            return;
        }

        const privkey = appState.getKeypair()?.private_key;
        const pubkey = this.selectedNostrContact.pubkey;

        if (!privkey || !pubkey) {
            window.notificationService.showError('Missing decryption keys');
            return;
        }

        try {
            console.log('[JS] Decrypting body content...');
            const decryptQuotedArmor = encParts.quotedArmor;
            let encryptedContent = encParts.bodyText;

            // Check if content is glossia-encoded (not raw base64) and decode first
            const stripped = encryptedContent.replace(/\s+/g, '');
            if (!/^[A-Za-z0-9+/=?]+$/.test(stripped)) {
                // Content contains non-base64 chars — likely glossia-encoded
                const gs = window.GlossiaService;
                if (gs && gs.isReady()) {
                    console.log('[JS] Glossia-decoding armored body content before decrypt...');
                    const meta = this.getGlossiaEncoding() || null;
                    const instruction = meta ? `decode from ${meta}` : 'decode';
                    const result = gs.transcode(encryptedContent, instruction);
                    let decoded = result.output;
                    // Base-N codec returns hex; convert to base64 for NIP decrypt
                    if (gs._isHex(decoded)) decoded = gs._hexToBase64(decoded);
                    encryptedContent = gs._autoUnpack ? gs._autoUnpack(decoded) : decoded;
                } else {
                    console.warn('[JS] Glossia not ready, cannot decode body content');
                }
            } else {
                encryptedContent = stripped;
            }
            
            // Create a mock email object for the decryption function
            const mockEmail = {
                sender_pubkey: pubkey,
                nostr_pubkey: pubkey // Fallback for backward compatibility
            };
            
            // Preserve quoted armor (reply chain) from inside the armor block
            let trailingQuoted = '';
            if (decryptQuotedArmor) {
                trailingQuoted = '\n\n' + decryptQuotedArmor;
            }

            // Try manifest decryption first
            const manifestResult = await this.decryptManifestMessage(mockEmail, encryptedContent, { private_key: privkey });

            if (manifestResult.type === 'manifest') {
                console.log('[JS] Successfully decrypted manifest body');
                domManager.setValue('messageBody', manifestResult.body + trailingQuoted);
                window.notificationService.showSuccess('Body decrypted successfully');
                // Clear signature when decrypting (body state changed)
                this.clearSignature();
            } else if (manifestResult.type === 'legacy') {
                console.log('[JS] Successfully decrypted legacy body');
                domManager.setValue('messageBody', manifestResult.body + trailingQuoted);
                window.notificationService.showSuccess('Body decrypted successfully');
                // Clear signature when decrypting (body state changed)
                this.clearSignature();
            }
            
        } catch (error) {
            console.error('[JS] Failed to decrypt body:', error);
            window.notificationService.showError('Failed to decrypt body content');
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
        this.currentMessageId = null; // Reset message ID when clearing draft
        this._quotedOriginalArmor = null; // Clear quoted reply armor
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
            sigHtml = `
  <hr style="border:none;border-top:1px solid #ccc;margin:1.5em 0;">
  <h4 style="margin:0 0 0.5em;color:#666;font-size:0.9em;">${sigLabel}</h4>
  <div style="border-left:2px solid #ccc;padding-left:1em;color:#888;font-style:italic;overflow-wrap:break-word;">
    ${escHtml(sigContent)}
  </div>`;
        }

        let sealHtml = '';
        // Only show separate seal block for unsigned messages (not when using combined sig+pubkey)
        if (encodedPubkey && !encodedSigPubkey) {
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
    buildRecursiveQuotedHtml(armorText, plaintextFallback) {
        if (!armorText) return null;
        const qParts = this.parseArmorComponents(armorText);
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
            deeperHtml = this.buildRecursiveQuotedHtml(qParts.quotedArmor, deeperFallback);
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
                if (gs && gs.isReady()) {
                    try {
                        const bytes = gs.transcodeToBytes(qParts.bodyText);
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

    parseArmorComponents(armorText) {
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
            const split = this._splitSigPubkey(allContent);
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

    // Split combined signature block content into sig (64 bytes) and pubkey (32 bytes).
    // Decodes the full block as 96 bytes, splits at byte boundary.
    // Returns { sigHex, pubkeyHex } with pre-decoded hex strings, or null if decode fails.
    _splitSigPubkey(content) {
        if (!content) return null;
        const gs = window.GlossiaService;

        // Try glossia decode: 96 bytes total → 192 hex chars
        if (gs && gs.isReady()) {
            try {
                const detections = gs.detectDialect(content);
                const dialect = (Array.isArray(detections) && detections.length > 0) ? detections[0].language : null;
                if (dialect) {
                    const hex = gs.decodeRawBaseN(content, dialect, 96);
                    if (hex && hex.length === 192) {
                        return { sigHex: hex.substring(0, 128), pubkeyHex: hex.substring(128) };
                    }
                }
            } catch (_) {}
        }

        // Fallback: raw hex (no glossia encoding)
        const stripped = content.replace(/\s+/g, '');
        if (/^[0-9a-fA-F]{192}$/.test(stripped)) {
            return { sigHex: stripped.substring(0, 128), pubkeyHex: stripped.substring(128) };
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
                // Combined 96-byte payload (glossia mode)
                lines.push(wordWrap(encodedSigPubkey));
            } else {
                // Hex mode: separate sig + pubkey encodings
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
                use_tls: useTls,
                // Only include private key if we should sign
                private_key: shouldSign && appState.getKeypair() ? appState.getKeypair().private_key : null
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

                // Send encrypted email using NIP-04
                await this.sendEncryptedEmail(emailConfig, contact, subject, body, messageId, toAddress);
            } else {
                // Send regular email
                console.log('[JS] Sending regular email');
                
                // Send email with attachments
                const attachmentData = this.prepareAttachmentsForEmail();
                console.log('[JS] Sending email with attachments:', attachmentData);
                
                const plainBody = this._plainBody || body;
                await TauriService.sendEmail(emailConfig, toAddress, subject, plainBody, null, messageId, attachmentData, this._htmlBody);
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
                use_tls: useTls,
                // Only include private key if we should sign
                private_key: shouldSign && appState.getKeypair() ? appState.getKeypair().private_key : null
            };
            
            // Construct headers (sender's pubkey will be extracted from private_key in backend)
            const headers = await TauriService.constructEmailHeaders(emailConfig, toAddress, previewSubject, previewBody, nostrNpub, messageId, null, this._htmlBody);
            
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
                    // Use depth-counting parser (parseArmorComponents) to properly handle
                    // nested reply armor, matching Rust extract_ciphertext_binary behavior.
                    let dataBytes;
                    const normalized = previewBody.replace(/\r\n/g, '\n');
                    const parts = this.parseArmorComponents(normalized);
                    const isEncrypted = !!normalized.match(/-{3,}\s*BEGIN NOSTR (?:NIP-(?:04|44) ENCRYPTED)/);
                    if (parts && parts.bodyText) {
                        dataBytes = this._decodeArmorBodyToBytes(parts.bodyText, isEncrypted);
                        if (dataBytes && parts.quotedArmor) {
                            const allQuotedBytes = this._extractAllBodyBytes(parts.quotedArmor);
                            if (allQuotedBytes) {
                                dataBytes = this._concatBytes(dataBytes, allQuotedBytes);
                            }
                        }
                        console.log('[JS] Header sig verify: decoded body bytes=', dataBytes?.length);
                    }
                    // Final fallback: raw UTF-8 bytes of the entire body (matches Rust fallback)
                    if (!dataBytes) {
                        console.log('[JS] Header sig verify: fallback to raw UTF-8 bytes');
                        dataBytes = new TextEncoder().encode(previewBody);
                    }
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
                    const sigResults = await this.verifyAllSignatures(plainBody);
                    if (sigResults.length > 0) {
                        htmlToRender = this.injectHtmlSigBadge(htmlToRender, sigResults);
                    }
                } catch (e) {
                    console.warn('[JS] Preview inline signature verification failed:', e);
                }
                Utils.renderHtmlBodyInIframe('preview-html-body', htmlToRender);
            };
            renderHtmlWithSigBadge();
        } else if (hasHtml) {
            Utils.renderHtmlBodyInIframe('preview-html-body', this._htmlBody);
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
                // Use the encrypted subject for the DM to match the email subject blob
                // Since we know the subject is already encrypted, we'll pass it as encrypted content
                try {
                    const dmResult = await TauriService.sendEncryptedDirectMessage(
                        keypair.private_key,
                        contact.pubkey,
                        subject, // This is already encrypted content
                        activeRelays
                    );
                    console.log('[JS] DM sent successfully, event ID:', dmResult);
                    notificationService.showSuccess(`DM sent successfully (event ID: ${dmResult.substring(0, 16)}...)`);
                } catch (dmError) {
                    console.error('[JS] Failed to send DM:', dmError);
                    notificationService.showError('Email sent but DM failed: ' + dmError);
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
            await TauriService.sendEmail(emailConfig, recipientEmail, subject, plainBody, null, messageId, attachmentData, this._htmlBody);
            console.log('[JS] Encrypted email sent successfully');
        } catch (error) {
            console.error('[JS] Error sending encrypted email:', error);
            throw new Error(`Failed to send encrypted email: ${error}`);
        }
    }

    // Load emails
    async loadEmails(searchQuery = '', append = false) {
        if (!appState.hasSettings()) {
            notificationService.showError('Please configure your email settings first');
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
                use_tls: settings.use_tls,
                private_key: keypair ? keypair.private_key : null
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
            const emails = await TauriService.getDbEmails(pageSize, this.inboxOffset, nostrOnly, userEmail, userPubkey);
            
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
            
            if (append) {
                // Append new emails to existing ones
                const existingEmails = appState.getEmails();
                appState.setEmails([...existingEmails, ...emails]);
            } else {
                // Replace emails (fresh load)
                appState.setEmails(emails);
            }
            
            // Update offset for next load
            this.inboxOffset += emails.length;
            
            // Show Load More button if we got a full page of results
            this.renderEmails(emails.length === pageSize);
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
            let emails = await TauriService.getDbSentEmails(pageSize, this.sentOffset, userEmail);
            
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
            
            if (append) {
                // Append new emails to existing ones
                const existingEmails = appState.getSentEmails();
                appState.setSentEmails([...existingEmails, ...emails]);
            } else {
                // Replace emails (fresh load)
                appState.setSentEmails(emails);
            }
            
            // Update offset for next load
            this.sentOffset += emails.length;
            
            // Show Load More button if we got a full page of results
            this.renderSentEmails(emails.length === pageSize);
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
                const keypair = appState.getKeypair();
                const userEmail = settings.email_address ? settings.email_address : null;
                const privateKey = keypair ? keypair.private_key : null;
                
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
                const result = await TauriService.searchSentEmails(searchQuery, userEmail, privateKey, pageSize, this.sentSearchOffset);
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

    // Decrypt manifest-based encrypted message
    async decryptManifestMessage(email, encryptedContent, keypair) {
        console.log('[JS] Attempting manifest decryption for email:', email.from || email.from_address);
        
        try {
            // First decrypt the manifest using NIP encryption
            let decryptedManifestJson;
            
            // Try with sender pubkey first (for inbox emails)
            const senderPubkey = email.sender_pubkey || email.nostr_pubkey; // Fallback for backward compatibility
            if (senderPubkey) {
                try {
                    console.log('[JS] Decrypting with sender pubkey...');
                    console.log('[JS] privateKey:', keypair.private_key ? 'present' : 'missing');
                    console.log('[JS] senderPubkey:', senderPubkey);
                    console.log('[JS] encryptedContent:', encryptedContent ? encryptedContent.substring(0, 50) + '...' : 'missing');
                    decryptedManifestJson = await TauriService.decryptDmContent(keypair.private_key, senderPubkey, encryptedContent);
                } catch (e) {
                    console.log('[JS] Failed with header pubkey:', e.message);
                }
            }
            
            // If that failed, try fallback methods
            if (!decryptedManifestJson) {
                decryptedManifestJson = await this.decryptNostrMessageWithFallback(email, encryptedContent, keypair);
            }
            
            // Try to parse as manifest JSON, if it fails it's probably legacy format
            let manifest;
            try {
                manifest = JSON.parse(decryptedManifestJson);
                console.log('[JS] Manifest parsed successfully:', manifest);
            } catch (parseError) {
                console.log('[JS] Not a manifest format, treating as legacy encrypted body');
                // Return the decrypted content as legacy format (with UTF-8 fixes)
                return {
                    type: 'legacy',
                    body: Utils.fixUtf8Encoding(decryptedManifestJson)
                };
            }
            
            // Decrypt body if present
            let decryptedBody = '';
            if (manifest.body && manifest.body.ciphertext) {
                console.log('[JS] Decrypting body from manifest...');
                console.log('[JS] Body ciphertext:', manifest.body.ciphertext);
                console.log('[JS] Body key_wrap:', manifest.body.key_wrap);
                
                const bodyKey = manifest.body.key_wrap;
                const encryptedBodyData = manifest.body.ciphertext;
                
                // Verify hash if present
                if (manifest.body.cipher_sha256) {
                    const actualHash = await this.calculateSHA256(encryptedBodyData);
                    console.log('[JS] Expected hash:', manifest.body.cipher_sha256);
                    console.log('[JS] Actual hash:', actualHash);
                    if (actualHash !== manifest.body.cipher_sha256) {
                        console.warn('[JS] Body hash mismatch!');
                    }
                }
                
                try {
                    // Decrypt body with AES
                    console.log('[JS] Attempting AES decryption...');
                    console.log('[JS] Input data length:', encryptedBodyData.length);
                    console.log('[JS] Key length:', bodyKey.length);
                    
                    const decryptedBodyBase64 = await this.decryptWithAES(encryptedBodyData, bodyKey);
                    console.log('[JS] Decrypted base64:', decryptedBodyBase64);
                    console.log('[JS] Decrypted base64 length:', decryptedBodyBase64.length);
                    
                    decryptedBody = atob(decryptedBodyBase64);
                    console.log('[JS] Final decrypted body:', decryptedBody);
                    console.log('[JS] Body decrypted successfully, length:', decryptedBody.length);
                } catch (aesError) {
                    console.error('[JS] AES decryption failed:', aesError);
                    console.error('[JS] Error details:', aesError.stack);
                    // Don't throw - let's see if we can continue with empty body
                    console.log('[JS] Continuing with empty body due to AES error');
                    decryptedBody = `[AES Decryption Failed: ${aesError.message}]`;
                }
            } else {
                console.log('[JS] No body ciphertext found in manifest');
            }
            
            return {
                type: 'manifest',
                body: Utils.fixUtf8Encoding(decryptedBody),
                manifest: manifest
            };
            
        } catch (error) {
            console.error('[JS] Manifest decryption failed:', error);
            throw error;
        }
    }

    // Decrypt manifest-based attachment
    async decryptManifestAttachment(email, attachmentId, manifest, keypair) {
        console.log('[JS] Decrypting manifest attachment:', attachmentId);
        
        // Find attachment metadata in manifest
        const attachmentMeta = manifest.attachments.find(a => a.id === attachmentId);
        if (!attachmentMeta) {
            throw new Error(`Attachment ${attachmentId} not found in manifest`);
        }
        
        // Find the actual attachment file in the email
        const attachments = await TauriService.getAttachmentsForEmail(email.id);
        const attachmentFile = attachments.find(a => a.filename === `${attachmentId}.dat`);
        if (!attachmentFile) {
            throw new Error(`Attachment file ${attachmentId}.dat not found`);
        }
        
        try {
            // Verify hash if present
            if (attachmentMeta.cipher_sha256) {
                const actualHash = await this.calculateSHA256(attachmentFile.data);
                if (actualHash !== attachmentMeta.cipher_sha256) {
                    console.warn(`[JS] Attachment ${attachmentId} hash mismatch!`);
                }
            }
            
            // Decrypt attachment with AES key from manifest (with padding removal)
            const decryptedData = await this.decryptWithAES(attachmentFile.data, attachmentMeta.key_wrap, true);
            
            console.log(`[JS] Attachment ${attachmentId} decrypted successfully`);
            
            return {
                id: attachmentId,
                filename: attachmentMeta.orig_filename,
                contentType: attachmentMeta.orig_mime,
                data: decryptedData,
                size: attachmentMeta.orig_size || decryptedData.length
            };
            
        } catch (error) {
            console.error(`[JS] Failed to decrypt attachment ${attachmentId}:`, error);
            throw error;
        }
    }

    // Fallback decryption method for Nostr messages
    async decryptNostrMessageWithFallback(email, encryptedContent, keypair) {
        console.log('[JS] Fallback decryption called for email:', email.from || email.from_address);
        // 1. Try the sender pubkey from the header/field first (for inbox emails)
        // For inbox emails, sender_pubkey should always be available from headers
        const senderPubkey = email.sender_pubkey || email.nostr_pubkey; // Fallback for backward compatibility
        if (senderPubkey) {
            try {
                const decrypted = await TauriService.decryptDmContent(keypair.private_key, senderPubkey, encryptedContent);
                if (decrypted && !decrypted.startsWith('Unable to decrypt')) {
                    // Fix UTF-8 encoding issues in decrypted text
                    return Utils.fixUtf8Encoding(decrypted);
                }
                // If decryption failed, it means our private key couldn't decrypt with this sender's pubkey
                // This is expected if the email wasn't encrypted for us
                return "Unable to decrypt: Your private key could not decrypt this message. The email may not have been encrypted for your keypair.";
            } catch (e) {
                // Decryption error - our private key couldn't decrypt
                console.error('[JS] Decryption failed with sender pubkey:', e);
                return "Unable to decrypt: Your private key could not decrypt this message. The email may not have been encrypted for your keypair.";
            }
        }

        // 2. Fallback: search DB for pubkeys matching sender email
        // This is only reached if sender_pubkey wasn't in headers (shouldn't happen for inbox emails)
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
            // For inbox emails, sender_pubkey should be in headers, so this is unexpected
            return "Unable to decrypt: sender pubkey not found in headers or contacts. Your private key may not be able to decrypt this message.";
        }

        // 3. Try each pubkey from contacts
        for (const pubkey of pubkeys) {
            try {
                const decrypted = await TauriService.decryptDmContent(keypair.private_key, pubkey, encryptedContent);
                if (decrypted && !decrypted.startsWith('Unable to decrypt')) {
                    // Update the email's sender_pubkey in the DB for future use
                    try {
                        await window.__TAURI__.core.invoke('db_update_email_sender_pubkey_by_id', {
                            id: Number(email.id),
                            senderPubkey: pubkey
                        });
                        email.sender_pubkey = pubkey; // Update local copy
                        email.nostr_pubkey = pubkey; // Keep for backward compatibility
                    } catch (err) {
                        console.warn('Failed to update sender_pubkey in DB:', err);
                    }
                    // Fix UTF-8 encoding issues in decrypted text
                    return Utils.fixUtf8Encoding(decrypted);
                }
            } catch (e) {
                // try next
            }
        }
        // All pubkeys tried, decryption failed with our private key
        return "Unable to decrypt: Your private key could not decrypt this message with any of the sender's pubkeys. The email may not have been encrypted for your keypair.";
    }

    // Decrypt manifest-based message for sent emails (using recipient pubkey)
    async decryptSentManifestMessage(email, encryptedContent, keypair) {
        console.log('[JS] Attempting sent manifest decryption for email:', email.to || email.to_address);
        
        try {
            let decryptedManifestJson;
            let foundPubkey = false;
            
            // Try recipient_pubkey from email first (if available)
            const recipientPubkey = email.recipient_pubkey || email.nostr_pubkey; // Fallback for backward compatibility
            if (recipientPubkey) {
                try {
                    console.log('[JS] Trying recipient_pubkey from email:', recipientPubkey);
                    decryptedManifestJson = await TauriService.decryptDmContent(keypair.private_key, recipientPubkey, encryptedContent);
                    if (decryptedManifestJson && !decryptedManifestJson.startsWith('Unable to decrypt')) {
                        foundPubkey = true;
                    }
                } catch (e) {
                    console.log('[JS] Failed with recipient_pubkey from email, falling back to contact lookup:', e);
                }
            }
            
            // Fallback: For sent emails, we need the recipient's pubkey from contacts
            if (!foundPubkey) {
                const recipientEmail = email.to || email.to_address;
                if (!recipientEmail) {
                    throw new Error('Recipient address not found');
                }
                
                // Normalize Gmail addresses (remove + aliases)
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
                console.log('[JS] Normalized recipient email:', normalizedEmail, 'from:', recipientEmail);
                
                let pubkeys = [];
                try {
                    // Try both original and normalized email
                    const emailVariants = [recipientEmail, normalizedEmail].filter((e, i, arr) => arr.indexOf(e) === i);
                    for (const emailVariant of emailVariants) {
                        try {
                            const found = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email', { email: emailVariant });
                            if (found && found.length > 0) {
                                pubkeys.push(...found);
                                console.log('[JS] Found pubkeys for', emailVariant, ':', found);
                            }
                        } catch (e) {
                            console.log('[JS] No pubkeys found for', emailVariant);
                        }
                    }
                    // Remove duplicates
                    pubkeys = [...new Set(pubkeys)];
                } catch (e) {
                    console.error('[JS] Error searching contacts:', e);
                    throw new Error('Error searching contacts: ' + e.message);
                }
                
                if (!pubkeys || pubkeys.length === 0) {
                    throw new Error('Recipient not found in contacts. Email: ' + recipientEmail + ' (normalized: ' + normalizedEmail + ')');
                }
                
                console.log('[JS] Found', pubkeys.length, 'pubkey(s) for recipient');
                
                // Try each recipient pubkey
                for (const pubkey of pubkeys) {
                    try {
                        console.log('[JS] Trying recipient pubkey:', pubkey);
                        decryptedManifestJson = await TauriService.decryptDmContent(keypair.private_key, pubkey, encryptedContent);
                        if (decryptedManifestJson && !decryptedManifestJson.startsWith('Unable to decrypt')) {
                            foundPubkey = true;
                            break;
                        }
                    } catch (e) {
                        console.log('[JS] Failed with pubkey:', pubkey, e.message);
                        continue;
                    }
                }
            }
            
            if (!decryptedManifestJson) {
                throw new Error('Failed to decrypt with any recipient pubkey');
            }
            
            // Try to parse as manifest JSON, if it fails it's probably legacy format
            let manifest;
            try {
                manifest = JSON.parse(decryptedManifestJson);
                console.log('[JS] Sent manifest parsed successfully:', manifest);
                console.log('[JS] Manifest has attachments:', manifest.attachments ? manifest.attachments.length : 0);
                if (manifest.attachments && manifest.attachments.length > 0) {
                    console.log('[JS] Manifest attachment IDs:', manifest.attachments.map(a => a.id));
                    manifest.attachments.forEach((att, idx) => {
                        console.log(`[JS] Manifest attachment ${idx}: id=${att.id}, orig_filename=${att.orig_filename}, orig_mime=${att.orig_mime}`);
                    });
                }
            } catch (parseError) {
                console.log('[JS] Not a sent manifest format, treating as legacy encrypted body');
                return {
                    type: 'legacy',
                    body: Utils.fixUtf8Encoding(decryptedManifestJson)
                };
            }
            
            // Decrypt body if present
            let decryptedBody = '';
            if (manifest.body && manifest.body.ciphertext) {
                console.log('[JS] Decrypting body from sent manifest...');
                const bodyKey = manifest.body.key_wrap;
                const encryptedBodyData = manifest.body.ciphertext;
                
                try {
                    const decryptedBodyBase64 = await this.decryptWithAES(encryptedBodyData, bodyKey);
                    decryptedBody = atob(decryptedBodyBase64);
                    console.log('[JS] Sent body decrypted successfully, length:', decryptedBody.length);
                } catch (aesError) {
                    console.error('[JS] Sent AES decryption failed:', aesError);
                    decryptedBody = `[AES Decryption Failed: ${aesError.message}]`;
                }
            }
            
            const result = {
                type: 'manifest',
                body: Utils.fixUtf8Encoding(decryptedBody),
                manifest: manifest
            };
            console.log('[JS] Returning manifest result, type:', result.type, 'has manifest:', !!result.manifest, 'has attachments:', result.manifest && result.manifest.attachments ? result.manifest.attachments.length : 0);
            return result;
            
        } catch (error) {
            console.error('[JS] Sent manifest decryption failed:', error);
            throw error;
        }
    }

    // Decrypt Nostr message for sent emails (using recipient pubkey from contacts DB)
    async decryptNostrSentMessageWithFallback(email, encryptedContent, keypair) {
        // Try recipient_pubkey from email first (if available)
        const recipientPubkey = email.recipient_pubkey || email.nostr_pubkey; // Fallback for backward compatibility
        if (recipientPubkey) {
            try {
                const decrypted = await TauriService.decryptDmContent(keypair.private_key, recipientPubkey, encryptedContent);
                if (decrypted && !decrypted.startsWith('Unable to decrypt')) {
                    return Utils.fixUtf8Encoding(decrypted);
                }
            } catch (e) {
                // Fall through to lookup from contacts
            }
        }
        
        // Fallback: look up the recipient's pubkey using the to address (checking both contacts and DMs)
        const recipientEmail = email.to || email.to_address;
        if (!recipientEmail) return "Unable to decrypt: recipient address not found";
        let pubkeys = [];
        try {
            // First try the new function that includes DMs
            pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email_including_dms', { email: recipientEmail });
            // If that fails or returns empty, fall back to contacts only
            if (!pubkeys || pubkeys.length === 0) {
                pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email', { email: recipientEmail });
            }
        } catch (e) {
            console.error('[JS] Error searching for recipient pubkeys:', e);
            // Try fallback to contacts only
            try {
                pubkeys = await window.__TAURI__.core.invoke('db_find_pubkeys_by_email', { email: recipientEmail });
            } catch (fallbackError) {
                return "Unable to decrypt: error searching contacts and DMs";
            }
        }
        if (!pubkeys || pubkeys.length === 0) {
            return "Unable to decrypt: recipient pubkey not found in contacts or DMs";
        }
        for (const pubkey of pubkeys) {
            try {
                const decrypted = await TauriService.decryptDmContent(keypair.private_key, pubkey, encryptedContent);
                if (decrypted && !decrypted.startsWith('Unable to decrypt')) {
                    // Successfully decrypted - save the recipient pubkey to database for future use
                    try {
                        await this._saveRecipientPubkeyToDb(email, pubkey);
                        console.log(`[JS] Saved recipient pubkey ${pubkey.substring(0, 16)}... to database for email ${email.id || email.message_id}`);
                    } catch (saveError) {
                        console.warn('[JS] Failed to save recipient pubkey to database:', saveError);
                        // Continue anyway - decryption was successful
                    }
                    // Fix UTF-8 encoding issues in decrypted text
                    return Utils.fixUtf8Encoding(decrypted);
                }
            } catch (e) {
                // try next
            }
        }
        return "Unable to decrypt: tried all candidate pubkeys";
    }

    async renderEmails(showLoadMore = false) {
        const emailList = domManager.get('emailList');
        if (!emailList) return;
        try {
            // Remove existing Load More button if it exists
            const existingLoadMoreBtn = document.getElementById('load-more-emails');
            if (existingLoadMoreBtn) {
                existingLoadMoreBtn.remove();
            }
            
            // Get all emails from state
            const emails = appState.getEmails();
            
            if (emails.length === 0) {
                emailList.innerHTML = '<div class="text-center text-muted">No emails found</div>';
                return;
            }
            
            // Check if we should hide undecryptable emails
            const settings = appState.getSettings();
            const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
            const hideUnverified = settings && settings.hide_unsigned_messages === true;
            
            // Always re-render all emails (simpler approach)
            emailList.innerHTML = '';
            
            let renderedCount = 0;
            
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
                let previewText = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                let showSubject = false;
                let previewSubject = email.subject;

                // Detect any NOSTR NIP-X ENCRYPTED MESSAGE
                const armorRegex = /-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}/;
                if (email.body && armorRegex.test(email.body)) {
                    const keypair = appState.getKeypair();
                    if (!keypair) {
                        previewText = 'Unable to decrypt: no keypair';
                    } else {
                        // Decrypt subject if it looks encrypted (base64 or glossia-encoded)
                        let subjectCipher = null;
                        if (Utils.isLikelyEncryptedContent(email.subject)) {
                            subjectCipher = email.subject;
                        } else {
                            subjectCipher = this.decodeGlossiaSubject(email.subject);
                        }
                        if (subjectCipher) {
                            try {
                                previewSubject = await this.decryptNostrMessageWithFallback(email, subjectCipher, keypair);
                                if (previewSubject && (previewSubject.startsWith('Unable to decrypt') || previewSubject.includes('Unable to decrypt'))) {
                                    previewSubject = 'Unable to decrypt';
                                }
                            } catch (e) {
                                previewSubject = 'Unable to decrypt';
                            }
                        }
                        // Decrypt body - try manifest format first, then fallback to legacy
                        const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                        if (encryptedBodyMatch) {
                            const armorContent = encryptedBodyMatch[1].trim();
                            let encryptedContent;
                            if (/^[A-Za-z0-9+/=\n?]+$/.test(armorContent.replace(/\s+/g, ''))) {
                                encryptedContent = armorContent.replace(/\s+/g, '');
                            } else {
                                // Glossia-encoded body — decode to ciphertext
                                const glossiaResult = this.decodeGlossiaArmoredBody(email.body);
                                encryptedContent = glossiaResult ? glossiaResult.ciphertext : null;
                            }
                            if (!encryptedContent) {
                                // Glossia decode failed — can't decrypt
                            } else try {
                                // Try manifest decryption first
                                console.log('[JS] Attempting manifest decryption for preview...');
                                const manifestResult = await this.decryptManifestMessage(email, encryptedContent, keypair);
                                console.log('[JS] Manifest result:', manifestResult);
                                
                                if (manifestResult.type === 'manifest') {
                                    console.log('[JS] Using manifest body for preview:', manifestResult.body.substring(0, 50));
                                    previewText = Utils.escapeHtml(manifestResult.body.substring(0, 100));
                                    showSubject = true;
                                } else if (manifestResult.type === 'legacy') {
                                    console.log('[JS] Using legacy body for preview:', manifestResult.body.substring(0, 50));
                                    previewText = Utils.escapeHtml(manifestResult.body.substring(0, 100));
                                    showSubject = true;
                                } else {
                                    // Fallback to legacy decryption
                                    console.log('[JS] Falling back to legacy decryption for preview...');
                                    const decrypted = await this.decryptNostrMessageWithFallback(email, encryptedContent, keypair);
                                    // Check if decryption returned an error message
                                    if (decrypted && (decrypted.startsWith('Unable to decrypt') || decrypted.includes('Unable to decrypt'))) {
                                        previewText = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                                    } else {
                                        previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                        showSubject = true;
                                    }
                                }
                            } catch (e) {
                                console.error('[JS] Manifest decryption failed for preview:', e);
                                // If manifest fails, try legacy decryption
                                try {
                                    const decrypted = await this.decryptNostrMessageWithFallback(email, encryptedContent, keypair);
                                    // Check if decryption returned an error message
                                    if (decrypted && (decrypted.startsWith('Unable to decrypt') || decrypted.includes('Unable to decrypt'))) {
                                        previewText = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                                    } else {
                                        previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                        showSubject = true;
                                    }
                                } catch (legacyError) {
                                    previewText = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                                }
                            }
                        }
                    }
                } else {
                    // Decode glossia signed message body for preview
                    let previewBody = email.body || '';
                    const signedMsg = this.decodeGlossiaSignedMessage(previewBody);
                    if (signedMsg && signedMsg.plaintextBody) {
                        previewBody = signedMsg.plaintextBody;
                    }
                    previewText = Utils.escapeHtml(previewBody ? previewBody.substring(0, 100) : '');
                    if (previewBody && previewBody.length > 100) previewText += '...';
                    showSubject = true;
                }

                // Check if email is decryptable (for filtering)
                // Only hide if the body cannot be decrypted (subject decryption failure is less critical)
                const isDecryptable = !previewText.includes('Unable to decrypt') &&
                                    !previewText.includes('Your private key could not decrypt this message') &&
                                    !previewText.includes('could not decrypt') &&
                                    !previewText.includes('Could not decrypt') &&
                                    previewText !== 'Unable to decrypt: no keypair';
                
                // Skip rendering if we should hide undecryptable emails and this email can't be decrypted
                if (hideUndecryptable && !isDecryptable) {
                    continue;
                }
                
                // Skip rendering if we should hide unverified messages and this email doesn't have a valid signature
                // Hide emails where signature_valid is not true (i.e., false, null, or undefined)
                if (hideUnverified && email.signature_valid !== true) {
                    continue;
                }

                // Add attachment indicator (same style as sent emails)
                const attachmentCount = email.attachments ? email.attachments.length : 0;
                const attachmentIndicator = attachmentCount > 0 ? 
                    `<span class="attachment-indicator" title="${attachmentCount} attachment${attachmentCount > 1 ? 's' : ''}">📎 ${attachmentCount}</span>` : '';

                // Add signature verification indicator
                let signatureIndicator = '';
                if (email.signature_valid === true) {
                    signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature"><i class="fas fa-pen"></i> Signature Verified</span>`;
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

                // Get sender contact for avatar
                const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                const contacts = appState.getContacts();
                let senderContact = senderPubkey ? contacts.find(c => c.pubkey === senderPubkey) : null;
                
                // If not found by pubkey, try to find by email address
                if (!senderContact) {
                    const senderEmail = email.from || email.from_address;
                    if (senderEmail) {
                        // Normalize Gmail addresses (remove + aliases)
                        const normalizeGmail = (email) => {
                            const lower = email.trim().toLowerCase();
                            if (lower.includes('@gmail.com')) {
                                const [local, domain] = lower.split('@');
                                const normalizedLocal = local.split('+')[0];
                                return `${normalizedLocal}@${domain}`;
                            }
                            return lower;
                        };
                        
                        const normalizedEmail = normalizeGmail(senderEmail);
                        // Try both original and normalized email
                        senderContact = contacts.find(c => {
                            if (!c.email) return false;
                            const contactEmail = c.email.trim().toLowerCase();
                            return contactEmail === senderEmail.toLowerCase() || contactEmail === normalizedEmail;
                        });
                    }
                }
                
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
                            <div class="email-date">${dateDisplay}</div>
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
                
                emailElement.addEventListener('click', () => this.showEmailDetail(email.id));
                emailList.appendChild(emailElement);
                renderedCount++;
            }
            
            // Show message if no emails were rendered (possibly all filtered out)
            if (renderedCount === 0) {
                const settings = appState.getSettings();
                const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
                const hideUnverified = settings && settings.hide_unsigned_messages === true;
                if (hideUndecryptable && emails.length > 0) {
                    emailList.innerHTML = '<div class="text-center text-muted">No decryptable emails found. All emails are encrypted for a different keypair.</div>';
                } else if (hideUnverified && emails.length > 0) {
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
        } catch (error) {
            console.error('Error rendering emails:', error);
        }
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
                const keypair = appState.getKeypair();
                const userEmail = settings.email_address ? settings.email_address : null;
                const privateKey = keypair ? keypair.private_key : null;
                
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
                const result = await TauriService.searchEmails(searchQuery, userEmail, privateKey, pageSize, this.searchOffset);
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
                const isEncryptedSubjectBase64 = Utils.isLikelyEncryptedContent(email.subject);
                // Permissive regex: matches both base64 and glossia word content between armor markers
                const encryptedBodyMatch = emailBody.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:(NIP-\d+) ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                let decryptedSubject = email.subject;
                let decryptedBody = emailBody;
                let originalSubject = email.subject;
                let decryptionAttempted = false;
                const keypair = appState.getKeypair();

                // Only try subject decryption if body is encrypted (has armor)
                let subjectCiphertext = null;
                if (encryptedBodyMatch) {
                    if (isEncryptedSubjectBase64) {
                        subjectCiphertext = email.subject;
                    } else {
                        subjectCiphertext = this.decodeGlossiaSubject(email.subject);
                        if (subjectCiphertext) {
                            console.log('[JS] Inbox: glossia-decoded subject to ciphertext');
                        }
                    }
                }
                const isEncryptedSubject = !!subjectCiphertext;

                // Try glossia decode for body if armor content is not base64
                let bodyCiphertext = null;
                let bodyIsGlossia = false;
                if (encryptedBodyMatch) {
                    const armorContent = encryptedBodyMatch[2].trim();
                    if (/^[A-Za-z0-9+/=\n?]+$/.test(armorContent.replace(/\s+/g, ''))) {
                        bodyCiphertext = armorContent.replace(/\s+/g, '');
                    } else {
                        const glossiaResult = this.decodeGlossiaArmoredBody(emailBody);
                        if (glossiaResult) {
                            bodyCiphertext = glossiaResult.ciphertext;
                            bodyIsGlossia = true;
                            console.log('[JS] Inbox: glossia-decoded body armor, dialect:', glossiaResult.dialect);
                        }
                    }
                }

                if ((isEncryptedSubject || bodyCiphertext) && keypair) {
                    (async () => {
                        let manifestResult = null;
                        try {
                            if (isEncryptedSubject && subjectCiphertext) {
                                decryptedSubject = await this.decryptNostrMessageWithFallback(email, subjectCiphertext, keypair);
                                if (decryptedSubject && (decryptedSubject.startsWith('Unable to decrypt') || decryptedSubject.includes('Unable to decrypt'))) {
                                    decryptedSubject = 'Unable to decrypt';
                                }
                            }
                            if (bodyCiphertext) {
                                try {
                                    console.log('[JS] Attempting manifest decryption for detail view...');
                                    manifestResult = await this.decryptManifestMessage(email, bodyCiphertext, keypair);
                                    console.log('[JS] Detail view manifest result:', manifestResult);
                                    console.log('[JS] Manifest result type:', manifestResult.type);
                                    console.log('[JS] Manifest result body length:', manifestResult.body ? manifestResult.body.length : 'no body');

                                    if (manifestResult.type === 'manifest') {
                                        console.log('[JS] Using manifest body for detail view:', manifestResult.body.substring(0, 100));
                                        decryptedBody = manifestResult.body;
                                    } else if (manifestResult.type === 'legacy') {
                                        console.log('[JS] Using legacy body for detail view:', manifestResult.body.substring(0, 100));
                                        decryptedBody = manifestResult.body;
                                    } else {
                                        console.log('[JS] Falling back to legacy decryption for detail view...');
                                        decryptedBody = await this.decryptNostrMessageWithFallback(email, bodyCiphertext, keypair);
                                        if (decryptedBody && (decryptedBody.startsWith('Unable to decrypt') || decryptedBody.includes('Unable to decrypt'))) {
                                            decryptedSubject = 'Unable to decrypt';
                                            decryptedBody = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                                        }
                                    }
                                } catch (e) {
                                    console.error('[JS] Manifest decryption failed for detail view:', e);
                                    try {
                                        decryptedBody = await this.decryptNostrMessageWithFallback(email, bodyCiphertext, keypair);
                                        if (decryptedBody && (decryptedBody.startsWith('Unable to decrypt') || decryptedBody.includes('Unable to decrypt'))) {
                                            decryptedSubject = 'Unable to decrypt';
                                            decryptedBody = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                                        }
                                    } catch (legacyErr) {
                                        console.error('[JS] Legacy decryption also failed:', legacyErr);
                                        decryptedSubject = 'Unable to decrypt';
                                        decryptedBody = 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.';
                                    }
                                }
                            }
                            // Verify all signatures recursively (handles nested quoted blocks)
                            let sigResults = null;
                            try {
                                const allSigs = await this.verifyAllSignatures(emailBody);
                                sigResults = allSigs.length > 0 ? allSigs : null;
                            } catch (e) {
                                console.warn('[JS] Signature verification error:', e);
                            }
                            await updateDetail(decryptedSubject, decryptedBody, manifestResult, true, sigResults);
                        } catch (err) {
                            console.error('[JS] Error decrypting email detail:', err);
                            // For inbox emails, decryption failure means our private key couldn't decrypt
                            await updateDetail('Unable to decrypt', 'Your private key could not decrypt this message. The email may not have been encrypted for your keypair.', null, true);
                        }
                    })();
                } else {
                    (async () => {
                        // Verify all signatures recursively (handles nested quoted blocks)
                        let sigResults = null;
                        try {
                            const allSigs = await this.verifyAllSignatures(emailBody);
                            sigResults = allSigs.length > 0 ? allSigs : null;
                        } catch (e) {
                            console.warn('[JS] Signature verification error:', e);
                        }
                        // Decode glossia signed message body for display
                        let displayBody = decryptedBody;
                        const signedMsg = this.decodeGlossiaSignedMessage(emailBody);
                        if (signedMsg && signedMsg.plaintextBody) {
                            displayBody = signedMsg.plaintextBody;
                        }
                        await updateDetail(decryptedSubject, displayBody, null, false, sigResults);
                    })();
                }
                const updateDetail = async (subject, body, cachedManifestResult, wasDecrypted = false, inlineSigResult = null) => {
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
                                    // Permissive regex: matches both base64 and glossia word content
                                    const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:(NIP-\d+) ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                                    if (encryptedBodyMatch) {
                                        const armorContent = encryptedBodyMatch[2].trim();
                                        let encryptedContent;
                                        if (/^[A-Za-z0-9+/=\n?]+$/.test(armorContent.replace(/\s+/g, ''))) {
                                            encryptedContent = armorContent.replace(/\s+/g, '');
                                        } else {
                                            const glossiaResult = this.decodeGlossiaArmoredBody(email.body);
                                            encryptedContent = glossiaResult ? glossiaResult.ciphertext : null;
                                        }
                                        if (encryptedContent) {
                                            const keypair = appState.getKeypair();
                                            manifestResult = await this.decryptManifestMessage(email, encryptedContent, keypair);
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
                    
                    // Add signature verification indicator
                    // Extract the outer sig result (last element if array, since DOM order is [quoted, outer])
                    const outerSigResult = Array.isArray(inlineSigResult) ? inlineSigResult[inlineSigResult.length - 1] : inlineSigResult;
                    let signatureIndicator = '';
                    if (outerSigResult && outerSigResult.isValid === true) {
                        signatureIndicator = `<span class="signature-indicator verified" title="Inline signature verified"><i class="fas fa-check-circle"></i> Signature Verified</span>`;
                    } else if (outerSigResult && outerSigResult.isValid === false) {
                        signatureIndicator = `<span class="signature-indicator invalid" title="Inline signature invalid"><i class="fas fa-times-circle"></i> Signature Invalid</span>`;
                    } else if (email.signature_valid === true) {
                        signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature"><i class="fas fa-pen"></i> Signature Verified</span>`;
                    } else if (email.signature_valid === false) {
                        signatureIndicator = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Invalid Nostr signature"><i class="fas fa-pen"></i> Signature Invalid</span>`;
                    }
                    
                    // Get sender info for header
                    const senderPubkey = email.sender_pubkey || email.nostr_pubkey;
                    const contacts = appState.getContacts();
                    const senderContact = senderPubkey ? contacts.find(c => c.pubkey === senderPubkey) : null;
                    
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
                    
                    // Update page header (back button and reply button)
                    if (emailDetailHeader) {
                        emailDetailHeader.innerHTML = `
                            <button id="back-to-inbox" class="btn btn-secondary">
                                <i class="fas fa-arrow-left"></i> Back to Inbox
                            </button>
                            <button id="reply-to-email" class="btn btn-primary" style="margin-left: 10px;">
                                <i class="fas fa-reply"></i> Reply
                            </button>
                        `;
                        // Re-attach back button event listener
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
                        // Attach reply button event listener
                        const replyBtn = emailDetailHeader.querySelector('#reply-to-email');
                        if (replyBtn) {
                            replyBtn.addEventListener('click', () => {
                                this.replyToEmail(email, subject, body);
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
${signatureIndicator}
</div>
<div class="email-sender-time">${Utils.escapeHtml(timeAgo)}</div>
</div>
</div>
<details class="email-metadata-details">
<summary class="email-metadata-summary"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></summary>
<div class="email-detail-header vertical" id="inbox-email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
</div>
</details>
</div>
<pre id="inbox-raw-header-info" class="email-raw-content">${Utils.escapeHtml(email.raw_headers || '')}</pre>
<div class="email-detail-body" id="inbox-email-body-info">${email.html_body ? '' : Utils.escapeHtml(body).replace(/\n/g, '<br>')}</div>
<pre id="inbox-raw-body-info" class="email-raw-content email-raw-body">${Utils.escapeHtml(email.raw_body)}${email.html_body ? '\n\n--- text/html ---\n\n' + Utils.escapeHtml(email.html_body) : ''}</pre>
${attachmentsHtml}
</div>
<button id="inbox-toggle-raw-btn" class="btn btn-secondary" style="margin: 18px 0 0 0;">Show Raw Content</button>
</div>`;
                    if (email.html_body && wasDecrypted) {
                        // Patch the HTML body: replace glossia-encoded div content with decrypted text
                        try {
                            const parser = new DOMParser();
                            const doc = parser.parseFromString(email.html_body, 'text/html');
                            const contentDiv = doc.querySelector('body > div > div') || doc.querySelector('body > div');
                            if (contentDiv) {
                                contentDiv.innerHTML = Utils.escapeHtml(body).replace(/\n/g, '<br>');
                            }
                            let patchedHtml = doc.documentElement.outerHTML;
                            if (inlineSigResult) patchedHtml = this.injectHtmlSigBadge(patchedHtml, inlineSigResult);
                            Utils.renderHtmlBodyInIframe('inbox-email-body-info', patchedHtml);
                        } catch (e) {
                            console.error('[JS] Failed to patch inbox HTML body with decrypted text:', e);
                            Utils.renderHtmlBodyInIframe('inbox-email-body-info', email.html_body);
                        }
                    } else if (email.html_body) {
                        let htmlToRender = email.html_body;
                        if (inlineSigResult) htmlToRender = this.injectHtmlSigBadge(htmlToRender, inlineSigResult);
                        Utils.renderHtmlBodyInIframe('inbox-email-body-info', htmlToRender);
                    }
                    // Decorate and verify inline signature blocks in the body
                    if (!email.html_body) {
                        Utils.decorateArmorBlocks('inbox-email-body-info');
                        this.verifyAndAnnotateSignatureBlocks(body, 'inbox-email-body-info');
                    }
                    const toggleRawBtn = document.getElementById('inbox-toggle-raw-btn');
                    const headerInfo = document.getElementById('inbox-email-header-info');
                    const rawHeaderInfo = document.getElementById('inbox-raw-header-info');
                    const bodyInfo = document.getElementById('inbox-email-body-info');
                    const rawBodyInfo = document.getElementById('inbox-raw-body-info');
                    const attachmentsInfo = document.getElementById('inbox-email-attachments');
                    
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
                    
                    if (toggleRawBtn && headerInfo && rawHeaderInfo && bodyInfo && rawBodyInfo) {
                        // Remove any existing event listeners by cloning the button
                        const newToggleBtn = toggleRawBtn.cloneNode(true);
                        toggleRawBtn.parentNode.replaceChild(newToggleBtn, toggleRawBtn);
                        
                        let showingRaw = false;
                        newToggleBtn.addEventListener('click', (event) => {
                            event.preventDefault();
                            event.stopPropagation();
                            
                            showingRaw = !showingRaw;
                            if (showingRaw) {
                                headerInfo.classList.add('hidden-header');
                                rawHeaderInfo.style.display = 'block';
                                bodyInfo.style.display = 'none';
                                rawBodyInfo.style.display = 'block';
                                newToggleBtn.textContent = 'Show Display Content';
                                
                                // Hide metadata details when showing raw content
                                const metadataDetails = headerInfo.closest('.email-metadata-details');
                                if (metadataDetails) {
                                    metadataDetails.style.display = 'none';
                                }
                                
                                // Update attachment filenames, sizes, and icons to show encrypted versions
                                if (attachmentsInfo) {
                                    const attachmentItems = attachmentsInfo.querySelectorAll('.attachment-item');
                                    attachmentItems.forEach(item => {
                                        const encryptedFilename = item.getAttribute('data-encrypted-filename');
                                        const encryptedSize = item.getAttribute('data-encrypted-size');
                                        const isEncrypted = item.getAttribute('data-is-encrypted') === 'true';
                                        const nameElement = item.querySelector('.attachment-name');
                                        const sizeElement = item.querySelector('.attachment-size');
                                        const iconElement = item.querySelector('.attachment-status-icon');
                                        const textElement = item.querySelector('.attachment-status-text');
                                        
                                        if (encryptedFilename && nameElement) {
                                            nameElement.textContent = encryptedFilename;
                                        }
                                        
                                        // Update size to show encrypted size
                                        if (encryptedSize && sizeElement) {
                                            const sizeFormatted = (parseFloat(encryptedSize) / 1024).toFixed(2) + ' KB';
                                            sizeElement.textContent = sizeFormatted;
                                        }
                                        
                                        // Update icon and text to show encrypted state
                                        if (isEncrypted && iconElement && textElement) {
                                            iconElement.textContent = '🔒';
                                            textElement.textContent = 'Encrypted';
                                        }
                                    });
                                }
                                
                                // Move attachments after raw body if they exist
                                if (attachmentsInfo && attachmentsInfo.parentNode) {
                                    attachmentsInfo.parentNode.removeChild(attachmentsInfo);
                                    rawBodyInfo.parentNode.insertBefore(attachmentsInfo, rawBodyInfo.nextSibling);
                                }
                                
                                // Move button outside the card (after the card closes)
                                const emailDetailCard = headerInfo.closest('.email-detail-card');
                                if (emailDetailCard && emailDetailCard.parentNode) {
                                    emailDetailCard.parentNode.insertBefore(newToggleBtn, emailDetailCard.nextSibling);
                                }
                            } else {
                                headerInfo.classList.remove('hidden-header');
                                rawHeaderInfo.style.display = 'none';
                                bodyInfo.style.display = 'block';
                                rawBodyInfo.style.display = 'none';
                                newToggleBtn.textContent = 'Show Raw Content';
                                
                                // Show metadata details when showing display content
                                const metadataDetails = headerInfo.closest('.email-metadata-details');
                                if (metadataDetails) {
                                    metadataDetails.style.display = '';
                                }
                                
                                // Update attachment filenames, sizes, and icons to show decrypted versions
                                if (attachmentsInfo) {
                                    const attachmentItems = attachmentsInfo.querySelectorAll('.attachment-item');
                                    attachmentItems.forEach(item => {
                                        const decryptedFilename = item.getAttribute('data-decrypted-filename');
                                        const decryptedSize = item.getAttribute('data-decrypted-size');
                                        const isEncrypted = item.getAttribute('data-is-encrypted') === 'true';
                                        const nameElement = item.querySelector('.attachment-name');
                                        const sizeElement = item.querySelector('.attachment-size');
                                        const iconElement = item.querySelector('.attachment-status-icon');
                                        const textElement = item.querySelector('.attachment-status-text');
                                        
                                        if (decryptedFilename && nameElement) {
                                            nameElement.textContent = decryptedFilename;
                                        }
                                        
                                        // Update size to show decrypted size
                                        if (decryptedSize && sizeElement) {
                                            const sizeFormatted = (parseFloat(decryptedSize) / 1024).toFixed(2) + ' KB';
                                            sizeElement.textContent = sizeFormatted;
                                        }
                                        
                                        // Update icon and text to show decrypted state
                                        if (isEncrypted && iconElement && textElement) {
                                            iconElement.textContent = '🔓';
                                            textElement.textContent = 'Decrypted';
                                        }
                                    });
                                }
                                
                                // Move attachments after regular body if they exist
                                if (attachmentsInfo && attachmentsInfo.parentNode) {
                                    attachmentsInfo.parentNode.removeChild(attachmentsInfo);
                                    bodyInfo.parentNode.insertBefore(attachmentsInfo, bodyInfo.nextSibling);
                                }
                                
                                // Keep button outside the card (after the card closes)
                                const emailDetailCard = headerInfo.closest('.email-detail-card');
                                if (emailDetailCard && emailDetailCard.parentNode) {
                                    emailDetailCard.parentNode.insertBefore(newToggleBtn, emailDetailCard.nextSibling);
                                }
                            }
                        });
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
                        const contacts = appState.getContacts();
                        const senderContact = contacts.find(contact => 
                            contact.email && contact.email.toLowerCase() === replyTo.toLowerCase()
                        );
                        if (senderContact && senderContact.pubkey) {
                            senderPubkey = senderContact.pubkey;
                        }
                    } catch (e) {
                        console.log('[JS] Could not look up pubkey by email:', e);
                    }
                }
                
                if (senderPubkey) {
                    // Try to find the contact by pubkey
                    const contacts = appState.getContacts();
                    const senderContact = contacts.find(contact => contact.pubkey === senderPubkey);
                    
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
            // Show the email list and hide the detail view
            const emailList = domManager.get('emailList');
            const emailDetailView = document.getElementById('email-detail-view');
            const inboxActions = document.getElementById('inbox-actions');
            const inboxTitle = document.getElementById('inbox-title');

            if (emailList) emailList.style.display = 'block';
            if (emailDetailView) emailDetailView.style.display = 'none';
            if (inboxActions) inboxActions.style.display = 'flex';
            
            // Re-render emails to update unread indicators
            this.renderEmails();
            if (inboxTitle) {
                inboxTitle.textContent = 'Inbox';
                inboxTitle.style.display = '';
            }
            
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
                use_tls: settings.use_tls,
                // Only include private key if we should sign (check auto-sign setting or manual sign)
                private_key: (() => {
                    const settings = appState.getSettings();
                    const autoSign = settings && settings.automatically_sign !== false;
                    const signBtn = domManager.get('signBtn');
                    const isManuallySigned = signBtn && signBtn.dataset.signed === 'true';
                    const shouldSign = autoSign || isManuallySigned;
                    return shouldSign && appState.getKeypair() ? appState.getKeypair().private_key : null;
                })()
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
        return settings?.glossia_encoding_pubkey ?? 'latin';
    }

    /**
     * Extract and decode glossia-encoded content from an ASCII-armored body.
     * Uses a permissive regex that matches both base64 and glossia word content.
     * Returns { ciphertext, nip, isGlossia, dialect } or null if no armor found.
     */
    decodeGlossiaArmoredBody(plainBody) {
        // Permissive armor regex: handles both "-----BEGIN" and "----- BEGIN" (with optional spaces)
        // Content stops at END NOSTR ... ENCRYPTED MESSAGE, END NOSTR MESSAGE, or BEGIN NOSTR SIGNATURE
        const armorRegex = /-{3,}\s*BEGIN NOSTR (?:(NIP-\d+) ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/;
        const match = plainBody.replace(/\r\n/g, '\n').match(armorRegex);
        if (!match) return null;

        const nip = match[1]; // e.g. "NIP-04" or "NIP-44"
        const content = match[2].trim();

        // If content is already base64 (no spaces), return as-is
        if (/^[A-Za-z0-9+/=\n?]+$/.test(content.replace(/\s+/g, ''))) {
            return { ciphertext: content.replace(/\s+/g, ''), nip, isGlossia: false };
        }

        // Content has spaces/words → glossia-encoded
        const gs = window.GlossiaService;
        if (!gs || !gs.isReady()) {
            console.warn('[JS] decodeGlossiaArmoredBody: GlossiaService not ready');
            return null;
        }

        try {
            const detections = gs.detectDialect(content);
            console.log('[JS] decodeGlossiaArmoredBody: detected dialects:', detections);
            if (!Array.isArray(detections) || detections.length === 0) {
                console.warn('[JS] decodeGlossiaArmoredBody: no dialect detected');
                return null;
            }
            // Use the best match (first element) — language field maps to meta instruction
            const dialect = detections[0].language;
            if (!dialect) {
                console.warn('[JS] decodeGlossiaArmoredBody: no language in detection result');
                return null;
            }

            const result = gs.transcode(content, `decode from ${dialect}`);
            let decoded = result.output;

            // Convert hex to base64 if needed
            if (gs._isHex(decoded)) {
                decoded = gs._hexToBase64(decoded);
            }

            // Unpack NIP-04 binary format if applicable
            decoded = gs._autoUnpack(decoded);

            return { ciphertext: decoded, nip, isGlossia: true, dialect };
        } catch (e) {
            console.error('[JS] decodeGlossiaArmoredBody: glossia decode failed:', e);
            return null;
        }
    }

    /**
     * Parse a BEGIN NOSTR (?:SIGNED (?:MESSAGE|BODY)) armor block.
     * Extracts glossia-encoded body, signature content, and seal content.
     * Decodes the glossia body back to the original UTF-8 plaintext.
     * Returns { plaintextBody, glossiaBody, sigContent, sealContent, profileName, displayName } or null.
     */
    decodeGlossiaSignedMessage(plainBody) {
        if (!plainBody) return null;

        // Delegate parsing to the depth-counting parseArmorComponents
        const parts = this.parseArmorComponents(plainBody);
        if (!parts) return null;

        const glossiaBody = parts.bodyText;
        const { sigContent, sealContent, profileName, displayName, quotedArmor } = parts;

        // Decode glossia body → original UTF-8 plaintext
        // Use transcodeToBytes (full pipeline with header word), not decodeToBytes (raw base_n)
        let plaintextBody = null;
        const gs = window.GlossiaService;
        if (gs && gs.isReady()) {
            try {
                const bytes = gs.transcodeToBytes(glossiaBody);
                if (bytes) {
                    plaintextBody = new TextDecoder().decode(bytes);
                }
            } catch (e) {
                console.warn('[JS] decodeGlossiaSignedMessage: transcodeToBytes failed:', e);
            }
        }

        return { plaintextBody, glossiaBody, sigContent, sealContent, profileName, displayName, quotedArmor };
    }

    /**
     * Decode a glossia-encoded subject line.
     * Subjects use payload_only encoding (bare words, no grammar) via "raw" mode.
     * Returns the decrypted ciphertext string, or null if not glossia-encoded.
     */
    decodeGlossiaSubject(subject) {
        if (!subject || typeof subject !== 'string') return null;

        // If already base64-like, not glossia
        if (Utils.isLikelyEncryptedContent(subject)) return null;

        const gs = window.GlossiaService;
        if (!gs || !gs.isReady()) return null;

        try {
            const detections = gs.detectDialect(subject);
            console.log('[JS] decodeGlossiaSubject: detected dialects:', detections);
            if (!Array.isArray(detections) || detections.length === 0) return null;
            // Require high hit rate — glossia subjects use payload_only (all words are payload),
            // so real glossia subjects should have hit_rate ≈ 1.0.
            // Normal English text may partially match BIP39/Latin wordlists.
            if (detections[0].hit_rate < 0.8) return null;
            // Use the best match — language field maps to meta instruction
            const dialect = detections[0].language;
            if (!dialect) return null;

            const result = gs.transcode(subject, `decode from ${dialect} raw`);
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
    parseRawSignedBody(fullBody) {
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
            if (gs && gs.isReady()) {
                // Match multi-line > quoted block or bare armor block
                const sigArmorRegex = /\n\n((?:>\s*[^\n]*\n?)*-{3,}[^\n]*\n[\s\S]*?-{3,}[^\n]*)\s*$/;
                const sigMatch = body.match(sigArmorRegex);
                if (sigMatch) {
                    try {
                        // Strip > prefixes from each line
                        const sigText = sigMatch[1].split('\n').map(l => l.replace(/^>\s*/, '')).join('\n').trim();
                        const sigResult = gs.transcode(sigText, `decode from sig nostr`);
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

    // Shared signature verification: parse body for sig+pubkey blocks and verify.
    // Returns { found, signed, isValid, pubkeyHex, signatureHex, body }
    async verifyBodySignature(fullBody, dataToVerify) {
        const gs = window.GlossiaService;
        const metaPubkey = this.getGlossiaEncodingPubkey();
        const metaSig = this.getGlossiaEncodingSignature();
        let parsed;
        if (gs && gs.isReady()) {
            parsed = gs.parseSignedBody(fullBody, metaPubkey, metaSig);
        } else {
            parsed = this.parseRawSignedBody(fullBody);
        }
        if (!parsed?.pubkeyHex) return { found: false };
        if (!parsed.signatureHex) return { found: true, signed: false, pubkeyHex: parsed.pubkeyHex, body: parsed.body };

        const npub = window.CryptoService._nip19.npubEncode(parsed.pubkeyHex);
        const dataBytes = window.CryptoService.ciphertextToBytes(dataToVerify);
        const isValid = await TauriService.verifySignature(npub, parsed.signatureHex, dataBytes);
        return { found: true, signed: true, isValid, pubkeyHex: parsed.pubkeyHex, signatureHex: parsed.signatureHex, body: parsed.body };
    }

    /**
     * Extract inline signature and pubkey from ASCII-armored email body,
     * and verify the signature against the raw encrypted message content.
     * Returns { signatureHex, pubkeyHex, isValid, rawSignedContent } or null.
     */
    async verifyInlineSignature(plainBody) {
        if (!plainBody) { console.log('[JS] verifyInlineSignature: no body, returning null'); return null; }

        console.log('[JS] verifyInlineSignature: body length=', plainBody.length, 'preview=', plainBody.substring(0, 200));
        const gs = window.GlossiaService;

        // Use depth-counting parser to extract components
        const parts = this.parseArmorComponents(plainBody);
        const isEncrypted = !!plainBody.match(/-{3,}\s*BEGIN NOSTR (?:NIP-(?:04|44) ENCRYPTED)/);
        const isSignedBody = !!plainBody.match(/-{3,}\s*BEGIN NOSTR (?:SIGNED (?:MESSAGE|BODY))/);

        // 1. Extract and decode the signed data
        let dataBytes;

        if (isEncrypted && parts && parts.bodyText) {
            dataBytes = this._decodeArmorBodyToBytes(parts.bodyText, true);
            console.log('[JS] verifyInlineSignature: encrypted mode, decoded bytes length=', dataBytes ? dataBytes.length : 0);
            if (parts.quotedArmor) {
                const allQuotedBytes = this._extractAllBodyBytes(parts.quotedArmor);
                if (allQuotedBytes) {
                    dataBytes = this._concatBytes(dataBytes, allQuotedBytes);
                    console.log('[JS] verifyInlineSignature: concatenated all quoted bytes, total length=', dataBytes.length);
                }
            }
        } else if (isSignedBody && parts && parts.bodyText) {
            // Signed plaintext: decode glossia body back to original plaintext
            if (gs && gs.isReady()) {
                try {
                    const detections = gs.detectDialect(parts.bodyText);
                    if (Array.isArray(detections) && detections.length > 0) {
                        const dialect = detections[0].language;
                        const result = gs.transcode(parts.bodyText, `decode from ${dialect}`);
                        dataBytes = new TextEncoder().encode(result.output);
                        console.log('[JS] verifyInlineSignature: signed message, transcoded back to original, bytes length=', dataBytes.length);
                    }
                } catch (e) {
                    console.warn('[JS] verifyInlineSignature: transcode decode failed:', e);
                }
            }
            // Fallback: transcodeToBytes
            if (!dataBytes && gs && gs.isReady()) {
                try {
                    const bytes = gs.transcodeToBytes(parts.bodyText);
                    if (bytes) {
                        dataBytes = new TextEncoder().encode(new TextDecoder().decode(bytes));
                        console.log('[JS] verifyInlineSignature: signed message, using transcodeToBytes fallback, bytes length=', dataBytes.length);
                    }
                } catch (_) {}
            }

            // Recursively concatenate all nested quoted body bytes
            if (parts.quotedArmor) {
                const allQuotedBytes = this._extractAllBodyBytes(parts.quotedArmor);
                if (allQuotedBytes) {
                    dataBytes = this._concatBytes(dataBytes, allQuotedBytes);
                    console.log('[JS] verifyInlineSignature: signed msg, concatenated all quoted bytes, total=', dataBytes.length);
                }
            }
        } else if (!isEncrypted && !isSignedBody) {
            // Plaintext without signed body armor — strip signature/seal blocks to get message body
            const bodyOnly = plainBody
                .replace(/\r\n/g, '\n')
                .replace(/-{3,}\s*BEGIN NOSTR SIGNATURE\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR (?:SIGNATURE|MESSAGE)\s*-{3,}/g, '')
                .replace(/-{3,}\s*BEGIN NOSTR SEAL\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR (?:SEAL|MESSAGE)\s*-{3,}/g, '')
                .trim();
            console.log('[JS] verifyInlineSignature: plaintext mode, bodyOnly length=', bodyOnly.length);
            if (!bodyOnly) { console.log('[JS] verifyInlineSignature: empty body after stripping blocks'); return null; }
            dataBytes = new TextEncoder().encode(bodyOnly);
        }

        if (!parts || !parts.sigContent) {
            console.log('[JS] verifyInlineSignature: no signature found via parseArmorComponents');
            return null;
        }

        // 2. Decode sig and pubkey from parsed components
        let signatureHex = null;
        let pubkeyHex = null;

        // Check if already hex (from _splitSigPubkey in parseArmorComponents)
        if (parts.sigContent && /^[0-9a-fA-F]{128}$/.test(parts.sigContent)) {
            signatureHex = parts.sigContent;
        }
        if (parts.sealContent && /^[0-9a-fA-F]{64}$/.test(parts.sealContent)) {
            pubkeyHex = parts.sealContent;
        }

        // Attempt glossia decode if not already hex
        if (!signatureHex || !pubkeyHex) {
            try {
                if (gs && gs.isReady()) {
                    if (!signatureHex && parts.sigContent) {
                        const sigDetections = gs.detectDialect(parts.sigContent);
                        const sigDialect = (Array.isArray(sigDetections) && sigDetections.length > 0) ? sigDetections[0].language : null;
                        if (sigDialect) {
                            const decoded = gs.decodeRawBaseN(parts.sigContent, sigDialect, 64);
                            if (decoded.length === 128) signatureHex = decoded;
                        }
                    }
                    if (!pubkeyHex && parts.sealContent) {
                        const pkDetections = gs.detectDialect(parts.sealContent);
                        const pkDialect = (Array.isArray(pkDetections) && pkDetections.length > 0) ? pkDetections[0].language : null;
                        if (pkDialect) {
                            const decoded = gs.decodeRawBaseN(parts.sealContent, pkDialect, 32);
                            if (decoded.length === 64) pubkeyHex = decoded;
                        }
                    }
                }
                // Fallback: try npub in seal content
                if (!pubkeyHex && parts.sealContent) {
                    const npubMatch = parts.sealContent.match(/(npub1[a-z0-9]+)/);
                    if (npubMatch) pubkeyHex = window.CryptoService._npubToHex(npubMatch[1]);
                }
                // Fallback: raw hex sig
                if (!signatureHex && parts.sigContent && /^[0-9a-fA-F]{128}$/.test(parts.sigContent.replace(/\s+/g, ''))) {
                    signatureHex = parts.sigContent.replace(/\s+/g, '');
                }
            } catch (e) {
                console.warn('[JS] verifyInlineSignature: sig/pubkey decode error:', e);
            }
        }

        if (!pubkeyHex) { console.log('[JS] verifyInlineSignature: could not decode pubkey'); return null; }
        if (!signatureHex) { console.log('[JS] verifyInlineSignature: could not decode signature hex (expected len 128)'); return null; }

        // 3. Verify signature against signed data
        try {
            const npub = window.CryptoService._nip19.npubEncode(pubkeyHex);
            const isValid = await TauriService.verifySignature(npub, signatureHex, dataBytes);
            console.log('[JS] verifyInlineSignature:', isValid ? 'VALID' : 'INVALID');
            return { signatureHex, pubkeyHex, isValid, ciphertext: isEncrypted ? dataBytes : null };
        } catch (e) {
            console.error('[JS] verifyInlineSignature: verification failed:', e);
            return { signatureHex, pubkeyHex, isValid: false, ciphertext: isEncrypted ? dataBytes : null };
        }
    }

    /**
     * Recursively verify ALL signatures in a body, including nested quoted blocks.
     * Returns an array of verification results ordered innermost-first
     * (matching DOM h4 order where blockquoted content appears before outer sig).
     */
    async verifyAllSignatures(plainBody) {
        if (!plainBody) return [];

        // Verify the outermost signature
        let outerResult = null;
        try {
            outerResult = await this.verifyInlineSignature(plainBody);
        } catch (e) {
            console.warn('[JS] verifyAllSignatures: outer verification error:', e);
        }

        // Extract nested armor from depth-counting parser
        let innerArmor = null;
        const parts = this.parseArmorComponents(plainBody);
        if (parts && parts.quotedArmor) {
            innerArmor = parts.quotedArmor;
        }

        // Recurse into nested armor
        let innerResults = [];
        if (innerArmor) {
            innerResults = await this.verifyAllSignatures(innerArmor);
        }

        // Return innermost-first (matches DOM h4 order: blockquoted content before outer)
        const results = [...innerResults, outerResult].filter(r => r != null);
        console.log(`[JS] verifyAllSignatures: found ${results.length} signature(s), ${innerResults.length} from nested quotes, outer=${outerResult ? (outerResult.isValid ? 'VALID' : 'INVALID') : 'none'}`);
        return results;
    }

    /**
     * Find all signature+seal block pairs in the plain body text,
     * verify each, and update the corresponding DOM indicators
     * created by Utils.decorateArmorBlocks().
     */
    async verifyAndAnnotateSignatureBlocks(bodyText, containerId) {
        if (!bodyText) return;
        const gs = window.GlossiaService;

        // Quote prefix: lines in quoted replies start with "> "
        // Allow zero or more levels of quoting before each delimiter line
        const QP = '(?:>\\s*)*';
        // First, handle nested armor blocks (plaintext signed or encrypted+signed) that end with END NOSTR MESSAGE
        // Matches both new combined SIGNATURE block and legacy separate SEAL block
        const signedMsgRegex = new RegExp(`${QP}-{3,}\\s*BEGIN NOSTR (?:SIGNED (?:MESSAGE|BODY)|NIP-\\d+ ENCRYPTED (?:MESSAGE|BODY))\\s*-{3,}\\s*([\\s\\S]+?)\\s*${QP}-{3,}\\s*BEGIN NOSTR SIGNATURE\\s*-{3,}\\s*([\\s\\S]+?)\\s*(?:${QP}-{3,}\\s*BEGIN NOSTR SEAL\\s*-{3,}\\s*([\\s\\S]+?)\\s*)?${QP}-{3,}\\s*END NOSTR MESSAGE\\s*-{3,}`, 'g');
        let signedBlockIndex = 0;
        let smMatch;
        while ((smMatch = signedMsgRegex.exec(bodyText)) !== null) {
            const armorBody = smMatch[1].trim();
            const rawSigContent = smMatch[2].trim();
            const rawSealContent = smMatch[3] ? smMatch[3].trim() : null;
            // Detect whether this is encrypted or plaintext signed
            const isEncryptedArmor = new RegExp(`${QP}-{3,}\\s*BEGIN NOSTR (?:NIP-\\d+ ENCRYPTED (?:MESSAGE|BODY))\\s*-{3,}`).test(smMatch[0]);

            const el = document.getElementById(`inline-sig-block-${signedBlockIndex}`);
            signedBlockIndex++;
            if (!el) continue;
            const indicator = el.querySelector('.inline-sig-indicator');

            // Decode body content for verification
            let dataBytes = null;
            if (isEncryptedArmor) {
                // Encrypted: body is bitpacked then prose-encoded, so use transcodeToBytes
                // (handles grammar/cover words) to get packed cipher bytes
                try {
                    const decoded = (gs && gs.isReady()) ? gs.transcodeToBytes(armorBody) : null;
                    if (decoded) {
                        dataBytes = decoded;
                    } else {
                        // Fallback: raw base64 ciphertext
                        dataBytes = window.CryptoService.ciphertextToBytes(armorBody.replace(/\s+/g, ''));
                    }
                } catch (_) {}
            } else {
                // Plaintext signed: glossia body → UTF-8 bytes
                if (gs && gs.isReady()) {
                    try {
                        const bytes = gs.decodeToBytes(armorBody);
                        if (bytes) dataBytes = bytes;
                    } catch (_) {}
                }
            }
            if (!dataBytes) {
                if (indicator) {
                    indicator.className = 'inline-sig-indicator invalid';
                    indicator.innerHTML = '<i class="fas fa-question-circle"></i> Cannot decode body';
                }
                continue;
            }

            // Split sig content: legacy has separate SEAL, new format has combined block
            let signatureHex = null;
            let pubkeyHex = null;

            if (rawSealContent) {
                // Legacy: separate SEAL block — decode each independently
                const sigContentLines = rawSigContent.split('\n').filter(l => !l.trim().startsWith('@'));
                const sigText = sigContentLines.join(' ').trim();
                const sealLines = rawSealContent.split('\n').filter(l => !l.trim().startsWith('@'));
                const pubkeyText = sealLines.join(' ').trim();

                try {
                    if (gs && gs.isReady()) {
                        const sigDetections = gs.detectDialect(sigText);
                        const sigDialect = (Array.isArray(sigDetections) && sigDetections.length > 0) ? sigDetections[0].language : null;
                        if (sigDialect) {
                            const decoded = gs.decodeRawBaseN(sigText, sigDialect, 64);
                            if (decoded.length === 128) signatureHex = decoded;
                        }
                    }
                    if (!signatureHex && /^[0-9a-fA-F]{128}$/.test(sigText)) signatureHex = sigText;
                } catch (_) {}

                try {
                    const npubMatch = pubkeyText.match(/(npub1[a-z0-9]+)/);
                    if (npubMatch) {
                        pubkeyHex = window.CryptoService._npubToHex(npubMatch[1]);
                    } else if (gs && gs.isReady()) {
                        const pkDetections = gs.detectDialect(pubkeyText);
                        const pkDialect = (Array.isArray(pkDetections) && pkDetections.length > 0) ? pkDetections[0].language : null;
                        if (pkDialect) {
                            const decoded = gs.decodeRawBaseN(pubkeyText, pkDialect, 32);
                            if (decoded.length === 64) pubkeyHex = decoded;
                        }
                    }
                } catch (_) {}
            } else {
                // New: combined block — decode as 96 bytes, split at byte boundary
                const sigContentLines = rawSigContent.split('\n').filter(l => !l.trim().startsWith('@'));
                const allContent = sigContentLines.join('\n').trim();
                const split = window.emailService._splitSigPubkey(allContent);
                if (split) {
                    signatureHex = split.sigHex;
                    pubkeyHex = split.pubkeyHex;
                }
            }

            if (!signatureHex || !pubkeyHex) {
                if (indicator) {
                    indicator.className = 'inline-sig-indicator invalid';
                    indicator.innerHTML = '<i class="fas fa-question-circle"></i> Cannot decode';
                }
                continue;
            }

            try {
                const npub = window.CryptoService._nip19.npubEncode(pubkeyHex);
                const shortNpub = npub.substring(0, 12) + '...' + npub.substring(npub.length - 6);
                const isValid = await TauriService.verifySignature(npub, signatureHex, dataBytes);
                if (indicator) {
                    if (isValid) {
                        indicator.className = 'inline-sig-indicator verified';
                        indicator.innerHTML = `<i class="fas fa-check-circle"></i> Signed by: ${shortNpub}`;
                        indicator.title = `Verified signature from ${npub}`;
                        el.classList.add('verified');
                    } else {
                        indicator.className = 'inline-sig-indicator invalid';
                        indicator.innerHTML = '<i class="fas fa-times-circle"></i> Signature Invalid';
                        el.classList.add('invalid');
                    }
                }
            } catch (e) {
                console.error('[JS] verifyAndAnnotateSignatureBlocks: signed message verification error:', e);
                if (indicator) {
                    indicator.className = 'inline-sig-indicator invalid';
                    indicator.innerHTML = '<i class="fas fa-times-circle"></i> Verification Error';
                    el.classList.add('invalid');
                }
            }
        }

        // Then handle traditional signature+seal pairs
        const sigSealRegex = new RegExp(`(${QP}-{3,}\\s*BEGIN NOSTR SIGNATURE\\s*-{3,}\\s*([\\s\\S]+?)\\s*${QP}-{3,}\\s*END NOSTR SIGNATURE\\s*-{3,})[\\s\\n]*(${QP}-{3,}\\s*BEGIN NOSTR SEAL\\s*-{3,}\\s*([\\s\\S]+?)\\s*${QP}-{3,}\\s*END NOSTR SEAL\\s*-{3,})`, 'g');

        // Also find the encrypted message block (ciphertext) to verify against
        const encMsgRegex = new RegExp(`${QP}-{3,}\\s*BEGIN NOSTR (?:NIP-\\d+ ENCRYPTED (?:MESSAGE|BODY))\\s*-{3,}\\s*([\\s\\S]+?)\\s*${QP}-{3,}\\s*(?:END NOSTR (?:NIP-\\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\\s*-{3,}`, 'g');
        const ciphertexts = [];
        let encMatch;
        while ((encMatch = encMsgRegex.exec(bodyText)) !== null) {
            const armorContent = encMatch[1].trim();
            let ct = null;
            if (/^[A-Za-z0-9+/=\n?\s]+$/.test(armorContent.replace(/\s+/g, ''))) {
                ct = armorContent.replace(/\s+/g, '');
            } else if (gs && gs.isReady()) {
                try {
                    const result = gs.transcode(armorContent, 'decode');
                    ct = result.output;
                } catch (_) {}
            }
            if (ct) ciphertexts.push(ct);
        }

        let blockIndex = signedBlockIndex;
        let match;
        while ((match = sigSealRegex.exec(bodyText)) !== null) {
            const sigContent = match[2].trim().split('\n')
                .filter(l => !l.trim().startsWith('@'))
                .join(' ').trim();
            const sealContent = match[4].trim().split('\n')
                .filter(l => !l.trim().startsWith('@'))
                .join(' ').trim();

            const el = document.getElementById(`inline-sig-block-${blockIndex}`);
            blockIndex++;
            if (!el) continue;

            const indicator = el.querySelector('.inline-sig-indicator');

            let pubkeyHex = null;
            let signatureHex = null;

            // Decode pubkey
            try {
                const npubMatch = sealContent.match(/(npub1[a-z0-9]+)/);
                if (npubMatch) {
                    pubkeyHex = window.CryptoService._npubToHex(npubMatch[1]);
                } else if (gs && gs.isReady()) {
                    const pkDetections = gs.detectDialect(sealContent);
                    const pkDialect = (Array.isArray(pkDetections) && pkDetections.length > 0) ? pkDetections[0].language : null;
                    if (pkDialect) {
                        const decoded = gs.decodeRawBaseN(sealContent, pkDialect, 32);
                        if (decoded.length === 64) pubkeyHex = decoded;
                    }
                }
            } catch (_) {}

            // Decode signature
            try {
                if (gs && gs.isReady()) {
                    const sigDetections = gs.detectDialect(sigContent);
                    const sigDialect = (Array.isArray(sigDetections) && sigDetections.length > 0) ? sigDetections[0].language : null;
                    if (sigDialect) {
                        const decoded = gs.decodeRawBaseN(sigContent, sigDialect, 64);
                        if (decoded.length === 128) signatureHex = decoded;
                    }
                }
            } catch (_) {}

            if (!pubkeyHex || !signatureHex) {
                if (indicator) {
                    indicator.className = 'inline-sig-indicator invalid';
                    indicator.innerHTML = '<i class="fas fa-question-circle"></i> Cannot decode';
                }
                continue;
            }

            // Verify against the most recent ciphertext preceding this sig block
            // Use the last ciphertext found before this match position, or the latest one
            const ciphertext = ciphertexts.length > 0 ? ciphertexts[Math.min(blockIndex - 1, ciphertexts.length - 1)] : null;

            let dataBytes;
            if (ciphertext) {
                dataBytes = window.CryptoService.ciphertextToBytes(ciphertext);
            } else {
                // Plaintext email: signed data is the body text with sig/seal blocks stripped
                // Normalize \r\n to \n to match what the textarea produced at sign time
                const bodyOnly = bodyText
                    .replace(/\r\n/g, '\n')
                    .replace(/-{3,}\s*BEGIN NOSTR SIGNATURE\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR (?:SIGNATURE|MESSAGE)\s*-{3,}/g, '')
                    .replace(/-{3,}\s*BEGIN NOSTR SEAL\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR (?:SEAL|MESSAGE)\s*-{3,}/g, '')
                    .trim();
                if (!bodyOnly) {
                    if (indicator) {
                        indicator.className = 'inline-sig-indicator invalid';
                        indicator.innerHTML = '<i class="fas fa-question-circle"></i> No content to verify';
                    }
                    continue;
                }
                dataBytes = new TextEncoder().encode(bodyOnly);
            }

            try {
                const npub = window.CryptoService._nip19.npubEncode(pubkeyHex);
                const shortNpub = npub.substring(0, 12) + '...' + npub.substring(npub.length - 6);
                const isValid = await TauriService.verifySignature(npub, signatureHex, dataBytes);
                if (indicator) {
                    if (isValid) {
                        indicator.className = 'inline-sig-indicator verified';
                        indicator.innerHTML = `<i class="fas fa-check-circle"></i> Signed by: ${shortNpub}`;
                        indicator.title = `Verified signature from ${npub}`;
                        el.classList.add('verified');
                    } else {
                        indicator.className = 'inline-sig-indicator invalid';
                        indicator.innerHTML = '<i class="fas fa-times-circle"></i> Signature Invalid';
                        el.classList.add('invalid');
                    }
                }
            } catch (e) {
                console.error('[JS] verifyAndAnnotateSignatureBlocks: verification error:', e);
                if (indicator) {
                    indicator.className = 'inline-sig-indicator invalid';
                    indicator.innerHTML = '<i class="fas fa-times-circle"></i> Verification Error';
                    el.classList.add('invalid');
                }
            }
        }
    }

    // Split compose body into reply text and quoted original.
    // Quoted lines start with "> " — the first contiguous block of quoted lines
    // at the end of the body is treated as the quoted original message.
    // Concatenate multiple Uint8Arrays into one
    _concatBytes(...arrays) {
        const filtered = arrays.filter(a => a && a.length);
        const total = filtered.reduce((sum, a) => sum + a.length, 0);
        const result = new Uint8Array(total);
        let offset = 0;
        for (const a of filtered) { result.set(a, offset); offset += a.length; }
        return result;
    }

    // Decode an armor body section to canonical bytes.
    // For encrypted: transcodeToBytes (packed cipher bytes) or base64 decode.
    // For signed plaintext: glossia round-trip decode to UTF-8 bytes.
    _decodeArmorBodyToBytes(bodyText, isEncrypted) {
        const gs = window.GlossiaService;
        if (isEncrypted) {
            const decoded = (gs && gs.isReady()) ? gs.transcodeToBytes(bodyText) : null;
            if (decoded) return decoded;
            return window.CryptoService.ciphertextToBytes(bodyText.replace(/\s+/g, ''));
        }
        // Signed plaintext: detect dialect, decode back to original text, UTF-8 encode
        if (gs && gs.isReady()) {
            try {
                const detections = gs.detectDialect(bodyText);
                if (Array.isArray(detections) && detections.length > 0 && detections[0].language) {
                    const result = gs.transcode(bodyText, `decode from ${detections[0].language}`);
                    if (result.output) return new TextEncoder().encode(result.output);
                }
            } catch (_) {}
            // Try transcodeToBytes as fallback
            try {
                const bytes = gs.transcodeToBytes(bodyText);
                if (bytes) return bytes;
            } catch (_) {}
        }
        return new TextEncoder().encode(bodyText);
    }

    // Recursively extract and concatenate all body bytes from nested armor.
    // For a 3-level chain, returns: decode(outer_body) + decode(middle_body) + decode(inner_body).
    _extractAllBodyBytes(armorText) {
        if (!armorText) return null;
        // Try encrypted/general armor format
        const parts = this.parseArmorComponents(armorText);
        if (parts && parts.bodyText) {
            const isEnc = !!armorText.match(/-{3,}\s*BEGIN NOSTR (?:NIP-(?:04|44) ENCRYPTED)/);
            const bodyBytes = this._decodeArmorBodyToBytes(parts.bodyText, isEnc);
            if (!bodyBytes) return null;
            if (parts.quotedArmor) {
                const deeperBytes = this._extractAllBodyBytes(parts.quotedArmor);
                return deeperBytes ? this._concatBytes(bodyBytes, deeperBytes) : bodyBytes;
            }
            return bodyBytes;
        }
        // Try signed plaintext format
        const signedMsg = this.decodeGlossiaSignedMessage(armorText);
        if (signedMsg && signedMsg.glossiaBody) {
            const bodyBytes = this._decodeArmorBodyToBytes(signedMsg.glossiaBody, false);
            if (!bodyBytes) return null;
            if (signedMsg.quotedArmor) {
                const deeperBytes = this._extractAllBodyBytes(signedMsg.quotedArmor);
                return deeperBytes ? this._concatBytes(bodyBytes, deeperBytes) : bodyBytes;
            }
            return bodyBytes;
        }
        return null;
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

    // Returns { replyText, quotedOriginal } where quotedOriginal includes plaintext quotes and armor.
    splitReplyAndQuoted(body) {
        if (!body) return { replyText: '', quotedOriginal: '' };

        // New format: quoted original starts at the first armor body BEGIN tag after the user's reply.
        // The textarea contains: reply text, then optional "> " quoted plaintext, then armor block.
        // Find the first body-level BEGIN NOSTR tag that isn't at the very start (the start would be the user's own armor).
        // Only match body-level tags (ENCRYPTED BODY/MESSAGE or SIGNED BODY/MESSAGE), NOT structural
        // tags like SEAL or SIGNATURE which are part of the current message's armor structure.
        const armorIdx = body.search(/\n-{3,}\s*BEGIN NOSTR (?:NIP-(?:04|44) ENCRYPTED |SIGNED )/);
        if (armorIdx >= 0) {
            // Check if there's also > quoted plaintext just before the armor
            const beforeArmor = body.substring(0, armorIdx);
            const afterArmor = body.substring(armorIdx + 1); // +1 to skip \n
            // Walk backwards from armorIdx to include any > quoted plaintext block
            const beforeLines = beforeArmor.split('\n');
            let splitAt = beforeLines.length;
            for (let i = beforeLines.length - 1; i >= 0; i--) {
                if (beforeLines[i].startsWith('> ') || beforeLines[i] === '>') {
                    splitAt = i;
                } else if (beforeLines[i].trim() === '') {
                    continue;
                } else {
                    break;
                }
            }
            const replyText = beforeLines.slice(0, splitAt).join('\n').trimEnd();
            const quotedPlain = beforeLines.slice(splitAt).join('\n');
            const quotedOriginal = (quotedPlain.trim() ? quotedPlain.trim() + '\n\n' : '') + afterArmor.trim();
            return { replyText, quotedOriginal };
        }

        // Backwards compat: detect > prefixed lines (old format or forwarded emails)
        const lines = body.split('\n');
        let lastNonQuoted = lines.length;
        for (let i = lines.length - 1; i >= 0; i--) {
            if (lines[i].startsWith('> ') || lines[i] === '>') {
                lastNonQuoted = i;
            } else if (lines[i].trim() === '') {
                continue;
            } else {
                break;
            }
        }
        if (lastNonQuoted >= lines.length) {
            return { replyText: body, quotedOriginal: '' };
        }
        const replyText = lines.slice(0, lastNonQuoted).join('\n').trimEnd();
        const quotedLines = lines.slice(lastNonQuoted);
        const stripped = quotedLines.map(l => {
            if (l.startsWith('> ')) return l.substring(2);
            if (l === '>') return '';
            return l;
        }).join('\n').trim();
        return { replyText, quotedOriginal: stripped };
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

        const privkey = appState.getKeypair().private_key;
        const pubkey = this.selectedNostrContact.pubkey;
        const settings = appState.getSettings();
        const encryptionAlgorithm = settings?.encryption_algorithm || 'nip44';

        try {
            const gs = window.GlossiaService;
            if (!gs || !gs.isReady()) {
                notificationService.showError('Glossia WASM not loaded');
                return false;
            }

            // 1. Pack plaintext into JSON struct
            const emailPayload = JSON.stringify({ version: 1, subject, body });

            // 2. Gzip compress
            const compressed = await this.gzipCompress(emailPayload);

            // 3. Encrypt the compressed data
            const compressedBase64 = btoa(String.fromCharCode(...compressed));
            const ciphertext = await TauriService.encryptMessageWithAlgorithm(
                privkey, pubkey, compressedBase64, encryptionAlgorithm
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
            if (!gs || !gs.isReady()) {
                notificationService.showError('Glossia WASM not loaded');
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
            const privkey = appState.getKeypair().private_key;
            const senderNpub = window.CryptoService._nip19.npubEncode(outer.pubkeyHex);
            let compressedBase64;
            try {
                compressedBase64 = await TauriService.decryptDmContent(
                    privkey, senderNpub, ciphertext
                );
            } catch (_) {
                // Sender pubkey didn't work — try the selected contact (other party)
                const contactPubkey = this.selectedNostrContact?.pubkey;
                if (!contactPubkey) throw new Error('Decryption failed and no contact selected to try');
                compressedBase64 = await TauriService.decryptDmContent(
                    privkey, contactPubkey, ciphertext
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
            if (!gs || !gs.isReady()) {
                notificationService.showError('Glossia WASM not loaded');
                return false;
            }

            // Encode subject (bitpack NIP-04 ciphertext for compactness)
            if (currentSubject) {
                console.log('[JS] Encoding subject with transcode("encode into ' + meta + ' raw")...');
                const packed = gs._packNip04(currentSubject.trim());
                const result = gs.transcode(packed, `encode into ${meta} raw`);
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
                const result = gs.transcode(packed, `encode into ${meta}`);
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
            if (!gs || !gs.isReady()) {
                notificationService.showError('Glossia WASM not loaded');
                return false;
            }

            if (currentSubject) {
                const subjectInstruction = decodeMeta ? `decode from ${decodeMeta} raw` : 'decode raw';
                console.log('[JS] Decoding subject with transcode("' + subjectInstruction + '")...');
                const result = gs.transcode(currentSubject, subjectInstruction);
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
                    const parsed = gs.parseSignedBody(currentBody, metaPubkey, metaSig);
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

                const result = gs.transcode(bodyToDecode, bodyInstruction);
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
                notificationService.showError('Select a Nostr contact to encrypt for');
                return false;
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
            // Check if we should use manifest encryption
            const hasAttachments = this.attachments.length > 0;
            const bodySize = new TextEncoder().encode(body || '').length;
            const shouldUseManifest = hasAttachments || bodySize > 64 * 1024; // 64 KB threshold
            
            console.log(`[JS] Body size: ${bodySize} bytes (${(bodySize / 1024).toFixed(1)} KB)`);
            console.log(`[JS] Has attachments: ${hasAttachments}`);
            console.log(`[JS] Should use manifest: ${shouldUseManifest}`);

            // Capture raw encrypted body ciphertext for _plainBody generation
            let rawEncryptedBody = null;

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
                    encryptedSubject = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, subject, encryptionAlgorithm);
                    console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
                    domManager.setValue('subject', encryptedSubject.trim());
                }

                // 4. JSON.stringify(manifest) → encrypt entire manifest with NIP → ASCII armor
                console.log('[JS] Creating encrypted manifest...');
                const manifestJson = JSON.stringify(manifest);
                console.log('[JS] Manifest JSON size:', manifestJson.length);
                
                const encryptedManifest = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, manifestJson, encryptionAlgorithm);
                console.log('[JS] Manifest encrypted, size:', encryptedManifest.length);
                rawEncryptedBody = encryptedManifest;

                // Wrap in ASCII armor
                const armoredManifest = this.armorCiphertext(encryptedManifest, encryptionAlgorithm);
                domManager.setValue('messageBody', armoredManifest.trim());
                
                // Clear signature when encrypting (body state changed)
                this.clearSignature();
                
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
                    encryptedSubject = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, subject, encryptionAlgorithm);
                    console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
                    domManager.setValue('subject', encryptedSubject.trim());
                }

                // Encrypt body directly with NIP
                let encryptedBody = body;
                if (body) {
                    console.log('[JS] Encrypting body...');
                    encryptedBody = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, body, encryptionAlgorithm);
                    console.log('[JS] Body encrypted:', encryptedBody.substring(0, 50) + '...');
                    rawEncryptedBody = encryptedBody;

                    // Wrap in ASCII armor
                    const armoredBody = this.armorCiphertext(encryptedBody, encryptionAlgorithm);
                    domManager.setValue('messageBody', armoredBody.trim());
                }
                
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
                if (gs && gs.isReady()) {
                    const metaSig = this.getGlossiaEncodingSignature();
                    const metaPubkey = this.getGlossiaEncodingPubkey();
                    if (sigHex && pkHex) {
                        // Encode sig+pubkey as single 96-byte payload for combined SIGNATURE block
                        const result = gs.encodeSigPubkey(sigHex, pkHex, metaSig);
                        if (result.combined) {
                            encodedSigPubkey = result.encodedSigPubkey;
                        } else {
                            encodedSig = result.encodedSig;
                            encodedPubkey = result.encodedPubkey;
                        }
                    }
                    // For unsigned messages, encode pubkey separately for SEAL block
                    if (!encodedPubkey && !encodedSigPubkey && pkHex) {
                        encodedPubkey = gs.encodePubkey(pkHex, metaPubkey);
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
                const quotedHtmlContent = this.buildRecursiveQuotedHtml(this._quotedOriginalArmor);
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
        
        const privkey = appState.getKeypair().private_key;
        const pubkey = contact.pubkey;
        console.log('[JS] Using privkey:', privkey.substring(0, 20) + '...');
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
                encryptedSubject = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, subject, encryptionAlgorithm);
                console.log('[JS] Subject encrypted:', encryptedSubject.substring(0, 50) + '...');
            }

            // Encrypt body
            let encryptedBody = body;
            if (body) {
                console.log('[JS] Encrypting body in memory...');
                encryptedBody = await TauriService.encryptMessageWithAlgorithm(privkey, pubkey, body, encryptionAlgorithm);
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
    async renderSentEmails(showLoadMore = false) {
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
            const emails = appState.getSentEmails();
            
            if (!emails || emails.length === 0) {
                sentList.innerHTML = '<div class="text-center text-muted">No sent emails found</div>';
                return;
            }
            
            // Always re-render all emails (simpler approach)
            sentList.innerHTML = '';
            
            console.log(`[JS] renderSentEmails: Rendering ${emails.length} sent emails`);
            
            // Check if we should hide unverified messages
            const settings = appState.getSettings();
            const hideUnverified = settings && settings.hide_unsigned_messages === true;
            
            // Process emails in parallel with timeout protection
            const emailPromises = emails
                .filter(email => {
                    // Filter out unverified emails if hideUnverified is enabled
                    if (hideUnverified && email.signature_valid !== true) {
                        return false;
                    }
                    return true;
                })
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
            
            // Show message if no emails were rendered (possibly all filtered out)
            if (renderedCount === 0) {
                const settings = appState.getSettings();
                const hideUndecryptable = settings && settings.hide_undecryptable_emails === true;
                const hideUnverified = settings && settings.hide_unsigned_messages === true;
                if (hideUndecryptable && emails.length > 0) {
                    sentList.innerHTML = '<div class="text-center text-muted">No decryptable emails found. All emails are encrypted for a different keypair.</div>';
                } else if (hideUnverified && emails.length > 0) {
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
        
        // Decode glossia signed message body for preview
        let sentPreviewBody = email.body || '';
        const sentSignedMsg = this.decodeGlossiaSignedMessage(sentPreviewBody);
        if (sentSignedMsg && sentSignedMsg.plaintextBody) {
            sentPreviewBody = sentSignedMsg.plaintextBody;
        }
        let previewText = sentPreviewBody ? Utils.escapeHtml(sentPreviewBody.substring(0, 100)) : '';
        let showSubject = true;
        let previewSubject = email.subject;

        // Detect any NOSTR NIP-X ENCRYPTED MESSAGE (same as inbox)
        const armorRegex = /-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}/;
        if (email.body && armorRegex.test(email.body)) {
            const keypair = appState.getKeypair();
            if (!keypair) {
                previewText = 'Unable to decrypt: no keypair';
            } else {
                        // Decrypt subject — try base64 first, then glossia decode
                        let subjectCiphertext = null;
                        if (Utils.isLikelyEncryptedContent(email.subject)) {
                            subjectCiphertext = email.subject;
                        } else {
                            subjectCiphertext = this.decodeGlossiaSubject(email.subject);
                        }
                        if (subjectCiphertext) {
                            try {
                                const decryptedSubject = await this.decryptNostrSentMessageWithFallback(email, subjectCiphertext, keypair);
                                if (decryptedSubject && (decryptedSubject.startsWith('Unable to decrypt') || decryptedSubject.includes('Unable to decrypt'))) {
                                    previewSubject = 'Could not decrypt';
                                } else if (decryptedSubject) {
                                    previewSubject = decryptedSubject;
                                } else {
                                    previewSubject = 'Could not decrypt';
                                }
                            } catch (e) {
                                previewSubject = 'Could not decrypt';
                            }
                        }
                        // Decrypt body - try manifest format first, then fallback to legacy
                        const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                if (encryptedBodyMatch) {
                    // Extract ciphertext: base64 as-is, glossia needs decoding
                    const armorContent = encryptedBodyMatch[1].trim();
                    let encryptedContent;
                    if (/^[A-Za-z0-9+/=\n?]+$/.test(armorContent.replace(/\s+/g, ''))) {
                        encryptedContent = armorContent.replace(/\s+/g, '');
                    } else {
                        const glossiaResult = this.decodeGlossiaArmoredBody(email.body);
                        encryptedContent = glossiaResult ? glossiaResult.ciphertext : armorContent.replace(/\s+/g, '');
                    }
                    try {
                        // Try sent manifest decryption first
                        const manifestResult = await this.decryptSentManifestMessage(email, encryptedContent, keypair);
                        if (manifestResult.type === 'manifest') {
                            previewText = Utils.escapeHtml(manifestResult.body.substring(0, 100));
                            showSubject = true;
                        } else if (manifestResult.type === 'legacy') {
                            previewText = Utils.escapeHtml(manifestResult.body.substring(0, 100));
                            showSubject = true;
                        } else {
                            // Fallback to legacy decryption
                            const decrypted = await this.decryptNostrSentMessageWithFallback(email, encryptedContent, keypair);
                            // Check if decryption returned an error message
                            if (decrypted && (decrypted.startsWith('Unable to decrypt') || decrypted.includes('Unable to decrypt'))) {
                                previewText = 'Could not decrypt';
                            } else if (decrypted) {
                                previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                showSubject = true;
                            } else {
                                previewText = 'Could not decrypt';
                            }
                        }
                    } catch (e) {
                        // If manifest fails, try legacy decryption
                        try {
                            const decrypted = await this.decryptNostrSentMessageWithFallback(email, encryptedContent, keypair);
                            // Check if decryption returned an error message
                            if (decrypted && (decrypted.startsWith('Unable to decrypt') || decrypted.includes('Unable to decrypt'))) {
                                previewText = 'Could not decrypt';
                            } else if (decrypted) {
                                previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                showSubject = true;
                            } else {
                                previewText = 'Could not decrypt';
                            }
                        } catch (legacyError) {
                            previewText = 'Could not decrypt';
                        }
                    }
                }
            }
        } else {
            // Body has no armor — not encrypted, show plaintext as-is
            // (sentSignedMsg already decoded above if signed message armor present)
            previewText = Utils.escapeHtml(sentPreviewBody ? sentPreviewBody.substring(0, 100) : '');
            if (sentPreviewBody && sentPreviewBody.length > 100) previewText += '...';
            showSubject = true;
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
        // Check signature_valid - handle both boolean and null/undefined cases
        if (email.signature_valid === true || email.signature_valid === 1) {
            signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature"><i class="fas fa-pen"></i> Signature Verified</span>`;
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

        // Get recipient contact for avatar (similar to inbox getting sender contact)
        const recipientEmail = email.to;
        const contacts = appState.getContacts();
        let recipientContact = null;
        
        if (recipientEmail) {
            // Normalize Gmail addresses (remove + aliases)
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
            // Try both original and normalized email
            recipientContact = contacts.find(c => {
                if (!c.email) return false;
                const contactEmail = c.email.trim().toLowerCase();
                return contactEmail === recipientEmail.toLowerCase() || contactEmail === normalizedEmail;
            });
        }
        
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
                    <div class="email-date">${dateDisplay}</div>
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
        emailElement.addEventListener('click', () => this.showSentDetail(email.id));
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

        // Get recipient contact for avatar (similar to inbox getting sender contact)
        const recipientEmail = email.to;
        const contacts = appState.getContacts();
        let recipientContact = null;
        
        if (recipientEmail) {
            // Normalize Gmail addresses (remove + aliases)
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
            // Try both original and normalized email
            recipientContact = contacts.find(c => {
                if (!c.email) return false;
                const contactEmail = c.email.trim().toLowerCase();
                return contactEmail === recipientEmail.toLowerCase() || contactEmail === normalizedEmail;
            });
        }
        
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
        emailElement.addEventListener('click', () => this.showSentDetail(email.id));
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
        const isEncryptedSubject = encryptedBodyMatch && (Utils.isLikelyEncryptedContent(email.subject) || !!this.decodeGlossiaSubject(email.subject));
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
                
                // Add signature verification indicator
                let signatureIndicator = '';
                // Check signature_valid - handle both boolean and null/undefined cases
                if (email.signature_valid === true || email.signature_valid === 1) {
                    signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature"><i class="fas fa-pen"></i> Signature Verified</span>`;
                } else if (email.signature_valid === false || email.signature_valid === 0) {
                    signatureIndicator = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Invalid Nostr signature"><i class="fas fa-pen"></i> Signature Invalid</span>`;
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
                    // Look up our profile in contacts list (user's profile is now added privately)
                    const contacts = appState.getContacts();
                    senderContact = contacts.find(c => c.pubkey === myPubkey) || null;
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
${signatureIndicator}
</div>
<div class="email-sender-time">${Utils.escapeHtml(timeAgo)}</div>
</div>
</div>
<details class="email-metadata-details">
<summary class="email-metadata-summary"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></summary>
<div class="email-detail-header vertical" id="sent-email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
</div>
</details>
</div>
<div class="error" style="margin-bottom: 15px;">Cannot decrypt: Decryption failed with all known contact keys. The recipient's pubkey may have changed since this email was sent.</div>
<pre id="sent-raw-header-info" class="email-raw-content">${Utils.escapeHtml(rawHeaders)}</pre>
<div class="email-detail-body" id="sent-email-body-info">${email.html_body ? '' : Utils.escapeHtml(rawBody).replace(/\n/g, '<br>')}</div>
<pre id="sent-raw-body-info" class="email-raw-content email-raw-body">${Utils.escapeHtml(rawBody)}${email.html_body ? '\n\n--- text/html ---\n\n' + Utils.escapeHtml(email.html_body) : ''}</pre>
</div>
<button id="sent-toggle-raw-btn" class="btn btn-secondary" style="margin: 18px 0 0 0;">Show Raw Content</button>
</div>`;
                // This is the error path (no recipient pubkey) - show raw HTML if available
                if (email.html_body) {
                    Utils.renderHtmlBodyInIframe('sent-email-body-info', email.html_body);
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
                
                // Set up the toggle button for raw content
                const toggleRawBtn = document.getElementById('sent-toggle-raw-btn');
                const headerInfo = document.getElementById('sent-email-header-info');
                const rawHeaderInfo = document.getElementById('sent-raw-header-info');
                const bodyInfo = document.getElementById('sent-email-body-info');
                const rawBodyInfo = document.getElementById('sent-raw-body-info');
                
                if (toggleRawBtn && headerInfo && rawHeaderInfo && bodyInfo && rawBodyInfo) {
                    // Remove any existing event listeners by cloning the button
                    const newToggleBtn = toggleRawBtn.cloneNode(true);
                    toggleRawBtn.parentNode.replaceChild(newToggleBtn, toggleRawBtn);
                    
                    let showingRaw = false;
                    newToggleBtn.addEventListener('click', (event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        
                        showingRaw = !showingRaw;
                        if (showingRaw) {
                            headerInfo.classList.add('hidden-header');
                            rawHeaderInfo.style.display = 'block';
                            bodyInfo.style.display = 'none';
                            rawBodyInfo.style.display = 'block';
                            newToggleBtn.textContent = 'Show Display Content';
                            
                            // Hide metadata details when showing raw content
                            const metadataDetails = headerInfo.closest('.email-metadata-details');
                            if (metadataDetails) {
                                metadataDetails.style.display = 'none';
                            }
                        } else {
                            headerInfo.classList.remove('hidden-header');
                            rawHeaderInfo.style.display = 'none';
                            bodyInfo.style.display = 'block';
                            rawBodyInfo.style.display = 'none';
                            newToggleBtn.textContent = 'Show Raw Content';
                            
                            // Show metadata details when showing display content
                            const metadataDetails = headerInfo.closest('.email-metadata-details');
                            if (metadataDetails) {
                                metadataDetails.style.display = '';
                            }
                        }
                    });
                }
                return;
            }
        }
        
        // Define updateDetail function first (before it's called)
        const updateDetail = async (subject, body, cachedManifestResult, wasDecrypted = false, inlineSigResult = null) => {
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
                            console.log('[JS] No valid cached manifest, need to decrypt');
                            console.log('[JS] No cached manifest, attempting to decrypt...');
                            console.log('[JS] Email body length:', email.body ? email.body.length : 0);
                            console.log('[JS] Email body preview:', email.body ? email.body.substring(0, 200) : 'null');
                            
                            // Extract encrypted content from the body (permissive regex)
                            const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:(NIP-\d+) ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                            if (!encryptedBodyMatch) {
                                console.warn('[JS] No encrypted content found in email body using regex, trying raw body');
                                const keypair = appState.getKeypair();
                                try {
                                    manifestResult = await this.decryptSentManifestMessage(email, email.body.replace(/\s+/g, ''), keypair);
                                } catch (e) {
                                    console.error('[JS] Failed to decrypt with raw body:', e);
                                    throw new Error('No encrypted content found in email body');
                                }
                            } else {
                                const armorContent = encryptedBodyMatch[2].trim();
                                let encryptedContent;
                                if (/^[A-Za-z0-9+/=\n?]+$/.test(armorContent.replace(/\s+/g, ''))) {
                                    encryptedContent = armorContent.replace(/\s+/g, '');
                                } else {
                                    const glossiaResult = this.decodeGlossiaArmoredBody(email.body);
                                    encryptedContent = glossiaResult ? glossiaResult.ciphertext : armorContent.replace(/\s+/g, '');
                                }
                                console.log('[JS] Extracted encrypted content, length:', encryptedContent.length);
                                const keypair = appState.getKeypair();

                                // Decrypt the manifest to get original attachment metadata
                                manifestResult = await this.decryptSentManifestMessage(email, encryptedContent, keypair);
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
            
            // Add signature verification indicator
            let signatureIndicator = '';
            console.log('[JS] Sent updateDetail: inlineSigResult=', inlineSigResult, 'email.signature_valid=', email.signature_valid);
            // Check inline signature result first (from body armor verification)
            // Extract the outer sig result (last element if array, since DOM order is [quoted, outer])
            const outerSigResult = Array.isArray(inlineSigResult) ? inlineSigResult[inlineSigResult.length - 1] : inlineSigResult;
            if (outerSigResult && outerSigResult.isValid === true) {
                signatureIndicator = `<span class="signature-indicator verified" title="Inline signature verified"><i class="fas fa-check-circle"></i> Signature Verified</span>`;
            } else if (outerSigResult && outerSigResult.isValid === false) {
                signatureIndicator = `<span class="signature-indicator invalid" title="Inline signature invalid"><i class="fas fa-times-circle"></i> Signature Invalid</span>`;
            } else if (email.signature_valid === true || email.signature_valid === 1) {
                signatureIndicator = `<span class="signature-indicator verified" title="Verified Nostr signature"><i class="fas fa-pen"></i> Signature Verified</span>`;
            } else if (email.signature_valid === false || email.signature_valid === 0) {
                signatureIndicator = `<span class="signature-indicator invalid" data-message-id="${Utils.escapeHtml(email.message_id || email.id)}" title="Invalid Nostr signature"><i class="fas fa-pen"></i> Signature Invalid</span>`;
            }
            console.log('[JS] Sent updateDetail: signatureIndicator=', signatureIndicator ? 'SET' : 'EMPTY');

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
                    const contacts = appState.getContacts();
                    senderContact = contacts.find(c => c.pubkey === myPubkey) || null;
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
${signatureIndicator}
</div>
<div class="email-sender-time">${Utils.escapeHtml(timeAgo)}</div>
</div>
</div>
<details class="email-metadata-details">
<summary class="email-metadata-summary"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></summary>
<div class="email-detail-header vertical" id="sent-email-header-info">
<div class="email-header-row"><span class="email-header-label">From:</span> <span class="email-header-value">${Utils.escapeHtml(email.from)}</span></div>
<div class="email-header-row"><span class="email-header-label">To:</span> <span class="email-header-value">${Utils.escapeHtml(email.to)}</span></div>
<div class="email-header-row"><span class="email-header-label">Date:</span> <span class="email-header-value">${new Date(email.date).toLocaleString()}</span></div>
</div>
</details>
</div>
<pre id="sent-raw-header-info" class="email-raw-content">${Utils.escapeHtml(email.raw_headers || '')}</pre>
<div class="email-detail-body" id="sent-email-body-info">${email.html_body ? '' : Utils.escapeHtml(body).replace(/\n/g, '<br>')}</div>
<pre id="sent-raw-body-info" class="email-raw-content email-raw-body">${Utils.escapeHtml(email.raw_body)}${email.html_body ? '\n\n--- text/html ---\n\n' + Utils.escapeHtml(email.html_body) : ''}</pre>
${attachmentsHtml}
</div>
<button id="sent-toggle-raw-btn" class="btn btn-secondary" style="margin: 18px 0 0 0;">Show Raw Content</button>
</div>`;
            if (email.html_body && wasDecrypted) {
                // Patch the HTML body: replace glossia-encoded div content with decrypted text
                try {
                    const parser = new DOMParser();
                    const doc = parser.parseFromString(email.html_body, 'text/html');
                    // Find the first content div (contains the glossia prose or ciphertext)
                    const contentDiv = doc.querySelector('body > div > div') || doc.querySelector('body > div');
                    if (contentDiv) {
                        contentDiv.innerHTML = Utils.escapeHtml(body).replace(/\n/g, '<br>');
                    }
                    let patchedHtml = doc.documentElement.outerHTML;
                    if (inlineSigResult) patchedHtml = this.injectHtmlSigBadge(patchedHtml, inlineSigResult);
                    Utils.renderHtmlBodyInIframe('sent-email-body-info', patchedHtml);
                } catch (e) {
                    console.error('[JS] Failed to patch HTML body with decrypted text:', e);
                    Utils.renderHtmlBodyInIframe('sent-email-body-info', email.html_body);
                }
            } else if (email.html_body) {
                let htmlToRender = email.html_body;
                if (inlineSigResult) htmlToRender = this.injectHtmlSigBadge(htmlToRender, inlineSigResult);
                Utils.renderHtmlBodyInIframe('sent-email-body-info', htmlToRender);
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
            
            const toggleRawBtn = document.getElementById('sent-toggle-raw-btn');
            const headerInfo = document.getElementById('sent-email-header-info');
            const rawHeaderInfo = document.getElementById('sent-raw-header-info');
            const bodyInfo = document.getElementById('sent-email-body-info');
            const rawBodyInfo = document.getElementById('sent-raw-body-info');
            const attachmentsInfo = document.getElementById('sent-email-attachments');
            
            if (toggleRawBtn && headerInfo && rawHeaderInfo && bodyInfo && rawBodyInfo) {
                // Remove any existing event listeners by cloning the button
                const newToggleBtn = toggleRawBtn.cloneNode(true);
                toggleRawBtn.parentNode.replaceChild(newToggleBtn, toggleRawBtn);
                
                let showingRaw = false;
                newToggleBtn.addEventListener('click', (event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    
                    showingRaw = !showingRaw;
                    if (showingRaw) {
                        headerInfo.classList.add('hidden-header');
                        rawHeaderInfo.style.display = 'block';
                        bodyInfo.style.display = 'none';
                        rawBodyInfo.style.display = 'block';
                        newToggleBtn.textContent = 'Show Display Content';
                        
                        // Hide metadata details when showing raw content
                        const metadataDetails = headerInfo.closest('.email-metadata-details');
                        if (metadataDetails) {
                            metadataDetails.style.display = 'none';
                        }
                        
                        // Update attachment filenames and sizes to show encrypted versions
                        if (attachmentsInfo) {
                            const attachmentItems = attachmentsInfo.querySelectorAll('.attachment-item');
                            attachmentItems.forEach(item => {
                                const encryptedFilename = item.getAttribute('data-encrypted-filename');
                                const encryptedSize = item.getAttribute('data-encrypted-size');
                                const nameElement = item.querySelector('.attachment-name');
                                const sizeElement = item.querySelector('.attachment-size');
                                
                                if (encryptedFilename && nameElement) {
                                    nameElement.textContent = encryptedFilename;
                                }
                                
                                // Update size to show encrypted size
                                if (encryptedSize && sizeElement) {
                                    const sizeFormatted = (parseFloat(encryptedSize) / 1024).toFixed(2) + ' KB';
                                    sizeElement.textContent = sizeFormatted;
                                }
                            });
                        }
                        
                        // Move attachments after raw body if they exist
                        if (attachmentsInfo && attachmentsInfo.parentNode) {
                            // Remove from current position
                            attachmentsInfo.parentNode.removeChild(attachmentsInfo);
                            // Insert after raw body
                            rawBodyInfo.parentNode.insertBefore(attachmentsInfo, rawBodyInfo.nextSibling);
                        }
                        
                        // Keep button outside the card (after the card closes)
                        const emailDetailCard = headerInfo.closest('.email-detail-card');
                        if (emailDetailCard && emailDetailCard.parentNode) {
                            emailDetailCard.parentNode.insertBefore(newToggleBtn, emailDetailCard.nextSibling);
                        }
                    } else {
                        headerInfo.classList.remove('hidden-header');
                        rawHeaderInfo.style.display = 'none';
                        bodyInfo.style.display = 'block';
                        rawBodyInfo.style.display = 'none';
                        newToggleBtn.textContent = 'Show Raw Content';
                        
                        // Show metadata details when showing display content
                        const metadataDetails = headerInfo.closest('.email-metadata-details');
                        if (metadataDetails) {
                            metadataDetails.style.display = '';
                        }
                        
                        // Update attachment filenames and sizes to show decrypted versions
                        if (attachmentsInfo) {
                            const attachmentItems = attachmentsInfo.querySelectorAll('.attachment-item');
                            attachmentItems.forEach(item => {
                                const decryptedFilename = item.getAttribute('data-decrypted-filename');
                                const decryptedSize = item.getAttribute('data-decrypted-size');
                                const nameElement = item.querySelector('.attachment-name');
                                const sizeElement = item.querySelector('.attachment-size');
                                
                                if (decryptedFilename && nameElement) {
                                    nameElement.textContent = decryptedFilename;
                                }
                                
                                // Update size to show decrypted size
                                if (decryptedSize && sizeElement) {
                                    const sizeFormatted = (parseFloat(decryptedSize) / 1024).toFixed(2) + ' KB';
                                    sizeElement.textContent = sizeFormatted;
                                }
                            });
                        }
                        
                        // Move attachments after regular body if they exist
                        if (attachmentsInfo && attachmentsInfo.parentNode) {
                            // Remove from current position
                            attachmentsInfo.parentNode.removeChild(attachmentsInfo);
                            // Insert after regular body
                            bodyInfo.parentNode.insertBefore(attachmentsInfo, bodyInfo.nextSibling);
                        }
                        
                        // Keep button outside the card (after the card closes)
                        const emailDetailCard = headerInfo.closest('.email-detail-card');
                        if (emailDetailCard && emailDetailCard.parentNode) {
                            emailDetailCard.parentNode.insertBefore(newToggleBtn, emailDetailCard.nextSibling);
                        }
                    }
                });
            }
        }; // End of updateDetail function
            
            // Now execute the decryption and update
            try {
                const cleanedBody = email.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('BEGIN NOSTR')).join('\n').trim();
                // For sent emails, use recipient_pubkey for decryption
                const recipientPubkey = email.recipient_pubkey || email.nostr_pubkey; // Fallback for backward compatibility
                const isEncryptedSubjectBase64 = Utils.isLikelyEncryptedContent(email.subject);
                // Permissive regex: matches both base64 and glossia word content between armor markers
                const encryptedBodyMatch = cleanedBody.match(/-{3,}\s*BEGIN NOSTR (?:(NIP-\d+) ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);

                let decryptedSubject = email.subject;
                let decryptedBody = cleanedBody;
                const keypair = appState.getKeypair();

                // Add timeout wrapper to prevent hanging
                const decryptWithTimeout = async (promise, timeoutMs = 30000) => {
                    return Promise.race([
                        promise,
                        new Promise((_, reject) =>
                            setTimeout(() => reject(new Error('Decryption timeout after ' + timeoutMs + 'ms')), timeoutMs)
                        )
                    ]);
                };

                // Only try subject decryption if body is encrypted (has armor)
                let subjectCiphertext = null;
                if (encryptedBodyMatch) {
                    if (isEncryptedSubjectBase64) {
                        subjectCiphertext = email.subject;
                    } else {
                        subjectCiphertext = this.decodeGlossiaSubject(email.subject);
                        if (subjectCiphertext) {
                            console.log('[JS] Sent: glossia-decoded subject to ciphertext');
                        }
                    }
                }
                const isEncryptedSubject = !!subjectCiphertext;

                // Try glossia decode for body if armor content is not base64
                let bodyCiphertext = null;
                let bodyIsGlossia = false;
                if (encryptedBodyMatch) {
                    const armorContent = encryptedBodyMatch[2].trim();
                    // Check if content is already base64
                    if (/^[A-Za-z0-9+/=\n?]+$/.test(armorContent.replace(/\s+/g, ''))) {
                        bodyCiphertext = armorContent.replace(/\s+/g, '');
                    } else {
                        // Try glossia decode on the full body (uses decodeGlossiaArmoredBody)
                        const glossiaResult = this.decodeGlossiaArmoredBody(cleanedBody);
                        if (glossiaResult) {
                            bodyCiphertext = glossiaResult.ciphertext;
                            bodyIsGlossia = true;
                            console.log('[JS] Sent: glossia-decoded body armor, dialect:', glossiaResult.dialect);
                        }
                    }
                }

                const hasEncryptedContent = isEncryptedSubject || bodyCiphertext;
                if (hasEncryptedContent && keypair) {
                    try {
                        // Decrypt subject
                        if (isEncryptedSubject && subjectCiphertext) {
                            console.log('[JS] Decrypting subject...');
                            decryptedSubject = await decryptWithTimeout(
                                this.decryptNostrSentMessageWithFallback(email, subjectCiphertext, keypair),
                                10000
                            );
                        }
                        let manifestResult = null;
                        if (bodyCiphertext) {
                            try {
                                console.log('[JS] Decrypting manifest...');
                                manifestResult = await decryptWithTimeout(
                                    this.decryptSentManifestMessage(email, bodyCiphertext, keypair),
                                    30000
                                );
                                if (manifestResult.type === 'manifest') {
                                    decryptedBody = manifestResult.body;
                                } else if (manifestResult.type === 'legacy') {
                                    decryptedBody = manifestResult.body;
                                } else {
                                    decryptedBody = await decryptWithTimeout(
                                        this.decryptNostrSentMessageWithFallback(email, bodyCiphertext, keypair),
                                        10000
                                    );
                                }
                            } catch (e) {
                                console.error('[JS] Failed to decrypt manifest:', e);
                                try {
                                    decryptedBody = await decryptWithTimeout(
                                        this.decryptNostrSentMessageWithFallback(email, bodyCiphertext, keypair),
                                        10000
                                    );
                                } catch (legacyErr) {
                                    console.error('[JS] Legacy decryption also failed:', legacyErr);
                                    decryptedBody = '[Decryption failed: ' + legacyErr.message + ']';
                                }
                            }
                        }
                        // Verify all signatures recursively (handles nested quoted blocks)
                        let sigResults = null;
                        try {
                            const allSigs = await this.verifyAllSignatures(email.body);
                            sigResults = allSigs.length > 0 ? allSigs : null;
                        } catch (e) {
                            console.warn('[JS] Signature verification error:', e);
                        }
                        // Pass manifestResult to updateDetail to avoid re-decrypting
                        await updateDetail(decryptedSubject, decryptedBody, manifestResult, true, sigResults);
                    } catch (err) {
                        console.error('[JS] Error decrypting sent email:', err);
                        await updateDetail('Could not decrypt', 'Could not decrypt: ' + err.message, null, true);
                    }
                } else {
                    // Non-encrypted sent email: verify all signatures recursively
                    let sigResults = null;
                    try {
                        const allSigs = await this.verifyAllSignatures(email.body);
                        sigResults = allSigs.length > 0 ? allSigs : null;
                    } catch (e) {
                        console.warn('[JS] Signature verification error:', e);
                    }
                    // Decode glossia signed message body for display
                    let displayBody = decryptedBody;
                    const signedMsg = this.decodeGlossiaSignedMessage(email.body);
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
            if (!keypair) {
                return false;
            }
            
            // Create a test email object with the recipient pubkey
            const testEmail = {
                ...email,
                recipient_pubkey: recipientPubkey
            };
            
            // Detect subject encryption (base64 or glossia-encoded)
            let subjectCiphertext = null;
            if (Utils.isLikelyEncryptedContent(email.subject)) {
                subjectCiphertext = email.subject;
            } else {
                subjectCiphertext = this.decodeGlossiaSubject(email.subject);
            }
            const isEncryptedSubject = !!subjectCiphertext;

            // Helper to extract body ciphertext (handles both base64 and glossia)
            const extractBodyCiphertext = (bodyMatch) => {
                if (!bodyMatch) return null;
                const armorContent = bodyMatch[1].trim();
                if (/^[A-Za-z0-9+/=\n?]+$/.test(armorContent.replace(/\s+/g, ''))) {
                    return armorContent.replace(/\s+/g, '');
                }
                const glossiaResult = this.decodeGlossiaArmoredBody(email.body);
                return glossiaResult ? glossiaResult.ciphertext : armorContent.replace(/\s+/g, '');
            };

            if (isEncryptedSubject) {
                try {
                    const decryptedSubject = await this.decryptNostrSentMessageWithFallback(testEmail, subjectCiphertext, keypair);
                    if (decryptedSubject && !decryptedSubject.startsWith('Unable to decrypt')) {
                        // Subject decryption successful, try body too
                        const cleanedBody = email.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('BEGIN NOSTR')).join('\n').trim();
                        const encryptedBodyMatch = cleanedBody.match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);

                        if (encryptedBodyMatch) {
                            const encryptedContent = extractBodyCiphertext(encryptedBodyMatch);
                            try {
                                const manifestResult = await this.decryptSentManifestMessage(testEmail, encryptedContent, keypair);
                                if (manifestResult && manifestResult.type !== 'error') {
                                    // Both subject and body decrypted successfully
                                    // Save to database
                                    await this._saveRecipientPubkeyToDb(email, recipientPubkey);
                                    window.notificationService.showSuccess('Decryption successful! Pubkey saved.');
                                    return true;
                                }
                            } catch (e) {
                                // Try legacy decryption
                                const decryptedBody = await this.decryptNostrSentMessageWithFallback(testEmail, encryptedContent, keypair);
                                if (decryptedBody && !decryptedBody.startsWith('Unable to decrypt')) {
                                    await this._saveRecipientPubkeyToDb(email, recipientPubkey);
                                    window.notificationService.showSuccess('Decryption successful! Pubkey saved.');
                                    return true;
                                }
                            }
                        } else {
                            // No encrypted body, subject decryption is enough
                            await this._saveRecipientPubkeyToDb(email, recipientPubkey);
                            window.notificationService.showSuccess('Decryption successful! Pubkey saved.');
                            return true;
                        }
                    }
                } catch (e) {
                    console.error('[JS] Subject decryption test failed:', e);
                }
            } else {
                // Subject not encrypted, try body
                const cleanedBody = email.body.replace(/\r\n/g, '\n').split('\n').filter(line => line.trim() !== '' || line.includes('BEGIN NOSTR')).join('\n').trim();
                const encryptedBodyMatch = cleanedBody.match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);

                if (encryptedBodyMatch) {
                    const encryptedContent = extractBodyCiphertext(encryptedBodyMatch);
                    try {
                        const manifestResult = await this.decryptSentManifestMessage(testEmail, encryptedContent, keypair);
                        if (manifestResult && manifestResult.type !== 'error') {
                            await this._saveRecipientPubkeyToDb(email, recipientPubkey);
                            window.notificationService.showSuccess('Decryption successful! Pubkey saved.');
                            return true;
                        }
                    } catch (e) {
                        const decryptedBody = await this.decryptNostrSentMessageWithFallback(testEmail, encryptedContent, keypair);
                        if (decryptedBody && !decryptedBody.startsWith('Unable to decrypt')) {
                            await this._saveRecipientPubkeyToDb(email, recipientPubkey);
                            window.notificationService.showSuccess('Decryption successful! Pubkey saved.');
                            return true;
                        }
                    }
                }
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
                
                // Extract encrypted content from email body
                const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                if (!encryptedBodyMatch) {
                    window.notificationService.showError('Cannot decrypt attachment: no encrypted manifest found');
                    return;
                }
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    return;
                }
                
                // Decrypt the manifest to get attachment keys
                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                const manifestResult = await this.decryptSentManifestMessage(email, encryptedContent, keypair);
                
                if (manifestResult.type !== 'manifest') {
                    window.notificationService.showError('Cannot decrypt attachment: invalid manifest');
                    return;
                }
                
                // Find attachment metadata in manifest
                // Extract opaque ID from filename (e.g., "a1.dat" -> "a1")
                const opaqueId = attachment.filename.replace(/\.dat$/, '');
                console.log(`[JS] Looking for manifest attachment with id: ${opaqueId} from filename: ${attachment.filename}`);
                const attachmentMeta = manifestResult.manifest.attachments.find(a => a.id === opaqueId);
                
                if (!attachmentMeta) {
                    console.warn(`[JS] Attachment metadata not found in manifest for id: ${opaqueId}, available ids:`, manifestResult.manifest.attachments.map(a => a.id));
                    window.notificationService.showError('Attachment metadata not found in manifest');
                    return;
                }
                
                console.log(`[JS] Found attachment metadata:`, attachmentMeta);
                
                // Decrypt attachment data (attachment.data is base64, need to decode first)
                const attachmentDataBase64 = attachment.data;
                const decryptedData = await this.decryptWithAES(attachmentDataBase64, attachmentMeta.key_wrap, true);
                
                // Save decrypted attachment to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    attachmentMeta.orig_filename, 
                    decryptedData, 
                    attachmentMeta.orig_mime || 'application/octet-stream'
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
                
                // Extract and decrypt manifest
                const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                if (!encryptedBodyMatch) {
                    window.notificationService.showError('Cannot decrypt attachments: no encrypted manifest found');
                    return;
                }
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    return;
                }
                
                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                manifestResult = await this.decryptSentManifestMessage(email, encryptedContent, keypair);
                
                if (manifestResult.type !== 'manifest') {
                    window.notificationService.showError('Cannot decrypt attachments: invalid manifest');
                    return;
                }
                
                console.log(`[JS] Manifest decrypted successfully, has ${manifestResult.manifest.attachments.length} attachments`);
            }
            
            // Process each attachment - decrypt if we have a manifest
            for (const attachment of email.attachments) {
                const shouldDecrypt = attachment.encryption_method === 'manifest_aes' || 
                                    (manifestResult && attachment.filename.endsWith('.dat'));
                
                if (shouldDecrypt && manifestResult) {
                    // Find attachment metadata in manifest
                    const opaqueId = attachment.filename.replace(/\.dat$/, ''); // a1.dat -> a1
                    console.log(`[JS] Looking for manifest attachment with id: ${opaqueId} from filename: ${attachment.filename}`);
                    const attachmentMeta = manifestResult.manifest.attachments.find(a => a.id === opaqueId);
                    
                    if (!attachmentMeta) {
                        console.warn(`[JS] Skipping attachment ${attachment.filename}: metadata not found in manifest (looking for id: ${opaqueId}, available ids: ${manifestResult.manifest.attachments.map(a => a.id).join(', ')})`);
                        // Fallback: add as-is if we can't find metadata
                        attachmentsForZip.push({
                            filename: attachment.filename,
                            data: attachment.data
                        });
                        continue;
                    }
                    
                    console.log(`[JS] Found attachment metadata:`, attachmentMeta);
                    
                    // Decrypt attachment data (attachment.data is base64)
                    const decryptedData = await this.decryptWithAES(attachment.data, attachmentMeta.key_wrap, true);
                    
                    attachmentsForZip.push({
                        filename: attachmentMeta.orig_filename,
                        data: decryptedData
                    });
                    
                    console.log(`[JS] Added decrypted attachment to ZIP: ${attachmentMeta.orig_filename} (${decryptedData.length} bytes)`);
                    
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
                
                // Extract encrypted content from email body
                const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                if (!encryptedBodyMatch) {
                    window.notificationService.showError('Cannot decrypt attachment: no encrypted manifest found');
                    return;
                }
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    return;
                }
                
                // Decrypt the manifest to get attachment keys (use decryptManifestMessage for inbox)
                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                const manifestResult = await this.decryptManifestMessage(email, encryptedContent, keypair);
                
                if (manifestResult.type !== 'manifest') {
                    window.notificationService.showError('Cannot decrypt attachment: invalid manifest');
                    return;
                }
                
                // Find attachment metadata in manifest
                const opaqueId = attachment.filename.replace(/\.dat$/, '');
                console.log(`[JS] Looking for manifest attachment with id: ${opaqueId} from filename: ${attachment.filename}`);
                const attachmentMeta = manifestResult.manifest.attachments.find(a => a.id === opaqueId);
                
                if (!attachmentMeta) {
                    console.warn(`[JS] Attachment metadata not found in manifest for id: ${opaqueId}, available ids:`, manifestResult.manifest.attachments.map(a => a.id));
                    window.notificationService.showError('Attachment metadata not found in manifest');
                    return;
                }
                
                console.log(`[JS] Found attachment metadata:`, attachmentMeta);
                
                // Decrypt attachment data
                const decryptedData = await this.decryptWithAES(attachment.data, attachmentMeta.key_wrap, true);
                
                // Save decrypted attachment to disk using Tauri
                const filePath = await TauriService.saveAttachmentToDisk(
                    attachmentMeta.orig_filename, 
                    decryptedData, 
                    attachmentMeta.orig_mime || 'application/octet-stream'
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
                
                const encryptedBodyMatch = email.body.replace(/\r\n/g, '\n').match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                if (!encryptedBodyMatch) {
                    window.notificationService.showError('Cannot decrypt attachments: no encrypted manifest found');
                    return;
                }
                
                const keypair = appState.getKeypair();
                if (!keypair) {
                    return;
                }
                
                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                manifestResult = await this.decryptManifestMessage(email, encryptedContent, keypair);
                
                if (manifestResult.type !== 'manifest') {
                    window.notificationService.showError('Cannot decrypt attachments: invalid manifest');
                    return;
                }
                
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
                        console.warn(`[JS] Skipping attachment ${attachment.filename}: metadata not found in manifest (looking for id: ${opaqueId}, available ids: ${manifestResult.manifest.attachments.map(a => a.id).join(', ')})`);
                        attachmentsForZip.push({
                            filename: attachment.filename,
                            data: attachment.data
                        });
                        continue;
                    }
                    
                    console.log(`[JS] Found attachment metadata:`, attachmentMeta);
                    
                    // Decrypt attachment data
                    const decryptedData = await this.decryptWithAES(attachment.data, attachmentMeta.key_wrap, true);
                    
                    attachmentsForZip.push({
                        filename: attachmentMeta.orig_filename,
                        data: decryptedData
                    });
                    
                    console.log(`[JS] Added decrypted attachment to ZIP: ${attachmentMeta.orig_filename} (${decryptedData.length} bytes)`);
                    
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
            const sentActions = domManager.get('sentActions');
            const sentTitle = domManager.get('sentTitle');
            if (sentList) sentList.style.display = 'block';
            if (sentDetailView) sentDetailView.style.display = 'none';
            if (sentActions) sentActions.style.display = 'flex';
            if (sentTitle) {
                sentTitle.textContent = 'Sent';
                sentTitle.style.display = '';
            }
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
                if (draft.body && draft.body.includes('BEGIN NOSTR')) {
                    const keypair = appState.getKeypair();
                    if (!keypair) {
                        previewText = 'Unable to decrypt: no keypair';
                    } else {
                        // Try to decrypt subject and body
                        try {
                            if (Utils.isLikelyEncryptedContent(draft.subject)) {
                                previewSubject = await this.decryptNostrMessageWithFallback(draft, draft.subject, keypair);
                            }
                            const encryptedBodyMatch = draft.body.match(/-{3,}\s*BEGIN NOSTR (?:NIP-\d+ ENCRYPTED (?:MESSAGE|BODY))\s*-{3,}\s*([\s\S]+?)\s*-{3,}\s*(?:END NOSTR (?:NIP-\d+ ENCRYPTED )?MESSAGE|BEGIN NOSTR (?:SIGNATURE|SEAL))\s*-{3,}/);
                            if (encryptedBodyMatch) {
                                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                                try {
                                    // Try manifest decryption first
                                    const manifestResult = await this.decryptManifestMessage(draft, encryptedContent, keypair);
                                    if (manifestResult.type === 'manifest') {
                                        previewText = Utils.escapeHtml(manifestResult.body.substring(0, 100));
                                    } else if (manifestResult.type === 'legacy') {
                                        previewText = Utils.escapeHtml(manifestResult.body.substring(0, 100));
                                    } else {
                                        // Fallback to legacy decryption
                                        const decrypted = await this.decryptNostrMessageWithFallback(draft, encryptedContent, keypair);
                                        previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                    }
                                } catch (e) {
                                    // If manifest fails, try legacy decryption
                                    const decrypted = await this.decryptNostrMessageWithFallback(draft, encryptedContent, keypair);
                                    previewText = Utils.escapeHtml(decrypted.substring(0, 100));
                                }
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
                            if (isEncryptedSubject) {
                                decryptedSubject = await this.decryptNostrMessageWithFallback(draft, draft.subject, keypair);
                            }
                            if (encryptedBodyMatch) {
                                const encryptedContent = encryptedBodyMatch[1].replace(/\s+/g, '');
                                try {
                                    // Try manifest decryption first
                                    const manifestResult = await this.decryptManifestMessage(draft, encryptedContent, keypair);
                                    if (manifestResult.type === 'manifest') {
                                        decryptedBody = manifestResult.body;
                                    } else if (manifestResult.type === 'legacy') {
                                        decryptedBody = manifestResult.body;
                                    } else {
                                        // Fallback to legacy decryption
                                        decryptedBody = await this.decryptNostrMessageWithFallback(draft, encryptedContent, keypair);
                                    }
                                } catch (e) {
                                    // If manifest fails, try legacy decryption
                                    decryptedBody = await this.decryptNostrMessageWithFallback(draft, encryptedContent, keypair);
                                }
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