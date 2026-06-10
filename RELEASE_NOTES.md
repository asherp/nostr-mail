# Release Notes - v1.0.8

## Overview

v1.0.8 is a large performance, sync, and security release. The IMAP layer was rebuilt around a warm connection pool with IDLE push and aggressive timeouts, inbox history now syncs by message count instead of a fragile date cutoff, and the decrypt/render path was parallelized and cached so threads paint progressively instead of blocking on a full batch. Alongside the speedups, private keys and email passwords no longer touch the frontend at all, spam handling became provider-aware, and a class of IMAP parser crashes on refresh was fixed.

## What's New

### ⚡ Faster, More Reliable IMAP Sync
- New IMAP connection pool with per-operation timeouts and **IDLE push** for near-instant new-mail delivery on the active account
- Inbox history now syncs by **message count** (`sync_initial_count` / `sync_max_scan`, configurable per folder) instead of a date cutoff that silently clipped older mail
- Backward UID pagination ("fetch older") with **infinite scroll** replaces the old Load More buttons on inbox and sent
- Scroll and bootstrap windows are prefiltered **server-side** instead of fetching every message body, and date-fetch on scroll was cut to a single UID
- Gap-fill now examines each UID once and watermarks it, instead of rescanning the whole inbox on every refresh; the **Refresh** button fills gaps and tab switches auto-sync new mail
- Multiple IMAP folders can be selected for inbox refresh, with provider-aware default folders

### 🚀 Faster Decryption & Rendering
- Thread email decryption is **parallelized**; inbox, sent, and thread cards render **progressively** as each message finishes instead of waiting for the whole batch
- Process-wide caches for glossia detect/decode, armor parsing, and subject decode; eager glossia decode skipped for NIP-44
- Attachments lazy-load with an `attachment_count` badge rather than being fetched up front
- Glossia submodule bumped for a cached wordlist tree, speeding up DM and email decoding
- "Decrypting…" placeholders and skeleton rows show on encrypted content until decryption completes

### 🗂️ Smarter Spam & Folder Handling
- Provider-aware default inbox folders; `*spam*`/`*junk*`/`*bulk*` folders auto-discovered and kept **out of** the main inbox
- Automated **spam rescue** for nostr mail, gated on transport auth and using `\Seen` as intent
- New **"Move to folder"** action for inbox emails and in the conversation view, with robust server-truth folder moves
- **"Auto-file Nostr Mail"** setting (default **off**, opt-in), scoped to INBOX only
- Read state now syncs with the IMAP `\Seen` flag in both directions

### 📎 Attachments
- Fixed Android attachment downloads; added **Download** and **Share** buttons
- Fixed inbox attachment download for self-authored mail
- Attachments render inline inside thread/conversation cards

## What's Fixed

### 🐛 IMAP Parser Desync Crash
- Fixed a parser desync that could crash the app on inbox refresh
- Fixed `gap_fill` treating the entire non-nostr inbox as recoverable gaps
- `require_signature` drops are now recoverable when the setting is toggled off

### 🔒 Security Hardening
- Private keys are **no longer persisted to the frontend** — they live only in the OS-native keychain
- Email passwords are **no longer written to `localStorage`**
- Removed private-key QR logging; revealing the private-key QR now requires explicit confirmation
- Verbose `[RUST]` / `[RUST-PERF]` and `get_email` logging gated behind `NOSTR_MAIL_DEBUG`

### 💬 Direct Messages
- NIP-17 DMs deduplicated by rumor id, with legacy duplicates collapsed and content-addressed dedup at insert time
- Conversation loading paginated with lighter contact-list previews

### 🎨 UI
- Sidebar shows the **real bundle version** instead of a hardcoded string
- "Unsigned" badge for messages with no signature; clearer "Require Signatures" / "Hide Unverified" help text
- Tom Select-based folder multiselect with palette-matched pills
- Dropped redundant email labels for known contacts in conversation cards

### 🛠️ Build & CI
- Staging pushes build all platforms; concurrency groups cancel superseded runs
- Android build scoped to the `android` branch and release tags; fixed Android build for imap 3.x

## Upgrade Notes

- The new count-based sync supersedes the old date cutoff. If older mail was previously clipped, it becomes reachable again via infinite scroll / Refresh on this version
- "Auto-file Nostr Mail" defaults to **off** — enable it in settings if you want nostr mail filed automatically
- Existing installs keep their database on upgrade (Android `-r` reinstall / desktop in-place update)

---

# Release Notes - v1.0.7

## Overview

v1.0.7 hardens **identity & decryption** in the email pipeline, brings the **Android build pipeline back to green**, expands the **integration test surface**, and continues mobile UX polish. Signed-email identity is now anchored to a Nostr pubkey rather than the `From:` header, and decryption no longer depends on relay lookups for sent-folder messages.

## What's New

### 🔐 Identity & Encryption
- **Pubkey-bound identity** – signed emails are now bound to the Nostr pubkey, not the `From:` header, eliminating spoofing via header rewrites
- **Relay-free decryption anchor** – new `X-Nostr-Recipient` header lets the client decrypt sent-folder copies without round-tripping a relay
- **Signature header toggles** – `X-Nostr-Pubkey` / `X-Nostr-Sig` headers can be enabled/disabled independently; header signing restored
- **DM ↔ email anchoring** fixed for NIP-04, and avatars locked to pubkey so identity is consistent across both surfaces
- **Removed brittle email-address → pubkey resolution** in favor of DM-anchored retrofit

### 📱 Mobile & UX Polish
- **Per-pubkey settings scope** – switching keys now correctly swaps the active settings profile
- **Inbox folder picker moved to Settings** for a cleaner detail view
- **Tightened mobile layout** for Settings and email views
- **Sent-side body rendering** fixed (thread `bodyId` is now scoped by source)
- Delete-UX repositioned for better one-handed use

### 🤖 Android
- **CI restored** – fixed `--apk` flag usage for tauri-cli 2.11 and bumped CI to tauri-cli **2.11.1** for `debugApplicationIdSuffix` support
- **Debug & release builds coexist** in separate sandboxes, no more reinstall conflicts
- Stopped tracking the generated Android frontend assets snapshot – regenerated at build time
- Non-Android CI jobs skipped on the `android` branch

### 🧪 Tests
- New `email_integration.rs` suite covering the full email pipeline
- Headless integration test for the default-settings email pipeline
- NIP-04, multipart, and non-ASCII subject coverage + dedicated `backend-tests` CI workflow
- Glossia-body and in-body `SIGNATURE` block roundtrip tests
- Sent-folder decryption via `X-Nostr-Recipient` header
- Reply-quoting preserves nested signatures + encryption
- Raw `bitpack_fixed` decoder + signature/threading tests

### 🧹 Cleanup
- Removed the legacy `bip39-encode/` crate (subsumed by glossia)
- Dropped tracked generated Android assets

---

# Release Notes - v1.0.6

## Overview

v1.0.6 is a patch release that fixes default-relay seeding on packaged desktop installs. v1.0.5 baked the build machine's source path into the binary, so first-run relay seeding silently failed on every Mac, Windows, and Linux machine that wasn't the build host — leaving new installs with no default relays. The config JSON is now embedded into the binary on every platform.

## What's Fixed

### 🛰️ Default Relays on Fresh Installs (All Desktop Platforms)
- Default relay list (`nostr-mail-config.json`) is now embedded at compile time on macOS, Windows, and Linux — matching the existing Android behavior
- Previous releases resolved the config via `env!("CARGO_MANIFEST_DIR")`, which expands to the build machine's source path. On packaged installs that directory doesn't exist, so seeding silently no-op'd and new accounts started with an empty relay list
- `NOSTR_MAIL_CONFIG` env var still overrides the embedded config for tests

### 🌐 Landing Page
- Added Windows (NSIS setup) and Linux (AppImage) download buttons
- Fixed Android download link (was pointing at the dropped universal APK; now points at the arm64 APK that releases actually ship)

## Upgrade Notes

- Existing installs with a populated relay list are unaffected — the seeding path only runs when the relays table is empty
- If your previous install ended up with no default relays due to this bug, restart the app on v1.0.6: the lazy first-read seeding clones the embedded defaults into your account's relay list on next access

---

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
