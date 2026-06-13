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
//! These are pure functions over already-extracted block text; the actual
//! envelope encryption/decryption uses the primitives in [`crate::crypto`]
//! (`generate_cek`, `aes_gcm_encrypt_raw`, `wrap_cek`, `unwrap_cek`). Wiring
//! these into the recursive armor parser/encoder lives in `email.rs`.

use sha2::{Digest, Sha256};

/// Workflow role tokens used in a RECIPIENTS stanza (spec Section 10.2 / 11.1).
pub const ROLE_SIGNER: &str = "signer";
pub const ROLE_VIEWER: &str = "viewer";
pub const ROLE_SELF: &str = "self";

/// A single entry in a `RECIPIENTS` block: `<role> <pubkey> <wrapped-cek>`
/// (spec Section 10.2). `pubkey` and `wrapped_cek` are retained exactly as they
/// appear on the wire (hex/npub for the key, base64 NIP-44 payload for the CEK).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    /// `signer` | `viewer` | `self`, or an unknown future token (lowercased).
    pub role: String,
    /// Recipient's Nostr public key, hex (64 chars) or npub (bech32).
    pub pubkey: String,
    /// `NIP44_encrypt(CEK)` to `pubkey`, base64.
    pub wrapped_cek: String,
}

impl Recipient {
    /// Serialize this entry as its canonical single line: `role pubkey wrapped-cek`.
    pub fn to_line(&self) -> String {
        format!("{} {} {}", self.role, self.pubkey, self.wrapped_cek)
    }

    /// True for the `signer` role (a required signatory; spec Section 11.1).
    pub fn is_signer(&self) -> bool {
        self.role == ROLE_SIGNER
    }
}

/// A `CONSENT` block: a signatory's binding consent to document `H`
/// (spec Section 11.3). The block carries no signature of its own — the level's
/// existing SIGNATURE binds it.
#[derive(Debug, Clone, PartialEq, Eq)]
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
/// Each non-blank line must have at least three space-separated tokens
/// `<role> <pubkey> <wrapped-cek>`; additional trailing tokens are tolerated
/// (forward compatibility) and ignored. Blank lines and `> ` quote prefixes are
/// ignored. Malformed lines (fewer than 3 tokens) are skipped.
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
        let wrapped_cek = match toks.next() {
            Some(t) => t.to_string(),
            None => continue,
        };
        out.push(Recipient { role, pubkey, wrapped_cek });
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    }
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
        assert_eq!(recips[0], Recipient { role: "signer".into(), pubkey: HEX_A.into(), wrapped_cek: "wrapcekA".into() });
        assert_eq!(recips[1].role, "viewer");
        assert_eq!(recips[2].role, "self");
    }

    #[test]
    fn test_parse_recipients_tolerates_extra_tokens_blanks_and_quotes() {
        let block = format!(
            "> signer {} wrapcekA extratoken\n\n>> viewer {} wrapcekB\n",
            HEX_A, HEX_B
        );
        let recips = parse_recipients_block(&block);
        assert_eq!(recips.len(), 2);
        // Extra trailing token is ignored, not folded into wrapped_cek.
        assert_eq!(recips[0].wrapped_cek, "wrapcekA");
        assert_eq!(recips[1].role, "viewer");
    }

    #[test]
    fn test_parse_recipients_skips_malformed_lines() {
        let block = format!("signer {}\njustonetoken\nsigner {} wrapcekB", HEX_A, HEX_B);
        let recips = parse_recipients_block(&block);
        assert_eq!(recips.len(), 1);
        assert_eq!(recips[0].pubkey, HEX_B);
    }

    #[test]
    fn test_serialize_recipients_roundtrip() {
        let recips = vec![
            Recipient { role: "signer".into(), pubkey: HEX_A.into(), wrapped_cek: "cekA".into() },
            Recipient { role: "self".into(), pubkey: HEX_B.into(), wrapped_cek: "cekS".into() },
        ];
        let body = serialize_recipients(&recips);
        assert_eq!(body, format!("signer {} cekA\nself {} cekS", HEX_A, HEX_B));
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
}
