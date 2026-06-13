# Security Policy

This document states nostr-mail's threat model and trust model, and records
what each authentication signal in the app does and does not prove. It is meant
to be read alongside the protocol spec; where the two disagree, the spec's
normative sections win, but the trust framing here is authoritative.

## Reporting a vulnerability

Please report security issues privately to the maintainer rather than opening a
public issue:

- Email the project owner (see the repository's primary contact), or
- Open a [GitHub security advisory](https://github.com/asherp/nostr-mail/security/advisories/new).

Include a description, affected version/commit, and a proof-of-concept if you
have one. We will acknowledge receipt and coordinate a fix and disclosure
timeline with you.

## Threat model

nostr-mail carries end-to-end encrypted, signed mail over ordinary email
transport (SMTP/IMAP) while rooting identity in Nostr keys. We assume the
following adversaries are *in scope* — the design must remain safe against them:

- **A malicious or compromised mail server (the user's own provider included).**
  An IMAP/SMTP server can read, drop, delay, reorder, duplicate, or inject
  messages and headers in either direction. It can fabricate transport metadata
  (including `Authentication-Results`, `Received`, and other trace headers). It
  must not be able to forge a contact's Nostr signature or learn plaintext of
  end-to-end encrypted content.
- **A network / header-injection adversary.** Anyone who can place bytes into a
  message we receive — a sender, an intermediate relay, or an on-path attacker —
  can add or alter headers. Any control decision we make from cleartext headers
  must assume those headers are attacker-chosen unless they are cryptographically
  bound or were stamped inside our own trust boundary.
- **A malicious sender / spoofer.** A sender may lie in the SMTP envelope, the
  `From:` header, the body, and any self-asserted Nostr metadata
  (`X-Nostr-Pubkey`, profile fields). Self-assertion is never proof.

Out of scope (mitigated by other means or accepted): a fully compromised local
device / OS keystore, a malicious build of the app, denial of service (a hostile
provider can always refuse to deliver), and traffic-analysis metadata inherent to
using email transport (sender/recipient addresses, timing, sizes).

### Lesson incorporated

The transport-auth hardening in this codebase follows the header-precedence
finding in [*Cryptographic Analysis of Delta Chat*, USENIX Security '24
(ePrint 2024/918)](https://eprint.iacr.org/2024/918): **do not trust cleartext
control data that has an authenticated counterpart, and when you must read a
provider-added header, select and verify the one your own trust boundary added —
not whichever copy an attacker could inject.**

## Trust model: npub ↔ email

The durable identity in nostr-mail is the **Nostr public key (npub)**, not the
email address.

- A Nostr profile's `email` field and an SMTP `From:` address are **both
  unauthenticated self-assertions**. Neither proves control of the mailbox, and
  neither proves which key owns that mailbox.
- **Trust is rooted in the npub the user deliberately follows or verifies.**
  Email is a *derived attribute* of a trusted contact (key), not the other way
  around.
- **Never resolve `email → key` from arbitrary profiles to choose an encryption
  target.** Picking "whatever key claims this email" lets any profile that
  asserts a victim's address hijack the encryption target. Resolution must go
  `trusted npub → its asserted email`, and the email attribute is only as
  trustworthy as the binding that produced it (see *Email-ownership binding*).

### Trust states (must be distinguished in the UI)

These are not interchangeable and must not be collapsed into a single
"signed / not signed" indicator:

| State | Meaning |
| --- | --- |
| **verified** | Signed by the **pinned key for this contact** — the key the user anchored to this npub. |
| **signed-but-unverified** | Carries a valid signature, but from a key **not anchored to this contact**. Cryptographically intact, identity unproven. |
| **unsigned** | No usable signature. |

`signed-but-unverified` must never be presented as `verified`. Promotion to
`verified` happens only when the signing key is pinned to the contact — e.g. via
the email-ownership binding below.

## What transport authentication (DKIM/DMARC/SPF) does and does not prove

nostr-mail reads the `Authentication-Results` (A-R) header to obtain a
**send-side, domain-attested** signal about the visible `From:` domain. This is
a legitimate MUA approach (RFC 8601) **only** under strict conditions, which this
codebase now enforces (issue #101):

- **Select the trusted header.** We take the **first/top-most** A-R header — the
  one stamped by the closest hop, i.e. the user's own receiving provider — not
  the last. Relays *prepend* trace headers, so a sender-injected A-R sits at the
  bottom; the previous code's `.pop()` selected exactly that forgeable line.
- **Verify the `authserv-id`.** An A-R header is honored only when its
  authserv-id's organizational domain matches the user's configured/derived
  provider (e.g. `mx.google.com` for Gmail). If our provider added no A-R header,
  a sender-forged one is **not** trusted by position alone — it is rejected.
- **SPF ≠ From-authentication.** SPF authenticates the envelope `MAIL FROM`
  (Return-Path), not the visible `From:` header. An SPF-only pass does **not**
  set the transport-verified flag.
- **Relaxed DMARC alignment.** DKIM alignment is checked at the organizational-
  domain level (RFC 7489 relaxed), so a legitimate subdomain signature
  (`From: example.com`, `header.d: mail.example.com`) is not a false negative.

What it **does not** prove:

- It is **not a read-access proof** and not an ownership proof. It says a domain
  authorized the message, not that any particular key controls the mailbox.
- It is **not trustless against a malicious sending domain**: a domain that
  controls its own DKIM keys can produce aligned, passing mail it fully
  authored.
- It **depends on trusting the receiving provider + DNS** at receipt time. It is
  not a self-validating artifact: nothing is checked against the raw bytes
  (no DKIM signature is cryptographically verified in-app yet, no key captured),
  so a stored verdict is only as trustworthy as the live pipeline that produced
  it and **cannot be re-verified offline/archivally**.

Treat transport-auth as a **secondary, provider/domain-attested hint** — useful
for filtering and spam-rescue gating — never as the identity root.

## Email-ownership binding (planned)

The durable, npub-rooted way to bind a key to a mailbox is a **SecureJoin-style
quote-and-sign handshake**, building on the same signed-over-quoted-content
primitive the reply/CONSENT mechanism already provides:

1. **Challenge.** To verify `K_B ↔ bob@example.com`, Alice emails a fresh nonce
   `N` **only** to `bob@example.com`, inside a body signed under her key `K_A`,
   binding both `K_B` and the asserted address.
2. **Response.** Bob replies, nesting Alice's challenge and signing over it with
   `K_B` (equivalently, a `CONSENT` block over `H = hash(assertion)`).
3. **Pin.** Alice verifies `K_B`'s signature covers the quoted nonce + asserted
   address, then pins `K_B ↔ bob@example.com` — promoting the contact from
   `signed-but-unverified` to `verified` with an email attribute attached.

Properties and limits, stated honestly:

- **A single message is never a self-sufficient ownership proof.** The proof is
  the **request + response pair**, conditioned on delivery: Alice sent `N`
  exclusively to the address; a `K_B`-signed message quoting `N` came back ⇒ the
  holder of `K_B` had **read** access to that mailbox.
- It is **npub-rooted, not provider-rooted**: a malicious/compromised server can
  *block or delay* the handshake but cannot *forge* `K_B`'s signature over a
  fresh nonce. Unlike the A-R verdict, the signed pair is **re-verifiable offline
  and archivally**.
- It proves *receive* control, **not** send control (the reply's SMTP `From` is
  spoofable and irrelevant). Use fresh nonces, short expiry, and bind both `K`
  and `E` into the signed bytes.

## References

- *Cryptographic Analysis of Delta Chat* — USENIX Security '24
  ([ePrint 2024/918](https://eprint.iacr.org/2024/918)),
  [hardening writeup](https://delta.chat/en/2024-03-25-crypto-analysis-securejoin)
- RFC 8601 (Authentication-Results), RFC 7489 (DMARC alignment),
  RFC 6376 (DKIM)
