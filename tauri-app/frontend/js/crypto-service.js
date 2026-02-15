// CryptoService — Frontend crypto using nostr-tools (loaded via import map)
// Falls back to TauriService (Rust backend) if nostr-tools is not available.

const CryptoService = {
    _nip04: null,
    _nip44: null,
    _nip19: null,
    _pure: null,
    _schnorr: null,
    _ready: false,
    _initPromise: null,

    async init() {
        if (this._initPromise) return this._initPromise;
        this._initPromise = this._doInit();
        return this._initPromise;
    },

    async _doInit() {
        try {
            const [nip04, nip44, nip19, pure, noble] = await Promise.all([
                import('nostr-tools/nip04'),
                import('nostr-tools/nip44'),
                import('nostr-tools/nip19'),
                import('nostr-tools/pure'),
                import('@noble/curves/secp256k1'),
            ]);
            this._nip04 = nip04;
            this._nip44 = nip44;
            this._nip19 = nip19;
            this._pure = pure;
            this._schnorr = noble.schnorr;
            this._ready = true;
            console.log('[CryptoService] loaded — nostr-tools crypto ready');
        } catch (e) {
            console.warn('[CryptoService] nostr-tools not available, will fall back to Tauri:', e.message);
            this._ready = false;
        }
    },

    isReady() {
        return this._ready;
    },

    // ---- Key helpers ----

    _hexToBytes(hex) {
        const bytes = new Uint8Array(hex.length / 2);
        for (let i = 0; i < hex.length; i += 2) {
            bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
        }
        return bytes;
    },

    _bytesToHex(bytes) {
        return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
    },

    /** Decode an nsec1 bech32 string to Uint8Array secret key. */
    _nsecToBytes(nsec) {
        const { type, data } = this._nip19.decode(nsec);
        if (type !== 'nsec') throw new Error('Not an nsec key');
        return data;
    },

    /** Decode an npub1 bech32 string to hex public key string. */
    _npubToHex(npub) {
        const { type, data } = this._nip19.decode(npub);
        if (type !== 'npub') throw new Error('Not an npub key');
        // nip19.decode returns data as hex string for npub
        return data;
    },

    // ---- Public API (mirrors TauriService crypto methods) ----

    generateKeypair() {
        const sk = this._pure.generateSecretKey();
        const pkHex = this._pure.getPublicKey(sk);
        return {
            private_key: this._nip19.nsecEncode(sk),
            public_key: this._nip19.npubEncode(pkHex),
        };
    },

    validatePrivateKey(privateKey) {
        try {
            const { type } = this._nip19.decode(privateKey);
            return type === 'nsec';
        } catch {
            return false;
        }
    },

    validatePublicKey(publicKey) {
        try {
            const { type } = this._nip19.decode(publicKey);
            return type === 'npub';
        } catch {
            return false;
        }
    },

    getPublicKeyFromPrivate(privateKey) {
        const skBytes = this._nsecToBytes(privateKey);
        const pkHex = this._pure.getPublicKey(skBytes);
        return this._nip19.npubEncode(pkHex);
    },

    async encryptMessageWithAlgorithm(privateKey, publicKey, message, algorithm) {
        const skBytes = this._nsecToBytes(privateKey);
        const pkHex = this._npubToHex(publicKey);

        const algo = (algorithm || 'nip44').toLowerCase();
        if (algo === 'nip04') {
            return await this._nip04.encrypt(skBytes, pkHex, message);
        }
        // Default to NIP-44
        const conversationKey = this._nip44.v2.utils.getConversationKey(skBytes, pkHex);
        return this._nip44.v2.encrypt(message, conversationKey);
    },

    async decryptDmContent(privateKey, senderPubkey, encryptedContent) {
        const skBytes = this._nsecToBytes(privateKey);
        const pkHex = this._npubToHex(senderPubkey);

        // Try NIP-44 first
        try {
            const conversationKey = this._nip44.v2.utils.getConversationKey(skBytes, pkHex);
            return this._nip44.v2.decrypt(encryptedContent, conversationKey);
        } catch (_) {
            // ignore
        }

        // Fall back to NIP-04
        try {
            return await this._nip04.decrypt(skBytes, pkHex, encryptedContent);
        } catch (_) {
            // ignore
        }

        throw new Error(
            `Failed to decrypt with both NIP-44 and NIP-04. Content length: ${encryptedContent.length}, Has '?iv=': ${encryptedContent.includes('?iv=')}`
        );
    },

    detectEncryptionFormat(content) {
        // Delegate to Utils if available (keeps single source of truth)
        if (window.Utils && typeof window.Utils.detectEncryptionFormat === 'function') {
            return window.Utils.detectEncryptionFormat(content);
        }
        // Minimal inline fallback
        if (!content) return 'unknown';
        if (content.includes('?iv=')) return 'nip04';
        try {
            const decoded = atob(content.trim());
            if (decoded.length > 0) {
                const v = decoded.charCodeAt(0);
                if (v === 1 || v === 2) return 'nip44';
            }
        } catch (_) { /* ignore */ }
        return 'unknown';
    },

    async signData(privateKey, data) {
        const skBytes = this._nsecToBytes(privateKey);

        // SHA-256 hash the data
        const encoder = new TextEncoder();
        const hashBuffer = await crypto.subtle.digest('SHA-256', encoder.encode(data));
        const messageHash = new Uint8Array(hashBuffer);

        // Schnorr sign
        const sig = this._schnorr.sign(messageHash, skBytes);
        return this._bytesToHex(sig);
    },

    async verifySignature(publicKey, signature, data) {
        const pkHex = this._npubToHex(publicKey);
        const pkBytes = this._hexToBytes(pkHex);
        const sigBytes = this._hexToBytes(signature);

        // SHA-256 hash the data
        const encoder = new TextEncoder();
        const hashBuffer = await crypto.subtle.digest('SHA-256', encoder.encode(data));
        const messageHash = new Uint8Array(hashBuffer);

        return this._schnorr.verify(sigBytes, messageHash, pkBytes);
    },

    async encryptSettingValue(privateKey, value) {
        if (!value) return '';

        const skBytes = this._nsecToBytes(privateKey);

        // Derive key: SHA-256("nostr-mail-settings-encryption-v1:" + secret_bytes)
        const prefix = new TextEncoder().encode('nostr-mail-settings-encryption-v1:');
        const combined = new Uint8Array(prefix.length + skBytes.length);
        combined.set(prefix);
        combined.set(skBytes, prefix.length);
        const keyMaterial = await crypto.subtle.digest('SHA-256', combined);

        const cryptoKey = await crypto.subtle.importKey(
            'raw', keyMaterial, { name: 'AES-GCM' }, false, ['encrypt']
        );

        // Generate 12-byte nonce
        const nonce = crypto.getRandomValues(new Uint8Array(12));
        const ciphertext = await crypto.subtle.encrypt(
            { name: 'AES-GCM', iv: nonce }, cryptoKey, new TextEncoder().encode(value)
        );

        // Combine nonce + ciphertext, encode as base64
        const result = new Uint8Array(nonce.length + ciphertext.byteLength);
        result.set(nonce);
        result.set(new Uint8Array(ciphertext), nonce.length);
        return btoa(String.fromCharCode(...result));
    },

    async decryptSettingValue(privateKey, encryptedValue) {
        if (!encryptedValue) return '';

        const skBytes = this._nsecToBytes(privateKey);

        // Derive key: same as encryptSettingValue
        const prefix = new TextEncoder().encode('nostr-mail-settings-encryption-v1:');
        const combined = new Uint8Array(prefix.length + skBytes.length);
        combined.set(prefix);
        combined.set(skBytes, prefix.length);
        const keyMaterial = await crypto.subtle.digest('SHA-256', combined);

        const cryptoKey = await crypto.subtle.importKey(
            'raw', keyMaterial, { name: 'AES-GCM' }, false, ['decrypt']
        );

        // Decode base64
        const raw = Uint8Array.from(atob(encryptedValue), c => c.charCodeAt(0));
        if (raw.length < 12) throw new Error('Encrypted value too short');

        const nonce = raw.slice(0, 12);
        const ciphertext = raw.slice(12);

        const plainBuffer = await crypto.subtle.decrypt(
            { name: 'AES-GCM', iv: nonce }, cryptoKey, ciphertext
        );
        return new TextDecoder().decode(plainBuffer);
    },
};

window.CryptoService = CryptoService;

// Auto-init on load
CryptoService.init();
