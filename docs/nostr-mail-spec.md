# Nostr-Mail Protocol Specification

**Version:** 0.2.0-draft
**Date:** 2026-03-26

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
| `END NOSTR MESSAGE` | Closing tag | Terminates the outermost block |
| `END NOSTR SEAL` | Closing tag | Terminates standalone seal blocks |

The encryption type is embedded directly in the BEGIN tag (e.g., `BEGIN NOSTR NIP-44 ENCRYPTED BODY`), keeping the format self-describing without metadata lines.

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

### 3.3 Unsigned + Encrypted (with Seal)

```
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<ciphertext, optionally glossia-encoded>
----- BEGIN NOSTR SEAL -----
@DisplayName
<pubkey: glossia-encoded, hex, or npub>
----- END NOSTR MESSAGE -----
```

The SEAL block provides the sender's pubkey, which is required for NIP-04/NIP-44 decryption and survives forwarding (unlike MIME headers). Decoders MUST accept glossia-encoded, hex-encoded, and npub (bech32-encoded, `npub1...`) formats for pubkeys in SEAL blocks.

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

## 4. Composable Signing Model

Signing is user-controlled and can be applied at any stage to the current body bytes:

| Operation | What gets signed | Result |
|-----------|-----------------|--------|
| Sign plaintext | `SHA-256(decode(armor_body) \|\| decode(quoted_bodies))` | Proves authorship of content + conversation history |
| Sign then encrypt | Signature inside ciphertext | Only recipient can verify |
| Encrypt then sign | `SHA-256(decode(armor_body) \|\| decode(quoted_bodies))` | Proves sender without revealing content |
| Sign, encrypt, sign | Both layers | Full trust chain |

The signing target is the **flat concatenation** of the decoded bytes from the current armor body and all nested quoted armor bodies (recursively). For glossia-encoded content, decoding means transcoding the prose back to binary. For base64 content, it means base64-decoding. The signature is never on the raw plaintext or the encoded prose itself — it is on the canonical decoded bytes, which survive transport regardless of reformatting.

For messages with no quoted content, this reduces to `SHA-256(decode(armor_body))`. For replies, the concatenation provides chain-of-custody integrity — modifying any message in the conversation history invalidates all subsequent signatures.

## 5. Content Encoding (Glossia)

Body content, signatures, and pubkeys may be encoded using the Glossia steganographic encoding system. Each field has an independent encoding setting:

- **Body/Subject**: Glossia prose encoding (e.g., Latin, BIP39, or hex)
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
7. **Verify** signatures against `SHA-256(decoded_body_bytes)` using the pubkey from the SIGNATURE block
8. **Decrypt** if encrypted, using recipient's private key and sender's pubkey

## 9. Versioning

This specification may be extended with additional block types in future versions. Decoders SHOULD ignore unrecognized block types rather than failing.
