// Glossia WASM Service
// Loads glossia's native WASM module for in-browser steganographic encode/decode.
// Falls through to TauriService if WASM is not loaded.

const GlossiaService = {
    _wasm: null,
    _ready: false,
    _initPromise: null,

    async init() {
        if (this._initPromise) return this._initPromise;
        this._initPromise = this._doInit();
        return this._initPromise;
    },

    async _doInit() {
        try {
            const wasm = await import('./pkg/glossia.js');
            await wasm.default();   // initialize wasm-pack --target web module
            this._wasm = wasm;
            this._ready = true;
            console.log('[GlossiaService] WASM module loaded (glossia native)');
            this._populateDialectDropdown();
        } catch (e) {
            console.warn('[GlossiaService] WASM not available, will fall back to Tauri:', e.message);
            this._ready = false;
        }
    },

    /** Populate the glossia encoding dropdown with meta encoding keywords. */
    _populateDialectDropdown() {
        const select = document.getElementById('bip39-encoding-select');
        if (!select) return;

        const metaKeywords = [
            { value: '',        label: 'ASCII Armor (base64)' },
            { value: 'english', label: 'English' },
            { value: 'latin',   label: 'Latin' },
        ];

        select.innerHTML = '';
        for (const kw of metaKeywords) {
            const opt = document.createElement('option');
            opt.value = kw.value;
            opt.textContent = kw.label;
            select.appendChild(opt);
        }
        console.log('[GlossiaService] Populated encoding dropdown with', select.options.length - 1, 'meta keywords');
    },

    isReady() {
        return this._ready;
    },

    // ---- NIP-04 binary packing helpers ----
    // NIP-04 ciphertext is "base64?iv=base64". To encode efficiently as binary
    // (rather than ASCII), we pack it as [payload_len(2 bytes BE), payload_bytes, iv_bytes]
    // and pass the packed blob as base64 to glossia's generic encoder.

    _packNip04(ciphertext) {
        const trimmed = ciphertext.trim();
        const ivIndex = trimmed.indexOf('?iv=');
        if (ivIndex === -1) return trimmed; // NIP-44 (pure base64), pass through

        const payloadB64 = trimmed.substring(0, ivIndex);
        const ivB64 = trimmed.substring(ivIndex + 4);
        const payloadBytes = Uint8Array.from(atob(payloadB64), c => c.charCodeAt(0));
        const ivBytes = Uint8Array.from(atob(ivB64), c => c.charCodeAt(0));

        const len = payloadBytes.length;
        const combined = new Uint8Array(2 + payloadBytes.length + ivBytes.length);
        combined[0] = (len >> 8) & 0xFF;
        combined[1] = len & 0xFF;
        combined.set(payloadBytes, 2);
        combined.set(ivBytes, 2 + payloadBytes.length);

        return btoa(String.fromCharCode(...combined));
    },

    /** Try to unpack NIP-04 binary format [len(2), payload, iv(16)].
     *  Returns "base64?iv=base64" if it looks like packed NIP-04, otherwise
     *  returns the decoded base64 as-is (NIP-44). */
    _autoUnpack(decoded) {
        try {
            const bytes = Uint8Array.from(atob(decoded), c => c.charCodeAt(0));
            if (bytes.length >= 2) {
                const payloadLen = (bytes[0] << 8) | bytes[1];
                const ivLen = bytes.length - 2 - payloadLen;
                // NIP-04 AES-CBC uses a 16-byte IV
                if (payloadLen > 0 && ivLen === 16) {
                    const payloadB64 = btoa(String.fromCharCode(...bytes.slice(2, 2 + payloadLen)));
                    const ivB64 = btoa(String.fromCharCode(...bytes.slice(2 + payloadLen)));
                    return payloadB64 + '?iv=' + ivB64;
                }
            }
        } catch (_) { /* not valid base64 or unpacking failed */ }
        return decoded;
    },

    // ---- Backward-compatible API (used by email-service.js) ----

    encode(ciphertext, language, wordlist, mode) {
        const input = this._packNip04(ciphertext);
        const seed = BigInt(Math.floor(Math.random() * 2 ** 32));
        const resultJson = this._wasm.encode(input, language, wordlist, mode, seed);
        const result = JSON.parse(resultJson);
        if (result.error) throw new Error(result.error);
        return result.encoded_text;
    },

    decode(text, language, wordlist, _algorithm) {
        const resultJson = this._wasm.decode(text, language, wordlist);
        const result = JSON.parse(resultJson);
        if (result.error) throw new Error(result.error);
        return this._autoUnpack(result.decoded_text);
    },

    // ---- Transcode API (meta-instruction based) ----

    transcode(input, metaInstruction) {
        const seed = BigInt(Math.floor(Math.random() * 2 ** 32));
        const resultJson = this._wasm.transcode(input, metaInstruction, seed);
        const result = JSON.parse(resultJson);
        if (result.error) throw new Error(result.error);
        return result;
    },

    // ---- New APIs from glossia native WASM ----

    encodeCharacters(input, language, wordlist, grammarDialect, seed) {
        const s = seed !== undefined ? BigInt(seed) : BigInt(Math.floor(Math.random() * 2 ** 32));
        const resultJson = this._wasm.encode_characters(input, language, wordlist, grammarDialect, s);
        return JSON.parse(resultJson);
    },

    encodeRandomWords(count, language, wordlist, grammarDialect, seed) {
        const s = seed !== undefined ? BigInt(seed) : BigInt(Math.floor(Math.random() * 2 ** 32));
        const resultJson = this._wasm.encode_random_words(count, language, wordlist, grammarDialect, s);
        return JSON.parse(resultJson);
    },

    detectDialect(text) {
        return JSON.parse(this._wasm.detect_dialect_from_text(text));
    },

    getAllDialects() {
        return JSON.parse(this._wasm.get_all_dialects());
    },

    getLanguages() {
        return JSON.parse(this._wasm.get_languages());
    },

    getWordlists(language) {
        return JSON.parse(this._wasm.get_wordlists(language));
    },

    getWordlistSize(language, wordlist) {
        return JSON.parse(this._wasm.get_wordlist_size(language, wordlist));
    },

    getBitsPerWord(language, wordlist) {
        return JSON.parse(this._wasm.get_bits_per_word(language, wordlist));
    },

    randomWords(count, language, wordlist, seed) {
        const s = seed !== undefined ? BigInt(seed) : BigInt(Math.floor(Math.random() * 2 ** 32));
        return JSON.parse(this._wasm.random_words(count, language, wordlist, s));
    },
};

window.GlossiaService = GlossiaService;

// Auto-init on load
GlossiaService.init();
