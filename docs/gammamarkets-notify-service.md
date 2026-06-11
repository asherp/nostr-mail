# GammaMarkets Email Notification Service

**Version:** 0.1.0-draft
**Date:** 2026-06-11
**Status:** Design proposal

## 1. Problem

The [GammaMarkets](https://github.com/GammaMarkets) market spec extends NIP-99 into
a full e-commerce protocol, and [nostr.boutique](https://nostr.boutique) storefronts
are deployed as **nsites** — static websites whose files live on Blossom servers,
addressed by a `kind:34128` path→hash map on relays
([nsite / NIP PR #1538](https://github.com/nostr-protocol/nips/pull/1538)).

An nsite is **pure client-side**: it runs only browser JavaScript (`fetch`, relay
websockets, a NIP-07 signer). It has **no server**, holds **no secrets**, and cannot
open SMTP sockets. Therefore an nsite **cannot send email itself**. Email requires an
always-on component that holds mail credentials.

The goal of this service is to provide that component as a small, stateless,
self-hostable piece — **not** the full nostr-mail desktop/mobile app — so that a
storefront can turn a commerce event into an email notification with a single HTTP
call.

## 2. Architecture options

| | A — Notify microservice (push) | B — Relay-watcher bridge (pull) |
|---|---|---|
| Trigger | nsite `POST`s to `/notify` | nsite publishes a normal `kind:16` commerce event; bridge subscribes |
| nsite changes | one `fetch()` | none |
| Public endpoint | yes (needs auth/anti-abuse) | no |
| Decentralization | operator holds SMTP creds | operator runs a daemon + opt-in registry |
| Recommended for | unblocking nsites now | longer-term decentralized story |

This document specifies **Option A**. Option B can be layered on later, reusing the
same `send_email` engine; the two are not mutually exclusive.

## 3. What already exists in nostr-mail

The service is a thin wrapper over code that is already in `tauri-app/backend`:

- **Headless server scaffold** — `bin/http_server.rs`, an axum server on `:1420` with
  `GET /health` and a `POST /invoke` command dispatcher. Today it does **not** expose
  email sending (only `sync_sent_emails`). We add a `/notify` route.
- **Email engine** — `email.rs::send_email(...)` takes `to_address`, `subject`, `body`,
  `recipient_pubkey: Option<&str>`, and `include_pubkey_header` / `include_sig_header` /
  `include_recipient_header` flags. It signs, attaches `X-Nostr-*` transport headers, and
  sends via SMTP.
- **Encryption** — `crypto.rs` (NIP-44 / NIP-04) for producing the armored encrypted body.
- **npub → email resolution** — `kind:0` profile `email` field, fetched via the existing
  `fetch_profile(pubkey, relays)` path (`lib.rs:1414`) and parsed by
  `parse_profile_from_event` (`nostr.rs:379`).

So the build is: **add a `/notify` handler that resolves npub→email, optionally encrypts,
and calls `send_email`** — plus auth and a service identity key.

## 4. `/notify` API contract

The endpoint an nsite (or any GammaMarkets backend) codes against. Framework-agnostic:
the storefront only needs `fetch`.

### Authentication (NIP-98)

The caller authenticates each request with a **[NIP-98](https://github.com/nostr-protocol/nips/blob/master/98.md)
HTTP-Auth** token rather than a shared API key. This is essential because an nsite is static
and **cannot hold a secret** — anything baked into the page is public. With NIP-98 the
storefront's own Nostr key signs each call, and the service learns *which npub* is calling
without any shared secret shipping in the static JS.

The nsite obtains the signature in-browser via **[NIP-07](https://nips.nostr.com/7)**
(`window.nostr.signEvent`) — provided by the visitor's signer extension (nos2x, Alby,
Nostrame) or a NIP-46 remote signer. nsites can use NIP-07 because they are ordinary browser
pages; the nsite hosting protocol (kind 34128) is not involved.

```
POST /notify
Authorization: Nostr <base64(kind:27235 event)>   // NIP-98: tags u, method, payload=sha256(body)
Content-Type: application/json
```

The service verifies: kind `27235`, timestamp within ~60s, `u` matches the request URL,
`method` matches, and `payload` matches `SHA-256(body)`. The signing pubkey is the
authenticated caller. The service MAY additionally require that caller to be an authorized
storefront (allowlist) and/or rate-limit per pubkey.

```jsonc
{
  // REQUIRED — who to notify. Service resolves this to an email via kind:0.
  "recipient_npub": "npub1...",

  // OPTIONAL — overrides npub resolution if the caller already knows the address.
  "recipient_email": "merchant@example.com",

  // REQUIRED — notification category, drives subject/template.
  // One of: order_new | payment_request | order_status | shipping_update | payment_receipt | message
  "type": "order_new",

  // REQUIRED — content mode. "plaintext" (default) sends a nudge with no sensitive data.
  // "encrypted" NIP-encrypts `details` to recipient_npub using the service identity key.
  "mode": "plaintext",

  // OPTIONAL — relays to use for kind:0 resolution + (encrypted mode) recipient inbox relays.
  "relays": ["wss://relay.damus.io"],

  // Notification payload. For plaintext mode only non-sensitive fields are rendered.
  "order": {
    "id": "order-abc123",
    "merchant_npub": "npub1...",
    "amount_sats": 21000,
    "item_count": 2,
    "status": "pending",
    "marketplace_url": "https://npub1....nsite.lol/orders/abc123"
  },

  // OPTIONAL — full human-readable body used verbatim in encrypted mode.
  "details": "Order #abc123\n2x Widget — 21,000 sats\nShip to: ..."
}
```

### Behavior

1. **Authenticate** the NIP-98 token (kind/timestamp/url/method/payload). Reject otherwise
   (`401`). Optionally check the signing pubkey against an authorized-storefront allowlist.
2. **Resolve recipient**: use `recipient_email` if present; else consult the private
   `npub → email` mapping learned from a `reply-to-email` opt-in DM (see §5.1); else fall back
   to the recipient's
   `kind:0` over `relays` and read the `email` field. If neither yields an address →
   `422 no_email_for_npub`.
3. **Compose**:
   - `mode: "plaintext"` (default): render a template for `type` containing only
     non-sensitive fields (order id, a "you have a new order" nudge, and
     `marketplace_url`). **No amounts/addresses unless the operator opts in.**
   - `mode: "encrypted"`: NIP-44-encrypt `details` (or a rendered template) to
     `recipient_npub` using the **service identity key** as sender, producing the
     nostr-mail armor body; set `recipient_pubkey` + `include_*_header` so nostr-mail
     clients can decrypt/verify.
4. **Send** via `send_email(...)` over the configured SMTP.
5. **Respond.**

### Response

```jsonc
// 200
{ "ok": true, "message_id": "<...@...>", "delivered_to": "merchant@example.com", "mode": "plaintext" }

// 422 — resolvable request, but no address
{ "ok": false, "error": "no_email_for_npub", "recipient_npub": "npub1..." }

// 4xx/5xx
{ "ok": false, "error": "unauthorized" | "invalid_request" | "smtp_error", "detail": "..." }
```

### nsite integration (the entire client side)

```js
const url = "https://notify.example.com/notify";
const body = JSON.stringify({
  recipient_npub: merchantNpub,
  type: "order_new",
  mode: "plaintext",
  order: { id: orderId, amount_sats: total, item_count: items.length,
           marketplace_url: orderUrl },
});

// NIP-98 token, signed by the storefront key via the visitor's NIP-07 signer.
const authEvent = await window.nostr.signEvent({
  kind: 27235,
  created_at: Math.floor(Date.now() / 1000),
  content: "",
  tags: [["u", url], ["method", "POST"], ["payload", await sha256Hex(body)]],
});
const token = btoa(JSON.stringify(authEvent));

await fetch(url, {
  method: "POST",
  headers: { "Content-Type": "application/json", Authorization: `Nostr ${token}` },
  body,
});
```

No secret ships in the static page — the signer holds the key, and the service verifies the
signature.

## 5. Plaintext vs. encrypted

- **Plaintext nudge (default):** "You have a new order — open your storefront/client."
  No SMTP-leakable order details, no sender key required for confidentiality. Safest
  default and the path that works even when the recipient has no Nostr-mail client.
- **Encrypted full details:** order body NIP-44-encrypted to the recipient's npub.
  Requires (a) a **service identity key** to act as the encrypting sender and (b) the
  recipient to have a nostr-mail-compatible client to decrypt. Decryptable in the
  nostr-mail app today via the existing armor/`X-Nostr-*` pipeline.

Operator config selects the default; callers may request a stronger/weaker mode per call,
bounded by an operator-set maximum.

### 5.1 Consent & signup via `reply-to-email` DM

The cleanest way for a user to **opt into** notifications is the nostr-mail spec's proposed
[private email exchange via DM](nostr-mail-spec.md#11-proposed-extension-draft-private-email-address-exchange-via-dm).
The subscriber sends **one NIP-17 gift-wrapped DM** to the merchant/service pubkey carrying a
`reply-to-email` tag with purpose `notify`:

```jsonc
{ "kind": 14,
  "tags": [["p", "<service_pubkey>"], ["reply-to-email", "alice@example.com", "notify"]],
  "content": "Subscribe me to order notifications." }
```

That single message **proves the subscriber's identity** (the NIP-17 seal is signed by their
real key), **grants consent**, and **privately delivers the address** — so the service never
needs the address in a public `kind:0`, and never emails an npub that did not ask for it.

This dovetails with §4 resolution: the service prefers a `reply-to-email` mapping learned from
an authenticated opt-in DM over the public-profile fallback. The same signature primitive (a
Nostr key signing a structured event) authenticates both signup (the DM seal) and each send
(the NIP-98 token).

## 6. Open decisions (for the GammaMarkets operator)

1. **Who hosts it?** A GammaMarkets-operated service (one SMTP sender, server-to-server)
   vs. per-merchant self-host. Determines trust + deliverability (SPF/DKIM on the sender
   domain).
2. **Auth & anti-abuse.** The endpoint is reachable from public nsite JS, which **cannot
   hold a secret** — so a baked-in API key is out. Use **NIP-98 HTTP-Auth** (§4): the
   storefront key signs each call via the visitor's NIP-07 signer, and the service verifies
   the signature. Layer per-pubkey rate limiting and an optional authorized-storefront
   allowlist on top. **Recommended:** NIP-98 + per-pubkey rate limit.
3. **Opt-in / consent.** Prefer the `reply-to-email` opt-in DM (§5.1) as the consent record —
   an authenticated, private subscription. Fall back to public `kind:0` `email` only if the
   operator allows it. Never email an npub that has neither opted in nor published an address.
4. **Service identity key.** Required for encrypted mode; also defines the `From:` Nostr
   identity recipients see. Provision and document its npub.
5. **Deliverability.** A shared sender domain needs SPF/DKIM/DMARC; otherwise nudges land
   in spam.

## 7. Scoped build plan

Phase 1 — **Plaintext notify microservice**
- Add `POST /notify` to `bin/http_server.rs` (or a dedicated `bin/notify_server.rs`).
- **NIP-98** token verification + per-pubkey rate limit.
- npub→email resolution via existing `fetch_profile`.
- Templates per `type`; call `send_email` (plaintext, no encryption).
- Dockerfile + config (SMTP creds, authorized-storefront allowlist, default relays) via env.

Phase 2 — **Consent & encrypted mode**
- `reply-to-email` opt-in DM ingestion (§5.1): read the tag on unwrapped gift wraps, store the
  private `npub → email` mapping, prefer it over public `kind:0`.
- Service identity key config; NIP-44 encryption of `details` via `crypto.rs`; set
  `recipient_pubkey` + `include_*_header` on `send_email`.

Phase 3 — **Hardening**
- Authorized-storefront allowlist, observability, abuse controls, deliverability (SPF/DKIM).

Phase 4 (optional) — **Option B bridge**
- Always-on relay subscription for `kind:16/17` commerce events → reuse the same notify
  path. No nsite changes required.
