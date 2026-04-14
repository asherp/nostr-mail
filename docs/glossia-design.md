# Glossia Design Philosophy

## Why glossia uses `text/plain`, not `multipart/alternative`

Glossia encodes encrypted ciphertext (NIP-04 or NIP-44) as human-readable natural language. The encoded text is sent as ordinary `text/plain` email. This is a deliberate design choice, not a limitation.

## The PGP UX problem

PGP has been available for decades, yet encrypted email never went mainstream. A major reason is that ASCII-armored ciphertext is hostile to normal email workflows:

- **Forwarding breaks**: Forward a PGP-encrypted email and the armor block gets mangled by quoting, line wrapping, or HTML conversion.
- **Replying breaks**: Reply inline to an armored message and the `-----BEGIN PGP MESSAGE-----` markers get interleaved with quoted text, making decryption fail.
- **Quoting breaks**: Email clients that quote with `>` prefixes corrupt the base64 payload.

The result is that PGP email only works when every participant treats it as a special artifact. It cannot participate in normal email chains. This friction is why PGP adoption stalled — not because the cryptography is weak, but because the format is incompatible with how people actually use email.

## Glossia's solution

Glossia takes a different approach: encode ciphertext as natural language that passes as plaintext. A glossia-encoded message looks like ordinary Latin, Spanish, or other natural language text. It travels through email infrastructure — SMTP relays, IMAP servers, webmail clients — without any special handling.

A recipient with nostr-mail decodes the glossia text back to ciphertext and decrypts it. A recipient without nostr-mail sees what appears to be a normal message in a foreign language. Either way, the email behaves like email.

## Why not `multipart/alternative`?

An alternative approach was considered: send both the ASCII-armored ciphertext and the glossia-encoded text in a single email using MIME `multipart/alternative`, with a custom `Content-Type: application/glossia` part.

This was rejected for three reasons:

1. **It reintroduces the structure problem.** The entire point of glossia is that encrypted email should not require special MIME structure. If a `multipart/alternative` envelope is needed, glossia has the same deployment problem as PGP — it works when the infrastructure cooperates and breaks when it doesn't.

2. **It positions glossia as secondary.** The `multipart/alternative` pattern implies that one part is the "real" content and the other is a fallback. This contradicts the design goal: glossia *is* the content. It is not a human-friendly wrapper around the "real" ciphertext.

3. **Email clients flatten multipart structure unpredictably.** Forwarding, replying, and quoting a `multipart/alternative` message through a chain of different email clients (Gmail, Outlook, Apple Mail, Thunderbird) produces inconsistent results. Parts get dropped, reordered, or converted. The glossia text may not survive the chain intact.

## The forwarding and reply argument

Glossia text survives email chains naturally because it is just text. Consider a legal email chain where multiple parties forward, reply, and quote messages inline:

```
On Feb 10, Alice wrote:
> Lingua antiqua verba secreta portant per vias silentes...
>
> On Feb 9, Bob wrote:
> > Carmina nova resonant in templis veteribus...
```

Every quoted block remains valid glossia. Any participant with nostr-mail can decode any quoted section independently. The email chain works exactly as it would with any other plaintext content — because it *is* plaintext content.

This property is impossible with ASCII armor or custom MIME types, where quoting prefixes and reformatting destroy the encoded payload.

## Content-Type is `text/plain`

Glossia does not need a custom MIME type. The `Content-Type` is `text/plain`. This means:

- No email client needs updating to handle glossia.
- No mail server needs configuration changes.
- No corporate email filter blocks it as an unknown type.
- No webmail interface fails to render it.

The encoding is invisible to infrastructure and visible only to participants who share the decryption key. This is the design goal: encrypted email that behaves like normal email.
