// Utilities
// Common helper functions used throughout the application

// Remove all import/export statements. Attach Utils to window. Replace any usage of imported symbols with window equivalents if needed.

class Utils {
    // Helper function to escape HTML
    static escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
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
} 
window.Utils = Utils; 