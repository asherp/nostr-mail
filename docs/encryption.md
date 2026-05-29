# Encryption & Signing

nostr-mail protects your messages with the same secp256k1 keypair you use on Nostr. This page is a practical overview of what gets encrypted, how messages are signed, and the options you can control. For the wire format, see the [Nostr-Mail Protocol Specification](nostr-mail-spec.md); for the cipher itself, see [NIP-44 Encryption](nip44.md).

## How encryption works

When you encrypt an email, the body is encrypted to a **shared secret** derived from your private key and the recipient's public key. Both sides can derive the same secret, so only the two of you can read the message — the email server only ever sees ciphertext.

### Algorithms

Choose your algorithm in **Settings → Advanced → Encryption Algorithm**:

| Algorithm | Status | Notes |
|-----------|--------|-------|
| **NIP-44** | Recommended (default) | Modern, authenticated encryption. See [NIP-44 details](nip44.md). |
| **NIP-04** | Legacy | Kept for backward compatibility with older messages and clients. Less secure. |

When **reading**, nostr-mail decrypts transparently: it tries NIP-44 first and falls back to NIP-04 automatically, so you can read messages sent with either scheme regardless of your current setting.

## Signing & verification

Emails can be **signed** with your Nostr key so recipients can verify they really came from you and were not tampered with in transit.

- **Automatically Sign** (Settings → Advanced) signs every outgoing email.
- Incoming signatures are verified against the sender's public key.
- **Require Signatures** rejects unsigned mail; **Hide Unverified** hides messages whose signatures are missing or invalid. See the [Inbox guide](pages/inbox.md) for how these filters behave.

Signing and encryption are independent: you can sign without encrypting (e.g. to a non-nostr-mail recipient) or do both.

## X-Nostr email headers

nostr-mail can attach custom headers so that cryptographic metadata travels with the email itself, rather than relying on a separate Nostr event. Each header is individually toggleable in **Settings → Advanced → Email Preferences**:

| Header | Purpose |
|--------|---------|
| `X-Nostr-Pubkey` | The sender's public key, so the recipient can derive the shared secret without scanning the body. |
| `X-Nostr-Sig` | A signature over the email body (requires `X-Nostr-Pubkey`). |
| `X-Nostr-Recipient` | The public key the body was encrypted to. Lets you and the recipient decrypt **without** a matching Nostr DM — useful when you aren't using relays. |

## NIP-17 gift-wrapped direct messages

Direct messages use **NIP-17** gift wrapping (kind `1059`) by default. Compared to legacy NIP-04 DMs, gift wrapping hides metadata — the message kind, your tags, and even the `message-id` that links a DM to its matching email are sealed inside the wrap and invisible to relays.

How it works in practice:

- nostr-mail checks whether the recipient publishes a NIP-17 inbox (kind `10050`). If they do, the DM is gift-wrapped and routed to that inbox.
- If the recipient has **not** published a NIP-17 inbox, nostr-mail transparently **falls back to NIP-04** so the message still gets through. The compose/messages views surface a warning when a recipient can't receive gift wraps.
- Every DM is wrapped **twice** — once to the recipient and once to yourself — so your sent messages reappear when you log in on a fresh install or second device.

Routing follows the recipient's NIP-17 inbox (kind `10050`) when available, falling back to their read relays (NIP-65) and finally to a broadcast.

## Attachments

File attachments are encrypted with **hybrid encryption**: each file is encrypted with a randomly generated AES-256 key, and that key is then encrypted to the recipient with NIP-44. This keeps large attachments efficient while still protecting the key material end-to-end. Received attachments can be saved individually or exported together as a zip.

## Related reading

- [Glossia Encoding](glossia.md) — rendering ciphertext as readable text so it survives email forwarding and reply quoting
- [NIP-44 Encryption](nip44.md) — the cipher used by default
- [Nostr-Mail Protocol Specification](nostr-mail-spec.md) — the exact armor/header wire format
- [Encryption Architecture](encryption-architecture.md) — how encryption, decryption, and encoding fit together internally
