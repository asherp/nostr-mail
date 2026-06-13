# Feature Test Checklist — Landing to `master`

Tracking checklist for the features and fixes currently queued to land on
`master` (from `staging`). Each item lists the user-visible behavior we are
promising and the test that guards it. An item is **ready to land** when its
box is checked: the code is on `staging` **and** a feature test covers the
behavior so it can't silently regress.

## How to run

```bash
# Backend integration tests (what CI runs on every PR to master/staging)
cd tauri-app/backend && cargo test --test email_integration

# Backend unit tests
cd tauri-app/backend && cargo test --lib
```

CI: `.github/workflows/test.yml` runs `cargo test --test email_integration`
on every push/PR to `main`, `master`, and `staging`.

Legend: `[x]` code landed **and** covered by a feature test · `[ ]` code
landed but **test still owed** before this can be considered safe to land.

---

## 🖋️ Draft rendering fix _(focus of this PR)_

Drafts are authored by the current user and are **unsigned until sent**, so the
inbound-protection settings (`hide_unsigned_messages` / `require_signature`)
must never hide them — mirroring the sent-mail render path. The draft preview
must also decode both glossia clear-signed bodies and NIP-44 encrypted bodies.

Code: `email-service.js` `renderDrafts()` / `renderDraftItem()`
(commit `5c8edce`, "Show self-authored drafts regardless of
`hide_unsigned_messages`").

- [ ] Self-authored draft renders even when `hide_unsigned_messages` /
      `require_signature` is on (must not be filtered as "unverified")
- [ ] NIP-44 encrypted draft body decrypts to a preview for the author
- [ ] Glossia clear-signed draft body decodes to plaintext preview
- [ ] Undecryptable draft is only hidden when `hide_undecryptable_emails` is on
- [ ] Draft attachment count badge renders without blocking the list

> **Owed:** a backend feature test in `email_integration.rs` exercising the
> draft preview path (encrypted + signed bodies) — there is currently **no**
> `draft` test in the integration suite.

---

## 🔏 Signing & verification

Covered today by `email_integration.rs`.

- [x] Header-signature roundtrip (`defaults_header_sig_roundtrip`)
- [x] Full roundtrip with inline signature (`defaults_full_roundtrip_with_inline_sig`)
- [x] Tampered body invalidates signature (`tampered_body_invalidates_signature`)
- [x] Clear-signed plaintext verifies via header (`clearsigned_plaintext_verifies_via_header`)
- [x] Inline-vs-header precedence reporting
      (`inline_valid_header_broken_reports_body`,
      `inline_broken_header_valid_reports_header`)
- [x] Broken pubkey fails verification (`broken_pubkey_fails_verification`)
- [x] `decode_sig_and_pubkey` honors schema canonical-first order
      (branch `claude/sig-decoder-schema-order`, merged)

## 🔐 Encryption (NIP-04 / NIP-44) & replies

- [x] NIP-04 legacy decrypt (`nip04_legacy_decrypt`)
- [x] NIP-04 falls back to `X-Nostr-Sig` header when no inline signature
      (`nip04_header_sig_fallback_unlocks_decrypt`)
- [x] Glossia latin body roundtrip (`glossia_body_latin_roundtrip`)
- [x] Signed plaintext reply preserves nested signature
      (`signed_plaintext_reply_preserves_nested_signature`)
- [x] NIP-44 reply preserves nested encrypted armor
      (`nip44_reply_preserves_nested_encrypted_armor`)
- [x] NIP-44 three-level reply chain (`nip44_three_level_reply_chain`)
- [x] Reply threading headers + encoded quote (`reply_threading_headers_and_encoded_quote`)

## 📎 Attachments

- [x] Manifest attachment compose, inline-sig verifies
      (`manifest_attachment_compose_inline_sig_verifies`)
- [x] Manifest attachment default js-format, inline-sig verifies
      (`manifest_attachment_default_jsformat_inline_sig_verifies`)
- [x] Attachments render inline in thread/conversation cards
      (branch `claude/attachment-decrypt-fix`, merged)
- [ ] Inbox attachment download for self-authored mail (manual; no automated test)

## 📬 Sent / inbox decryption

- [x] Sent mail decrypts via recipient header without a DM
      (`sent_mail_decrypts_via_recipient_header_without_dm`)
- [x] Sent mail undecryptable without any counterparty hint
      (`sent_mail_undecryptable_without_any_counterparty_hint`)
- [x] Multipart HTML + text (`multipart_html_and_text`)
- [x] Non-ASCII subject roundtrip (`non_ascii_subject_roundtrip`)
- [x] Quoted-printable body roundtrip (`quoted_printable_body_roundtrip`)

## 🛡️ Transport authentication _(in flight — PR #103)_

Hardening of `verify_transport_authentication` (forgeable `dmarc=pass`
verdict). Targets `staging` and must land before/with this PR.

- [ ] Forged bottom-most `Authentication-Results` header is ignored
      (top-most, provider-stamped header is selected)
- [ ] `authserv-id` gate fails closed for unknown authserv-ids
- [ ] SPF-only does **not** mark a message From-authenticated
- [ ] Relaxed DMARC alignment on organizational domain (subdomain DKIM passes)

> Tests live on PR #103's branch (`claude/eloquent-volta-t3c4cj`); confirm
> they are green and merged into `staging` before this lands.

## 🔄 IMAP sync, folders & spam _(manual / no automated coverage yet)_

- [ ] Count-based history sync (`sync_initial_count` / `sync_max_scan`)
- [ ] Backward UID pagination / infinite scroll on inbox + sent
- [ ] `gap_fill` examines each UID once and watermarks it (no full rescan)
- [ ] IDLE push delivers new mail on the active account
- [ ] Spam/junk/bulk folders kept out of the main inbox
- [ ] Spam rescue gated on transport auth, uses `\Seen` as intent
- [ ] `\Seen` read-state syncs in both directions

## 💬 Direct messages _(manual / no automated coverage yet)_

- [ ] NIP-17 DMs deduplicated by rumor id (legacy duplicates collapse)
- [ ] Conversation pagination with lighter contact-list previews

## 👥 Group / multi-recipient _(spec only — future, not landing now)_

Spec landed (To/Cc roles, group encryption, CONSENT block — PRs #97/#99/#100);
implementation and tests are future work, tracked here so they aren't lost.

- [ ] Multi-recipient (group) encryption: one content key wrapped per recipient
- [ ] To/Cc role semantics
- [ ] CONSENT block disambiguates signing from replying
