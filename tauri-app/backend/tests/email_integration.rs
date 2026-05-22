mod common;

use base64::{engine::general_purpose, Engine as _};
use common::*;
use nostr_mail_lib::{crypto, email};

// glossia is also a `[dependencies]` of nostr-mail, so it's available to
// integration tests by importing directly.

/// NIP-44 encrypt plaintext under the given recipient npub. Returns the
/// raw base64 ciphertext (no armor).
fn nip44_encrypt(sender_nsec: &str, recipient_npub: &str, plaintext: &str) -> String {
    crypto::encrypt_message(sender_nsec, recipient_npub, plaintext, Some("nip44"))
        .expect("nip44 encrypt")
}

/// Glossia-encode arbitrary text into latin words using the `body` dialect
/// (matches production's `transcode("encode into latin")` for the body
/// region — see `email-service.js:5982` and the `Pipeline::from_meta` path).
fn glossia_encode_latin_body(text: &str) -> String {
    let (encoded, _, _, _) = glossia::encode_into_language(
        text,
        "latin",
        "default",
        "body",
        None,
        42, // deterministic seed for testable output
        false,
        None,
        None,
        None,
        None,
    )
    .expect("glossia encode_into_language");
    encoded
}

/// Reconstruct a raw-headers string from the mock store's parsed header map,
/// in the format `extract_nostr_*_from_headers` expects (one `Key: value` per line).
fn raw_headers_from_store(email: &mock_email::Email) -> String {
    let mut out = String::new();
    for (k, v) in &email.headers {
        out.push_str(k);
        out.push_str(": ");
        out.push_str(v);
        out.push('\n');
    }
    out
}

/// Headline test: NIP-44 encrypt → ASCII armor → SMTP send (via lettre) →
/// mock SMTP receive (via mailparse) → verify schnorr signature on
/// X-Nostr-Sig header → NIP-44 decrypt → assert bytewise plaintext recovery.
///
/// This exercises the default-settings header-signing path. The body- glossia
/// path and the in-body SIGNATURE block are covered by separate tests once
/// this baseline lands.
///
/// We bypass `fetch_emails` because it gates on Authentication-Results /
/// DKIM headers (transport authentication) that the mock SMTP can't
/// synthesize. That gate is orthogonal to what this test verifies; the
/// store-direct path still exercises lettre's outbound encoding +
/// mailparse's inbound parsing, signing, encryption, and armor handling.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn defaults_header_sig_roundtrip() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _alice_keys) = test_keypair(1);
    let (bob_nsec, bob_npub, _bob_keys) = test_keypair(2);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let plaintext = "Hello Bob,\n\nThis is a test of NIP-44 encrypted email.\n\n— alice";
    let subject = "Test message from alice";

    // 1. NIP-44 encrypt with alice's nsec + bob's npub
    let ciphertext = crypto::encrypt_message(&alice_nsec, &bob_npub, plaintext, Some("nip44"))
        .expect("nip44 encrypt");

    // 2. Wrap in ASCII armor (raw base64 body for now — glossia body
    // encoding is layered on by a later test)
    let armored_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        ciphertext
    );

    // 3. send_email signs over extract_ciphertext_binary(body) and adds
    // X-Nostr-Pubkey + X-Nostr-Sig headers automatically.
    email::send_email(
        &alice_config,
        "bob@test.local",
        subject,
        &armored_body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true, // include_pubkey_header
        true, // include_sig_header
    )
    .await
    .expect("send_email");

    // 4. Pull the delivered email out of the mock store.
    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1, "expected exactly one delivered email");
    let delivered = &inbox[0];

    // 5. Headers + subject sanity
    assert_eq!(delivered.subject, subject, "subject mismatch");
    assert_eq!(
        delivered.from.to_string(),
        "alice@test.local",
        "From mismatch"
    );
    assert!(
        delivered.body.contains("BEGIN NOSTR NIP-44 ENCRYPTED BODY"),
        "armor markers missing from received body: {}",
        delivered.body
    );

    // 6. Synthesize raw_headers in the format the verifier expects.
    let raw_headers = raw_headers_from_store(delivered);

    // 7. Verify the header-attached schnorr signature.
    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(
        sig_valid,
        Some(true),
        "header signature failed to verify; source={:?}, headers=\n{}\nbody={}",
        sig_source,
        raw_headers,
        delivered.body
    );
    assert_eq!(
        sig_source.as_deref(),
        Some("header"),
        "expected header trust path (no in-body SIGNATURE block was emitted)"
    );

    // 8. Confirm the X-Nostr-Pubkey header is alice's npub.
    let header_pubkey = email::extract_nostr_pubkey_from_headers(&raw_headers)
        .expect("X-Nostr-Pubkey header present");
    assert_eq!(header_pubkey, alice_npub, "header pubkey mismatch");

    // 9. Decrypt and assert plaintext recovery.
    // `extract_ciphertext_binary` returns base64-decoded raw bytes; NIP-44
    // decrypt expects the base64 *string*, so re-encode before calling.
    let ciphertext_bytes = email::extract_ciphertext_binary(&delivered.body);
    let ciphertext_b64 = general_purpose::STANDARD.encode(&ciphertext_bytes);
    let decrypted = crypto::decrypt_message(&bob_nsec, &alice_npub, &ciphertext_b64)
        .expect("nip44 decrypt");
    assert_eq!(decrypted, plaintext, "plaintext round-trip mismatch");
}

/// Body region is glossia-encoded as latin words (matches default settings:
/// glossiaEncodingBody=latin). Sig still comes from the X-Nostr-Sig header.
///
/// Wire layout:
///   ----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
///   <latin tokens encoding the base64 ciphertext>
///   ----- END NOSTR MESSAGE -----
///
/// On the receive side, `extract_ciphertext_binary` glossia-decodes the
/// latin tokens back to the original base64 string, returning its UTF-8
/// bytes — which is what send_email signed over.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn glossia_body_latin_roundtrip() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (bob_nsec, bob_npub, _) = test_keypair(2);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let plaintext = "the quick brown fox jumps over the lazy dog";
    let subject = "latin body test";

    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, plaintext);
    let glossia_body = glossia_encode_latin_body(&ciphertext);

    // Sanity: the encoded body should be latin words, not the raw ciphertext.
    assert!(
        !glossia_body.contains('+') && !glossia_body.contains('/'),
        "glossia-encoded body unexpectedly contains base64 chars: {}",
        glossia_body
    );
    assert!(
        glossia_body.split_whitespace().count() > 10,
        "glossia body is suspiciously short: {}",
        glossia_body
    );

    let armored_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        glossia_body
    );

    email::send_email(
        &alice_config,
        "bob@test.local",
        subject,
        &armored_body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    let delivered = &inbox[0];

    // The latin-encoded body must survive the SMTP round-trip with armor intact.
    assert!(
        delivered.body.contains("BEGIN NOSTR NIP-44 ENCRYPTED BODY"),
        "armor missing: {}",
        delivered.body
    );

    // Verify the header signature.
    let raw_headers = raw_headers_from_store(delivered);
    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(
        sig_valid,
        Some(true),
        "header sig invalid; source={:?}, headers=\n{}\nbody=\n{}",
        sig_source,
        raw_headers,
        delivered.body
    );

    // extract_ciphertext_binary glossia-decodes the latin words and then
    // recognizes the input as base64 → returns the raw NIP-44 binary
    // ciphertext (not the base64 string). Re-encode to compare to the
    // original ciphertext (modulo base64 normalization), then decrypt.
    let recovered_bytes = email::extract_ciphertext_binary(&delivered.body);
    let recovered_b64 = general_purpose::STANDARD.encode(&recovered_bytes);
    assert_eq!(
        general_purpose::STANDARD
            .decode(&ciphertext)
            .expect("original ciphertext is valid base64"),
        recovered_bytes,
        "ciphertext bytes did not survive glossia round-trip"
    );

    let decrypted = crypto::decrypt_message(&bob_nsec, &alice_npub, &recovered_b64)
        .expect("nip44 decrypt");
    assert_eq!(decrypted, plaintext);
}

/// Combined sig+pubkey block inside a NOSTR SIGNATURE armor section,
/// bitpack-encoded as latin words (matches default settings:
/// glossiaEncodingSig=latin, glossiaEncodingPubkey=latin).
///
/// Wire layout:
///   ----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
///   <latin tokens encoding ciphertext>
///   ----- BEGIN NOSTR SIGNATURE -----
///   <latin tokens encoding the 96-byte sig||pubkey blob>
///   ----- END NOSTR MESSAGE -----
///
/// The 96-byte concatenation is bitpacked across the sig/pubkey boundary,
/// so the "pubkey region" words are NOT deterministic — they shift by 0..9
/// bits depending on the random schnorr sig tail. Assertions are at the
/// bytes level (decode → split → verify), never on literal word strings.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn defaults_full_roundtrip_with_inline_sig() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (bob_nsec, bob_npub, _) = test_keypair(2);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let plaintext =
        "Hello Bob,\n\nThis is a test of NIP-44 encrypted email with\nglossia latin wordlist encoding.\n\n— alice";
    let subject = "Test message from alice";

    // 1. NIP-44 encrypt → base64 ciphertext.
    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, plaintext);

    // 2. Glossia-encode the body region (latin).
    let glossia_body = glossia_encode_latin_body(&ciphertext);

    // 3. Build a *partial* body (just BODY block + END) so we can compute
    // the bytes that need signing. The verifier's `extract_ciphertext_binary`
    // looks between BEGIN BODY and either BEGIN SIGNATURE/SEAL or END NOSTR.
    let partial_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        glossia_body
    );
    let body_bytes_to_sign = email::extract_ciphertext_binary(&partial_body);

    // Sanity: glossia + base64 round-trip should recover the same raw binary
    // ciphertext bytes that base64-decoding the original ciphertext produces.
    assert_eq!(
        body_bytes_to_sign,
        general_purpose::STANDARD
            .decode(&ciphertext)
            .expect("original ciphertext is valid base64"),
        "glossia roundtrip lost bytes"
    );

    // 4. Compute schnorr sig over those bytes.
    let signature_hex = crypto::sign_data_bytes(&alice_nsec, &body_bytes_to_sign)
        .expect("sign_data_bytes");
    assert_eq!(signature_hex.len(), 128, "schnorr sig should be 64 bytes hex");

    // 5. Concatenate sig (64B) || pubkey (32B) and bitpack-encode as latin.
    let alice_pubkey_hex = npub_to_hex(&alice_npub);
    let mut combined = Vec::with_capacity(96);
    combined.extend_from_slice(&hex_decode(&signature_hex));
    combined.extend_from_slice(&hex_decode(&alice_pubkey_hex));
    assert_eq!(combined.len(), 96, "sig+pubkey should be exactly 96 bytes");

    let latin_words = glossia::load_payload_words_for_wordlist("latin", "default")
        .expect("load latin wordlist");
    let latin_tree = glossia::WordlistTree::new(latin_words);
    let sig_block_words =
        glossia::codec::encode_base_n(&combined, &latin_tree, "bitpack_fixed")
            .expect("bitpack_fixed encode");
    let sig_block_text = sig_block_words.join(" ");

    // 6. Assemble the final body with the SIGNATURE block.
    let final_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{glossia_body}\n-----BEGIN NOSTR SIGNATURE-----\n{sig_block_text}\n-----END NOSTR MESSAGE-----"
    );

    // 7. Send. send_email will *also* add X-Nostr-Pubkey + X-Nostr-Sig
    // headers (signed over the same bytes), giving us both trust paths.
    email::send_email(
        &alice_config,
        "bob@test.local",
        subject,
        &final_body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    let delivered = &inbox[0];

    // 8. Verify signatures (both body and header paths should validate).
    let raw_headers = raw_headers_from_store(delivered);
    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(
        sig_valid,
        Some(true),
        "signature failed to verify; source={:?}, headers=\n{}\nbody=\n{}",
        sig_source,
        raw_headers,
        delivered.body
    );
    // With both paths valid, the verifier should report "both".
    assert_eq!(
        sig_source.as_deref(),
        Some("both"),
        "expected both trust paths to validate, got {:?}",
        sig_source
    );

    // 9. Recover pubkey from the in-body SIGNATURE block and confirm it
    // matches alice. This is the non-deterministic byte-level assertion
    // (literal words would change every run because the sig is random).
    let parsed = email::parse_armor_components(&delivered.body)
        .expect("parse_armor_components");
    let recovered_sig_pubkey_hex = parsed.sig_pubkey_hex.as_deref().expect("sig_pubkey_hex");
    assert_eq!(
        recovered_sig_pubkey_hex, alice_pubkey_hex,
        "in-body pubkey did not match alice"
    );

    // 10. Decrypt and confirm plaintext recovery.
    let recovered_bytes = email::extract_ciphertext_binary(&delivered.body);
    let recovered_b64 = general_purpose::STANDARD.encode(&recovered_bytes);
    let decrypted = crypto::decrypt_message(&bob_nsec, &alice_npub, &recovered_b64)
        .expect("nip44 decrypt");
    assert_eq!(decrypted, plaintext);
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
        .collect()
}
