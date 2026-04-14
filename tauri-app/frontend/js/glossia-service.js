// Glossia Service
// Routes all glossia encoding/decoding through backend Tauri commands.

const GlossiaService = {
    _ready: true,

    async init() {
        // No-op: backend is always available when Tauri is running
    },

    isReady() {
        return true;
    },

    // ---- NIP-04 binary packing helpers (pure JS) ----

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

    _autoUnpack(decoded) {
        try {
            const bytes = Uint8Array.from(atob(decoded), c => c.charCodeAt(0));
            if (bytes.length >= 2) {
                const payloadLen = (bytes[0] << 8) | bytes[1];
                const ivLen = bytes.length - 2 - payloadLen;
                if (payloadLen > 0 && ivLen === 16) {
                    const payloadB64 = btoa(String.fromCharCode(...bytes.slice(2, 2 + payloadLen)));
                    const ivB64 = btoa(String.fromCharCode(...bytes.slice(2 + payloadLen)));
                    return payloadB64 + '?iv=' + ivB64;
                }
            }
        } catch (_) { /* not valid base64 or unpacking failed */ }
        return decoded;
    },

    _hexToBase64(hex) {
        const bytes = hex.match(/.{1,2}/g).map(b => parseInt(b, 16));
        return btoa(String.fromCharCode(...bytes));
    },

    _isHex(str) {
        return str.length > 0 && str.length % 2 === 0 && /^[0-9a-fA-F]+$/.test(str);
    },

    _base64ToHex(b64) {
        const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
        return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
    },

    // ---- Core async methods (routed through backend) ----

    async _resolveMetaLanguage(meta) {
        const parts = meta.split(/[\s\/]+/).filter(Boolean);
        const language = parts[0];
        const wordlist = parts[1] || await TauriService.glossiaGetDefaultWordlist(language);
        return { language, wordlist };
    },

    async detectDialect(text) {
        const json = await TauriService.glossiaDetectDialect(text);
        return JSON.parse(json);
    },

    async transcode(input, metaInstruction) {
        const resultJson = await TauriService.glossiaTranscode(input, metaInstruction);
        const result = JSON.parse(resultJson);
        if (result.error) throw new Error(result.error);
        return result;
    },

    async decodeRawBaseN(text, meta, expectedBytes) {
        const { language, wordlist } = await this._resolveMetaLanguage(meta);
        const resultJson = await TauriService.glossiaDecodeRawBaseN(text, language, wordlist, expectedBytes || 0);
        const json = JSON.parse(resultJson);
        if (json.error) throw new Error(json.error);
        let hex = json.decoded_hex;
        if (expectedBytes && hex.length < expectedBytes * 2) {
            hex = hex.padStart(expectedBytes * 2, '0');
        }
        return hex;
    },

    async decodeToBytes(text) {
        try {
            const detections = await this.detectDialect(text);
            if (!Array.isArray(detections) || detections.length === 0) return null;
            const { language, wordlist } = detections[0];
            if (!language || !wordlist) return null;
            const resultJson = await TauriService.glossiaDecodeRawBaseN(text, language, wordlist, 0);
            const json = JSON.parse(resultJson);
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

    async transcodeToBytes(text) {
        try {
            const detections = await this.detectDialect(text);
            if (!Array.isArray(detections) || detections.length === 0) return null;
            const dialect = detections[0].language;
            if (!dialect) return null;
            const result = await this.transcode(text, `decode from ${dialect}`);
            let output = result.output;
            if (!output) return null;
            if (this._isHex(output)) {
                const bytes = new Uint8Array(output.length / 2);
                for (let i = 0; i < bytes.length; i++) {
                    bytes[i] = parseInt(output.substr(i * 2, 2), 16);
                }
                return bytes;
            }
            // Decoded output is UTF-8 plaintext (Rust decoder returns the
            // original string when the bytes are valid UTF-8, e.g. signed
            // plaintext bodies encoded as Ascii7).
            return new TextEncoder().encode(output);
        } catch (e) {
            console.warn('[GlossiaService] transcodeToBytes failed:', e);
            return null;
        }
    },

    async encodePubkey(pubkeyHex, meta) {
        if (!meta) return window.CryptoService._nip19.npubEncode(pubkeyHex);
        const { language, wordlist } = await this._resolveMetaLanguage(meta);
        const resultJson = await TauriService.glossiaEncodeRawBaseN(pubkeyHex, language, wordlist, '');
        const json = JSON.parse(resultJson);
        if (json.error) throw new Error(json.error);
        return json.encoded_text;
    },

    async encodeSignature(sigHex, meta) {
        if (meta) {
            const { language, wordlist } = await this._resolveMetaLanguage(meta);
            const resultJson = await TauriService.glossiaEncodeRawBaseN(sigHex, language, wordlist, '');
            const json = JSON.parse(resultJson);
            if (json.error) throw new Error(json.error);
            return json.encoded_text;
        }
        const b64 = this._hexToBase64(sigHex);
        const result = await this.transcode(b64, `encode into cs/hex/sig_nostr`);
        return result.output;
    },

    async encodeSigPubkey(sigHex, pubkeyHex, metaSig, metaPubkey) {
        if (metaPubkey) {
            const combinedHex = sigHex + pubkeyHex;
            const { language, wordlist } = await this._resolveMetaLanguage(metaPubkey);
            const resultJson = await TauriService.glossiaEncodeRawBaseN(combinedHex, language, wordlist, '');
            const json = JSON.parse(resultJson);
            if (json.error) throw new Error(json.error);
            return { encodedSigPubkey: json.encoded_text, combined: true };
        }
        return {
            encodedSig: await this.encodeSignature(sigHex, metaSig),
            encodedPubkey: await this.encodePubkey(pubkeyHex, null),
            combined: false
        };
    },

    /**
     * Parse a glossia-encoded body to extract embedded pubkey and optional signature.
     * Blocks are separated by \n\n; pubkey is last, signature (if present) is second-to-last.
     * Returns { body, pubkeyHex, signatureHex } or { body } if no pubkey found.
     */
    async parseSignedBody(fullBody, metaPubkey, metaSig) {
        if (metaSig === undefined) metaSig = metaPubkey;

        const paragraphs = fullBody.split('\n\n');
        if (paragraphs.length < 2) {
            return { body: fullBody, pubkeyHex: null, signatureHex: null };
        }

        let pubkeyHex = null;
        let signatureHex = null;

        try {
            let lastPara = paragraphs[paragraphs.length - 1].trim();
            const lines = lastPara.split('\n');
            const pubkeyLines = lines.filter(l => {
                const t = l.trim();
                return t && t !== 'Seal' && t !== '**Seal**' && !t.startsWith('@');
            });
            let pubkeyLine = pubkeyLines.join('').replace(/`/g, '');

            if (!metaPubkey) {
                const { type, data } = window.CryptoService._nip19.decode(pubkeyLine);
                if (type === 'npub') {
                    pubkeyHex = data;
                }
            } else {
                const decoded = await this.decodeRawBaseN(pubkeyLine, metaPubkey, 32);
                if (decoded.length === 64) {
                    pubkeyHex = decoded;
                }
            }

            if (!pubkeyHex && window.emailService) {
                let combinedText = paragraphs[paragraphs.length - 1].trim();
                combinedText = combinedText.split('\n').map(l => l.replace(/^>\s*/, '')).join(' ').trim();
                const split = await window.emailService._splitSigPubkey(combinedText);
                if (split) {
                    signatureHex = split.sigHex;
                    pubkeyHex = split.pubkeyHex;
                    paragraphs.pop();
                    if (paragraphs.length >= 2) {
                        const maybeLabel = paragraphs[paragraphs.length - 1].trim();
                        if (maybeLabel && !maybeLabel.includes('\n')) {
                            paragraphs.pop();
                        }
                    }
                }
            }

            if (pubkeyHex && !signatureHex) {
                paragraphs.pop();

                if (paragraphs.length >= 1) {
                    const maybeAt = paragraphs[paragraphs.length - 1].trim();
                    if (maybeAt.startsWith('@')) {
                        paragraphs.pop();
                    }
                }

                if (paragraphs.length >= 1) {
                    const maybeSeal = paragraphs[paragraphs.length - 1].trim();
                    if (maybeSeal && !maybeSeal.includes('\n')) {
                        paragraphs.pop();
                    }
                }

                if (paragraphs.length >= 2) {
                    try {
                        let sigPara = paragraphs[paragraphs.length - 1].trim();
                        sigPara = sigPara.split('\n').map(l => l.replace(/^>\s*/, '')).join(' ').trim();
                        let sigDecoded;
                        if (metaSig) {
                            sigDecoded = await this.decodeRawBaseN(sigPara, metaSig, 64);
                        } else {
                            const sigResult = await this.transcode(sigPara, `decode from sig nostr`);
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
};

window.GlossiaService = GlossiaService;
