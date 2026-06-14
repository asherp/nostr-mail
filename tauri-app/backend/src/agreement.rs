//! Multi-recipient (group) encryption and agreement-workflow primitives.
//!
//! This module implements the wire-level pieces of nostr-mail spec v0.4 that
//! support DocuSign-style agreements (spec Sections 10 & 11):
//!
//!   * the `RECIPIENTS` block — per-recipient NIP-44-wrapped CEK + role
//!     (Section 10.2), with deterministic canonicalization (Section 4.2);
//!   * the `CONSENT` block — a signatory's explicit, intentional consent to a
//!     specific document hash `H` (Section 11.3);
//!   * the document hash `H` over the originating level (Section 11.3.1);
//!   * the per-level signing contribution `level(L)` that folds in the
//!     recipients and consent blocks so they are tamper-evident (Section 4.2);
//!   * agreement completion accounting — "M of N signatories signed"
//!     (Section 11.5).
//!
//! Most of these are pure functions over already-extracted block text; the
//! envelope encryption/decryption uses the primitives in [`crate::crypto`]
//! (`generate_cek`, `aes_gcm_encrypt_raw`, `wrap_cek`, `unwrap_cek`).
//! [`encode_hybrid_agreement`] composes them into a complete multi-recipient
//! armor message. Wiring into the recursive armor *parser* lives in `email.rs`.

use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Workflow role tokens used in a RECIPIENTS stanza (spec Section 10.2 / 11.1).
pub const ROLE_SIGNER: &str = "signer";
pub const ROLE_VIEWER: &str = "viewer";
pub const ROLE_SELF: &str = "self";

/// A single entry in a `RECIPIENTS` block (spec Section 10.2). After the fixed
/// `role` and `pubkey`, the remaining tokens are **optional and typed by
/// content**, in any order:
///
///   * `wrapped-cek` — base64 (no `@`, no `:`): `NIP44_encrypt(CEK)` to `pubkey`
///   * `email`       — contains `@`: the address this stanza was delivered to
///   * `reference`   — `scheme:value` (contains `:`): pointer to out-of-band
///                     material, e.g. a NIP-59 gift-wrapped DM (`evt:<id>`)
///
/// Any field may be absent: a plaintext (public) agreement omits the cek; a
/// gift-wrap-mode stanza carries only a `reference` (cek + email travel in the
/// referenced DM); a bare `role pubkey` stanza is workflow-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipient {
    /// `signer` | `viewer` | `self`, or an unknown future token (lowercased).
    pub role: String,
    /// Recipient's Nostr public key, hex (64 chars) or npub (bech32).
    pub pubkey: String,
    /// `NIP44_encrypt(CEK)` to `pubkey` (base64). `None` when the CEK is not
    /// carried in the email — a plaintext agreement (no CEK) or gift-wrap mode
    /// (the CEK travels in the referenced DM; see `reference`).
    pub wrapped_cek: Option<String>,
    /// The address this stanza was delivered to (contains `@`). Authenticated by
    /// the signature (§10.6), binding `(pubkey, email)` for the handshake (#102).
    /// `None` when not carried in the email (gift-wrap mode delivers it in the DM).
    pub email: Option<String>,
    /// A `scheme:value` pointer to out-of-band per-recipient material — e.g. a
    /// NIP-59 gift-wrapped DM (`evt:<id>`) carrying this recipient's CEK and/or
    /// `(npub, email)` binding (spec §11.7 gift-wrap mode). `None` when all
    /// material is in the email.
    pub reference: Option<String>,
}

impl Recipient {
    /// Serialize as its canonical line: `role pubkey [cek] [email] [reference]`,
    /// emitting only the present fields.
    pub fn to_line(&self) -> String {
        let mut s = format!("{} {}", self.role, self.pubkey);
        if let Some(cek) = &self.wrapped_cek {
            s.push(' ');
            s.push_str(cek);
        }
        if let Some(email) = &self.email {
            s.push(' ');
            s.push_str(email);
        }
        if let Some(reference) = &self.reference {
            s.push(' ');
            s.push_str(reference);
        }
        s
    }

    /// True for the `signer` role (a required signatory; spec Section 11.1).
    pub fn is_signer(&self) -> bool {
        self.role == ROLE_SIGNER
    }
}

/// A `CONSENT` block: a signatory's binding consent to document `H`
/// (spec Section 11.3). The block carries no signature of its own — the level's
/// existing SIGNATURE binds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Consent {
    /// The document hash `H` being consented to, hex (64 chars).
    pub agreement_hash: String,
    /// The consenting party's pubkey, hex or npub. MUST equal this level's
    /// SIGNATURE pubkey (spec Section 11.3).
    pub signer: String,
}

impl Consent {
    /// Serialize as the two canonical lines of a CONSENT block body.
    pub fn to_block_body(&self) -> String {
        format!("agreement {}\nsigner    {}", self.agreement_hash, self.signer)
    }
}

/// Strip leading email-quote prefixes (`>` optionally followed by a space,
/// possibly repeated for nested quoting) from a line. Mirrors how glossia
/// decoders ignore quote prefixes as non-payload (spec Sections 3.5.4, 10.2).
fn strip_quote_prefix(line: &str) -> &str {
    let mut s = line;
    loop {
        if let Some(rest) = s.strip_prefix("> ") {
            s = rest;
        } else if let Some(rest) = s.strip_prefix('>') {
            s = rest;
        } else {
            return s;
        }
    }
}

/// Canonicalize a block body for signing (spec Section 4.2): for each line,
/// strip any `> ` quote prefix and trailing whitespace; drop the (now-empty)
/// blank lines that Sections 10.2 / 11.3 require decoders to ignore; join the
/// surviving lines with `\n` and emit no trailing newline.
///
/// Used for both `canonical(recipients_L)` and `canonical(consent_L)`. An
/// absent block canonicalizes to the empty string (handled by the caller).
pub fn canonicalize_block(block_body: &str) -> String {
    block_body
        .split('\n')
        .map(|line| strip_quote_prefix(line).trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse a `RECIPIENTS` block body (the lines between `BEGIN NOSTR RECIPIENTS`
/// and the following delimiter) into entries (spec Section 10.2).
///
/// Each non-blank line is `<role> <pubkey>` followed by any subset of the typed
/// optional tokens, in any order (spec Section 10.2): an `email` (contains `@`),
/// a `reference` (`scheme:value`, contains `:`), and a `wrapped-cek` (base64 —
/// neither `@` nor `:`). Classification is by content, so the cek is never
/// confused with an email or a reference. The first token of each type wins;
/// further unrecognized tokens are tolerated (forward compatibility) and ignored.
/// Blank lines and `> ` quote prefixes are ignored. Lines without a `pubkey`
/// (fewer than 2 tokens) are skipped.
pub fn parse_recipients_block(block_body: &str) -> Vec<Recipient> {
    let mut out = Vec::new();
    for raw in block_body.split('\n') {
        let line = strip_quote_prefix(raw).trim();
        if line.is_empty() {
            continue;
        }
        let mut toks = line.split_whitespace();
        let role = match toks.next() {
            Some(t) => t.to_ascii_lowercase(),
            None => continue,
        };
        let pubkey = match toks.next() {
            Some(t) => t.to_string(),
            None => continue,
        };
        let mut wrapped_cek = None;
        let mut email = None;
        let mut reference = None;
        for tok in toks {
            if tok.contains('@') {
                email.get_or_insert_with(|| tok.to_string());
            } else if tok.contains(':') {
                reference.get_or_insert_with(|| tok.to_string());
            } else {
                wrapped_cek.get_or_insert_with(|| tok.to_string());
            }
        }
        out.push(Recipient { role, pubkey, wrapped_cek, email, reference });
    }
    out
}

/// Serialize recipients into a canonical RECIPIENTS block body (no BEGIN/END
/// delimiters). Entries are emitted in the order given; the caller is
/// responsible for the deterministic ordering of spec Section 10.2 (To, then
/// Cc, then `self` last).
pub fn serialize_recipients(recipients: &[Recipient]) -> String {
    recipients
        .iter()
        .map(Recipient::to_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse a `CONSENT` block body into a [`Consent`] (spec Section 11.3).
///
/// Recognizes the `agreement <H>` and `signer <pubkey>` lines; unknown lines
/// are tolerated and ignored (forward compatibility), as are blank lines and
/// `> ` quote prefixes. Returns `None` if either required field is absent.
pub fn parse_consent_block(block_body: &str) -> Option<Consent> {
    let mut agreement_hash: Option<String> = None;
    let mut signer: Option<String> = None;
    for raw in block_body.split('\n') {
        let line = strip_quote_prefix(raw).trim();
        if line.is_empty() {
            continue;
        }
        let mut toks = line.split_whitespace();
        match toks.next() {
            Some("agreement") => {
                if let Some(v) = toks.next() {
                    agreement_hash = Some(v.to_string());
                }
            }
            Some("signer") => {
                if let Some(v) = toks.next() {
                    signer = Some(v.to_string());
                }
            }
            _ => {} // unknown line — ignore
        }
    }
    Some(Consent {
        agreement_hash: agreement_hash?,
        signer: signer?,
    })
}

/// Normalize a pubkey (hex or npub) to lowercase 64-char hex for comparison.
/// Returns `None` if the input is neither a valid npub nor 32-byte hex.
pub fn normalize_pubkey_hex(pubkey: &str) -> Option<String> {
    use nostr_sdk::{FromBech32, PublicKey};
    let p = pubkey.trim();
    if let Ok(pk) = PublicKey::from_bech32(p) {
        return Some(pk.to_hex());
    }
    if let Ok(pk) = PublicKey::from_hex(p) {
        return Some(pk.to_hex());
    }
    None
}

/// Compute the document hash `H` for an agreement (spec Section 11.3.1):
///
/// ```text
/// H = SHA-256( decode(body_1) || canonical(recipients_1) )
/// ```
///
/// `body_1_decoded` is the originating level's decoded body bytes (glossia- or
/// base64-decoded), and `canonical_recipients_1` is the canonicalized
/// originating RECIPIENTS block (see [`canonicalize_block`]). CONSENT blocks are
/// deliberately excluded so that `H` is fixed for the life of the agreement.
/// Returns the 32-byte hash; use [`hex::encode`] for the on-wire form.
pub fn document_hash(body_1_decoded: &[u8], canonical_recipients_1: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(body_1_decoded);
    hasher.update(canonical_recipients_1.as_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Compute one level's signing contribution `level(L)` (spec Section 4.2):
///
/// ```text
/// level(L) = decode(body_L) || canonical(recipients_L) || canonical(consent_L)
/// ```
///
/// concatenated in the fixed order body → recipients → consent. Pass an empty
/// string for `canonical_recipients_l` / `canonical_consent_l` when the level
/// has no such block; a level with neither reduces to the plain Section 4 body
/// model. The full signing target for a level and its nested levels is
/// `SHA-256( level(L) || level(L-1) || … || level(1) )`.
pub fn level_signing_bytes(
    body_l_decoded: &[u8],
    canonical_recipients_l: &str,
    canonical_consent_l: &str,
) -> Vec<u8> {
    let mut bytes =
        Vec::with_capacity(body_l_decoded.len() + canonical_recipients_l.len() + canonical_consent_l.len());
    bytes.extend_from_slice(body_l_decoded);
    bytes.extend_from_slice(canonical_recipients_l.as_bytes());
    bytes.extend_from_slice(canonical_consent_l.as_bytes());
    bytes
}

/// Agreement completion summary (spec Section 11.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgreementStatus {
    /// Number of required signatories with a verified consent over `H`.
    pub m: usize,
    /// Total number of required signatories `N`.
    pub n: usize,
    /// `true` iff every required signatory has consented (`m == n` and `n > 0`).
    pub complete: bool,
    /// The required signatory set, normalized to hex, deduplicated.
    pub required_signers: Vec<String>,
    /// The subset of required signatories that have consented, normalized to hex.
    pub consented_signers: Vec<String>,
    /// The agreement's document hash `H` (hex), set when computed over a thread
    /// (Section 11.3.1). Empty for the bare [`compute_completion`] primitive,
    /// which is given the signatory sets directly and does not see the document.
    #[serde(default)]
    pub document_hash: String,
}

/// A proven email↔npub binding (issue #102): the `pubkey` is demonstrably
/// controlled by a party who also demonstrated read access to `email`.
///
/// The proof is a self-contained thread, so this verdict is **stateless** —
/// re-derivable from the message alone, with no outstanding-challenge store. The
/// issuer asserted the `(pubkey, email)` pairing in a RECIPIENTS stanza of a
/// level *they* signed; the holder of `pubkey` proved control + read access by
/// signing an outer level that quotes (nests) that signed challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    /// The bound party's Nostr pubkey, hex — proven controlled (reply signature).
    pub pubkey: String,
    /// The address bound to `pubkey` — asserted by the issuer in a signed
    /// RECIPIENTS stanza, proven by the reply's delivery/read access.
    pub email: String,
    /// The pubkey (hex) of the party who issued the challenge and asserted the
    /// pairing (the verifier's own key, for an issuer-side verification).
    pub issuer_pubkey: String,
}

/// Compute "M of N signed" completion (spec Section 11.5).
///
/// `required` is the required signatory set (the `signer` stanzas of the
/// originating RECIPIENTS block, plus the originator if they themselves
/// consented). `consented` is the set of pubkeys that carry a CONSENT block
/// over the agreement's `H` **bound by a verified signature** — the caller is
/// responsible for only passing pubkeys whose level signature verified.
///
/// Both sets are normalized to hex and deduplicated. A consenting pubkey that
/// is not in the required set (e.g. a `viewer` who consents anyway) does not
/// change `N` and is excluded from `M` (Section 11.5); the caller MAY surface
/// it informationally.
pub fn compute_completion(required: &[String], consented: &[String]) -> AgreementStatus {
    let required_signers = dedup_normalized(required);
    let consented_norm: Vec<String> = dedup_normalized(consented);

    let consented_signers: Vec<String> = required_signers
        .iter()
        .filter(|pk| consented_norm.contains(pk))
        .cloned()
        .collect();

    let n = required_signers.len();
    let m = consented_signers.len();
    AgreementStatus {
        m,
        n,
        complete: n > 0 && m == n,
        required_signers,
        consented_signers,
        document_hash: String::new(),
    }
}

/// A cryptographic recipient for [`encode_hybrid_agreement`]: a `To:`/`Cc:`
/// party with a role (`signer` for `To:`, `viewer` for `Cc:`; spec Section 6.3)
/// and their Nostr pubkey (hex or npub). The `self` stanza is added automatically
/// by the encoder, so it MUST NOT be passed here.
#[derive(Debug, Clone)]
pub struct AgreementRecipientInput {
    pub role: String,
    pub pubkey: String,
    /// The address this recipient was delivered to. When present it is written
    /// as the stanza's fourth token, binding `(pubkey, email)` under the
    /// signature (spec Sections 10.2, 10.6) — required for the binding handshake
    /// (issue #102). `None` when the recipient is known only by pubkey.
    pub email: Option<String>,
}

/// Compose a complete multi-recipient (group-encrypted) armor message — the
/// envelope of spec Sections 10–11.
///
/// The body is AES-256-GCM-encrypted once under a fresh CEK and emitted in a
/// generic `ENCRYPTED BODY` block (there is no `HYBRID` keyword — the presence
/// of the RECIPIENTS block selects the CEK-envelope path; Sections 8, 10.5).
/// The CEK is NIP-44 wrapped to every recipient plus a trailing `self` stanza
/// (Section 10.1, 10.4).
/// When `originator_consents` is true, the originator is themselves a required
/// signatory and a CONSENT block over the document hash `H` is included
/// (Section 11.2). The SIGNATURE covers body + recipients + consent per
/// Section 4.2, so membership, roles, and consent are all tamper-evident.
///
/// `recipients_in` should already be ordered `To:` (signers) then `Cc:`
/// (viewers) to match the deterministic ordering of Section 10.2; the `self`
/// stanza is appended last. Each recipient's optional `email` is written as the
/// stanza's fourth token, binding `(pubkey, email)` under the signature; pass
/// `sender_email` to do the same for the `self` stanza. Returns the armored
/// `text/plain` payload.
///
/// This composes the *cryptographic* envelope; glossia prose encoding of the
/// body/signature (Section 5) is applied by the caller's send pipeline if
/// desired — here the body is emitted as base64 and the signature/pubkey as hex,
/// both of which decoders accept.
pub fn encode_hybrid_agreement(
    sender_priv: &str,
    sender_pub: &str,
    sender_email: Option<&str>,
    profile_name: &str,
    body_plaintext: &[u8],
    recipients_in: &[AgreementRecipientInput],
    originator_consents: bool,
) -> Result<String> {
    let cek = crate::crypto::generate_cek();
    encode_hybrid_agreement_with_cek(
        &cek, sender_priv, sender_pub, sender_email, profile_name, body_plaintext, recipients_in, originator_consents,
    )
}

/// Like [`encode_hybrid_agreement`] but uses a caller-supplied CEK, so the same
/// key can also encrypt out-of-band material (e.g. the email subject) under one
/// envelope. The caller is responsible for generating a fresh random CEK
/// ([`crate::crypto::generate_cek`]) per message.
pub fn encode_hybrid_agreement_with_cek(
    cek: &[u8; 32],
    sender_priv: &str,
    sender_pub: &str,
    sender_email: Option<&str>,
    profile_name: &str,
    body_plaintext: &[u8],
    recipients_in: &[AgreementRecipientInput],
    originator_consents: bool,
) -> Result<String> {
    if recipients_in.is_empty() {
        return Err(anyhow::anyhow!(
            "encode_hybrid_agreement requires at least one recipient (use the pairwise format for single-recipient messages)"
        ));
    }
    let sender_pub_hex = normalize_pubkey_hex(sender_pub)
        .ok_or_else(|| anyhow::anyhow!("invalid sender pubkey"))?;

    // 1–2. Encrypt the body once under the CEK (Section 10.1 steps 1–2).
    let ciphertext = crate::crypto::aes_gcm_encrypt_raw(cek, body_plaintext)?;
    let body_b64 = general_purpose::STANDARD.encode(&ciphertext);

    // 3. Wrap the CEK to each recipient, then to self (Section 10.1 step 3, 10.4).
    let mut recipients: Vec<Recipient> = Vec::with_capacity(recipients_in.len() + 1);
    for r in recipients_in {
        let wrapped = crate::crypto::wrap_cek(sender_priv, &r.pubkey, cek)?;
        recipients.push(Recipient {
            role: r.role.to_ascii_lowercase(),
            pubkey: r.pubkey.clone(),
            wrapped_cek: Some(wrapped),
            email: r.email.clone(),
            reference: None,
        });
    }
    let self_wrapped = crate::crypto::wrap_cek(sender_priv, &sender_pub_hex, cek)?;
    recipients.push(Recipient {
        role: ROLE_SELF.to_string(),
        pubkey: sender_pub_hex.clone(),
        wrapped_cek: Some(self_wrapped),
        email: sender_email.map(|s| s.to_string()),
        reference: None,
    });

    let recipients_body = serialize_recipients(&recipients);
    let canon_recipients = canonicalize_block(&recipients_body);

    // The signed body bytes are the decoded armor body = the ciphertext bytes
    // (decode_armor_section base64-decodes the emitted body), matching §4/§4.2.
    let body_decoded = &ciphertext;

    // 4. Optional originator CONSENT over H (Sections 11.2, 11.3.1).
    let (consent_body, canon_consent) = if originator_consents {
        let h = document_hash(body_decoded, &canon_recipients);
        let consent = Consent {
            agreement_hash: hex::encode(h),
            signer: sender_pub_hex.clone(),
        };
        let body = consent.to_block_body();
        let canon = canonicalize_block(&body);
        (Some(body), canon)
    } else {
        (None, String::new())
    };

    // 5. Sign the §4.2 per-level target: body || recipients || consent.
    let signing_bytes = level_signing_bytes(body_decoded, &canon_recipients, &canon_consent);
    let sig_hex = crate::crypto::sign_data_bytes(sender_priv, &signing_bytes)?;

    // 6. Assemble the armor (body → RECIPIENTS → [CONSENT] → SIGNATURE; §11.3.2).
    // The body uses the generic `ENCRYPTED BODY` tag (AES-256-GCM under a CEK);
    // there is no dedicated keyword — the presence of the RECIPIENTS block is
    // what tells a decoder to take the CEK-envelope path rather than pairwise
    // NIP-44 (spec Sections 8, 10.5).
    let mut out = String::new();
    out.push_str("----- BEGIN NOSTR ENCRYPTED BODY -----\n");
    out.push_str(&body_b64);
    out.push('\n');
    out.push_str("----- BEGIN NOSTR RECIPIENTS -----\n");
    out.push_str(&recipients_body);
    out.push('\n');
    if let Some(ref cbody) = consent_body {
        out.push_str("----- BEGIN NOSTR CONSENT -----\n");
        out.push_str(cbody);
        out.push('\n');
    }
    out.push_str("----- BEGIN NOSTR SIGNATURE -----\n");
    out.push('@');
    out.push_str(profile_name);
    out.push('\n');
    out.push_str(&sig_hex);
    out.push('\n');
    out.push_str(&sender_pub_hex);
    out.push('\n');
    out.push_str("----- END NOSTR MESSAGE -----");
    Ok(out)
}

/// Normalize each pubkey to hex (dropping any that fail to parse) and dedup,
/// preserving first-seen order.
fn dedup_normalized(pubkeys: &[String]) -> Vec<String> {
    let mut seen = Vec::new();
    for pk in pubkeys {
        if let Some(hex) = normalize_pubkey_hex(pk) {
            if !seen.contains(&hex) {
                seen.push(hex);
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX_A: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const HEX_B: &str = "0000000000000000000000000000000000000000000000000000000000000002";

    // A valid pubkey hex (generator point x-coord) and its npub, for normalization tests.
    const PK_HEX: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn test_strip_quote_prefix() {
        assert_eq!(strip_quote_prefix("> hello"), "hello");
        assert_eq!(strip_quote_prefix(">> hello"), "hello");
        assert_eq!(strip_quote_prefix("> > hello"), "hello");
        assert_eq!(strip_quote_prefix(">hello"), "hello");
        assert_eq!(strip_quote_prefix("hello"), "hello");
    }

    #[test]
    fn test_parse_recipients_basic() {
        let block = format!(
            "signer {} wrapcekA\nviewer {} wrapcekB\nself {} wrapcekS",
            HEX_A, HEX_B, PK_HEX
        );
        let recips = parse_recipients_block(&block);
        assert_eq!(recips.len(), 3);
        assert_eq!(recips[0], Recipient { role: "signer".into(), pubkey: HEX_A.into(), wrapped_cek: Some("wrapcekA".into()), email: None, reference: None });
        assert_eq!(recips[1].role, "viewer");
        assert_eq!(recips[2].role, "self");
        assert!(recips.iter().all(|r| r.email.is_none() && r.reference.is_none()));
    }

    #[test]
    fn test_parse_recipients_typed_tokens_and_tolerates_quotes() {
        let block = format!(
            "> signer {} wrapcekA bob@example.com\n\n>> viewer {} wrapcekB\n",
            HEX_A, HEX_B
        );
        let recips = parse_recipients_block(&block);
        assert_eq!(recips.len(), 2);
        // base64 token → cek; '@' token → email.
        assert_eq!(recips[0].wrapped_cek.as_deref(), Some("wrapcekA"));
        assert_eq!(recips[0].email.as_deref(), Some("bob@example.com"));
        // A stanza without an email leaves it None.
        assert_eq!(recips[1].role, "viewer");
        assert!(recips[1].email.is_none());
    }

    #[test]
    fn test_parse_recipients_classifies_by_content_any_order() {
        // email (@), reference (scheme:value), cek (base64) — given out of order.
        let block = format!("signer {} bob@example.com evt:deadbeef wrapcekA", HEX_A);
        let recips = parse_recipients_block(&block);
        assert_eq!(recips.len(), 1);
        assert_eq!(recips[0].wrapped_cek.as_deref(), Some("wrapcekA"));
        assert_eq!(recips[0].email.as_deref(), Some("bob@example.com"));
        assert_eq!(recips[0].reference.as_deref(), Some("evt:deadbeef"));
    }

    #[test]
    fn test_parse_recipients_optional_fields() {
        // Plaintext (no cek), gift-wrap (reference only), and bare stanzas.
        let block = format!(
            "signer {} bob@example.com\nviewer {} evt:abc123\nself {}",
            HEX_A, HEX_B, PK_HEX
        );
        let recips = parse_recipients_block(&block);
        assert_eq!(recips.len(), 3);
        // plaintext signatory: email, no cek
        assert!(recips[0].wrapped_cek.is_none());
        assert_eq!(recips[0].email.as_deref(), Some("bob@example.com"));
        // gift-wrap stanza: reference only
        assert!(recips[1].wrapped_cek.is_none() && recips[1].email.is_none());
        assert_eq!(recips[1].reference.as_deref(), Some("evt:abc123"));
        // bare workflow-only stanza
        assert!(recips[2].wrapped_cek.is_none() && recips[2].email.is_none() && recips[2].reference.is_none());
    }

    #[test]
    fn test_parse_recipients_skips_lines_without_pubkey() {
        // `role pubkey` is the minimum; a lone token has no pubkey and is skipped.
        let block = format!("justonetoken\nsigner {} wrapcekB", HEX_B);
        let recips = parse_recipients_block(&block);
        assert_eq!(recips.len(), 1);
        assert_eq!(recips[0].pubkey, HEX_B);
    }

    #[test]
    fn test_serialize_recipients_roundtrip() {
        let recips = vec![
            Recipient { role: "signer".into(), pubkey: HEX_A.into(), wrapped_cek: Some("cekA".into()), email: Some("a@x.io".into()), reference: None },
            Recipient { role: "self".into(), pubkey: HEX_B.into(), wrapped_cek: Some("cekS".into()), email: None, reference: None },
            Recipient { role: "viewer".into(), pubkey: PK_HEX.into(), wrapped_cek: None, email: None, reference: Some("evt:abc".into()) },
        ];
        let body = serialize_recipients(&recips);
        // Only present fields are emitted, in cek/email/reference order.
        assert_eq!(body, format!("signer {} cekA a@x.io\nself {} cekS\nviewer {} evt:abc", HEX_A, HEX_B, PK_HEX));
        assert_eq!(parse_recipients_block(&body), recips);
    }

    #[test]
    fn test_canonicalize_block_strips_and_drops_blanks() {
        let raw = "> signer abc cek  \n\n>> viewer def cek2\n  \n";
        let canon = canonicalize_block(raw);
        assert_eq!(canon, "signer abc cek\nviewer def cek2");
        // No trailing newline.
        assert!(!canon.ends_with('\n'));
    }

    #[test]
    fn test_canonicalize_is_quote_prefix_invariant() {
        // The same logical block with and without email quoting canonicalizes identically.
        let plain = format!("signer {} cekA\nself {} cekS", HEX_A, HEX_B);
        let quoted = format!("> signer {} cekA\n> self {} cekS", HEX_A, HEX_B);
        assert_eq!(canonicalize_block(&plain), canonicalize_block(&quoted));
    }

    #[test]
    fn test_parse_consent_basic() {
        let block = format!("agreement {}\nsigner    {}", HEX_A, PK_HEX);
        let consent = parse_consent_block(&block).unwrap();
        assert_eq!(consent.agreement_hash, HEX_A);
        assert_eq!(consent.signer, PK_HEX);
    }

    #[test]
    fn test_parse_consent_tolerates_unknown_lines_and_quotes() {
        let block = format!("> agreement {}\n> future-field xyz\n> signer {}", HEX_A, PK_HEX);
        let consent = parse_consent_block(&block).unwrap();
        assert_eq!(consent.agreement_hash, HEX_A);
        assert_eq!(consent.signer, PK_HEX);
    }

    #[test]
    fn test_parse_consent_missing_field_is_none() {
        assert!(parse_consent_block(&format!("agreement {}", HEX_A)).is_none());
        assert!(parse_consent_block(&format!("signer {}", PK_HEX)).is_none());
    }

    #[test]
    fn test_consent_roundtrip() {
        let consent = Consent { agreement_hash: HEX_A.into(), signer: PK_HEX.into() };
        let parsed = parse_consent_block(&consent.to_block_body()).unwrap();
        assert_eq!(parsed, consent);
    }

    #[test]
    fn test_document_hash_excludes_consent_and_is_stable() {
        let body = b"This Mutual NDA is entered into as of 2026-06-13.";
        let recips = format!("signer {} cekA\nself {} cekS", HEX_A, HEX_B);
        let h1 = document_hash(body, &canonicalize_block(&recips));
        // Recomputing with the (quote-prefixed) same logical recipients yields identical H.
        let recips_quoted = format!("> signer {} cekA\n> self {} cekS", HEX_A, HEX_B);
        let h2 = document_hash(body, &canonicalize_block(&recips_quoted));
        assert_eq!(h1, h2);
        // Changing the body changes H.
        let h3 = document_hash(b"different terms", &canonicalize_block(&recips));
        assert_ne!(h1, h3);
        // Changing the recipient/role set changes H.
        let recips_tampered = format!("signer {} cekA\nsigner {} cekS", HEX_A, HEX_B);
        let h4 = document_hash(body, &canonicalize_block(&recips_tampered));
        assert_ne!(h1, h4);
    }

    #[test]
    fn test_level_signing_bytes_order_and_emptiness() {
        let body = b"body";
        let recips = "signer abc cek";
        let consent = "agreement H\nsigner abc";
        let full = level_signing_bytes(body, recips, consent);
        let mut expected = Vec::new();
        expected.extend_from_slice(body);
        expected.extend_from_slice(recips.as_bytes());
        expected.extend_from_slice(consent.as_bytes());
        assert_eq!(full, expected);

        // No recipients / consent → reduces to body only (Section 4 model).
        assert_eq!(level_signing_bytes(body, "", ""), body.to_vec());
    }

    #[test]
    fn test_normalize_pubkey_hex_accepts_hex_and_npub() {
        use nostr_sdk::{PublicKey, ToBech32};
        let npub = PublicKey::from_hex(PK_HEX).unwrap().to_bech32().unwrap();
        assert_eq!(normalize_pubkey_hex(PK_HEX).unwrap(), PK_HEX);
        assert_eq!(normalize_pubkey_hex(&npub).unwrap(), PK_HEX);
        assert_eq!(normalize_pubkey_hex(&format!("  {}  ", PK_HEX)).unwrap(), PK_HEX);
        assert!(normalize_pubkey_hex("not-a-key").is_none());
    }

    #[test]
    fn test_compute_completion_partial_and_full() {
        let alice = PK_HEX.to_string();
        let bob = nostr_sdk::Keys::generate().public_key().to_hex();
        let required = vec![alice.clone(), bob.clone()];
        // Only Alice has consented → 1 of 2, not complete.
        let status = compute_completion(&required, &[alice.clone()]);
        assert_eq!((status.m, status.n), (1, 2));
        assert!(!status.complete);

        // Both consented → 2 of 2, complete.
        let status = compute_completion(&required, &[alice.clone(), bob.clone()]);
        assert_eq!((status.m, status.n), (2, 2));
        assert!(status.complete);
    }

    #[test]
    fn test_compute_completion_dedups_and_ignores_non_required() {
        let alice = PK_HEX.to_string();
        let bob = nostr_sdk::Keys::generate().public_key().to_hex();
        let viewer = nostr_sdk::Keys::generate().public_key().to_hex();
        let required = vec![alice.clone(), bob.clone()];

        // Alice consents twice (dedup → counts once); a non-required viewer consents (ignored).
        let consented = vec![alice.clone(), alice.clone(), viewer.clone()];
        let status = compute_completion(&required, &consented);
        assert_eq!((status.m, status.n), (1, 2));
        assert!(!status.complete);
        assert_eq!(status.consented_signers, vec![alice]);
    }

    #[test]
    fn test_compute_completion_normalizes_npub_vs_hex() {
        use nostr_sdk::{PublicKey, ToBech32};
        let alice_npub = PublicKey::from_hex(PK_HEX).unwrap().to_bech32().unwrap();
        // Required lists hex, consent lists npub for the same identity → must match.
        let status = compute_completion(&[PK_HEX.to_string()], &[alice_npub]);
        assert_eq!((status.m, status.n), (1, 1));
        assert!(status.complete);
    }

    #[test]
    fn test_compute_completion_empty_required_not_complete() {
        let status = compute_completion(&[], &[]);
        assert_eq!((status.m, status.n), (0, 0));
        assert!(!status.complete);
    }

    // ── encode_hybrid_agreement round-trips ──────────────────────────────

    /// Recover the body from an encoded message as a given reader (by unwrapping
    /// the matching RECIPIENTS stanza and AES-GCM-decrypting). Returns the bytes.
    fn reader_recovers_body(armor: &str, reader_priv: &str, reader_pub_hex: &str, sender_pub_hex: &str) -> Vec<u8> {
        // Extract the base64 body (line after the ENCRYPTED BODY BEGIN).
        let body_b64 = armor
            .lines()
            .skip_while(|l| !l.contains("BEGIN NOSTR ENCRYPTED BODY"))
            .nth(1)
            .expect("body line")
            .trim()
            .to_string();
        let ciphertext = general_purpose::STANDARD.decode(body_b64).expect("b64 body");

        // Extract the RECIPIENTS block body.
        let recip_body: String = armor
            .lines()
            .skip_while(|l| !l.contains("BEGIN NOSTR RECIPIENTS"))
            .skip(1)
            .take_while(|l| !l.contains("BEGIN NOSTR"))
            .collect::<Vec<_>>()
            .join("\n");
        let recips = parse_recipients_block(&recip_body);
        let stanza = recips
            .iter()
            .find(|r| normalize_pubkey_hex(&r.pubkey).as_deref() == Some(reader_pub_hex))
            .expect("reader has a stanza");
        let cek = crate::crypto::unwrap_cek(reader_priv, sender_pub_hex, stanza.wrapped_cek.as_deref().unwrap()).unwrap();
        crate::crypto::aes_gcm_decrypt_raw(&cek, &ciphertext).unwrap()
    }

    #[test]
    fn test_encode_hybrid_agreement_roundtrip_two_recipients() {
        let sender = crate::crypto::generate_keypair().unwrap();
        let sender_pub_hex = crate::crypto::get_public_key_from_private(&sender.private_key).unwrap();
        let sender_pub_hex = normalize_pubkey_hex(&sender_pub_hex).unwrap();
        let alice = crate::crypto::generate_keypair().unwrap();
        let bob = crate::crypto::generate_keypair().unwrap();
        let alice_hex = normalize_pubkey_hex(&alice.public_key).unwrap();
        let bob_hex = normalize_pubkey_hex(&bob.public_key).unwrap();

        let body = b"This Mutual NDA is entered into as of 2026-06-13.";
        let recips = vec![
            AgreementRecipientInput { role: ROLE_SIGNER.into(), pubkey: alice.public_key.clone(), email: Some("alice@example.com".into()) },
            AgreementRecipientInput { role: ROLE_VIEWER.into(), pubkey: bob.public_key.clone(), email: Some("bob@example.org".into()) },
        ];
        let armor = encode_hybrid_agreement(
            &sender.private_key, &sender.public_key, Some("me@example.net"), "Originator", body, &recips, false,
        ).unwrap();

        // Structure: generic encrypted body, recipients (incl. self last), no consent.
        assert!(armor.contains("BEGIN NOSTR ENCRYPTED BODY"));
        assert!(armor.contains("BEGIN NOSTR RECIPIENTS"));
        assert!(!armor.contains("BEGIN NOSTR CONSENT"));
        let recip_body: String = armor
            .lines()
            .skip_while(|l| !l.contains("BEGIN NOSTR RECIPIENTS"))
            .skip(1)
            .take_while(|l| !l.contains("BEGIN NOSTR"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = parse_recipients_block(&recip_body);
        assert_eq!(parsed.len(), 3, "two recipients + self");
        assert_eq!(parsed[2].role, ROLE_SELF, "self stanza is last");

        // The (pubkey, email) pairing is carried inside the signed block (§10.2/#102).
        assert_eq!(parsed[0].email.as_deref(), Some("alice@example.com"));
        assert_eq!(parsed[1].email.as_deref(), Some("bob@example.org"));
        assert_eq!(parsed[2].email.as_deref(), Some("me@example.net"));

        // Each party (incl. self) recovers the exact body.
        assert_eq!(reader_recovers_body(&armor, &alice.private_key, &alice_hex, &sender_pub_hex), body);
        assert_eq!(reader_recovers_body(&armor, &bob.private_key, &bob_hex, &sender_pub_hex), body);
        assert_eq!(reader_recovers_body(&armor, &sender.private_key, &sender_pub_hex, &sender_pub_hex), body);
    }

    #[test]
    fn test_encode_hybrid_agreement_with_consent_has_matching_h() {
        let sender = crate::crypto::generate_keypair().unwrap();
        let sender_pub_hex = normalize_pubkey_hex(
            &crate::crypto::get_public_key_from_private(&sender.private_key).unwrap()).unwrap();
        let alice = crate::crypto::generate_keypair().unwrap();

        let body = b"terms";
        let recips = vec![
            AgreementRecipientInput { role: ROLE_SIGNER.into(), pubkey: alice.public_key.clone(), email: None },
        ];
        let armor = encode_hybrid_agreement(
            &sender.private_key, &sender.public_key, None, "Originator", body, &recips, true,
        ).unwrap();

        assert!(armor.contains("BEGIN NOSTR CONSENT"));
        let consent_body: String = armor
            .lines()
            .skip_while(|l| !l.contains("BEGIN NOSTR CONSENT"))
            .skip(1)
            .take_while(|l| !l.contains("BEGIN NOSTR"))
            .collect::<Vec<_>>()
            .join("\n");
        let consent = parse_consent_block(&consent_body).expect("consent parses");
        assert_eq!(normalize_pubkey_hex(&consent.signer).unwrap(), sender_pub_hex);

        // The consent's H must equal SHA-256(ciphertext || canonical(recipients)).
        let body_b64 = armor.lines()
            .skip_while(|l| !l.contains("BEGIN NOSTR ENCRYPTED BODY")).nth(1).unwrap().trim();
        let ciphertext = general_purpose::STANDARD.decode(body_b64).unwrap();
        let recip_body: String = armor.lines()
            .skip_while(|l| !l.contains("BEGIN NOSTR RECIPIENTS")).skip(1)
            .take_while(|l| !l.contains("BEGIN NOSTR")).collect::<Vec<_>>().join("\n");
        let h = document_hash(&ciphertext, &canonicalize_block(&recip_body));
        assert_eq!(consent.agreement_hash, hex::encode(h));
    }

    #[test]
    fn test_encode_hybrid_agreement_rejects_empty_recipients() {
        let sender = crate::crypto::generate_keypair().unwrap();
        let err = encode_hybrid_agreement(
            &sender.private_key, &sender.public_key, None, "X", b"x", &[], false);
        assert!(err.is_err());
    }
}
