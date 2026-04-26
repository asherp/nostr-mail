# Release Notes - v1.0.5

## Overview

v1.0.5 is a major architectural release. Glossia encoding, email decryption, and signature verification all moved from the JS/WASM frontend to the Rust backend, removing the ~38MB WASM blob and consolidating cryptography in one place. Private keys now live in the OS-native keychain (Apple Keychain / Windows Credential Manager / Linux Secret Service / Android Keystore-backed encrypted vault) instead of `localStorage`. A new threaded conversation view, multi-profile account switching, and per-field glossia encoding settings round out the user-facing changes.

## What's New

### 🔐 OS-Native Keychain for Private Keys
- Private keys migrated out of `localStorage` into the platform keychain via the `keyring` crate
- All keypairs stored in a single `nostr-mail/vault` entry so macOS only prompts once per session
- In-memory vault cache to avoid repeated keychain prompts
- Android: vault persisted via Jetpack Security's `EncryptedFile` (master key in Android Keystore) — fixes the bug where every Android restart cleared the active account
- Backend Tauri commands no longer require the frontend to pass private keys in JS memory

### 👥 Multi-Profile Account Switching
- Sidebar profile switcher for instant account switching with no re-entry of private keys
- Multiple Nostr keypairs stored in the unified vault
- Per-profile settings (already per-pubkey in the database) now exposed via UI
- Account removal with optional data purge
- Stale emails/DMs/contacts cleared on profile switch

### 🧵 Threaded Conversation View
- Inbox/Sent switched from flat lists to thread summaries with message-count badges
- Gmail-like collapsible thread cards: most recent message expanded, others show a one-line preview
- Per-email decryption inside threads with correct sender/recipient pubkey direction
- Inline signature badges injected into HTML iframes
- Compact icon-only sig/transport indicators with full details in metadata panel

### ⚡ Performance
- Parallel inbox decryption via `Promise.allSettled` (was sequential)
- Batch decryption Tauri command (`decrypt_email_bodies_batch`) — N IPC calls collapse to 1
- Preview cache (`_previewCache`) avoids re-decryption on re-render
- O(1) contact lookup via `_contactsByPubkey` / `_contactsByEmail` index maps
- "Load More" appends instead of full DOM clear
- Drafts list upgraded to match sent list rendering (avatars, indicators, parallel)

### 🦀 Backend Migration (JS → Rust)
- **Glossia**: 5 new Tauri commands (`glossia_transcode`, `glossia_detect_dialect`, `glossia_encode_raw_base_n`, `glossia_decode_raw_base_n`, `glossia_get_default_wordlist`); ~38MB WASM blob and `wasm-pack` build hook removed
- **Email decryption**: `decrypt_email_body` replaces ~100 lines of JS per detail view; full pipeline (body + subject + manifest attachments + nested armor) now in `email.rs`
- **Signature verification**: `verify_all_signatures` recursively verifies nested signatures (innermost-first), fixing a bug where only one signature in a reply chain was validated; ~500 lines of JS verification code removed
- **DM decryption**: `TauriService.decryptDmContent` and `decryptManifestAttachment` move DM content + attachment decryption to backend
- **Cap'n Proto**: New schema (`nostr_mail.capnp`) for `Manifest`, `ArmorMessage`, `Body`, `SignatureBlock`, `SealBlock`, `Email` as the typed parsing target between backend and frontend

### ✍️ Compose & Signing UX
- Clicking **Encrypt** now auto-triggers signing for both NIP-04 and NIP-44 (single button labeled "Encrypt")
- Reply quotes are now included **inside** the encryption envelope (was outside, leaking quoted context)
- **Signed plaintext** format: human-readable plaintext above the armor block; signature covers the canonical decoded bytes so it survives transport reformatting
- Per-field glossia encoding settings (`glossia_encoding_body` / `_signature` / `_pubkey`) under Advanced settings — defaults to Latin
- SIGNATURE and SEAL merged into a combined block for signed messages (legacy separate-block format still parsed)

### 🛡️ NIP-04 Hardening
- NIP-04 messages now **require** signatures to mitigate padding oracle / bit-flipping attacks against unauthenticated CBC encryption
- In-body signature is the primary trust path (per spec §6.1); X-Nostr-Sig header is secondary
- New `signature_source` field (`body` / `header` / `both`) tracks which signature verified, surfaced in tooltip
- Pubkey verification accepts both hex and npub formats

### 📧 Email & Reply Threading
- `In-Reply-To` and `References` headers on replies for proper threading in upstream clients
- Sent-mail discovery rewritten: Gmail IMAP TEXT search is broken in `[Gmail]/Sent Mail`, replaced with SINCE-based fetch + client-side `BEGIN NOSTR` filtering
- Per-folder sync timestamps (`last_sync_at:sent`) prevent cursor drift from non-nostr emails
- Inbox email deletion with Local / Everywhere choice modal
- HTML email support in send/construct (multipart/alternative)
- Glossia previews decoded for inbox/sent list rendering
- Reply-To header preferred over From for replies

### 💬 Direct Messages
- DMs now sent as raw NIP ciphertext (base64) in Kind 4 content — interoperates with other Nostr clients (was previously glossia-encoded, only decryptable by nostr-mail)
- Live DM decryption before display
- DM↔email matching race fixed via immediate sent-email stub (`db_save_sent_email_stub`) so `subject_hash` is available before IMAP sync
- Cross-profile DM pubkey leak fixed (`get_all_dm_pubkeys[_sorted]` now scoped to active user)
- Account-switch decrypt fix: when `contactPubkey == userPubkey`, fall back to sender pubkey to avoid `ECDH(myPriv, myPub)` producing wrong shared secret

### 📱 Android
- Encrypted keypair vault (Jetpack Security `EncryptedFile` + Android Keystore)
- Debug deploy script no longer wipes app data on rebuild (`adb install -r` upgrades in place)
- `adb reverse tcp:1430` + `TAURI_DEV_HOST=127.0.0.1` for reliable dev server reachability over USB
- Portrait UX: back-to-nav buttons, account switcher visible in full-screen menu, toast z-index raised to render above page overlay
- QR/camera button listeners wired during init (was inert on fresh install)

### 📜 Spec v0.2.0
- Accept npub (bech32) in addition to hex for `X-Nostr-Pubkey` header
- HTML rendering section: armor-to-div mapping, reply threading, signature display, inline styles
- Signed plaintext reply format with email-quoted previous text
- Reply format restructured into encrypted, plaintext, chains, and quote handling subsections
- Signature coverage in replies: flat concatenation of all nested body bytes (not per-level independent)

### 🌐 Landing Page & Release Tooling
- Zapstore download link added; hero buttons reorganized into 2x2 grid
- Hardcoded version strings replaced with `{{VERSION_TAG}}` / `{{VERSION}}` placeholders, substituted at deploy time from `tauri.conf.json`
- Multi-OS installer build workflow (`build.yml`) for Windows/Linux/macOS
- Release flow documented in `docs/development.md` (tag-first push order to avoid 404s on download buttons)
- `-beta` suffix dropped from versions (Windows MSI bundler rejects non-numeric pre-release identifiers); pre-release status now lives on the GitHub Release flag

### 🐛 Notable Bug Fixes
- Sign handler double-wrapping (encrypt→sign produced spurious outer armor)
- Subject and nested block decryption when `sender_pubkey` is missing (now falls back to inline armor pubkey)
- Inline signature block ID collisions in thread view (now scoped to container)
- Reply state staleness when opening a new reply
- HTML mojibake in quoted encrypted bodies (now shows glossia prose, not raw binary)
- NIP-04 reply signing didn't cover quoted chain (spec §3.5.0)
- Compose decrypt toggle dropped quoted content
- Header signature verification stripped whitespace from glossia prose
- Glossia decode language detection switched to `detect_dialect_best` (longest-wins heuristic was brittle)
- HTML newlines preserved as `<br>` in encrypted email alts
- Drafts excluded from inbox query
- Self-heal default contact when its email column is empty on first add

## Platform Support

- ✅ Windows
- ✅ macOS
- ✅ Linux
- ✅ Android (now with persistent encrypted keypair storage)

## Upgrade Notes

- **Private keys auto-migrate** from `localStorage` to the OS keychain on first launch
- macOS users will see a one-time keychain authorization prompt
- Android users with existing installs: vault keys held only in process memory previously, so reconfiguration may be needed
- NIP-04 messages without signatures are now **rejected** — re-send any in-flight unsigned NIP-04 messages

---

**License**: Apache License 2.0
