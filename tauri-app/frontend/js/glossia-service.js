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
        } catch (e) {
            console.warn('[GlossiaService] WASM not available, will fall back to Tauri:', e.message);
            this._ready = false;
        }
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

    /** Convert a hex string to base64. */
    _hexToBase64(hex) {
        const bytes = hex.match(/.{1,2}/g).map(b => parseInt(b, 16));
        return btoa(String.fromCharCode(...bytes));
    },

    /** Returns true if the string is all hex characters with even length. */
    _isHex(str) {
        return str.length > 0 && str.length % 2 === 0 && /^[0-9a-fA-F]+$/.test(str);
    },

    /** Decode glossia text directly to binary (Uint8Array).
     *  Auto-detects dialect, then calls WASM decode_raw_base_n and converts hex→bytes.
     *  Returns null if detection fails or no payload words found. */
    decodeToBytes(text) {
        if (!this._ready) return null;
        const detections = this.detectDialect(text);
        if (!Array.isArray(detections) || detections.length === 0) return null;
        const { language, wordlist } = detections[0];
        if (!language || !wordlist) return null;
        try {
            const json = JSON.parse(this._wasm.decode_raw_base_n(text, language, wordlist, 0));
            if (json.error) return null;
            const hex = json.decoded_hex;
            const bytes = new Uint8Array(hex.length / 2);
            for (let i = 0; i < bytes.length; i++) {
                bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
            }
            return bytes;
        } catch (e) {
            console.warn('[GlossiaService] decodeToBytes failed:', e);
            return null;
        }
    },

    /** Decode glossia prose text to binary (Uint8Array) via transcode.
     *  Unlike decodeToBytes (which uses decode_raw_base_n for raw/payload-only words),
     *  this handles full prose with grammar/cover words.
     *  Auto-detects dialect, transcodes to hex, converts to bytes.
     *  Returns null if detection or decode fails. */
    transcodeToBytes(text) {
        if (!this._ready) return null;
        const detections = this.detectDialect(text);
        if (!Array.isArray(detections) || detections.length === 0) return null;
        const dialect = detections[0].language;
        if (!dialect) return null;
        try {
            const result = this.transcode(text, `decode from ${dialect}`);
            let hex = result.output;
            if (!hex || !this._isHex(hex)) return null;
            const bytes = new Uint8Array(hex.length / 2);
            for (let i = 0; i < bytes.length; i++) {
                bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
            }
            return bytes;
        } catch (e) {
            console.warn('[GlossiaService] transcodeToBytes failed:', e);
            return null;
        }
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

    // ---- Pubkey / Signature encoding helpers ----

    /** Encode a 32-byte hex pubkey.
     *  With glossia encoding (meta): base_n payload words (no bitpacking).
     *  Without (Hex mode): bech32 npub. */
    encodePubkey(pubkeyHex, meta) {
        if (!meta) return window.CryptoService._nip19.npubEncode(pubkeyHex);
        const { language, wordlist } = this._resolveMetaLanguage(meta);
        const json = JSON.parse(this._wasm.encode_raw_base_n(pubkeyHex, language, wordlist, '', 0n));
        if (json.error) throw new Error(json.error);
        return json.encoded_text;
    },

    /** Encode a 64-byte hex signature.
     *  With glossia encoding (meta): base_n payload words (no bitpacking).
     *  Without (Hex mode): sig_nostr ASCII armor. */
    encodeSignature(sigHex, meta) {
        if (meta) {
            const { language, wordlist } = this._resolveMetaLanguage(meta);
            const json = JSON.parse(this._wasm.encode_raw_base_n(sigHex, language, wordlist, '', 0n));
            if (json.error) throw new Error(json.error);
            return json.encoded_text;
        }
        const b64 = this._hexToBase64(sigHex);
        const result = this.transcode(b64, `encode into cs/hex/sig_nostr`);
        return result.output;
    },

    /** Resolve a meta keyword (e.g. 'latin', 'english bip39', 'latin/bip39')
     *  to { language, wordlist }. */
    _resolveMetaLanguage(meta) {
        const parts = meta.split(/[\s\/]+/).filter(Boolean);
        const language = parts[0];
        const wordlist = parts[1] || this._wasm.get_default_wordlist(language);
        return { language, wordlist };
    },

    /** Decode base_n encoded payload words to hex.
     *  Used for fixed-size fields (signatures, pubkeys) that use base_n not bitpack.
     *  @param {string} text - payload words (bare or with cover words)
     *  @param {string} meta - language meta (e.g. 'latin')
     *  @param {number} [expectedBytes] - expected byte length (e.g. 32 for pubkey, 64 for sig);
     *         left-pads with zeros if base_n decode lost leading zero bytes
     *  @returns {string} hex string */
    decodeRawBaseN(text, meta, expectedBytes) {
        const { language, wordlist } = this._resolveMetaLanguage(meta);
        const json = JSON.parse(this._wasm.decode_raw_base_n(text, language, wordlist, expectedBytes || 0));
        if (json.error) throw new Error(json.error);
        let hex = json.decoded_hex;
        if (expectedBytes && hex.length < expectedBytes * 2) {
            hex = hex.padStart(expectedBytes * 2, '0');
        }
        return hex;
    },

    /** Convert base64 string to hex. */
    _base64ToHex(b64) {
        const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
        return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
    },

    /**
     * Parse a glossia-encoded body to extract embedded pubkey and optional signature.
     * Blocks are separated by \n\n; pubkey is last, signature (if present) is second-to-last.
     * Returns { body, pubkeyHex, signatureHex } or { body } if no pubkey found.
     */
    parseSignedBody(fullBody, metaPubkey, metaSig) {
        // Support old single-meta call signature for backwards compat
        if (metaSig === undefined) metaSig = metaPubkey;

        const paragraphs = fullBody.split('\n\n');
        if (paragraphs.length < 2) {
            return { body: fullBody, pubkeyHex: null, signatureHex: null };
        }

        let pubkeyHex = null;
        let signatureHex = null;

        // Extract pubkey from last paragraph.
        // Format: "Seal\n@name\nfirst\nsecond" or "Seal\nfirst\nsecond"
        // Legacy: "`pubkey`" or bare pubkey words
        try {
            let lastPara = paragraphs[paragraphs.length - 1].trim();
            // Strip Seal header and @name line, collect remaining lines as pubkey
            const lines = lastPara.split('\n');
            // Filter out Seal header and @name lines
            const pubkeyLines = lines.filter(l => {
                const t = l.trim();
                return t && t !== 'Seal' && t !== '**Seal**' && !t.startsWith('@');
            });
            // Join remaining lines, strip legacy tick marks
            let pubkeyLine = pubkeyLines.join('').replace(/`/g, '');

            if (!metaPubkey) {
                // Bech32 mode: try nip19 decode
                const { type, data } = window.CryptoService._nip19.decode(pubkeyLine);
                if (type === 'npub') {
                    pubkeyHex = data;
                }
            } else {
                const decoded = this.decodeRawBaseN(pubkeyLine, metaPubkey, 32);
                if (decoded.length === 64) {
                    pubkeyHex = decoded;
                }
            }

            if (pubkeyHex) {
                paragraphs.pop();

                // Strip @name paragraph if present (when Seal has blank line after it)
                if (paragraphs.length >= 1) {
                    const maybeAt = paragraphs[paragraphs.length - 1].trim();
                    if (maybeAt.startsWith('@')) {
                        paragraphs.pop();
                    }
                }

                // Strip Seal header (or display name) if present as its own paragraph
                if (paragraphs.length >= 1) {
                    const maybeSeal = paragraphs[paragraphs.length - 1].trim();
                    if (maybeSeal && !maybeSeal.includes('\n')) {
                        paragraphs.pop();
                    }
                }

                // Try decode next paragraph as signature
                // Multi-line > quoted: join lines, strip > prefixes
                if (paragraphs.length >= 2) {
                    try {
                        let sigPara = paragraphs[paragraphs.length - 1].trim();
                        sigPara = sigPara.split('\n').map(l => l.replace(/^>\s*/, '')).join(' ').trim();
                        let sigDecoded;
                        if (metaSig) {
                            sigDecoded = this.decodeRawBaseN(sigPara, metaSig, 64);
                        } else {
                            const sigResult = this.transcode(sigPara, `decode from sig nostr`);
                            sigDecoded = sigResult.output;
                            if (!this._isHex(sigDecoded)) {
                                sigDecoded = this._base64ToHex(sigDecoded);
                            }
                        }
                        if (sigDecoded.length === 128) {
                            signatureHex = sigDecoded;
                            paragraphs.pop();
                        }
                    } catch (_) { /* no signature block */ }
                }

                // Strip signature label if present (could be "Signature" or user's profile name)
                if (signatureHex && paragraphs.length >= 1) {
                    const maybeSigHeader = paragraphs[paragraphs.length - 1].trim();
                    if (maybeSigHeader && !maybeSigHeader.includes('\n')) {
                        paragraphs.pop();
                    }
                }
            }
        } catch (_) { /* no pubkey block */ }

        return {
            body: paragraphs.join('\n\n'),
            pubkeyHex,
            signatureHex
        };
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
