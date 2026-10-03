//! Attachment manifest: build and parse the hybrid attachment envelope of spec
//! §11.2.
//!
//! When a message carries attachments (or a very large body), the encrypted
//! *body* is a serialized **manifest** rather than the raw text: a nested
//! AES-encrypted body blob plus one entry per attachment carrying that
//! attachment's AES key, MIME type, and ciphertext hash. The encrypted
//! attachment bytes themselves travel as separate MIME parts (`a1.dat`, …); the
//! manifest only carries the metadata + keys needed to find and decrypt them.
//!
//! The manifest is protected by whatever encrypts the body around it — pairwise
//! NIP-44 (1:1) or the per-recipient-wrapped CEK (multi-recipient) — so the
//! cleartext AES keys inside it are only readable once that layer is opened.
//! Because the manifest (including every `cipherSha256`) is inside the signed
//! body, attachments are bound to the message tamper-evidently even though their
//! bytes ride outside the armor.
//!
//! ## Wire format
//!
//! New manifests are **Cap'n Proto** (schema `Manifest`/`EncryptedBlob`/
//! `Attachment`). The manifest is the plaintext *body*, encrypted by the outer
//! layer (pairwise NIP-44 or the per-recipient-wrapped CEK) and then glossia-
//! encoded for transport, so it never touches the wire in the clear. The
//! serialized capnp rides behind one of two markers depending on the transport:
//!
//!   * [`CAPNP_PREFIX`] (`capnp:`) — raw bytes, for the byte-clean CEK envelope
//!     (multi-recipient).
//!   * [`CAPNP_B64_PREFIX`] (`capnp64:`) — base64, for the string-typed NIP-44
//!     transport (1:1), whose decrypt yields a `String` and so can't carry raw
//!     binary.
//!
//! The decoder also still reads the **legacy JSON** manifest (first byte `{`) so
//! old emails keep opening.

use anyhow::Result;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Marker prefixing the *raw* Cap'n Proto manifest bytes, used on byte-clean
/// transports (the multi-recipient CEK envelope). A decoder distinguishes capnp
/// (`capnp:…`) from a legacy JSON manifest (`{…`) and from a plaintext body.
pub const CAPNP_PREFIX: &str = "capnp:";

/// Marker prefixing a *base64* Cap'n Proto manifest, used on the string-typed
/// transport (pairwise NIP-44, whose decrypt yields a `String` and so cannot
/// carry raw binary). The bytes after the marker are base64 of the same
/// serialized capnp message that [`CAPNP_PREFIX`] carries raw.
pub const CAPNP_B64_PREFIX: &str = "capnp64:";

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// A plaintext attachment to be encrypted into the manifest.
pub struct AttachmentInput {
    pub filename: String,
    pub mime: String,
    pub data: Vec<u8>,
}

/// An encrypted attachment, ready to ride as a MIME part named `<id>.dat`.
pub struct EncryptedAttachmentPart {
    pub opaque_id: String,
    pub filename: String,
    /// AES-256-GCM ciphertext (size-prefixed + padded); the MIME part body.
    pub ciphertext: Vec<u8>,
}

/// One parsed attachment entry (unified across capnp + legacy JSON).
pub struct ParsedAttachment {
    pub id: String,
    pub orig_filename: String,
    pub orig_mime: String,
    /// Raw 32-byte SHA-256 of the encrypted file (hex for the UI).
    pub cipher_sha256: Vec<u8>,
    pub cipher_size: u64,
    /// Raw AES-256 key (base64 for the UI / `decrypt_attachment_pipeline`).
    pub key_wrap: Vec<u8>,
}

impl ParsedAttachment {
    pub fn cipher_sha256_hex(&self) -> String {
        hex::encode(&self.cipher_sha256)
    }
    pub fn key_wrap_b64(&self) -> String {
        b64().encode(&self.key_wrap)
    }
}

/// A parsed manifest: the already-decrypted body text plus attachment metadata.
pub struct ParsedManifest {
    /// `Some` once the nested body blob has been AES-decrypted; `None` if there
    /// was no body blob or it failed to decrypt.
    pub body_text: Option<String>,
    pub attachments: Vec<ParsedAttachment>,
}

/// Serialize a Cap'n Proto manifest from a plaintext body + attachments.
///
/// AES-encrypts the body (raw) and each attachment (size-prefixed + padded)
/// under independent random keys, records each in the manifest, and returns the
/// serialized capnp bytes (no marker) plus the encrypted attachment MIME parts.
/// Callers wrap the bytes for their transport via [`build_capnp_manifest`] (raw)
/// or [`build_capnp_manifest_armored`] (base64).
fn build_capnp_bytes(
    body: &str,
    attachments: &[AttachmentInput],
) -> Result<(Vec<u8>, Vec<EncryptedAttachmentPart>)> {
    let mut message = ::capnp::message::Builder::new_default();
    let mut manifest = message.init_root::<crate::nostr_mail_capnp::manifest::Builder>();
    manifest.set_version(0);

    // Body blob: AES-256-GCM under its own key (no inner base64 — capnp Data is
    // binary-safe, unlike the legacy JSON manifest).
    {
        let body_key = crate::crypto::generate_cek();
        let body_ct = crate::crypto::aes_gcm_encrypt_raw(&body_key, body.as_bytes())?;
        let sha = Sha256::digest(&body_ct);
        let mut blob = manifest.reborrow().init_body();
        blob.set_ciphertext(&body_ct);
        blob.set_cipher_sha256(&sha);
        blob.set_key_wrap(&body_key);
        blob.set_cipher_size(body_ct.len() as u64);
    }

    let mut parts = Vec::with_capacity(attachments.len());
    {
        let mut att_list = manifest
            .reborrow()
            .init_attachments(attachments.len() as u32);
        for (i, att) in attachments.iter().enumerate() {
            let id = format!("a{}", i + 1);
            let att_key = crate::crypto::generate_cek();
            let ct = crate::crypto::aes_gcm_encrypt_padded(&att_key, &att.data)?;
            let sha = Sha256::digest(&ct);
            {
                let mut entry = att_list.reborrow().get(i as u32);
                entry.set_id(id.as_str());
                entry.set_orig_filename(att.filename.as_str());
                entry.set_orig_mime(att.mime.as_str());
                entry.set_cipher_sha256(&sha);
                entry.set_cipher_size(ct.len() as u64);
                entry.set_key_wrap(&att_key);
            }
            parts.push(EncryptedAttachmentPart {
                opaque_id: id.clone(),
                filename: format!("{}.dat", id),
                ciphertext: ct,
            });
        }
    }

    let mut bytes = Vec::new();
    ::capnp::serialize::write_message(&mut bytes, &message)?;
    Ok((bytes, parts))
}

/// Build a manifest for a **byte-clean** transport (the CEK envelope): the
/// [`CAPNP_PREFIX`] marker followed by the raw serialized capnp bytes (to be
/// encrypted as the body), plus the encrypted attachment MIME parts.
pub fn build_capnp_manifest(
    body: &str,
    attachments: &[AttachmentInput],
) -> Result<(Vec<u8>, Vec<EncryptedAttachmentPart>)> {
    let (bytes, parts) = build_capnp_bytes(body, attachments)?;
    let mut payload = CAPNP_PREFIX.as_bytes().to_vec();
    payload.extend_from_slice(&bytes);
    Ok((payload, parts))
}

/// Build a manifest for a **string-typed** transport (pairwise NIP-44): the
/// [`CAPNP_B64_PREFIX`] marker followed by base64 of the serialized capnp bytes,
/// so the payload is text-safe for an API whose decrypt yields a `String`. The
/// returned payload is what the caller NIP-encrypts as the body.
pub fn build_capnp_manifest_armored(
    body: &str,
    attachments: &[AttachmentInput],
) -> Result<(String, Vec<EncryptedAttachmentPart>)> {
    let (bytes, parts) = build_capnp_bytes(body, attachments)?;
    Ok((format!("{}{}", CAPNP_B64_PREFIX, b64().encode(&bytes)), parts))
}

/// Parse a decrypted body payload as a manifest, decrypting the nested body blob.
///
/// Returns `None` when the payload is not a manifest (an ordinary plaintext
/// body). Detects a Cap'n Proto manifest by the [`CAPNP_PREFIX`] marker (the
/// remaining bytes are the raw serialized capnp) and a legacy JSON manifest by a
/// leading `{`.
pub fn parse_manifest(payload: &[u8]) -> Option<ParsedManifest> {
    // base64 capnp (NIP-44 transport) — check before the raw `capnp:` prefix,
    // which is NOT a prefix of `capnp64:`.
    if let Some(rest) = payload.strip_prefix(CAPNP_B64_PREFIX.as_bytes()) {
        let bytes = b64().decode(std::str::from_utf8(rest).ok()?.trim()).ok()?;
        return parse_capnp_manifest(&bytes);
    }
    // raw capnp (CEK transport)
    if let Some(rest) = payload.strip_prefix(CAPNP_PREFIX.as_bytes()) {
        return parse_capnp_manifest(rest);
    }
    // Legacy JSON manifest (or an ordinary plaintext body): text-typed.
    let t = std::str::from_utf8(payload).ok()?.trim();
    if t.starts_with('{') {
        return parse_json_manifest(t);
    }
    None
}

fn parse_capnp_manifest(bytes: &[u8]) -> Option<ParsedManifest> {
    let reader = ::capnp::serialize::read_message(bytes, ::capnp::message::ReaderOptions::new()).ok()?;
    let manifest = reader
        .get_root::<crate::nostr_mail_capnp::manifest::Reader>()
        .ok()?;

    let body_text = manifest.has_body().then(|| manifest.get_body()).and_then(|b| {
        let blob = b.ok()?;
        let ct = blob.get_ciphertext().ok()?;
        let key = blob.get_key_wrap().ok()?;
        let pt = crate::crypto::aes_gcm_decrypt_raw(key, ct).ok()?;
        String::from_utf8(pt).ok()
    });

    let mut attachments = Vec::new();
    if let Ok(list) = manifest.get_attachments() {
        for a in list.iter() {
            let id = a.get_id().ok()?.to_str().ok()?.to_string();
            let orig_filename = a.get_orig_filename().ok()?.to_str().ok()?.to_string();
            let orig_mime = a.get_orig_mime().ok()?.to_str().ok()?.to_string();
            let cipher_sha256 = a.get_cipher_sha256().ok()?.to_vec();
            let key_wrap = a.get_key_wrap().ok()?.to_vec();
            attachments.push(ParsedAttachment {
                id,
                orig_filename,
                orig_mime,
                cipher_sha256,
                cipher_size: a.get_cipher_size(),
                key_wrap,
            });
        }
    }
    Some(ParsedManifest { body_text, attachments })
}

/// Legacy JSON manifest (read-only). The body blob's AES plaintext is the
/// base64 of the UTF-8 body (a JS artifact), so it is base64-decoded after AES.
fn parse_json_manifest(json: &str) -> Option<ParsedManifest> {
    #[derive(serde::Deserialize)]
    struct JBlob {
        ciphertext: String,
        key_wrap: String,
    }
    #[derive(serde::Deserialize)]
    struct JAtt {
        id: String,
        orig_filename: String,
        orig_mime: String,
        cipher_sha256: Option<String>,
        cipher_size: Option<u64>,
        key_wrap: String,
    }
    #[derive(serde::Deserialize)]
    struct JManifest {
        body: Option<JBlob>,
        attachments: Option<Vec<JAtt>>,
    }
    let m: JManifest = serde_json::from_str(json).ok()?;
    // A JSON object that isn't actually a manifest (no body, no attachments) is
    // not a manifest — treat as plaintext.
    if m.body.is_none() && m.attachments.is_none() {
        return None;
    }

    let body_text = m.body.and_then(|blob| {
        let key = b64().decode(blob.key_wrap).ok()?;
        let ct = b64().decode(blob.ciphertext).ok()?;
        let pt = crate::crypto::aes_gcm_decrypt_raw(&key, &ct).ok()?;
        let b64_body = String::from_utf8(pt).ok()?;
        match b64().decode(b64_body.trim()) {
            Ok(body_bytes) => Some(String::from_utf8(body_bytes).unwrap_or(b64_body)),
            Err(_) => Some(b64_body),
        }
    });

    let attachments = m
        .attachments
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| {
            Some(ParsedAttachment {
                id: a.id,
                orig_filename: a.orig_filename,
                orig_mime: a.orig_mime,
                cipher_sha256: a
                    .cipher_sha256
                    .and_then(|h| hex::decode(h).ok())
                    .unwrap_or_default(),
                cipher_size: a.cipher_size.unwrap_or(0),
                key_wrap: b64().decode(a.key_wrap).ok()?,
            })
        })
        .collect();
    Some(ParsedManifest { body_text, attachments })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capnp_manifest_roundtrips_body_and_attachments() {
        let body = "Please review and sign the attached contract.";
        let atts = vec![
            AttachmentInput {
                filename: "contract.pdf".into(),
                mime: "application/pdf".into(),
                data: b"%PDF-1.7 fake contract bytes".to_vec(),
            },
            AttachmentInput {
                filename: "exhibit a.txt".into(),
                mime: "text/plain".into(),
                data: b"exhibit body".to_vec(),
            },
        ];
        let (payload, parts) = build_capnp_manifest(body, &atts).unwrap();
        assert!(payload.starts_with(CAPNP_PREFIX.as_bytes()));
        // The marker is followed by raw capnp bytes (no base64 armor): a single-
        // segment message's serialized form opens with a `00 00 00 00` segment
        // header, which is not printable base64 text.
        assert_eq!(&payload[CAPNP_PREFIX.len()..CAPNP_PREFIX.len() + 4], &[0, 0, 0, 0]);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].filename, "a1.dat");

        let parsed = parse_manifest(&payload).expect("capnp manifest parses");
        assert_eq!(parsed.body_text.as_deref(), Some(body));
        assert_eq!(parsed.attachments.len(), 2);
        assert_eq!(parsed.attachments[0].id, "a1");
        assert_eq!(parsed.attachments[0].orig_filename, "contract.pdf");
        assert_eq!(parsed.attachments[1].orig_filename, "exhibit a.txt");

        // The encrypted MIME part decrypts back to the original file using the
        // manifest's key + recorded hash (the receive path).
        let dec = crate::email::decrypt_attachment_pipeline(
            &b64().encode(&parts[0].ciphertext),
            &parsed.attachments[0].key_wrap_b64(),
            Some(&parsed.attachments[0].cipher_sha256_hex()),
            &parsed.attachments[0].orig_filename,
            &parsed.attachments[0].orig_mime,
        )
        .unwrap();
        assert_eq!(b64().decode(dec.data_b64).unwrap(), b"%PDF-1.7 fake contract bytes");
    }

    #[test]
    fn tampered_attachment_is_rejected() {
        let atts = vec![AttachmentInput {
            filename: "doc.txt".into(),
            mime: "text/plain".into(),
            data: b"original".to_vec(),
        }];
        let (payload, parts) = build_capnp_manifest("body", &atts).unwrap();
        let parsed = parse_manifest(&payload).unwrap();
        // Flip a ciphertext byte → the recorded hash no longer matches → reject.
        let mut tampered = parts[0].ciphertext.clone();
        tampered[0] ^= 0xff;
        let err = crate::email::decrypt_attachment_pipeline(
            &b64().encode(&tampered),
            &parsed.attachments[0].key_wrap_b64(),
            Some(&parsed.attachments[0].cipher_sha256_hex()),
            "doc.txt",
            "text/plain",
        );
        assert!(err.is_err(), "tampered attachment must be rejected");
        assert!(err.unwrap_err().contains("integrity check failed"));
    }

    #[test]
    fn armored_capnp_manifest_roundtrips() {
        // The base64 (`capnp64:`) variant used by the string-typed NIP-44 path:
        // text-safe, and decodes back to the same body + attachments.
        let atts = vec![AttachmentInput {
            filename: "report.pdf".into(),
            mime: "application/pdf".into(),
            data: b"%PDF report bytes".to_vec(),
        }];
        let (payload, parts) = build_capnp_manifest_armored("the 1:1 body", &atts).unwrap();
        assert!(payload.starts_with(CAPNP_B64_PREFIX));
        assert!(payload.is_ascii(), "armored payload must be text-safe for NIP-44");
        assert_eq!(parts.len(), 1);

        let parsed = parse_manifest(payload.as_bytes()).expect("armored capnp parses");
        assert_eq!(parsed.body_text.as_deref(), Some("the 1:1 body"));
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].orig_filename, "report.pdf");

        // The encrypted part decrypts back to the original file.
        let dec = crate::email::decrypt_attachment_pipeline(
            &b64().encode(&parts[0].ciphertext),
            &parsed.attachments[0].key_wrap_b64(),
            Some(&parsed.attachments[0].cipher_sha256_hex()),
            &parsed.attachments[0].orig_filename,
            &parsed.attachments[0].orig_mime,
        )
        .unwrap();
        assert_eq!(b64().decode(dec.data_b64).unwrap(), b"%PDF report bytes");
    }

    #[test]
    fn plaintext_is_not_a_manifest() {
        assert!(parse_manifest(b"just a normal body").is_none());
        assert!(parse_manifest(b"{ not really json").is_none());
    }
}
