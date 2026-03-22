# Nostr-Mail Protocol Specification

**Version:** 0.1.0-draft
**Date:** 2026-03-19

## 1. Overview

Nostr-Mail is a protocol for sending Nostr-encrypted and/or Nostr-signed messages over standard email (SMTP/IMAP). It embeds cryptographic content in ASCII armor blocks within the `text/plain` part of an email, enabling end-to-end encryption and identity verification while remaining compatible with all email infrastructure.

## 2. Armor Block Format

All Nostr-Mail content is enclosed in ASCII armor blocks using `-----` delimiters. Blocks are identified by type-specific tags.

### 2.1 Block Types

| Tag | Role | Contains |
|-----|------|----------|
| `BEGIN NOSTR NIP-XX ENCRYPTED BODY` | Encrypted content | Ciphertext (NIP-04 or NIP-44) |
| `BEGIN NOSTR SIGNED BODY` | Signed plaintext | Plaintext body content |
| `BEGIN NOSTR SIGNATURE` | Proof of authorship | Schnorr signature bytes |
| `BEGIN NOSTR SEAL` | Identity declaration | Sender's Nostr public key |
| `END NOSTR MESSAGE` | Closing tag | Terminates the outermost block |
| `END NOSTR SIGNATURE` | Closing tag | Terminates standalone signature blocks |
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

## 3. Message Formats

### 3.1 Signed + Encrypted

```
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<ciphertext, optionally glossia-encoded>
----- BEGIN NOSTR SIGNATURE -----
@ProfileName
<signature bytes, optionally glossia-encoded>
----- BEGIN NOSTR SEAL -----
@DisplayName
<pubkey bytes, optionally glossia-encoded>
----- END NOSTR MESSAGE -----
```

### 3.2 Signed Plaintext

The original plaintext appears above the armor block for readability. The armored body contains the glossia-encoded plaintext, which is the verifiable payload.

```
<plaintext body>

----- BEGIN NOSTR SIGNED BODY -----
<glossia-encoded plaintext>
----- BEGIN NOSTR SIGNATURE -----
@ProfileName
<signature bytes>
----- BEGIN NOSTR SEAL -----
@DisplayName
<pubkey bytes>
----- END NOSTR MESSAGE -----
```

### 3.3 Unsigned + Encrypted (with Seal)

```
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<ciphertext, optionally glossia-encoded>
----- BEGIN NOSTR SEAL -----
@DisplayName
<pubkey bytes>
----- END NOSTR MESSAGE -----
```

### 3.4 Unsigned Plaintext (with optional Seal)

Body text is not armored. A standalone seal block may follow:

```
Hello, this is a plaintext message.

----- BEGIN NOSTR SEAL -----
@DisplayName
<pubkey bytes>
----- END NOSTR SEAL -----
```

### 3.5 Reply Format

When replying to an encrypted message, only the new reply text is encrypted. The original message is appended as a `> ` quoted block outside the encryption boundary. This preserves the original message's signature for independent verification and avoids double-encryption.

```
----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
<reply ciphertext, encrypted independently>
----- BEGIN NOSTR SIGNATURE -----
@ReplyAuthor
<reply signature>
----- BEGIN NOSTR SEAL -----
@ReplyAuthor
<reply author pubkey>
----- END NOSTR MESSAGE -----

> ----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
> <original message ciphertext>
> ----- BEGIN NOSTR SIGNATURE -----
> @OriginalAuthor
> <original signature>
> ----- BEGIN NOSTR SEAL -----
> @OriginalAuthor
> <original author pubkey>
> ----- END NOSTR MESSAGE -----
```

For reply chains, quote depth increases with each level:

```
<current reply armor>

> <previous reply armor>
>
> > <original message armor>
```

Decoders MUST accept armor block delimiters preceded by any number of `> ` quote prefixes. Glossia decoders naturally ignore quote prefixes as non-payload words.

## 4. Composable Signing Model

Signing is user-controlled and can be applied at any stage to the current body bytes:

| Operation | What gets signed | Result |
|-----------|-----------------|--------|
| Sign plaintext | `SHA-256(decode(armor_body))` | Proves authorship of content |
| Sign then encrypt | Signature inside ciphertext | Only recipient can verify |
| Encrypt then sign | `SHA-256(decode(armor_body))` | Proves sender without revealing content |
| Sign, encrypt, sign | Both layers | Full trust chain |

The signing target is always the bytes obtained by decoding the armor block content. For glossia-encoded content, this means decoding the prose back to binary. For base64 content, this means base64-decoding. The signature is never on the raw plaintext or the encoded prose itself — it is on the canonical decoded bytes, which survive transport regardless of reformatting.

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
| `X-Nostr-Pubkey` | hex-encoded pubkey | Sender identification |
| `X-Nostr-Sig` | hex-encoded signature | Message authentication |

**Primary trust path**: In-body SEAL and SIGNATURE blocks (survive forwarding, quoting, and re-encoding).

**Secondary trust path**: X-Nostr-* MIME headers (for fast IMAP filtering and older client compatibility).

## 7. Identity Model

| Layer | Source | Trust Level |
|-------|--------|-------------|
| Transport sender | Email `From:` header | None (spoofable) |
| Cryptographic author | SEAL block (npub) | Verified via SIGNATURE block |
| Client attribution | `X-Mailer` header | Informational only |

## 8. Decoder Algorithm

1. **Scan** the `text/plain` body for armor block delimiters (`-{3,}\s*BEGIN NOSTR`)
2. **Normalize** whitespace and line endings (`\r\n` -> `\n`)
3. **Parse** the BEGIN tag to determine encryption type (NIP-04, NIP-44, or signed plaintext)
4. **Detect** content encoding: base64 (no spaces) vs Glossia (word patterns)
5. **Decode** content: base64 decode or Glossia transcode -> bytes
6. **Unpack** NIP-04 if applicable (bitpacked binary -> `base64?iv=base64`)
7. **Verify** signatures against `SHA-256(decoded_body_bytes)` using the SEAL pubkey
8. **Decrypt** if encrypted, using recipient's private key and sender's pubkey

## 9. Versioning

This specification may be extended with additional block types in future versions. Decoders SHOULD ignore unrecognized block types rather than failing.
