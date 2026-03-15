// Utilities
// Common helper functions used throughout the application

// Remove all import/export statements. Attach Utils to window. Replace any usage of imported symbols with window equivalents if needed.

class Utils {
    /**
     * Render an HTML email body inside a sandboxed iframe.
     * Call after the container element is in the DOM.
     * @param {string} containerId - ID of the container div
     * @param {string} htmlContent - Raw HTML body of the email
     */
    static renderHtmlBodyInIframe(containerId, htmlContent) {
        const container = document.getElementById(containerId);
        if (!container) return;
        // Clear existing content
        container.innerHTML = '';
        container.style.whiteSpace = 'normal';
        const iframe = document.createElement('iframe');
        iframe.sandbox = 'allow-same-origin';
        iframe.style.width = '100%';
        iframe.style.border = 'none';
        iframe.style.overflow = 'hidden';
        iframe.style.minHeight = '100px';
        container.appendChild(iframe);
        const doc = iframe.contentDocument || iframe.contentWindow.document;
        // Inject dark mode aware base styles + the email HTML
        const isDark = document.body.classList.contains('dark-mode');
        doc.open();
        const darkOverrides = isDark ? `
            h4 { color: #9ca3af !important; }
            blockquote, .seal-block { background: #1f2937 !important; border-color: #60a5fa !important; }
            blockquote p, blockquote strong, .seal-block p, .seal-block strong { color: #e0e0e0 !important; }
            blockquote code, .seal-block code { color: #93c5fd !important; }
            div[style*="border-left"][style*="color:#888"],
            div[style*="border-left"][style*="color: #888"] { color: #9ca3af !important; }
            hr { border-color: #374151 !important; }
        ` : '';
        doc.write(`<!DOCTYPE html><html><head><style>
            body { margin: 0; padding: 8px; font-family: sans-serif; line-height: 1.6;
                   color: ${isDark ? '#e0e0e0' : '#111827'};
                   background: ${isDark ? '#232946' : '#fff'}; }
            img { max-width: 100%; height: auto; }
            a { color: ${isDark ? '#93c5fd' : '#2563eb'}; }
            ${darkOverrides}
        </style></head><body>${htmlContent}</body></html>`);
        doc.close();
        // Auto-resize iframe to fit content
        const resize = () => {
            if (doc.body) {
                iframe.style.height = doc.body.scrollHeight + 'px';
            }
        };
        iframe.addEventListener('load', resize);
        // Also resize after a short delay for late-loading content
        setTimeout(resize, 100);
        setTimeout(resize, 500);
    }

    /**
     * Post-process a body container to wrap SIGNATURE and SEAL armor blocks
     * in styled divs with a verification indicator placeholder.
     * Works on plain-text bodies rendered as escaped HTML with <br> tags.
     */
    static decorateArmorBlocks(containerId) {
        const container = document.getElementById(containerId);
        if (!container) return;
        // Work on innerHTML — armor delimiters are escaped text with <br> newlines
        let html = container.innerHTML;
        // Match nested armor blocks (plaintext signed or encrypted+signed) that end with END NOSTR MESSAGE
        // Must match before sigSealPattern to prevent partial matches on nested SIGNATURE/SEAL
        const signedMsgPattern = /(-{3,}\s*BEGIN NOSTR (?:SIGNED|NIP-\d+ ENCRYPTED) MESSAGE\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR MESSAGE\s*-{3,})/g;
        let blockIndex = 0;
        html = html.replace(signedMsgPattern, (match) => {
            const id = `inline-sig-block-${blockIndex++}`;
            return `<div class="inline-sig-block" id="${id}"><span class="inline-sig-indicator pending"><i class="fas fa-spinner fa-spin"></i> Verifying…</span><div class="inline-sig-content">${match}</div></div>`;
        });
        // Match signature block (optionally followed by seal block)
        // Each block: ---+ BEGIN NOSTR (SIGNATURE|SEAL) ---+ ... ---+ END NOSTR (SIGNATURE|SEAL) ---+
        const sigSealPattern = /(-{3,}\s*BEGIN NOSTR SIGNATURE\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR SIGNATURE\s*-{3,}(?:<br>)*(?:\s*<br>)*(?:\s*-{3,}\s*BEGIN NOSTR SEAL\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR SEAL\s*-{3,})?)/g;
        html = html.replace(sigSealPattern, (match) => {
            const id = `inline-sig-block-${blockIndex++}`;
            return `<div class="inline-sig-block" id="${id}"><span class="inline-sig-indicator pending"><i class="fas fa-spinner fa-spin"></i> Verifying…</span><div class="inline-sig-content">${match}</div></div>`;
        });
        // Also wrap standalone seal blocks (pubkey without signature)
        const sealOnlyPattern = /(?<!<\/div>)(-{3,}\s*BEGIN NOSTR SEAL\s*-{3,}[\s\S]*?-{3,}\s*END NOSTR SEAL\s*-{3,})/g;
        html = html.replace(sealOnlyPattern, (match) => {
            const id = `inline-sig-block-${blockIndex++}`;
            return `<div class="inline-sig-block" id="${id}"><div class="inline-sig-content">${match}</div></div>`;
        });
        if (blockIndex > 0) {
            container.innerHTML = html;
        }
    }

    // Helper function to escape HTML
    static escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    // Fix UTF-8 encoding issues in decrypted text
    // Handles cases where UTF-8 bytes are misinterpreted as Latin-1
    static fixUtf8Encoding(text) {
        if (!text) return text;
        return text
            .replace(/\u00E2\u0080\u0099/g, "'")      // Right single quotation mark (U+2019)
            .replace(/\u00E2\u0080\u009C/g, '"')     // Left double quotation mark (U+201C)
            .replace(/\u00E2\u0080\u009D/g, '"')     // Right double quotation mark (U+201D)
            .replace(/\u00E2\u0080\u0094/g, "—")     // Em dash (U+2014)
            .replace(/\u00E2\u0080\u0093/g, "–")     // En dash (U+2013)
            .replace(/\uFFFD/g, "'")                 // Replacement character -> apostrophe
            // Fix common contractions
            .replace(/doesn\u00E2/g, "doesn't")
            .replace(/won\u00E2/g, "won't")
            .replace(/can\u00E2/g, "can't")
            .replace(/isn\u00E2/g, "isn't")
            .replace(/aren\u00E2/g, "aren't")
            .replace(/wasn\u00E2/g, "wasn't")
            .replace(/weren\u00E2/g, "weren't")
            .replace(/haven\u00E2/g, "haven't")
            .replace(/hasn\u00E2/g, "hasn't")
            .replace(/hadn\u00E2/g, "hadn't")
            .replace(/wouldn\u00E2/g, "wouldn't")
            .replace(/couldn\u00E2/g, "couldn't")
            .replace(/shouldn\u00E2/g, "shouldn't")
            .replace(/mustn\u00E2/g, "mustn't")
            .replace(/mightn\u00E2/g, "mightn't")
            .replace(/needn\u00E2/g, "needn't")
            .replace(/daren\u00E2/g, "daren't")
            .replace(/mayn\u00E2/g, "mayn't")
            .replace(/shan\u00E2/g, "shan't");
    }

    // Helper function to format time ago
    static formatTimeAgo(date) {
        // Ensure date is a Date object (handle both Date objects and date strings from cache)
        const dateObj = date instanceof Date ? date : new Date(date);
        
        // Check if the date is valid
        if (isNaN(dateObj.getTime())) {
            return 'Unknown time';
        }
        
        const now = new Date();
        const diffMs = now - dateObj;
        const diffMins = Math.floor(diffMs / 60000);
        const diffHours = Math.floor(diffMs / 3600000);
        const diffDays = Math.floor(diffMs / 86400000);
        
        if (diffMins < 1) return 'Just now';
        if (diffMins < 60) return `${diffMins}m ago`;
        if (diffHours < 24) return `${diffHours}h ago`;
        if (diffDays < 7) return `${diffDays}d ago`;
        
        return dateObj.toLocaleDateString();
    }

    // Function to detect email provider from saved settings
    static detectEmailProvider(settings) {
        if (!settings.smtp_host || !settings.imap_host) return '';
        
        const smtpHost = settings.smtp_host.toLowerCase();
        const imapHost = settings.imap_host.toLowerCase();
        
        if (smtpHost.includes('gmail.com') && imapHost.includes('gmail.com')) {
            return 'gmail';
        } else if (smtpHost.includes('outlook.com') && imapHost.includes('office365.com')) {
            return 'outlook';
        } else if (smtpHost.includes('yahoo.com') && imapHost.includes('yahoo.com')) {
            return 'yahoo';
        } else {
            return 'custom';
        }
    }

    // Check if in development mode
    static isDevMode() {
        // Tauri dev mode: window.location.hostname is localhost or 127.0.0.1
        return window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';
    }

    // Debounce function
    static debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    }

    // Throttle function
    static throttle(func, limit) {
        let inThrottle;
        return function() {
            const args = arguments;
            const context = this;
            if (!inThrottle) {
                func.apply(context, args);
                inThrottle = true;
                setTimeout(() => inThrottle = false, limit);
            }
        };
    }

    // Generate a unique ID
    static generateId() {
        return Date.now().toString(36) + Math.random().toString(36).substr(2);
    }

    // Validate email format
    static isValidEmail(email) {
        const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
        return emailRegex.test(email);
    }

    // Validate Nostr public key format
    static isValidNostrPubkey(pubkey) {
        return pubkey.startsWith('npub1') && pubkey.length > 50;
    }

    // Validate Nostr private key format
    static isValidNostrPrivkey(privkey) {
        return (privkey.startsWith('nsec1') || privkey.startsWith('npriv1')) && privkey.length > 50;
    }

    // Copy text to clipboard
    static async copyToClipboard(text) {
        try {
            await navigator.clipboard.writeText(text);
            return true;
        } catch (error) {
            console.error('Failed to copy to clipboard:', error);
            return false;
        }
    }

    // Download data as file
    static downloadAsFile(data, filename, type = 'text/plain') {
        const blob = new Blob([data], { type });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    }

    // Format file size
    static formatFileSize(bytes) {
        if (bytes === 0) return '0 Bytes';
        const k = 1024;
        const sizes = ['Bytes', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    }

    // Deep clone object
    static deepClone(obj) {
        if (obj === null || typeof obj !== 'object') return obj;
        if (obj instanceof Date) return new Date(obj.getTime());
        if (obj instanceof Array) return obj.map(item => this.deepClone(item));
        if (typeof obj === 'object') {
            const clonedObj = {};
            for (const key in obj) {
                if (obj.hasOwnProperty(key)) {
                    clonedObj[key] = this.deepClone(obj[key]);
                }
            }
            return clonedObj;
        }
    }

    // Merge objects
    static mergeObjects(target, ...sources) {
        if (!sources.length) return target;
        const source = sources.shift();
        
        if (source === undefined) return target;
        
        if (this.isMergeableObject(target) && this.isMergeableObject(source)) {
            Object.keys(source).forEach(key => {
                if (this.isMergeableObject(source[key])) {
                    if (!target[key]) {
                        target[key] = {};
                    }
                    this.mergeObjects(target[key], source[key]);
                } else {
                    target[key] = source[key];
                }
            });
        }
        
        return this.mergeObjects(target, ...sources);
    }

    // Check if object is mergeable
    static isMergeableObject(item) {
        return item && typeof item === 'object' && !Array.isArray(item);
    }

    // Sleep function for async operations
    static sleep(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    // Retry function with exponential backoff
    static async retry(fn, maxAttempts = 3, baseDelay = 1000) {
        for (let attempt = 1; attempt <= maxAttempts; attempt++) {
            try {
                return await fn();
            } catch (error) {
                if (attempt === maxAttempts) {
                    throw error;
                }
                const delay = baseDelay * Math.pow(2, attempt - 1);
                await this.sleep(delay);
            }
        }
    }

    // Generate random string
    static randomString(length = 8) {
        const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
        let result = '';
        for (let i = 0; i < length; i++) {
            result += chars.charAt(Math.floor(Math.random() * chars.length));
        }
        return result;
    }

    // Truncate text with ellipsis
    static truncateText(text, maxLength = 100) {
        if (text.length <= maxLength) return text;
        return text.substring(0, maxLength) + '...';
    }

    // Capitalize first letter
    static capitalize(str) {
        return str.charAt(0).toUpperCase() + str.slice(1);
    }

    // Convert snake_case to camelCase
    static snakeToCamel(str) {
        return str.replace(/_([a-z])/g, (match, letter) => letter.toUpperCase());
    }

    // Convert camelCase to snake_case
    static camelToSnake(str) {
        return str.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`);
    }

    // Extract X-Nostr-Pubkey from raw headers
    static extractNostrPubkeyFromHeaders(rawHeaders) {
        if (!rawHeaders) return null;
        const match = rawHeaders.match(/^X-Nostr-Pubkey:\s*([a-zA-Z0-9]+)$/m);
        return match ? match[1] : null;
    }

    // Check if content is likely encrypted (base64-like pattern, reasonable length)
    static isLikelyEncryptedContent(content) {
        if (!content || typeof content !== 'string') return false;
        
        // Skip empty or very short content
        if (content.length < 20) {
            return false;
        }
        
        // Check for NIP-04 format: base64?iv=base64
        const nip04Regex = /^[A-Za-z0-9+/=]+\?iv=[A-Za-z0-9+/=]+$/;
        if (nip04Regex.test(content)) {
            return true;
        }
        
        // Check if it looks like base64 encoded content (typical for encrypted data)
        // Base64 contains A-Z, a-z, 0-9, +, /, and = for padding
        const base64Regex = /^[A-Za-z0-9+/=]+$/;
        const isBase64 = base64Regex.test(content);
        
        // Also check that it doesn't contain typical email subject patterns
        const hasEmailPatterns = content.includes('@') || 
                                content.includes('Re:') || 
                                content.includes('Fwd:') ||
                                content.includes('FW:') ||
                                content.includes('Subject:') ||
                                content.includes('From:') ||
                                content.includes('To:');
        
        return isBase64 && !hasEmailPatterns;
    }

    /**
     * Detect encryption format from encrypted content
     * Returns 'nip04', 'nip44', or 'unknown'
     * @param {string} content - The encrypted content to analyze
     * @returns {string} - The detected encryption format
     */
    static detectEncryptionFormat(content) {
        if (!content || typeof content !== 'string') {
            return 'unknown';
        }

        // Remove ASCII armor if present (for body content)
        let cleanContent = content
            .replace(/-----BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE-----/g, '')
            .replace(/-----END NOSTR NIP-04 ENCRYPTED MESSAGE-----/g, '')
            .replace(/-----BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE-----/g, '')
            .replace(/-----END NOSTR NIP-44 ENCRYPTED MESSAGE-----/g, '')
            .trim();

        // Check for NIP-04 format: base64?iv=base64
        const nip04Pattern = /^[A-Za-z0-9+/=]+\?iv=[A-Za-z0-9+/=]+$/;
        if (nip04Pattern.test(cleanContent)) {
            return 'nip04';
        }

        // Check for NIP-44 format: versioned format (starts with version byte 1 or 2)
        // NIP-44 is base64 encoded, and when decoded, the first byte indicates version
        const base64Pattern = /^[A-Za-z0-9+/=]+$/;
        if (base64Pattern.test(cleanContent)) {
            try {
                // Decode base64 to check version byte
                const decoded = atob(cleanContent);
                if (decoded.length > 0) {
                    const versionByte = decoded.charCodeAt(0);
                    // NIP-44 v1 uses version byte 1 (0x01), v2 uses version byte 2 (0x02)
                    if (versionByte === 1 || versionByte === 2) {
                        return 'nip44';
                    }
                }
            } catch (e) {
                // Not valid base64 or can't decode, fall through to unknown
            }
        }

        return 'unknown';
    }
} 
window.Utils = Utils; 