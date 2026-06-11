# Glossia Design Philosophy

## Why glossia's source of truth is `text/plain`

Glossia encodes encrypted ciphertext (NIP-04 or NIP-44) as human-readable natural language. The encoded text is carried in the email's `text/plain` part, which is the canonical source of truth. nostr-mail also attaches a `text/html` rendering aid in a standard `multipart/alternative` (see the [protocol spec](nostr-mail-spec.md#6-mime-structure)), but that HTML part is disposable: it never holds the authoritative payload, and the message decodes correctly from `text/plain` alone. Keeping the verifiable content in `text/plain` — rather than in a custom MIME type or behind special structure — is a deliberate design choice, not a limitation.

## The PGP UX problem

PGP has been available for decades, yet encrypted email never went mainstream. A major reason is that ASCII-armored ciphertext is hostile to normal email workflows:

- **Forwarding breaks**: Forward a PGP-encrypted email and the armor block gets mangled by quoting, line wrapping, or HTML conversion.
- **Replying breaks**: Reply inline to an armored message and the `-----BEGIN PGP MESSAGE-----` markers get interleaved with quoted text, making decryption fail.
- **Quoting breaks**: Email clients that quote with `>` prefixes corrupt the base64 payload.

The result is that PGP email only works when every participant treats it as a special artifact. It cannot participate in normal email chains. This friction is why PGP adoption stalled — not because the cryptography is weak, but because the format is incompatible with how people actually use email.

## Glossia's solution

Glossia takes a different approach: encode ciphertext as natural language that passes as plaintext. A glossia-encoded message looks like ordinary Latin, Spanish, or other natural language text. It travels through email infrastructure — SMTP relays, IMAP servers, webmail clients — without any special handling.

A recipient with nostr-mail decodes the glossia text back to ciphertext and decrypts it. A recipient without nostr-mail sees what appears to be a normal message in a foreign language. Either way, the email behaves like email.

## Why not a custom glossia MIME part?

An alternative approach was considered: carry the canonical payload in its own MIME part — for example, send the ASCII-armored ciphertext in a `multipart/alternative` with a custom `Content-Type: application/glossia` part — so that glossia is treated as structured data rather than plain text.

This was rejected for three reasons:

1. **It reintroduces the structure problem.** The entire point of glossia is that encrypted email should not require special MIME structure to be readable. If the canonical payload depends on a custom part surviving intact, glossia has the same deployment problem as PGP — it works when the infrastructure cooperates and breaks when it doesn't.

2. **It positions glossia as secondary.** Putting the "real" payload in a custom part implies that the `text/plain` is just a fallback. This contradicts the design goal: the glossia `text/plain` *is* the content, not a human-friendly wrapper around the "real" ciphertext.

3. **Email clients flatten multipart structure unpredictably.** Forwarding, replying, and quoting through a chain of different email clients (Gmail, Outlook, Apple Mail, Thunderbird) produces inconsistent results: non-`text/plain` parts get dropped, reordered, or converted. nostr-mail therefore keeps the authoritative payload in `text/plain`, which survives this reflow, and treats the accompanying `text/html` part as a throwaway rendering aid that can be lost without breaking decryption.

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

## The canonical part is `text/plain`

Glossia does not need a custom MIME type. Its authoritative content is a `text/plain` part (optionally accompanied by a disposable `text/html` rendering aid). This means:

- No email client needs updating to handle glossia.
- No mail server needs configuration changes.
- No corporate email filter blocks it as an unknown type.
- No webmail interface fails to render it.

The encoding is invisible to infrastructure and visible only to participants who share the decryption key. This is the design goal: encrypted email that behaves like normal email.
