# Nostr-Mail Protocol Specification

**Version:** 0.4.0-draft
**Date:** 2026-06-13

## 1. Overview

Nostr-Mail is a protocol for sending Nostr-encrypted and/or Nostr-signed messages over standard email (SMTP/IMAP). It embeds cryptographic content in ASCII armor blocks within the `text/plain` part of an email, enabling end-to-end encryption and identity verification while remaining compatible with all email infrastructure.

## 2. Armor Block Format

All Nostr-Mail content is enclosed in ASCII armor blocks using `-----` delimiters. Blocks are identified by type-specific tags.

### 2.1 Block Types

| Tag | Role | Contains |
|-----|------|----------|
| `BEGIN NOSTR NIP-XX ENCRYPTED BODY` | Encrypted content | Ciphertext (NIP-04 or NIP-44) |
| `BEGIN NOSTR SIGNED BODY` | Signed plaintext | Plaintext body content |
| `BEGIN NOSTR SIGNATURE` | Proof of authorship + identity | Schnorr signature (64 bytes) followed by sender's pubkey (32 bytes) |
| `BEGIN NOSTR SEAL` | Identity declaration | Sender's Nostr public key (unsigned messages only) |
| `BEGIN NOSTR ENCRYPTED BODY` | Multi-recipient encrypted content | AES-256-GCM ciphertext under a per-message Content Encryption Key (CEK). Identified as the envelope (CEK) path by the accompanying RECIPIENTS block — there is no distinct keyword |
| `BEGIN NOSTR RECIPIENTS` | Recipient key-wrap + roles | A per-message `ephemeral` pubkey header, then per-recipient NIP-44-wrapped CEK (sealed via the ephemeral key), pubkey, and role |
| `BEGIN NOSTR CONSENT` | Explicit consent marker | A signatory's binding consent to a specific agreement (document hash + signer pubkey); see Section 11.3 |
| `BEGIN NOSTR ATTACHMENTS` | Signed attachment hashes | Public messages only: one line per plaintext attachment (`id`, SHA-256, size, MIME, filename), bound by the signature; see Section 11.2 |
| `END NOSTR MESSAGE` | Closing tag | Terminates the outermost block |
| `END NOSTR SEAL` | Closing tag | Terminates standalone seal blocks |
| `END NOSTR RECIPIENTS` | Closing tag | Terminates a standalone recipients block |
| `END NOSTR CONSENT` | Closing tag | Terminates a standalone consent block |
| `END NOSTR ATTACHMENTS` | Closing tag | Terminates a standalone attachments block |

The encryption type is embedded directly in the BEGIN tag (e.g., `BEGIN NOSTR NIP-44 ENCRYPTED BODY`), keeping the format self-describing without metadata lines.

**No `HYBRID` keyword.** The multi-recipient envelope reuses the generic `BEGIN NOSTR ENCRYPTED BODY` tag (AES-256-GCM under a CEK) rather than a dedicated keyword. This mirrors how the existing attachment manifest rides inside an ordinary encrypted body: a decoder does not need the tag to announce the scheme. The **presence of a RECIPIENTS block** is the unambiguous signal to take the CEK-envelope decryption path (unwrap the CEK, then AES-256-GCM-decrypt) instead of the pairwise NIP-04/NIP-44 path; see Sections 8 and 10.5.

### 2.2 Legacy Tag Names (Backwards Compatibility)

Decoders MUST accept the following legacy tag names:

| Legacy Tag | Equivalent New Tag |
|------------|--------------------|
| `BEGIN NOSTR SIGNED MESSAGE` | `BEGIN NOSTR SIGNED BODY` |
| `BEGIN NOSTR NIP-04 ENCRYPTED MESSAGE` | `BEGIN NOSTR NIP-04 ENCRYPTED BODY` |
| `BEGIN NOSTR NIP-44 ENCRYPTED MESSAGE` | `BEGIN NOSTR NIP-44 ENCRYPTED BODY` |
| `END NOSTR NIP-04 ENCRYPTED MESSAGE` | `END NOSTR MESSAGE` |
| `END NOSTR NIP-44 ENCRYPTED MESSAGE` | `END NOSTR MESSAGE` |

Encoders MUST produce only the new format. Decoders MUST accept both old and new formats via `(?:MESSAGE|BODY)` alternations in regex patterns.

Additionally, decoders MUST accept the legacy format where SIGNATURE and SEAL are separate blocks (i.e., a `BEGIN NOSTR SEAL` block following `BEGIN NOSTR SIGNATURE`). In the new format, the SIGNATURE block contains both signature and pubkey; the SEAL block is only used for unsigned messages.

## 3. Message Formats

### 3.1 Signed + Encrypted

```
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<ciphertext, optionally glossia-encoded>
----- BEGIN NOSTR SIGNATURE -----
@ProfileName
<signature: glossia-encoded or hex (64 bytes)>
<pubkey: glossia-encoded, hex, or npub>
----- END NOSTR MESSAGE -----
```

### 3.2 Signed Plaintext

The armored body contains the glossia-encoded plaintext. Glossia encoding is required (not optional) for signed plaintext, because the signature is computed over the decoded binary bytes. Raw plaintext cannot be reliably round-tripped through email transport (line wrapping, whitespace normalization, quote prefixes), so glossia encoding is the canonical representation that ensures signature verification succeeds regardless of how the message is reformatted in transit.

The original plaintext also appears above the armor block for readability in non-Nostr-Mail clients. Nostr-Mail clients SHOULD display the decoded glossia content from within the armor block rather than the plaintext above it, as the armor content is the verified payload.

```
<plaintext body>

----- BEGIN NOSTR SIGNED BODY -----
<glossia-encoded plaintext>
----- BEGIN NOSTR SIGNATURE -----
@ProfileName
<signature: glossia-encoded or hex (64 bytes)>
<pubkey: glossia-encoded, hex, or npub>
----- END NOSTR MESSAGE -----
```

Signature and pubkey are encoded independently with separate glossia settings. Decoders MUST accept both the new separate format and the legacy combined format (sig + pubkey encoded together as a single 96-byte payload).

### 3.3 Unsigned + Encrypted (with Seal) — NIP-44 only

```
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<ciphertext, optionally glossia-encoded>
----- BEGIN NOSTR SEAL -----
@DisplayName
<pubkey: glossia-encoded, hex, or npub>
----- END NOSTR MESSAGE -----
```

The SEAL block provides the sender's pubkey, which is required for NIP-44 decryption and survives forwarding (unlike MIME headers). Decoders MUST accept glossia-encoded, hex-encoded, and npub (bech32-encoded, `npub1...`) formats for pubkeys in SEAL blocks.

**NIP-04 messages MUST NOT use this format.** Because NIP-04 (AES-256-CBC) lacks authenticated encryption, an unsigned NIP-04 message is vulnerable to ciphertext manipulation (bit-flipping, padding oracle attacks). NIP-04 encrypted messages MUST include a SIGNATURE block (see Section 3.1). See Section 4.1 for rationale.

### 3.4 Unsigned Plaintext (with optional Seal)

Body text is not armored. A standalone seal block may follow:

```
Hello, this is a plaintext message.

----- BEGIN NOSTR SEAL -----
@DisplayName
<pubkey: glossia-encoded, hex, or npub>
----- END NOSTR SEAL -----
```

### 3.5 Reply Format

When replying, the new reply content is encrypted independently, but the reply's signature covers the entire conversation — the reply body bytes concatenated with all nested quoted body bytes (flattened). The original message is nested inside the outer armor block, before the reply's SIGNATURE. Inner signatures are preserved for independent verification of earlier messages. Nesting depth is determined by BEGIN/END tag pairing, not by quote prefixes.

#### 3.5.0 Signature Coverage in Replies

Each signature covers the **flat concatenation** of its own body's decoded bytes plus all nested quoted body bytes, recursively:

```
L1 original signature covers:  decode(L1)
L2 reply signature covers:     decode(L2) || decode(L1)
L3 reply signature covers:     decode(L3) || decode(L2) || decode(L1)
```

Where `decode()` means glossia-decoding (for encoded content) or base64-decoding (for raw ciphertext) the armor body content at that level, and `||` is byte concatenation.

This ensures that each reply author authenticates not just their own message but the entire conversation history they are responding to. A tampered inner message will cause the outer signature to fail verification, providing chain-of-custody integrity.

#### 3.5.1 Encrypted Reply

```
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<reply ciphertext, encrypted independently>
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<original message ciphertext>
----- BEGIN NOSTR SIGNATURE -----
@OriginalAuthor
<original signature (64 bytes)>              ← signs: decode(original)
<original author pubkey (32 bytes)>
----- END NOSTR MESSAGE -----
----- BEGIN NOSTR SIGNATURE -----
@ReplyAuthor
<reply signature (64 bytes)>                 ← signs: decode(reply) || decode(original)
<reply author pubkey (32 bytes)>
----- END NOSTR MESSAGE -----
```

#### 3.5.2 Signed Plaintext Reply

In a signed plaintext reply, the new reply text appears above the armor block for readability. When composing a reply, any text above the outermost armor block in the original message (including the original's plaintext and any previously quoted text) is carried forward as email-quoted lines (prefixed with `> `). This quoted plaintext is informational only — the verifiable content is always inside the armor blocks.

```
<new reply plaintext>

> <previous plaintext, email-quoted>

----- BEGIN NOSTR SIGNED BODY -----
<reply glossia-encoded plaintext>
----- BEGIN NOSTR SIGNED BODY -----
<original glossia-encoded plaintext>
----- BEGIN NOSTR SIGNATURE -----
@OriginalAuthor
<original signature (64 bytes)>              ← signs: decode(original)
<original author pubkey (32 bytes)>
----- END NOSTR MESSAGE -----
----- BEGIN NOSTR SIGNATURE -----
@ReplyAuthor
<reply signature (64 bytes)>                 ← signs: decode(reply) || decode(original)
<reply author pubkey (32 bytes)>
----- END NOSTR MESSAGE -----
```

#### 3.5.3 Reply Chains

For reply chains, nesting increases with each level. Signatures close in innermost-first order. Each signature covers the flat concatenation of all body bytes from its level inward:

```
----- BEGIN NOSTR ... -----
<L3 reply body>
----- BEGIN NOSTR ... -----
<L2 reply body>
----- BEGIN NOSTR ... -----
<L1 original body>
----- BEGIN NOSTR SIGNATURE -----
@L1Author                            ← signs: decode(L1)
...
----- END NOSTR MESSAGE -----
----- BEGIN NOSTR SIGNATURE -----
@L2Author                            ← signs: decode(L2) || decode(L1)
...
----- END NOSTR MESSAGE -----
----- BEGIN NOSTR SIGNATURE -----
@L3Author                            ← signs: decode(L3) || decode(L2) || decode(L1)
...
----- END NOSTR MESSAGE -----
```

#### 3.5.4 Quote Prefix Handling

Decoders MUST also accept armor block delimiters preceded by `> ` quote prefixes, since email clients may add quote prefixes when forwarding or replying. Glossia decoders naturally ignore quote prefixes as non-payload words.

### 3.6 Multi-Recipient (Group-Encrypted)

When a message has more than one cryptographic recipient — for example multiple `To:` signatories and/or `Cc:` viewers — the body cannot be encrypted with a single pairwise NIP-44 shared secret, because each recipient derives a different secret. Instead the body is encrypted **once** under a random Content Encryption Key (CEK), and the CEK is wrapped to each recipient with NIP-44. This is the same hybrid construction already used for attachments (AES-256 for payload, NIP-44 for the key), generalized to N recipients. The envelope mechanics are normative in Section 10.

```
----- BEGIN NOSTR ENCRYPTED BODY -----
<AES-256-GCM ciphertext under CEK, glossia-encoded (see note below)>
----- BEGIN NOSTR RECIPIENTS -----
ephemeral <ephemeral-pubkey>
signer <pubkey-1> <CEK wrapped via ephemeral key>
signer <pubkey-2> <CEK wrapped via ephemeral key>
viewer <pubkey-3> <CEK wrapped via ephemeral key>
self   <sender-pubkey> <CEK wrapped via ephemeral key>
----- BEGIN NOSTR SIGNATURE -----
@ProfileName
<signature: glossia-encoded or hex (64 bytes)>
<pubkey: glossia-encoded, hex, or npub>
----- END NOSTR MESSAGE -----
```

**Body encoding — glossia, not base64.** The body ciphertext MUST be glossia-encoded (Section 5), not base64. This is not merely steganographic: an agreement is a signed reply chain, and when a counterparty's email client quotes the prior message it prefixes lines with `> ` and re-wraps them. base64 is corrupted by those mutations, so the decoded bytes — and therefore every nested signature over them — would change and fail to verify. Glossia's word tokens survive quoting, wrapping, and reflow (decoders ignore `> ` and non-payload whitespace, Section 3.5.4), so the canonical decoded bytes are recovered intact. This is why email-native document signing cannot use base64 for the signed payload. (Large bodies/attachments still ride the base64 manifest path, which is not part of the signed reply chain.)

Each RECIPIENTS entry is a single line of three or four space-separated tokens: `<role> <pubkey> [<wrapped-cek>] [<email>]` (see Section 10.2). The sender's own `self` stanza makes the Sent copy decryptable on any device, mirroring the "wrap twice" behavior of NIP-17 DMs.

A multi-recipient message SHOULD be signed; the SIGNATURE then covers both the body and the recipients block (Section 4.2), making the membership and role set tamper-evident. An unsigned multi-recipient message MAY instead carry a SEAL block. The SEAL declares the sender's **identity** pubkey — the "family seal"/signet attesting authorship — not a key-unwrap input: with ephemeral wrapping the CEK is unwrapped via the RECIPIENTS `ephemeral` pubkey (Section 10.1), and the SEAL pubkey is the unwrap counterparty only for legacy envelopes that lack an `ephemeral` line. Being unsigned, the role set is unauthenticated and MUST NOT be relied upon to designate required signatories.

#### 3.6.1 Multi-Recipient Reply

Each encrypted level in a reply chain has its own independent CEK, and therefore its own RECIPIENTS block. The recipients block is **not** inherited from an enclosing or enclosed level. This both is cryptographically required (a single block cannot hand out different per-level CEKs) and records the recipient/role set as it stood at each point in the thread.

```
----- BEGIN NOSTR ENCRYPTED BODY -----
<reply ciphertext under CEK_reply>
----- BEGIN NOSTR RECIPIENTS -----
ephemeral <reply-ephemeral-pubkey>
signer <pubkey-1> <CEK_reply wrapped to pubkey-1>
signer <pubkey-2> <CEK_reply wrapped to pubkey-2>
self   <reply-author-pubkey> <CEK_reply wrapped to self>
----- BEGIN NOSTR ENCRYPTED BODY -----
<original ciphertext under CEK_orig>
----- BEGIN NOSTR RECIPIENTS -----
ephemeral <original-ephemeral-pubkey>
signer <pubkey-1> <CEK_orig wrapped to pubkey-1>
signer <pubkey-2> <CEK_orig wrapped to pubkey-2>
self   <original-author-pubkey> <CEK_orig wrapped to self>
----- BEGIN NOSTR SIGNATURE -----
@OriginalAuthor
<original signature (64 bytes)>              ← signs: decode(original) || recipients(original)
<original author pubkey (32 bytes)>
----- END NOSTR MESSAGE -----
----- BEGIN NOSTR SIGNATURE -----
@ReplyAuthor
<reply signature (64 bytes)>                 ← signs: level(reply) || level(original)
<reply author pubkey (32 bytes)>
----- END NOSTR MESSAGE -----
```

Single-recipient messages are unaffected and continue to use the pairwise `BEGIN NOSTR NIP-44 ENCRYPTED BODY` format with no RECIPIENTS block (Section 10.5).

## 4. Composable Signing Model

Signing is user-controlled for NIP-44 messages and can be applied at any stage to the current body bytes. **For NIP-04 messages, signing is mandatory** — encoders MUST produce a SIGNATURE block, and decoders MUST reject NIP-04 messages without one (see Section 4.1).

| Operation | What gets signed | Result |
|-----------|-----------------|--------|
| Sign plaintext | `SHA-256(decode(armor_body) \|\| decode(quoted_bodies))` | Proves authorship of content + conversation history |
| Sign then encrypt | Signature inside ciphertext | Only recipient can verify |
| Encrypt then sign | `SHA-256(decode(armor_body) \|\| decode(quoted_bodies))` | Proves sender without revealing content |
| Sign, encrypt, sign | Both layers | Full trust chain |

The signing target is the **flat concatenation** of the decoded bytes from the current armor body and all nested quoted armor bodies (recursively). For glossia-encoded content, decoding means transcoding the prose back to binary. For base64 content, it means base64-decoding. The signature is never on the raw plaintext or the encoded prose itself — it is on the canonical decoded bytes, which survive transport regardless of reformatting.

For messages with no quoted content, this reduces to `SHA-256(decode(armor_body))`. For replies, the concatenation provides chain-of-custody integrity — modifying any message in the conversation history invalidates all subsequent signatures.

### 4.1 Mandatory Signing for NIP-04

NIP-04 uses AES-256-CBC without a MAC (unauthenticated encryption). Without an external integrity check, NIP-04 ciphertext is vulnerable to:

- **Padding oracle attacks**: An attacker who can modify the ciphertext and observe whether decryption produces valid or invalid PKCS7 padding can recover the plaintext one byte at a time — without knowing the key.
- **Bit-flipping**: In CBC mode, flipping bits in ciphertext block *N* produces predictable changes to plaintext block *N+1*, allowing targeted message tampering.

The Schnorr signature over `SHA-256(ciphertext_bytes)` serves as the authentication mechanism that NIP-04 itself lacks. This converts NIP-04 into an Encrypt-then-Sign (EtS) scheme, binding the ciphertext to the sender's identity and preventing tampering.

**Compose-time requirement**: Encoders MUST produce a SIGNATURE block for all NIP-04 encrypted messages. A compose UI MUST either auto-sign NIP-04 messages or prevent sending unsigned NIP-04 messages.

**Decode-time requirement**: Decoders MUST verify the signature **before** attempting NIP-04 decryption. If the signature is missing or invalid, the decoder MUST reject the message without decrypting. This verify-then-decrypt ordering is critical — if decryption errors (including padding errors) are surfaced before signature verification, the padding oracle window remains open.

NIP-44 messages are exempt from this requirement because NIP-44 uses ChaCha20 (no padding) with HMAC-SHA256 authentication built in.

### 4.2 Signature Coverage of the Recipients, Consent & Attachments Blocks

For multi-recipient messages (Section 3.6), the signature MUST cover the RECIPIENTS block — and, when present, the CONSENT block (Section 11.3) and the ATTACHMENTS block (Section 11.2) — in addition to the body, so that the membership list, per-recipient roles, any declared consent, and any attached document hashes cannot be altered without invalidating the signature. The per-level signing contribution becomes:

```
level(L) = decode(body_L) || canonical(recipients_L) || canonical(consent_L) || canonical(attachments_L)
```

where `decode(body_L)` is the level's decoded body bytes (as in Section 4); `canonical(recipients_L)` is the **canonicalized recipients block**: the lines between `BEGIN NOSTR RECIPIENTS` and the following delimiter, each stripped of trailing whitespace and any `> ` quote prefix, joined with `\n`, with no trailing newline; `canonical(consent_L)` is the **canonicalized consent block**, formed by the identical rule over the lines between `BEGIN NOSTR CONSENT` and the following delimiter; and `canonical(attachments_L)` is the **canonicalized attachments block**, formed by the identical rule over the lines between `BEGIN NOSTR ATTACHMENTS` and the following delimiter. The blocks are concatenated in the fixed order **body, then recipients, then consent, then attachments**. If a level lacks any of these blocks, the corresponding canonical string is empty; a level with none reduces to the Section 4 model. (The ATTACHMENTS block is only used by **public** messages — an encrypted message binds its attachments via the in-body manifest instead; Section 11.2.)

The signing target for a level (and its nested levels) is then:

```
SHA-256( level(L) || level(L-1) || … || level(1) )
```

This preserves the flat-concatenation, chain-of-custody property of Section 3.5: tampering with any level's body, its recipient/role set, **or its declared consent** invalidates that level's signature and every signature above it.

## 5. Content Encoding (Glossia)

Body content, subjects, signatures, and pubkeys may be encoded using the Glossia steganographic encoding system. Each field has an independent encoding setting:

- **Body**: Glossia prose encoding (e.g., Latin, BIP39, or hex)
- **Subject**: Independent encoding setting (defaults to the body's when unset)
- **Signature**: Independent encoding setting
- **Pubkey**: Independent encoding setting

Encoding is transparent to the protocol — decoders detect the encoding format automatically (base64 vs glossia word patterns) and decode accordingly.

### 5.1 NIP-04 Bitpacking

NIP-04 ciphertext (`base64?iv=base64`) is bitpacked into a compact binary format before Glossia encoding, and unpacked after decoding. NIP-44 ciphertext (pure base64) passes through unchanged.

## 6. MIME Structure

```
multipart/alternative
  |-- text/plain   <-- source of truth (armor blocks)
  +-- text/html    <-- rendering aid only
```

### 6.1 MIME Headers

The following custom MIME headers are included for backwards compatibility and fast IMAP filtering:

| Header | Value | Purpose |
|--------|-------|---------|
| `X-Nostr-Pubkey` | hex or npub (bech32) pubkey | Sender identification |
| `X-Nostr-Sig` | hex-encoded signature | Message authentication |
| `X-Nostr-Agreement` | identifier (optional) | Marks an agreement/signing thread for fast IMAP filtering (Section 11.2); non-authoritative |

Decoders MUST accept both hex-encoded and npub (bech32-encoded, `npub1...`) formats for `X-Nostr-Pubkey`. Encoders MAY produce either format.

**Primary trust path**: In-body SIGNATURE blocks (signed) or SEAL blocks (unsigned) (survive forwarding, quoting, and re-encoding).

**Secondary trust path**: X-Nostr-* MIME headers (for fast IMAP filtering and older client compatibility).

### 6.2 HTML Rendering

The `text/html` part is a rendering aid for human readability. It mirrors the structure of the armor blocks in `text/plain` but presents decoded, readable content. Clients that understand Nostr-Mail SHOULD generate the HTML part according to the following rules.

#### 6.2.1 General Structure

Each armor block maps to a `<div>` in the HTML. Nested armor blocks (from replies) map to nested `<blockquote>` elements. Signature blocks are rendered as labeled sections below their associated body.

```html
<div>
  <div>{body content}</div>
  <blockquote>
    {nested message HTML, recursively structured}
  </blockquote>
  <hr>
  <h4>{signature author}</h4>
  <div>{encoded signature + pubkey}</div>
</div>
```

#### 6.2.2 Body Content

- **Signed plaintext**: The body `<div>` contains the decoded glossia plaintext (human-readable text), so that non-Nostr-Mail clients can display the message content directly.
- **Encrypted**: The body `<div>` contains the glossia-encoded ciphertext (since the plaintext is not available without decryption).

#### 6.2.3 Reply Threading

For replies, the outermost body `<div>` MUST contain only the new reply content — not the quoted text from previous messages. Previous messages appear as nested `<blockquote>` elements, each containing their own decoded body content and signature sections. This ensures the first visible content is the new reply, with conversation history indented below.

```html
<div>
  <!-- Outermost: only the new reply text -->
  <div>{L3 new reply plaintext}</div>
  <blockquote>
    <!-- L2 previous reply -->
    <div>{L2 decoded plaintext}</div>
    <blockquote>
      <!-- L1 original message -->
      <div>{L1 decoded plaintext}</div>
      <hr>
      <h4>{L1 author}</h4>
      <div>{L1 encoded signature + pubkey}</div>
    </blockquote>
    <hr>
    <h4>{L2 author}</h4>
    <div>{L2 encoded signature + pubkey}</div>
  </blockquote>
  <hr>
  <h4>{L3 author}</h4>
  <div>{L3 encoded signature + pubkey}</div>
</div>
```

#### 6.2.4 Signature Display

Each signature section consists of:
1. A horizontal rule (`<hr>`) separator
2. A heading (`<h4>`) with the author's profile name (from the `@ProfileName` line in the SIGNATURE block)
3. A `<div>` containing the glossia-encoded signature and pubkey bytes (preserving the encoded form, not decoded)

Seal blocks (for unsigned messages) are rendered similarly, with the display name and encoded pubkey.

#### 6.2.5 Inline Styles

Since email clients strip `<style>` tags and external stylesheets, all styling MUST use inline `style` attributes. Recommended styles:

- **Blockquote**: `border-left: 2px solid #ccc; margin: 1em 0; padding: 0 1em;`
- **Signature div**: `border-left: 2px solid #ccc; padding-left: 1em; color: #888; font-style: italic; overflow-wrap: break-word;`
- **Signature heading**: `margin: 0 0 0.5em; color: #666; font-size: 0.9em;`
- **HR separator**: `border: none; border-top: 1px solid #ccc; margin: 1.5em 0;`

#### 6.2.6 Agreement & Consent Rendering

When a thread is an agreement — any level carries a CONSENT block (Section 11.3), or the `X-Nostr-Agreement` header is set — the HTML SHOULD make the **reply-vs-consent distinction visible**, so a reader can tell a binding signature apart from an ordinary comment at a glance. These are rendering aids only: a Nostr-Mail client MUST recompute completion from the verified armor (Section 11.5), never from the rendered HTML.

1. **Status banner.** At the top of the outermost `<div>`, render `M of N signatories signed` plus the short document hash `H` (Section 11.3.1). `M` and `N` come from verified CONSENT blocks intersected with the declared signatory set, never from the header.
2. **Consent section.** A level whose SIGNATURE is accompanied by a CONSENT block renders its signature section as a **"✓ Signed by @Name — consents to document H"** panel, visually distinct (green accent) from a plain signature.
3. **Plain reply / comment.** A level with a SIGNATURE but **no** CONSENT block renders the normal muted §6.2.4 signature section, labeled with the author's role (e.g. `viewer · comment`). It MUST NOT show a consent badge.
4. **Don't rely on colour alone.** Use the literal `✓ Signed` / `comment` text and the role token alongside any colour, so the distinction survives in clients that strip styling and for accessibility. Show the short `H` inline and the full `H` on hover/`title`, so two different documents cannot look identical at a glance.

The following renders an agreement that @Alice originated and signed, @Bob counter-signed, and @Carol (a `viewer`) replied to with a comment. Newest content is outermost, per Section 6.2.3:

```html
<div>
  <!-- Status banner -->
  <div style="border: 1px solid #cfe8cf; border-left: 4px solid #4caf50;
              padding: 0.5em 0.75em; color: #2e7d32; font-size: 0.9em; margin: 0 0 1em;">
    ✓ Agreement complete — 2 of 2 signatories signed
    <div style="color: #666; font-size: 0.85em;" title="{full H}">Document H: a1b2c3…9f</div>
  </div>

  <!-- L3: Carol's new comment (outermost = newest) -->
  <div>Looks good to me — no changes needed on my end.</div>

  <blockquote style="border-left: 2px solid #ccc; margin: 1em 0; padding: 0 1em;">
    <!-- L2: Bob -->
    <div>I agree to the terms as written.</div>

    <blockquote style="border-left: 2px solid #ccc; margin: 1em 0; padding: 0 1em;">
      <!-- L1: Alice's original terms -->
      <div>This Mutual NDA is entered into as of 2026-06-13 between …</div>

      <!-- Alice's CONSENT + SIGNATURE → consent panel -->
      <hr style="border: none; border-top: 1px solid #ccc; margin: 1.5em 0;">
      <h4 style="margin: 0 0 0.5em; color: #2e7d32; font-size: 0.9em;">✓ Signed by @Alice</h4>
      <div style="color: #2e7d32; font-size: 0.85em;" title="{full H}">consents to document a1b2c3…9f</div>
      <div style="border-left: 2px solid #4caf50; padding-left: 1em; color: #888;
                  font-style: italic; overflow-wrap: break-word;">{Alice encoded sig + pubkey}</div>
    </blockquote>

    <!-- Bob's CONSENT + SIGNATURE → consent panel -->
    <hr style="border: none; border-top: 1px solid #ccc; margin: 1.5em 0;">
    <h4 style="margin: 0 0 0.5em; color: #2e7d32; font-size: 0.9em;">✓ Signed by @Bob</h4>
    <div style="color: #2e7d32; font-size: 0.85em;" title="{full H}">consents to document a1b2c3…9f</div>
    <div style="border-left: 2px solid #4caf50; padding-left: 1em; color: #888;
                font-style: italic; overflow-wrap: break-word;">{Bob encoded sig + pubkey}</div>
  </blockquote>

  <!-- Carol's SIGNATURE, NO CONSENT → plain comment (muted, not green) -->
  <hr style="border: none; border-top: 1px solid #ccc; margin: 1.5em 0;">
  <h4 style="margin: 0 0 0.5em; color: #666; font-size: 0.9em;">@Carol · viewer · comment</h4>
  <div style="border-left: 2px solid #ccc; padding-left: 1em; color: #888;
              font-style: italic; overflow-wrap: break-word;">{Carol encoded sig + pubkey}</div>
</div>
```

Recommended additional inline styles:

- **Status banner**: `border: 1px solid #cfe8cf; border-left: 4px solid #4caf50; padding: 0.5em 0.75em; color: #2e7d32; font-size: 0.9em;` (use a neutral/amber accent — e.g. `#e0a800` — while an agreement is still `M of N` with `M < N`).
- **Consent heading**: `margin: 0 0 0.5em; color: #2e7d32; font-size: 0.9em;`
- **Consent signature div**: same as the §6.2.5 signature div but with a green left border: `border-left: 2px solid #4caf50;`

### 6.3 Transport Recipients (To / Cc)

The email `To:` and `Cc:` headers carry the transport recipients. For multi-recipient messages, nostr-mail assigns a default cryptographic role from the header a recipient appears in:

| Header | Default role | Meaning |
|--------|--------------|---------|
| `To:` | `signer` | Participant expected to sign (a signatory) |
| `Cc:` | `viewer` | Participant granted read access but not expected to sign |

Both `To:` and `Cc:` recipients receive a NIP-44-wrapped CEK and can decrypt the body — the distinction is one of **role, not access** (a viewer can read the agreement, just isn't asked to sign it). Encoders MUST include a wrapped-CEK stanza in the RECIPIENTS block for every `To:` and `Cc:` recipient (plus a `self` stanza for the sender).

Because email headers are spoofable and may be rewritten on forward (Section 7), the authoritative role for each recipient is the `role` token inside the signed RECIPIENTS block, **not** the header. The headers are a transport convenience and a mirror for non-Nostr-Mail clients. Where the two disagree, decoders MUST trust the signed RECIPIENTS block.

To make the `(pubkey, email)` pairing itself authenticated rather than inferred from the spoofable header order, an encoder MAY include the delivered address as the optional fourth `email` token of each stanza (Section 10.2). Because the signature covers the canonicalized RECIPIENTS block (Section 10.6), an in-block `email` binds that address to its `pubkey` tamper-evidently — this is what the email↔npub binding handshake (issue #102) relies on. When a stanza carries an `email`, decoders MUST treat the in-block value, not the header, as the authoritative pairing.

`Bcc:` recipients, if supported, MUST NOT appear in the RECIPIENTS block of the copy sent to other recipients (doing so would disclose the blind recipient); each Bcc recipient instead receives a separately addressed copy.

## 7. Identity Model

| Layer | Source | Trust Level |
|-------|--------|-------------|
| Transport sender | Email `From:` header | None (spoofable) |
| Cryptographic author | SIGNATURE block (pubkey) or SEAL block (npub) | Verified via signature (if signed) |
| Client attribution | `X-Mailer` header | Informational only |

## 8. Decoder Algorithm

1. **Scan** the `text/plain` body for armor block delimiters (`-{3,}\s*BEGIN NOSTR`)
2. **Normalize** whitespace and line endings (`\r\n` -> `\n`)
3. **Parse** the BEGIN tag to determine encryption type (NIP-04, NIP-44, or signed plaintext)
4. **Detect** content encoding: base64 (no spaces) vs Glossia (word patterns)
5. **Decode** content: base64 decode or Glossia transcode -> bytes
6. **Unpack** NIP-04 if applicable (bitpacked binary -> `base64?iv=base64`)
7. **Verify** signatures against the per-level signing target using the pubkey from the SIGNATURE block
   - Single-recipient / plaintext: `SHA-256(decoded_body_bytes)` (Section 4)
   - Multi-recipient (RECIPIENTS block present): include the canonicalized recipients block per Section 4.2
   - For NIP-04: signature MUST be present and MUST verify. If the signature is missing or invalid, the decoder MUST reject the message **without proceeding to step 8** (see Section 4.1)
   - For NIP-44 / CEK-envelope: signature verification is recommended but not required (NIP-44 and AES-256-GCM provide their own authentication via HMAC / GCM tag)
8. **Decrypt** if encrypted. The path is selected by the **presence of a RECIPIENTS block**, not by a distinct body tag:
   - **Pairwise** (`NIP-04`/`NIP-44 ENCRYPTED BODY`, **no** RECIPIENTS block): use the recipient's private key and the sender's pubkey
   - **Multi-recipient** (generic `ENCRYPTED BODY` **with** a RECIPIENTS block): locate the stanza whose pubkey matches the reader's own pubkey, NIP-44-unwrap the CEK using the reader's private key and the message's **ephemeral pubkey** (the `ephemeral` line; falling back to the sender's pubkey for legacy envelopes — Section 10.1), then AES-256-GCM-decrypt the body with the CEK. If no stanza matches the reader's pubkey, the reader is not a recipient of that level and cannot decrypt it (this is expected for levels predating the reader's addition to the thread — see Section 10.8)

## 9. Spam Rescue & Folder Handling

Providers routinely misclassify Nostr-encrypted mail as spam (opaque bodies, armor blocks). Spam rescue is a client behavior that recovers it. Spam/junk/bulk folders are **never** part of the inbox folder selection; how their contents are handled is decided per sync by the `spam_rescue` setting (default ON):

| `spam_rescue` | Spam folders scanned? | Behavior |
|---------------|-----------------------|----------|
| **ON** | No | Authenticated Nostr mail is **moved** out of spam into the rescue target folder (default `nostr-mail`), which is part of the synced set, so it surfaces in the inbox. Spam stays out of the inbox's read-state (`\Seen`) bookkeeping. |
| **OFF** | Yes | Spam folders are scanned so misfiled Nostr mail still **appears** in the inbox, but nothing is moved — it stays in spam and is eventually auto-purged by the provider. |

### 9.1 Rescue eligibility (stateless)

A message in a spam folder is rescued **⟺** it is `UNSEEN` **AND** is authenticated Nostr mail — it carries a Nostr marker (`X-Nostr-Pubkey`/`X-Nostr-Sig` header, or a `BEGIN NOSTR …` armor block) **and** passes transport authentication (SPF/DKIM/alignment). Transport-failing mail is left in spam, matching the inbox's existing enforcement.

The rescue decision is derived entirely from server state on every sync, so all devices compute the same answer. There is **no rescue-once ledger.**

### 9.2 `\Seen` as the "leave it in spam" signal

Intent is carried by the `\Seen` flag, which the IMAP server replicates to every client. The `UNSEEN` guard means a message the user has read **and deliberately filed into spam** is left alone. This is trustworthy because nothing automated ever sets `\Seen` on spam mail:

- All IMAP fetches use `BODY.PEEK[]`, so reading a body never sets `\Seen`.
- Read-state sync (`mark_inbox_email_seen_on_server`) only sets `\Seen` in non-spam inbox folders.
- The **only** code path that sets `\Seen` within a spam folder is a deliberate move-to-spam (`move_message_to_folder`), which marks the message `\Seen` *before* moving it.

### 9.3 On-enable catch-up

When the user first switches spam rescue ON, a one-time catch-up (`rescue_spam_now`) runs the rescue with the `UNSEEN` guard **dropped**, so already-read Nostr mail sitting in spam — which the per-sync rescue intentionally skips — is swept out too. The user is told how many messages were moved.

### 9.4 Future work: a first-class "mark as spam" action

There is currently **no dedicated "mark as spam" UI.** A message can only reach a spam folder via the generic *Move to folder* picker, which already routes through `move_message_to_folder` and therefore sets `\Seen` on a spam target. When a dedicated mark-as-spam action is added:

- Route it through the same `move_message_to_folder` → spam-folder path so it sets `\Seen`; the per-sync rescue (§9.2) will then automatically respect it as a "leave it" signal with no further changes.
- Reconsider the on-enable catch-up (§9.3): with a real spam-filing action, the catch-up's `unseen_only = false` would *undo* a deliberate spam-filing. At that point the catch-up should likely honor `\Seen` (keep `unseen_only = true`) so it only sweeps provider-misclassified mail, not user-filed spam.

## 10. Multi-Recipient Encryption (Group Encryption)

NIP-44 is pairwise: a ciphertext encrypted with the shared secret of `(senderPriv, recipientPub)` can only be opened by that one recipient (and the sender). To deliver one message to N recipients, nostr-mail uses **envelope encryption** — the same hybrid construction already used for attachments (random AES-256 key for the payload, NIP-44 to wrap the key), generalized from one recipient to many.

### 10.1 Envelope Scheme

```
1. Generate a random 256-bit Content Encryption Key (CEK).
2. Encrypt the body ONCE with AES-256-GCM under the CEK            → one ENCRYPTED BODY
   (attachments are encrypted under the same CEK, as today).
3. Generate a per-message ephemeral keypair (ephPriv, ephPub).    → published as the
   The ephemeral private key is used only for the wraps below,        RECIPIENTS `ephemeral` line
   then discarded.
4. For each pubkey P in { To ∪ Cc ∪ sender-self }:
       wrapped[P] = NIP44_encrypt(CEK, ephPriv, P)                → one RECIPIENTS stanza per P
5. Emit: [ENCRYPTED BODY] + [RECIPIENTS block] + optional [SIGNATURE]
```

**Ephemeral key wrapping (AGE-style).** The CEK is wrapped against a fresh **per-message ephemeral key**, not the sender's identity key. The ephemeral public key is published as the RECIPIENTS block's `ephemeral <pubkey>` header line (Section 10.2); each `wrapped-cek` is `NIP44_encrypt(CEK, ephPriv, recipientPub)`. This keeps the encryption layer separate from the signing/identity key: the long-term secret never performs the CEK ECDH, the ECDH is not reused across messages, and an unsigned envelope (SEAL) discloses no sender-derived key material in its wraps. Because the ephemeral line lives inside the RECIPIENTS block, it is covered by the level's SIGNATURE along with the recipient set.

To read the message, a recipient locates the stanza addressed to their own pubkey, NIP-44-unwraps the CEK with **their private key and the message's ephemeral public key** (from the `ephemeral` line), then AES-256-GCM-decrypts the body. The body ciphertext is produced once regardless of recipient count; only the 32-byte CEK is wrapped per recipient.

For backward compatibility, a decoder that finds no `ephemeral` line MUST fall back to unwrapping against the **sender's identity pubkey** (legacy envelopes wrapped the CEK directly to the sender's key). The sender's pubkey is taken from the SIGNATURE block, the SEAL block, or the `X-Nostr-Pubkey` header — a multi-recipient message MUST therefore carry one of these (the sender's pubkey also remains the unwrap counterparty for legacy envelopes).

### 10.2 RECIPIENTS Block Format

The RECIPIENTS block opens with an optional `ephemeral <pubkey>` header line (Section 10.1) and then contains one recipient entry per line. Each recipient entry begins with the two required tokens `role` and `pubkey`, followed by any subset of the **optional, content-typed** tokens below, in any order:

```
ephemeral <pubkey>
<role> <pubkey> [<wrapped-cek>] [<email>] [<reference>]
```

The `ephemeral` line is **not** a recipient stanza: it publishes the per-message ephemeral public key against which every `wrapped-cek` in the block was sealed. Decoders MUST recognize a line whose first token is `ephemeral` as this header (its second token is the ephemeral pubkey) and MUST NOT treat it as a recipient. It SHOULD be the first non-blank line of the block. When absent, decoders fall back to the sender's identity pubkey as the unwrap counterparty (Section 10.1).

| Field | Encoding | Notes |
|-------|----------|-------|
| `role` | `signer` \| `viewer` \| `self` (lowercase token) | See Section 11.1. Unknown roles MUST be ignored for workflow purposes but still treated as recipients for decryption. |
| `pubkey` | hex (64 chars) or npub (bech32, `npub1…`) | The recipient's Nostr public key. Glossia is **not** used here, to keep entries single-token and line-parseable. |
| `wrapped-cek` (optional) | base64 (NIP-44 payload) | `NIP44_encrypt(CEK)` to `pubkey`. **Absent** when the CEK is not carried in the email: a plaintext (public) agreement has no CEK at all (Section 11.8), and gift-wrap mode delivers it in the referenced DM (Section 11.7). |
| `email` (optional) | RFC 5322 addr-spec, no display name | The address this stanza was delivered to. Binds the recipient's `(pubkey, email)` pairing **inside** the signed block (Section 10.6), so it cannot be altered or re-paired without breaking the signature — the prerequisite for the email↔npub binding handshake (issue #102). |
| `reference` (optional) | `scheme:value` | A pointer to out-of-band per-recipient material — e.g. `evt:<gift-wrap-event-id>` for a NIP-59 gift-wrapped DM carrying this recipient's CEK and/or `(npub, email)` binding (Section 11.7). |

**Tokens are typed by content, not position.** A decoder classifies each token after `pubkey` by inspection: a token containing `@` is the `email`; a token containing `:` is a `reference`; any other token is the base64 `wrapped-cek` (which never contains `@` or `:`). This makes every field independently optional and order-independent, with no sentinel or positional ambiguity. The first token of each type wins.

Rules:

- Entries SHOULD be ordered deterministically: `To:` recipients in header order, then `Cc:` recipients in header order, then the `self` stanza last. Within a stanza, encoders SHOULD emit fields in `wrapped-cek`, `email`, `reference` order. Deterministic ordering keeps the signed canonical form stable across encoders.
- Display names are intentionally **omitted**; clients resolve names from the Nostr social registry / profile cache by pubkey.
- A stanza with neither a `wrapped-cek` nor a `reference` is workflow-only (it declares a `pubkey` and `role` but carries no key material) — valid for a plaintext agreement or a participant known by pubkey alone.
- Decoders MUST tolerate additional unrecognized tokens on a line (forward compatibility) and MUST ignore blank lines and `> ` quote prefixes. Future positional extensions SHOULD use a distinguishable `key:value` or `key=value` shape so they are never mistaken for a bare `wrapped-cek`.

### 10.3 Roles

`signer` and `viewer` are the workflow roles (Section 11). `self` is the sender's own wrap, present so the Sent copy is decryptable on any of the sender's devices — it is neither a signatory nor a viewer and MUST be excluded from signatory-completion accounting.

### 10.4 Self-Stanza

Every multi-recipient message MUST include a `self` stanza wrapping the CEK to the sender's own pubkey (`NIP44_encrypt(CEK, ephPriv, senderPub)` — the same ephemeral wrap as every other stanza; Section 10.1). This mirrors the "wrap twice — once to the recipient, once to yourself" behavior of NIP-17 DMs and is what makes sent agreements readable after a fresh install or on a second device.

The `self` role is an **accounting marker**, distinct from which pubkey appears in the stanza. The stanza's `pubkey` is the sender's own identity key (the unwrap target, so the sender can recover the CEK from the ephemeral pubkey), but the role is `self` precisely so the sender is **excluded** from signatory / required-signer / completion accounting (Sections 10.3, 11.5). Labeling the sender's own stanza `signer` or `viewer` would wrongly count the sender as a counterparty to their own agreement; `self` must therefore be used even though the pubkey is the sender's real identity.

### 10.5 Single-Recipient Gating & Backward Compatibility

The envelope format is used **only** when a message has more than one cryptographic recipient (i.e. more than one of `To ∪ Cc`, excluding `self`). A message to a single recipient continues to use the pairwise `BEGIN NOSTR NIP-44 ENCRYPTED BODY` format with no RECIPIENTS block, so existing decoders are unaffected.

- Encoders MUST emit the pairwise format for single-recipient messages and the envelope format (generic `ENCRYPTED BODY` + RECIPIENTS block) for multi-recipient messages.
- Decoders MUST select the decryption path by the **presence of a RECIPIENTS block**: a RECIPIENTS block present ⇒ envelope (CEK) path (Section 8 step 8, multi-recipient); absent ⇒ pairwise path keyed on the `NIP-44`/`NIP-04` body tag. There is no `HYBRID` keyword.
- NIP-04 MUST NOT be used as the body cipher for multi-recipient messages (the body cipher is always AES-256-GCM under the CEK, which is authenticated; see Section 4.1 for why unauthenticated CBC is disallowed).

### 10.6 Signature Coverage

When signed, the signature covers the body **and** the canonicalized RECIPIENTS block, per Section 4.2. This authenticates the membership list and each recipient's role, which is required for the agreement workflow (a `viewer` must not be silently re-labeled a `signer`, nor a signatory removed, without breaking the signature).

### 10.7 Replies: Per-Level Recipients

Each encrypted level in a reply chain is encrypted independently and therefore has its **own CEK and its own RECIPIENTS block** (Section 3.6.1). The recipients block is never inherited across levels — it cannot be, since one block cannot distribute multiple per-level CEKs. The common case (reply-all to the same parties) simply repeats the same membership at each level; the per-recipient overhead is one ~100-byte stanza per recipient per level, which is negligible against the body and preserves the property that every level is independently decryptable and verifiable.

### 10.8 Access-Control Consequences

Because each level is sealed under its own CEK wrapped only to that level's listed recipients:

- A reader can decrypt a level **iff** they hold a stanza in that level's RECIPIENTS block.
- Adding a participant at reply *k* grants them access to level *k* **and forward only**. They cannot read levels `< k`, because the replier does not hold the earlier CEKs and so cannot re-wrap them. This is a deliberate, desirable property: a party added late cannot be silently back-doored into earlier private deliberations.
- To intentionally share history with a newly added participant, the replier (who *can* decrypt the levels they received) MAY re-wrap each historical CEK to the new pubkey and add the corresponding stanzas — this MUST be an explicit "include history" action, never automatic.
- Encoders MUST NOT re-encrypt nested historical bodies under a new CEK, as that would discard the original per-level ciphertext and its independent signature, breaking chain-of-custody verification.

### 10.9 Privacy Considerations

The RECIPIENTS block lists each participant's pubkey in the (cleartext) `text/plain` body, so relays/mail servers that see the message learn the participant set. When the optional `email` token (Section 10.2) is included, the block also restates the addresses — but those already appear in the `To:`/`Cc:` headers, so this discloses nothing new to the transport. Overall this is no worse than the `To:`/`Cc:` headers, which already expose the email addresses, but it is strictly less private than NIP-59 gift wrap, which hides recipients behind an ephemeral key. The metadata-private mode noted below would omit both the pubkey labels and the `email` tokens. A future version MAY define a metadata-private mode that omits pubkey labels and requires readers to trial-decrypt each stanza (as the `age` format does); this is out of scope for v0.4.

A planned future direction is a **Nostr-only agreement transport** that folds in SIGit's privacy model (Section 11.7): instead of carrying the agreement in the email body, the message body and document(s) would be sealed and NIP-59 gift-wrapped to each counterparty (optionally with large files on Blossom), hiding the participant set behind ephemeral keys. The consent/chain-of-custody semantics defined here (the document hash `H`, per-signatory consent, and signature chaining) are transport-independent and would carry over unchanged; only the envelope and recipient-addressing would differ. This is out of scope for v0.4 and noted here so the consent model is not specialized to the cleartext email envelope.

## 11. Recipient Roles & Agreement Workflow

This section defines how multi-recipient messages support DocuSign-style agreements: a document distributed to signatories and viewers, signed in rounds, and independently verifiable from the resulting email thread.

### 11.1 Signatory vs Viewer Semantics

| Role | Can decrypt | Expected to sign | Default header |
|------|-------------|------------------|----------------|
| `signer` | Yes | Yes | `To:` |
| `viewer` | Yes | No | `Cc:` |
| `self` | Yes (sender) | n/a | — |

The role is a **workflow** attribute, not an access attribute — every role can read the agreement. A client uses the role only to decide whether it expects a signature reply from that participant and to compute completion status.

### 11.2 Agreement Message

An agreement is initiated as a signed multi-recipient message:

- **Body**: the agreement cover text / terms, in a signed `ENCRYPTED BODY` (the envelope is signalled by the RECIPIENTS block, not a keyword) — or, for a **plaintext (public) agreement**, a signed `SIGNED BODY` with the terms in the clear (Section 11.8).
- **Subject**: for an encrypted agreement the `Subject:` header is AES-256-GCM-encrypted under the **same CEK** as the body and **glossia-encoded** under the subject's own encoding setting (Section 5; defaults to the body's), so it reads as prose, survives header folding, is readable by exactly the recipients, and never leaks in cleartext; a reader recovers it by unwrapping the CEK from their RECIPIENTS stanza. For a plaintext (public) agreement the subject is sent in the clear. The subject is metadata and is **not** covered by the signature or `H` (GCM provides its own integrity).
- **Attachment(s)**: the contract document(s), bound to the message in one of two ways depending on whether it is encrypted:
  - *Encrypted message* — when it carries attachments, the `ENCRYPTED BODY` is a **manifest** rather than the raw body: a nested AES-encrypted body blob plus one entry per attachment carrying that attachment's AES key (`keyWrap`), MIME type, and ciphertext hash (`cipherSha256`). The manifest is what is AES-256-GCM-encrypted under the CEK and wrapped per-recipient; each attachment's ciphertext rides as an ordinary MIME part (`a1.dat`, `a2.dat`, …). Because the manifest (including every `cipherSha256`) is inside the signed, `H`-bound body, the attachments are bound tamper-evidently even though their bytes travel outside the armor. A reader recomputes each part's SHA-256 and **rejects** a mismatch. A message with no attachments encrypts the raw body directly (no manifest). The manifest is serialized as **Cap'n Proto** (schema `Manifest`/`EncryptedBlob`/`Attachment`) to raw binary behind a `capnp:` marker. No inner armor is needed: the manifest is the plaintext body, so it is encrypted by the outer layer (per-recipient-wrapped CEK) and that ciphertext is glossia-encoded for transport. Legacy **JSON** manifests (leading `{`) are still read for old mail.
  - *Public message* — the body is in the clear, so attachments ride as **plaintext** MIME parts and are bound by a signed `ATTACHMENTS` block listing, per attachment, its `id`, plaintext SHA-256, size, MIME type, and filename. The block is covered by the level signature (Section 4.2), so a swapped/added/removed attachment breaks the signature; a reader verifies each delivered file's SHA-256 against the block.
- **RECIPIENTS**: a `signer` stanza for each required signatory, a `viewer` stanza for each viewer, and the `self` stanza.
- **CONSENT** (optional): if the originator is themselves a required signatory, they include their own CONSENT block (Section 11.3) declaring consent. The originator's `self` stanza handles only decryption access and is never counted as a consent (Section 10.3) — an originator who signs MUST do so via a CONSENT block.
- **SIGNATURE**: the originator's signature, covering body + recipients + any consent (Section 4.2), which fixes the set of required signatories and the exact document bytes.

The originating message's body + RECIPIENTS define the **document hash** `H` that every consent in the thread refers to (Section 11.3.1).

Clients MAY include an `X-Nostr-Agreement` MIME header (boolean/identifier) to let IMAP filtering surface agreement threads without decrypting bodies. The authoritative agreement state always comes from the signed armor blocks, not the header.

### 11.3 Consent Block

Because every reply in a thread carries a SIGNATURE (it is how chain-of-custody works, Section 3.5.0), the mere presence of a signatory's signature over the document cannot, by itself, mean "I agree" — a signatory might be replying to negotiate, ask a question, or object. To make consent an **explicit, intentional act** rather than a side effect of replying, a signatory declares consent with a dedicated CONSENT block. This is the armor-grammar analogue of a purpose-built signing action: a comment carries no CONSENT block; a signature does.

A CONSENT block contains exactly two fields, one per line:

```
----- BEGIN NOSTR CONSENT -----
agreement <H>
signer    <pubkey>
```

| Field | Encoding | Meaning |
|-------|----------|---------|
| `agreement` | hex (64 chars) | The document hash `H` being consented to (Section 11.3.1). |
| `signer` | hex (64 chars) or npub | The consenting party's pubkey. MUST equal the pubkey in this level's SIGNATURE block. |

The CONSENT block does **not** carry its own signature — the level's existing SIGNATURE provides the binding. Because that signature covers `level(reply) || … || level(1)` (Sections 4.2, 3.5.0) and `level(L)` now includes the CONSENT block, the consent is bound to (a) the consenting identity, (b) the exact document `H`, and (c) the full nested history, and it cannot be stripped or altered without invalidating that signature and every signature above it. (A future version MAY define an optional self-contained variant carrying its own signature over `H`, for consents that must be verifiable in isolation from the thread; out of scope for v0.4.)

Decoders MUST tolerate additional unknown lines in a CONSENT block (forward compatibility) and MUST ignore blank lines and `> ` quote prefixes.

#### 11.3.1 Document Hash `H`

```
H = SHA-256( decode(body_1) || canonical(recipients_1) )
```

`H` is computed over the **originating** level's (level 1) body and RECIPIENTS block only — it deliberately **excludes all CONSENT blocks** (including the originator's own), so that `H` is fixed for the life of the agreement no matter how many consents accumulate, and so that the originator's level-1 CONSENT referencing `H` is not self-referential. Note this is distinct from `level(1)` (Section 4.2), which *does* include level 1's CONSENT block: `H` identifies the document, `level(L)` is what a level's signature covers.

#### 11.3.2 Ordering

Within a level, the CONSENT block is **content**, not a trailer: it is the last block of the level's content group, in the fixed order **body → RECIPIENTS → CONSENT**, and appears before any nested (inner) level and before the trailing SIGNATURE stack. Each level has **at most one** CONSENT block (a level has a single author). As an agreement accumulates signatures, each consenting reply contributes its own CONSENT block at its own level — they are never merged into one growing block, since nesting the prior message unchanged forbids mutating an inner level. The CONSENT block at a given nesting depth is bound by the SIGNATURE at the same depth (the same body↔signature depth-pairing used throughout Section 3.5).

### 11.4 Signing Round

A signatory signs by **replying** to the agreement thread (Section 3.5 / 3.6.1) and including a CONSENT block in their reply:

- The reply nests the prior message unchanged, adds a CONSENT block declaring consent to `H`, and adds the signatory's SIGNATURE, which — per Section 4.2 / 3.5.0 — covers `level(reply) || level(prior) || …`, i.e. the signatory's own content (including their CONSENT) plus the full nested history including the original document bytes.
- A signatory's consenting reply therefore cryptographically binds **their identity and intent to the exact agreement and to every signature already in the thread**, giving an ordered, tamper-evident chain of custody. Signing order is recoverable from nesting depth (innermost = first signer).
- A reply **without** a CONSENT block is a comment: authenticated and part of the chain of custody, but **not** a consent, and never counted toward completion (Section 11.5). This is how a signatory negotiates or a `viewer` comments without being mistaken for having signed.
- Reply-all preserves the `signer`/`viewer` roles from the prior level so the membership is carried forward (subject to the access rules of Section 10.8).

### 11.5 Completion & Status

An agreement is **complete** when, for every required signatory, the thread contains a valid CONSENT block referencing the agreement's `H`, bound by a verified SIGNATURE from that signatory's pubkey. The required signatory set is the set of `signer` stanzas in the originating message's RECIPIENTS block, **plus** the originator if the originating message itself carries a CONSENT block (Section 11.2). Clients:

- SHOULD compute "M of N signed" by intersecting the required signatory set with the set of pubkeys that have a **verified CONSENT block over `H`** in the thread.
- MUST key completion off the presence of a verified CONSENT block, **not** off the mere presence of a signature from a signatory — a signed comment (no CONSENT) does not count.
- MUST deduplicate by pubkey: a signatory who appears in multiple levels (e.g. commented, then signed) counts at most once.
- MAY use the `X-Nostr-Sig` / `X-Nostr-Pubkey` / `X-Nostr-Agreement` headers (Section 6.1) for a fast, decrypt-free progress estimate, but MUST confirm completion against verified in-body CONSENT blocks.
- SHOULD NOT count `viewer` or `self` pubkeys toward completion. A `viewer` who nonetheless emits a valid CONSENT block is not a required signatory and MUST NOT change the `N`, though clients MAY surface the extra consent informationally.

### 11.6 Verification

A completed agreement is verifiable offline and without any nostr-mail or SIGit server: a verifier walks the thread and, for each level, checks the SIGNATURE against `SHA-256(level(L) || … || level(1))` (Section 4.2) using the pubkey in the SIGNATURE block, then collects the CONSENT blocks. If, for every required signatory, a CONSENT block over the agreement's `H` is bound by a verified signature from that signatory's pubkey, the agreement is valid and attributable to those Nostr identities. Tampering with the document, the recipient/role set, any declared consent, or any prior signature invalidates the affected signature and every signature above it.

### 11.7 Relation to SIGit

This agreement model is conceptually aligned with [SIGit](https://sigit.io)'s document-signing design, re-expressed for an **email-native, self-contained** transport rather than SIGit's Nostr-relay + Blossom + account-server split. The core concepts map directly:

| SIGit | nostr-mail (this spec) |
|-------|------------------------|
| "Agreement" (documents + counterparties + metadata) | the agreement thread (Section 11) |
| `signers` / `viewers` arrays; Creator | `signer` / `viewer` roles (Section 11.1); originator + `self` stanza |
| `keys: { npub: ENCRYPTD }` — decryption key NIP-44-wrapped per counterparty | RECIPIENTS block — CEK NIP-44-wrapped per recipient (Section 10) |
| `meta` hash that Sign events commit to | document hash `H` (Section 11.3.1) |
| `prevSig` chain + `docSignatures` map | nested per-level SIGNATURE chain-of-custody (Sections 3.5, 4.2) |
| Sign event (Kind 938) added to `docSignatures` | the CONSENT block bound by the level's signature (Section 11.3) |
| Offline-verifiable from the encrypted zip | offline-verifiable from the email thread (Section 11.6) |

The deliberate difference is transport and privacy. SIGit hides the participant set by gift-wrapping (NIP-59) metadata to ephemeral keys and storing encrypted files on Blossom; nostr-mail keeps the whole agreement in one email thread, which is more self-contained and offline-verifiable but exposes the participant set in the cleartext RECIPIENTS block (Section 10.9). The **gift-wrap mode** below folds in SIGit's gift-wrap approach for stronger metadata privacy; because the consent semantics here (`H`, per-signatory CONSENT, signature chaining) are transport-independent, that mode reuses this section's model with a different addressing channel. SIGit's own event-kind numbering is **not** adopted; nostr-mail consent lives in armor blocks, not relay-published Nostr events.

#### 11.7.1 Gift-Wrap Mode (metadata-private delivery)

> **Status:** designed; the wire format (the `reference` token, Section 10.2) is reserved now, but the NIP-59 delivery and DM-aware decrypt/verify paths are a later milestone.

The cleartext RECIPIENTS block exposes two things to relays and mail servers that the participants may wish to keep private: the per-recipient **wrapped CEKs** and the **`(npub, email)` bindings**. Gift-wrap mode moves this sensitive per-recipient material off the email and into a **NIP-59 gift-wrapped DM**, one per recipient, leaving the email to carry the document plus a minimal, signed reference.

**What moves, what stays.** The split is chosen so the *workflow* stays tamper-evident and offline-checkable while the *sensitive* material becomes private:

| Stays in the signed email | Moves to the per-recipient gift wrap |
|---|---|
| the body (`ENCRYPTED BODY` ciphertext, or `SIGNED BODY`) | the recipient's `wrapped-cek` |
| each recipient's `role` + `pubkey` + a `reference` token | the recipient's `(npub, email)` binding |
| the originator's SIGNATURE (covers the above) | — |

So a gift-wrap-mode stanza is `signer <pubkey> evt:<gift-wrap-id>` — role and pubkey remain in the signed block (so the required-signatory set, the document hash `H`, and completion accounting are still authenticated and computable from the email), while the CEK and the address binding travel in the referenced DM.

**The reference.** The `reference` token (`evt:<event-id>`, Section 10.2) points to the gift wrap addressed to that recipient. The gift wrap's inner *rumor* SHOULD carry: the agreement reference (the email `Message-ID` and/or the document hash `H`), this recipient's `wrapped-cek` (if the body is encrypted), and the `(npub, email)` binding. The gift wrap is sealed and wrapped per NIP-59 (ephemeral outer key), so relays learn neither the sender nor the recipient.

**Verification with the DM in hand.** A recipient: (1) verifies the email's signature chain and reads the signed `role`/`pubkey` set and `H` as usual (Section 11.6); (2) unwraps their gift wrap, confirms it references this `H`/`Message-ID`, and takes the CEK (to decrypt) and the `(npub, email)` binding; (3) completion is computed exactly as in Section 11.5 — the consents still live in the (email or DM) CONSENT blocks bound by signatures over `H`.

**Privacy gradations.** This is the middle of three points on the privacy/self-containedness curve:

1. **Cleartext (Sections 10–11):** everything in the email thread; fully self-contained and offline-verifiable by anyone; participant set, CEKs, and bindings are public.
2. **Gift-wrap mode (this section):** CEKs and `(npub, email)` bindings are private; the **pubkey + role set is still public** in the signed email (so the document and signatory set remain tamper-evident and the agreement is still anchored in one email). Decryption and binding-verification now also require the recipient's DM.
3. **Full metadata-private (future):** omit the RECIPIENTS block entirely; all addressing — including pubkeys — lives in gift wraps, and readers trial-decrypt (as the `age` format does). Maximum privacy, least self-contained; out of scope here.

**Trade-off.** Gift-wrap mode trades the Section 11.6 property — *a third party can verify completion offline from the email alone* — for metadata privacy: a participant now needs their gift-wrapped DM (and thus a relay) to obtain the CEK and binding. The email still anchors the document, the signed signatory set, and `H`, so tampering is still detectable from the email; only the private openings require the DM. `Bcc:`-style blind participants are naturally expressible here, since a gift wrap to a recipient need not appear in any other recipient's view.

### 11.8 Plaintext (Public) Agreements

An agreement need not be confidential. A **plaintext (public) agreement** carries its terms in the clear so that anyone — not only a cryptographic recipient — can read them and verify completion offline. This suits open letters, public multi-party statements, and on-the-record contracts, where transparency is the point.

A plaintext agreement is identical to the encrypted agreement (Sections 10–11) except for the body and the key-wrap:

- **Body**: a signed `SIGNED BODY` (Section 3.2) instead of an `ENCRYPTED BODY`. The terms are glossia-encoded inside the armor (the signed payload) and also appear above the armor for non-Nostr-Mail clients. There is no CEK and nothing is encrypted.
- **RECIPIENTS**: the same block declares the signatories (`signer`) and viewers (`viewer`), but each stanza simply **omits the `wrapped-cek` token** (Section 10.2), since there is no CEK to wrap — a stanza is then `signer <pubkey> [<email>]`. The optional `email` token still binds the `(pubkey, email)` pairing (Sections 10.2, 6.3). There is **no `self` stanza** — nothing is encrypted, so the originator needs no self-wrap; the originator is identified by the SIGNATURE.
- **CONSENT**, **document hash `H`**, **signature coverage (§4.2)**, and **completion (§11.5)** are unchanged. `H = SHA-256(decode(body₁) || canonical(recipients₁))` is computed over the *decoded* (glossia → plaintext) body bytes, so the same machinery applies regardless of cipher. A counter-signature is a `SIGNED BODY` reply that nests the prior message and adds the signatory's CONSENT (Section 11.4).

Path selection is unambiguous: a `SIGNED BODY` is never decrypted (it has no ciphertext), so the absent CEK is never an issue. A decoder MUST treat an `ENCRYPTED BODY` whose stanza carries no `wrapped-cek` (and no `reference`) as malformed.

The trade-off versus the encrypted agreement is **confidentiality**: the terms and the participant set (pubkeys, and any `email` tokens) are public to anyone who sees the message. In return, the agreement is readable and its completion is verifiable by third parties who are not signatories — the same offline, self-contained verification of Section 11.6, without needing to be a recipient.

## 12. Versioning

This specification may be extended with additional block types in future versions. Decoders SHOULD ignore unrecognized block types rather than failing. Unknown `role` tokens in a RECIPIENTS block (Section 10.2) MUST be treated as recipients for decryption while being ignored for workflow accounting, so that future roles do not break older clients.

The CONSENT block (Section 11.3) is likewise backward compatible: a client that predates it ignores the unknown block and simply sees an ordinary signed reply, so message decryption and signature verification are unaffected — only the agreement-completion view (which such a client does not compute) is unavailable. Agreement-aware clients MUST tolerate unknown trailing lines within a CONSENT block so that future consent fields do not break older clients.
