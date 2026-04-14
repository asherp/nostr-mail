# Nostr-Mail Encryption & Encoding Architecture

## Current State

This document describes how encryption, decryption, and glossia encoding currently work in nostr-mail, with a view toward migrating encryption to the Rust backend and adopting system keychain storage.

---

## 1. Key Management

### Current: Browser-Side Storage

Private keys live in the browser's `localStorage` as a JSON object `{ private_key: "nsec1...", public_key: "npub1..." }`.

**Load order** (`app.js:455-492`):
1. Try `TauriService.getDefaultPrivateKeyFromConfig()` (reads from a config file on disk)
2. Fall back to `localStorage.getItem('nostr_keypair')`
3. If neither exists, user must generate/import via Settings UI

**Runtime access**: `appState.getKeypair()` returns `{ private_key, public_key }` in bech32 (nsec/npub). Every crypto call passes the raw nsec string from JS to either:
- `CryptoService` (JS/nostr-tools, in-browser), or
- `TauriService` → Tauri IPC → Rust backend

**Multi-account**: `ProfileManager` stores multiple `{ private_key, public_key }` objects in `localStorage`. Switching calls `appState.setKeypair(keypair)` and re-initializes.

**Settings encryption**: Sensitive settings (IMAP/SMTP passwords) are encrypted with AES-256-GCM using a key derived from `SHA-256("nostr-mail-settings-encryption-v1:" + secret_key_bytes)`. This happens in both the frontend (`CryptoService.encryptSettingValue`) and backend (`crypto::encrypt_setting_value`).

### Problem

- Private keys stored in plaintext in localStorage (accessible to any JS in the page)
- Keys are passed over Tauri IPC as strings on every encrypt/decrypt call
- No OS-level protection (no keychain, no hardware-backed storage)

---

## 2. NIP Encryption (Subject & Body)

### Libraries

| Layer | Library | Format |
|-------|---------|--------|
| Frontend | `nostr-tools/nip44` (JS, via import map) | NIP-44 v2: `version(1) + nonce(32) + padded_plaintext + MAC(32)` → base64 |
| Frontend | `nostr-tools/nip04` (JS, via import map) | NIP-04: AES-256-CBC → `base64?iv=base64` |
| Backend | `nostr_sdk::nip44` / `nostr_sdk::nip04` (Rust) | Same wire formats |

### Encrypt Flow (`email-service.js:encryptEmailFields`)

```
User clicks Encrypt
       │
       ▼
encryptEmailFields()
       │
       ├── subject: TauriService.encryptMessageWithAlgorithm(privkey, pubkey, subject, algo)
       │             → CryptoService.encryptMessageWithAlgorithm() [JS, nostr-tools]
       │             → base64 NIP ciphertext
       │             → stored in this._subjectCiphertext (for DM matching)
       │             → written to DOM subject field
       │
       ├── body (simplified, <64KB, no attachments):
       │     TauriService.encryptMessageWithAlgorithm(privkey, pubkey, body, algo)
       │     → base64 NIP ciphertext
       │     → wrapped in ASCII armor: --- BEGIN NOSTR NIP-44 ENCRYPTED BODY ---
       │     → written to DOM body field
       │
       └── body (manifest, ≥64KB or has attachments):
             1. generateSymmetricKey() → AES-256-GCM key [WebCrypto]
             2. encryptWithAES(body, aesKey) → AES ciphertext
             3. Build manifest JSON { body: { ciphertext, key_wrap, ... }, attachments: [...] }
             4. TauriService.encryptMessageWithAlgorithm(privkey, pubkey, manifestJSON, algo)
             5. Wrap in ASCII armor
```

**Key detail**: `TauriService.encryptMessageWithAlgorithm` checks if `CryptoService.isReady()`. If yes, it runs entirely in JS (nostr-tools). If no, it falls back to Tauri IPC → Rust `encrypt_message_with_algorithm`.

### Decrypt Flow

Decryption mirrors encryption:
1. Subject: glossia-decode (if needed) → `TauriService.decryptDmContent(privkey, pubkey, ciphertext)`
2. Body: strip ASCII armor → glossia-decode (if needed) → NIP decrypt → if manifest, AES-decrypt body/attachments

---

## 3. Glossia Encoding

### What It Is

Glossia converts binary data (NIP ciphertext) into natural-looking prose. It's a **Rust library** compiled to both:
- **WASM** (`glossia/web/glossia.js` + `glossia_bg.wasm`) → loaded by frontend `GlossiaService`
- **Native Rust** → linked directly into the Tauri backend (`glossia = { path = "../../glossia" }`)

### Encode Flow (`email-service.js:encodeEmailFields`)

Called automatically after `encryptEmailFields()` if a glossia encoding is selected.

```
NIP ciphertext (base64)
       │
       ▼
_packNip04(ciphertext)
  - NIP-04 "base64?iv=base64" → binary pack [len(2), payload, iv(16)] → base64
  - NIP-44 pure base64 → pass through unchanged
       │
       ▼
GlossiaService.transcode(packed, instruction)
  │
  ├── Subject: "encode into ${meta} raw"
  │   → bare payload words, no grammar/cover words (compact for subject line)
  │   e.g. "abandon rifle federal goat canoe fossil"
  │
  └── Body: "encode into ${meta}"
      → full prose with grammar and cover words
      e.g. "Abandon is bad across rifle to the federal lie to cap."
       │
       ▼
Written to DOM / email fields
```

**Per-field settings** control encoding dialect:
- `glossia_encoding_body` → body & subject ciphertext (default: `latin`)
- `glossia_encoding_signature` → signature block
- `glossia_encoding_pubkey` → pubkey/seal block
- Empty string = no encoding (raw base64/hex)

### Decode Flow

```
Glossia words
       │
       ▼
GlossiaService.detectDialect(content)
  → auto-detect: returns [{ language, wordlist, confidence }, ...]
       │
       ▼
GlossiaService.transcode(content, "decode from ${dialect}")
  → hex string (or base64 depending on dialect)
       │
       ▼
_isHex(output) ? _hexToBase64(output) : output
       │
       ▼
_autoUnpack(decoded)
  - If bytes match [len(2), payload, iv(16)]: reconstruct "base64?iv=base64"
  - Otherwise: return as-is (NIP-44)
       │
       ▼
NIP ciphertext ready for decryption
```

### Backend Glossia Usage

The Rust backend **already uses glossia natively** for:
- `detect_dialect_best()` — dialect auto-detection for incoming emails (`email.rs:2328`)
- `decode_from_language()` — decoding glossia words to hex (`email.rs:2336`)
- `encode_into_language()` — encoding for transport auth verification (`email.rs:3275`)
- `encode_bip39` / `decode_bip39` Tauri commands (`lib.rs:3628+`) — direct encoding/decoding

---

## 4. Signing

### Flow (`app.js` sign handler)

1. Extract ciphertext from the body (armor or glossia-decode)
2. `CryptoService.ciphertextToBytes(ciphertext)` → canonical binary
3. `CryptoService.signData(privateKey, bytes)` → Schnorr sign SHA-256 hash
4. Signature (64-byte hex) + pubkey (32-byte hex) encoded as glossia words
5. Appended to email body as `--- BEGIN NOSTR SIGNATURE ---` or `--- BEGIN NOSTR SEAL ---` blocks

### Verification

1. Extract signature and pubkey from armor blocks
2. Glossia-decode each to hex
3. Extract and canonicalize the signed ciphertext
4. `CryptoService.verifySignature(pubkey, signature, bytes)` → Schnorr verify

---

## 5. DM Encryption

### Send Flow (`tauri-service.js:sendDirectMessage`)

```
sendDirectMessage(privateKey, recipientPubkey, message, relays)
       │
       ▼
Utils.isLikelyEncryptedContent(message)?
  ├── true  → { Encrypted: message }   ← already NIP ciphertext, pass through
  └── false → { Plaintext: message }   ← backend will NIP-encrypt
       │
       ▼
Backend send_direct_message (lib.rs:418)
  ├── Plaintext → nip44::encrypt(sk, recipient, text) or nip04::encrypt
  └── Encrypted → use content as-is in Kind 4 event
       │
       ▼
EventBuilder::new(Kind::EncryptedDirectMessage, content)
  → sign → publish to relays
```

**Email-linked DMs**: When sending an encrypted email, the subject's NIP ciphertext (`_subjectCiphertext`) is sent as the DM content via `sendEncryptedDirectMessage` (sends as `{ Encrypted: ciphertext }`). This lets other Nostr clients decrypt the DM, and enables DM↔email hash matching.

---

## 6. Where Crypto Happens Today

| Operation | Frontend (JS) | Backend (Rust) |
|-----------|:---:|:---:|
| NIP-44/04 encrypt | **primary** (nostr-tools) | fallback |
| NIP-44/04 decrypt | **primary** (nostr-tools) | fallback + DM fetch |
| Schnorr sign/verify | **primary** (@noble/curves) | — |
| AES-256-GCM (manifest) | **primary** (WebCrypto) | — |
| Settings AES-GCM encrypt | **primary** (WebCrypto) | also (for batch save) |
| Glossia encode/decode | **primary** (WASM) | also (detect, encode, decode) |
| Key generation | **primary** (nostr-tools) | fallback |
| Key storage | localStorage | config file (default key) |

---

## 7. Migration Path: Backend Encryption + System Keychain

### Goals

1. **Private keys never leave the backend** — no nsec strings over IPC or in localStorage
2. **System keychain** (macOS Keychain, Windows Credential Locker, Linux Secret Service) for key storage
3. **Glossia in Rust** — eliminate WASM build; the backend already links glossia natively

### What Changes

#### Key Management
- Store nsec in system keychain via `keyring` or `tauri-plugin-stronghold`
- Frontend identifies accounts by npub only
- Backend holds nsec in memory for the active session; clears on lock/logout
- IPC calls no longer pass `private_key`; backend looks up the active key internally

#### NIP Encryption
- New Tauri commands: `encrypt_email_fields(pubkey, subject, body, algo)` → returns `{ encrypted_subject, encrypted_body, subject_ciphertext }`
- Frontend sends plaintext, receives ciphertext — never touches keys
- Manifest AES encryption also moves to backend (Rust `aes-gcm` crate)

#### Glossia Encoding
- Already available in Rust; backend can encode/decode inline after encryption
- Single command: `encrypt_and_encode_email(pubkey, subject, body, algo, glossia_settings)` → returns fully encoded fields
- Eliminates the WASM build step and `glossia-service.js` entirely

#### Signing
- `sign_ciphertext(ciphertext_bytes)` → backend signs with active key
- `verify_signature(pubkey, sig, data)` → can stay frontend or move to backend

#### DM Sending
- Backend already handles NIP encryption for `Plaintext` DMs
- `send_email_with_dm(email_params, dm_recipient)` → backend encrypts subject, encodes body, sends email, sends DM with ciphertext — all in one atomic operation

### What Stays in Frontend
- UI rendering and user interaction
- Glossia dialect selection (settings)
- Calling backend commands with plaintext + npub
- Displaying decrypted content

### Rust Crates Needed
- `keyring` or `tauri-plugin-stronghold` — system keychain
- `aes-gcm` — manifest body/attachment encryption (replace WebCrypto)
- `sha2` — already used
- `glossia` — already linked
- `nostr-sdk` — already used for NIP-44/04
- `capnp` — already linked (Cap'n Proto runtime for structured IPC)

---

## 8. Cap'n Proto as the Internal Representation

### Problem: Duplicated Parsing, Ad-Hoc Data Shapes

Armor parsing is currently implemented twice — once in JS (`parseArmorComponents`, ~100 lines) and once in Rust (`parse_armor_depth`, ~75 lines). Both use regex-based depth-counting state machines and return different ad-hoc shapes:

| | JS | Rust |
|---|---|---|
| Return type | `{ bodyText, sigContent, sealContent, quotedArmor, profileName, displayName, rawSigPubkey, prefixText, isEncryptedBody }` | `(String, Option<String>)` (body + nested only) |
| Type detection | Regex on raw text at every call site: `plainBody.match(/-{3,}\s*BEGIN NOSTR (?:NIP-(?:04\|44) ENCRYPTED)/)` | `body.contains("BEGIN NOSTR NIP-04 ENCRYPTED")` |
| Sig/pubkey decode | Deferred — each of 45+ call sites glossia-decodes independently | Partial — only for signing verification |
| Nesting | Returns `quotedArmor` as raw text; callers re-parse recursively | Returns nested text; callers re-parse |

### Solution: Capnp Structs as the Parsing Contract

The Cap'n Proto schema (`schema/nostr_mail.capnp`) already defines typed structs that mirror this structure exactly:

```
ArmorMessage { body: Body, signature: SignatureBlock, seal: SealBlock }
Body { union { encrypted { nip, ciphertext }, signed { plaintext }, plain { text } }, quoted: ArmorMessage }
SignatureBlock { profileName, signature (64 bytes), pubkey (32 bytes) }
SealBlock { displayName, pubkey (32 bytes) }
```

A single Rust parser produces a capnp `ArmorMessage`, and the frontend reads typed fields. This eliminates:

1. **The JS parser entirely** — `parseArmorComponents()` and its 45+ ad-hoc field accesses are replaced by a Tauri command that returns capnp bytes.

2. **Per-site type detection** — the `Body` union is a proper tagged discriminant (`body.which()` → `encrypted | signed | plain`), replacing regex checks scattered across call sites.

3. **Per-site glossia decoding** — the Rust parser decodes signatures and pubkeys before populating the capnp struct. `SignatureBlock.signature` arrives as 64 raw bytes; `SignatureBlock.pubkey` as 32 raw bytes. No caller-side decode needed.

4. **Recursive re-parsing** — `Body.quoted` is a nested `ArmorMessage`, so walking a reply chain is a pointer traversal, not a re-parse of raw text at each level.

### How It Works

#### Reading emails (incoming)

```
text/plain body (ASCII armor)
       │
       ▼
Backend: parse_armor(text) → capnp ArmorMessage
       │
       ├── Body.encrypted.ciphertext (raw bytes, glossia-decoded)
       ├── Body.encrypted.nip (NIP-04 or NIP-44 tag)
       ├── Body.quoted → nested ArmorMessage (already parsed)
       ├── SignatureBlock.signature (64 bytes, glossia-decoded)
       ├── SignatureBlock.pubkey (32 bytes, glossia-decoded)
       └── SignatureBlock.profileName
       │
       ▼
Frontend: reads typed fields from capnp message
       │
       ├── Render body (request decryption via backend if encrypted)
       ├── Display signature status (verify via backend)
       └── Recurse into .quoted for reply threading
```

#### Writing emails (outgoing)

```
Frontend: user composes message
       │
       ▼
Backend: build_armor(ArmorMessage) → text/plain
       │
       ├── Encrypt body → populate Body.encrypted
       ├── Glossia-encode all fields
       ├── Sign → populate SignatureBlock (concatenate all nested body bytes)
       ├── Nest quoted original into Body.quoted
       └── Serialize to ASCII armor text
       │
       ▼
text/plain body ready for SMTP
```

#### Manifest (binary transport)

The `Manifest` struct is the one place where capnp is also the **wire format**, not just the internal representation. After NIP decryption, the manifest payload is either JSON (legacy, first byte `0x7B`) or capnp binary (first byte `0x00`). See the schema comments for detection logic.

### What This Replaces in JS

| Current JS code | Replaced by |
|---|---|
| `parseArmorComponents()` (100 lines) | `parse_armor` Tauri command |
| `_extractAllBodyBytes()` (recursive re-parse) | Walk `Body.quoted` chain on capnp struct |
| `_decodeArmorBodyToBytes()` | Already decoded in capnp fields |
| `_splitSigPubkey()` (glossia decode sig+pubkey) | Already decoded in `SignatureBlock` |
| `verifyInlineSignature()` regex type detection | `body.which()` union discriminant |
| `decodeGlossiaSignedMessage()` | `Body.signed.plaintext` field |
| `buildPlainBody()` (10 positional params) | `build_armor(ArmorMessage)` Tauri command |

### What Stays the Same

- **The wire format is still ASCII armor.** Capnp is the internal representation between backend and frontend, not the email transport encoding. Emails remain human-readable for non-nostr-mail clients.
- **The capnp schema is documentation as much as code.** Even before full migration, the schema defines the canonical structure that both layers agree on.
