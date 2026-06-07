# Glossia Encoding

Glossia encodes encrypted ciphertext as **human-readable natural-language text** so that encrypted email survives the normal email lifecycle — forwarding, inline replies, and `>` quoting — without being mangled. The encoded message is sent as ordinary `text/plain`, so no email server, filter, or client needs to know anything special about it.

This page covers how to use Glossia in nostr-mail. For the reasoning behind the design, see [Glossia Design Philosophy](glossia-design.md) and the [Nostr-Mail Protocol Specification](nostr-mail-spec.md).

## Why it exists

Traditional PGP-style ASCII armor (`-----BEGIN PGP MESSAGE-----` … base64 …) breaks the moment an email is forwarded, replied to inline, or quoted with `>` prefixes — the payload gets line-wrapped, reflowed, or interleaved with quoted text and no longer decrypts. Glossia sidesteps this: a glossia-encoded message reads like ordinary text in some language, and because it *is* plain text, every quoted copy in an email chain remains independently decodable by anyone with nostr-mail.

## Choosing an encoding

Glossia encoding is configured in **Settings → Advanced → Glossia Encoding**. You can pick the encoding **per email component** independently:

| Component | What it carries |
|-----------|-----------------|
| **Ciphertext** | The encrypted email body |
| **Signature** | The Nostr signature over the body |
| **Pubkey** | The sender / recipient public key metadata |

For each component you can choose:

| Option | Behaviour |
|--------|-----------|
| **Latin** | Encodes the bytes as Latin-looking words. Reads as natural language and survives forwarding/reply/quoting. |
| **English – BIP39** | Encodes the bytes using the BIP39 English word list. Also survives email chains, and the word list is a well-known standard. |
| **Base64 / npub** | No word encoding. Compact, but ⚠️ **breaks on forward/reply** because raw base64 armor gets corrupted by quoting and reflow. For the pubkey component this option emits a plain `npub`. |

!!! tip "Recommended"
    Leave the body and signature on a word-based encoding (**Latin** or **English – BIP39**) if you expect messages to be forwarded or replied to. Reserve the Base64 option for cases where you control the whole chain or are debugging.

## How it behaves end to end

- **Sending**: nostr-mail encrypts the body, signs it, then renders ciphertext/signature/pubkey using your chosen encodings and places them in the `text/plain` body.
- **Receiving with nostr-mail**: the encoded text is detected, decoded back to ciphertext, and decrypted automatically.
- **Receiving without nostr-mail**: the recipient just sees what looks like a message in a foreign language — the email still delivers and renders normally.

## Related reading

- [Glossia Design Philosophy](glossia-design.md) — why `text/plain` instead of `multipart/alternative`
- [Encryption & Signing](encryption.md) — how Glossia fits into the overall crypto flow
