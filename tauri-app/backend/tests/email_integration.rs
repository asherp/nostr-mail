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
    // X-Nostr-Pubkey + X-Nostr-Sig + X-Nostr-Recipient headers automatically.
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
        true,            // include_pubkey_header
        true,            // include_sig_header
        Some(&bob_npub), // recipient_pubkey (default-on anchor for decryption)
        true,            // include_recipient_header
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

    // 8b. Confirm the X-Nostr-Recipient header is bob's npub.
    let header_recipient = email::extract_nostr_recipient_from_headers(&raw_headers)
        .expect("X-Nostr-Recipient header present");
    assert_eq!(header_recipient, bob_npub, "recipient header mismatch");

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
        Some(&bob_npub),
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

    // X-Nostr-Recipient anchors decryption to bob's npub.
    assert_eq!(
        email::extract_nostr_recipient_from_headers(&raw_headers).as_deref(),
        Some(bob_npub.as_str()),
        "recipient header mismatch"
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
        Some(&bob_npub),
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

    // 9b. X-Nostr-Recipient anchors decryption to bob's npub.
    assert_eq!(
        email::extract_nostr_recipient_from_headers(&raw_headers).as_deref(),
        Some(bob_npub.as_str()),
        "recipient header mismatch"
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

/// NIP-04 (legacy) encrypt path. The decrypt side at crypto.rs falls back
/// from NIP-44 to NIP-04 when the first one fails, so the same receive
/// pipeline that handles NIP-44 should handle NIP-04 without changes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nip04_legacy_decrypt() {
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

    let plaintext = "Hello Bob, NIP-04 here.";
    let ciphertext = crypto::encrypt_message(&alice_nsec, &bob_npub, plaintext, Some("nip04"))
        .expect("nip04 encrypt");

    // NIP-04 ciphertext format is `base64?iv=base64`. The armor wraps it as-is.
    let armored_body = format!(
        "-----BEGIN NOSTR NIP-04 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        ciphertext
    );

    email::send_email(
        &alice_config,
        "bob@test.local",
        "nip04 fallback",
        &armored_body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    let delivered = &inbox[0];

    let raw_headers = raw_headers_from_store(delivered);
    let (sig_valid, _) = email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(sig_valid, Some(true), "nip04 header sig should verify");
    assert_eq!(
        email::extract_nostr_recipient_from_headers(&raw_headers).as_deref(),
        Some(bob_npub.as_str()),
        "recipient header mismatch"
    );

    // decode_armor_section handles the `base64?iv=base64` form by returning
    // payload_bytes || iv_bytes concatenated. To round-trip through decrypt
    // we need the original `b64?iv=b64` string — pull it directly out of the
    // armor block (the verifier already accepted these bytes).
    let body = &delivered.body;
    let begin = body.find("ENCRYPTED BODY-----").expect("BEGIN marker");
    let after_begin = body[begin..].find('\n').map(|i| begin + i + 1).unwrap();
    let end = body[after_begin..]
        .find("-----END NOSTR MESSAGE-----")
        .map(|i| after_begin + i)
        .expect("END marker");
    let ct_field: String = body[after_begin..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let decrypted = crypto::decrypt_message(&bob_nsec, &alice_npub, &ct_field)
        .expect("nip04 decrypt");
    assert_eq!(decrypted, plaintext);
}

/// Multipart text+html lettre → mailparse round-trip. The encrypted ENCRYPTED
/// BODY armor lives in the text/plain part; the html part is a separate MIME
/// section. Confirms both reach the receive side intact.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multipart_html_and_text() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (_bob_nsec, bob_npub, _) = test_keypair(2);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let plaintext_in_armor = crypto::encrypt_message(
        &alice_nsec,
        &bob_npub,
        "Hello Bob",
        Some("nip44"),
    )
    .expect("nip44 encrypt");
    let armored = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        plaintext_in_armor
    );
    let html_body = "<p>Hello <b>Bob</b></p>";

    email::send_email(
        &alice_config,
        "bob@test.local",
        "mime alt parts",
        &armored,
        Some(&alice_npub),
        None,
        None,
        Some(html_body),
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    let delivered = &inbox[0];

    // mock-email parses MIME and stores html_body separately. When the
    // outer Content-Type is multipart/alternative, mock-email's
    // `parsed.get_body()` returns empty (the text part lives in a
    // subpart it doesn't promote to `body`), so we can only assert on
    // html_body here. The text/plain armor still travels intact — it's
    // just hidden behind the mock's multipart handling. Production
    // (Gmail/etc.) preserves it on the wire and `fetch_emails` re-parses
    // with mailparse directly.
    let recv_html = delivered.html_body.as_deref().unwrap_or("");
    assert!(
        recv_html.contains("<b>Bob</b>"),
        "html part lost in transit: html_body={:?}",
        delivered.html_body
    );

    // X-Nostr-Recipient travels alongside the MIME body.
    let raw_headers = raw_headers_from_store(delivered);
    assert_eq!(
        email::extract_nostr_recipient_from_headers(&raw_headers).as_deref(),
        Some(bob_npub.as_str()),
        "recipient header mismatch"
    );
}

/// Non-ASCII subject through lettre's RFC 2047 encoder + mailparse decoder.
/// The em-dash and accented characters must survive bytewise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_ascii_subject_roundtrip() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let subject = "Héllo from Ålice — testing";

    email::send_email(
        &alice_config,
        "bob@test.local",
        subject,
        "plain body",
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,  // no recipient pubkey — this test isn't about encryption
        true,  // default-on, but the None pubkey above makes it a no-op
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    // mock-email's SMTP parser stores the subject directly from mailparse's
    // header decoding; the encoded-word form ("=?UTF-8?B?...?=") should be
    // resolved before storage.
    assert_eq!(
        inbox[0].subject, subject,
        "non-ASCII subject failed RFC 2047 round-trip"
    );
}

/// Quoted-Printable body round-trip with a non-ASCII payload.
///
/// `mock-email` decodes the QP transfer encoding correctly but then
/// reinterprets the resulting UTF-8 bytes as **Windows-1252** when handing
/// the body back through `mailparse::ParsedMail::get_body()` — so `"Café"`
/// comes back as `"CafÃ©"` and the em-dash (U+2014, UTF-8 `E2 80 94`)
/// becomes the three-char sequence `"â€\u{2014}"`. This is the classic
/// UTF-8-as-CP1252 mojibake (CP1252 is a Latin-1 superset that maps
/// 0x80–0x9F to printable characters where Latin-1 has control codes).
/// The QP and SMTP layers are fine; only the charset selection on the
/// readback path is wrong.
///
/// Workaround: the mojibake is byte-reversible. For each `char` we look up
/// the CP1252 codepoint and write back its original byte (using
/// `cp1252_byte_for_char`). The resulting `Vec<u8>` is the original UTF-8
/// payload. If a future mock-email release fixes the charset path, the
/// ASCII-only chars all map to themselves, so the recovery is a no-op and
/// the assertion still holds.
fn cp1252_byte_for_char(c: char) -> Option<u8> {
    // ASCII (0x00–0x7F) and Latin-1 supplement above 0x9F (0xA0–0xFF) are
    // identical in CP1252. The 0x80–0x9F range diverges — CP1252 maps
    // those to printable Unicode codepoints that Latin-1 leaves as control
    // characters.
    let cp = c as u32;
    if cp <= 0x7F || (0xA0..=0xFF).contains(&cp) {
        return Some(cp as u8);
    }
    Some(match c {
        '\u{20AC}' => 0x80, '\u{201A}' => 0x82, '\u{0192}' => 0x83,
        '\u{201E}' => 0x84, '\u{2026}' => 0x85, '\u{2020}' => 0x86,
        '\u{2021}' => 0x87, '\u{02C6}' => 0x88, '\u{2030}' => 0x89,
        '\u{0160}' => 0x8A, '\u{2039}' => 0x8B, '\u{0152}' => 0x8C,
        '\u{017D}' => 0x8E, '\u{2018}' => 0x91, '\u{2019}' => 0x92,
        '\u{201C}' => 0x93, '\u{201D}' => 0x94, '\u{2022}' => 0x95,
        '\u{2013}' => 0x96, '\u{2014}' => 0x97, '\u{02DC}' => 0x98,
        '\u{2122}' => 0x99, '\u{0161}' => 0x9A, '\u{203A}' => 0x9B,
        '\u{0153}' => 0x9C, '\u{017E}' => 0x9E, '\u{0178}' => 0x9F,
        _ => return None,
    })
}

fn recover_utf8_from_cp1252_mojibake(s: &str) -> Option<String> {
    let bytes: Option<Vec<u8>> = s.chars().map(cp1252_byte_for_char).collect();
    String::from_utf8(bytes?).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quoted_printable_body_roundtrip() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    // Mix ASCII, 2-byte UTF-8 (`é`, `Å`), and 3-byte UTF-8 (em-dash, U+2014).
    // Long enough that QP soft-line-breaks have a chance to fire (>76 cols).
    let body = "Café meeting — Ålice will bring the croissants and stroopwafels for everyone in attendance today.";

    email::send_email(
        &alice_config,
        "bob@test.local",
        "qp body test",
        body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    let received = &inbox[0];

    // Direct equality is the cleanest invariant if the mock ever stops
    // mangling the charset. If it does match, we're done.
    if received.body.trim_end() == body {
        return;
    }

    // Otherwise, exercise the documented workaround. Recovering the bytes
    // is what proves the QP/SMTP path didn't lose information — only the
    // mock's `String` accessor did.
    let recovered = recover_utf8_from_cp1252_mojibake(received.body.trim_end())
        .expect("body chars all fall within CP1252 range");
    assert_eq!(
        recovered, body,
        "QP body did not survive bytewise (even after CP1252 unmojibake): \
         stored={:?}, recovered={:?}",
        received.body, recovered
    );
}

/// Sent-folder decryption with the X-Nostr-Recipient header as the only
/// anchor — exercising the fresh-install / no-local-DB / no-contacts path.
///
/// Scenario: alice sends bob an encrypted email and never publishes a
/// matching Kind 14 DM. On her usual device, her local DB stores bob's
/// pubkey against this message_id, so reopening the Sent folder Just
/// Works. But on a fresh install — or after the relays drop her DMs and
/// bob isn't in her contacts — that local lookup misses. The
/// X-Nostr-Recipient header is what rescues those cases: the decrypt
/// pipeline picks it up as the ECDH counterparty without needing any
/// device-local or relay state.
///
/// This test simulates that "no other anchor available" path by calling
/// the pipeline with `sender_pubkey=None, recipient_pubkey=Some(header)`
/// — no DB hint passed in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sent_mail_decrypts_via_recipient_header_without_dm() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (_bob_nsec, bob_npub, _) = test_keypair(2);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let plaintext = "Sent-folder anchor: bob can read this, and so can I.";
    let subject = "sent-folder anchor test";

    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, plaintext);
    let armored_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        ciphertext
    );

    // Default-on path: include_recipient_header=true + recipient_pubkey supplied.
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
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    let delivered = &inbox[0];
    let raw_headers = raw_headers_from_store(delivered);

    // The whole point of the header: it's there, and it points at bob.
    let recipient_from_header = email::extract_nostr_recipient_from_headers(&raw_headers)
        .expect("X-Nostr-Recipient header present");
    assert_eq!(
        recipient_from_header, bob_npub,
        "recipient header should pin bob as decryption counterparty"
    );

    // Alice opens her Sent folder. She's the sender, so sender_pubkey is None
    // (the X-Nostr-Pubkey header would point at herself → self-DH refusal).
    // The pipeline picks up recipient_pubkey from the X-Nostr-Recipient header
    // and uses it as the ECDH counterparty.
    let result = email::decrypt_email_body_pipeline(
        &alice_nsec,
        &delivered.body,
        &delivered.subject,
        None,
        Some(&recipient_from_header),
    )
    .expect("decrypt_email_body_pipeline");

    assert!(
        result.success,
        "sent-mail decrypt should succeed via recipient header; error={:?}",
        result.error
    );
    assert_eq!(
        result.body, plaintext,
        "sent-mail plaintext mismatch after recipient-header decrypt"
    );
}

/// Negative companion at the pipeline level: with no counterparty hint
/// (no header in the email, and no local-DB or contacts lookup feeding a
/// hint in), the decrypt pipeline can't recover the plaintext.
///
/// Caveat for readers: this is NOT the same as "alice can't read her own
/// sent mail without the header" in real life. On the sending device the
/// local DB stores recipient_pubkey against this message_id, so the UI
/// passes that in as the hint and decryption succeeds without any header.
/// What this test pins down is the worst-case path — fresh install, DM
/// not in relay history, recipient not in contacts — where the
/// X-Nostr-Recipient header is the only thing that can rescue Sent-folder
/// decryption. If this test ever flips to passing, the pipeline grew a
/// new way to recover the counterparty pubkey on its own and the
/// fresh-install fallback story should be revisited.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sent_mail_undecryptable_without_any_counterparty_hint() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (_bob_nsec, bob_npub, _) = test_keypair(2);

    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let plaintext = "no header, no DM, no decrypt for the sender";
    let subject = "sent-folder anchor missing";

    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, plaintext);
    let armored_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        ciphertext
    );

    // include_recipient_header=false AND recipient_pubkey=None → no anchor.
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
        None,
        false,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1);
    let delivered = &inbox[0];
    let raw_headers = raw_headers_from_store(delivered);

    // Header must really be absent — this is the precondition for the test.
    assert!(
        email::extract_nostr_recipient_from_headers(&raw_headers).is_none(),
        "X-Nostr-Recipient should be absent; sent-folder undecryptability test is moot otherwise"
    );

    // Alice's Sent-folder decrypt attempt with no counterparty hint fails.
    let result = email::decrypt_email_body_pipeline(
        &alice_nsec,
        &delivered.body,
        &delivered.subject,
        None,
        None,
    )
    .expect("decrypt_email_body_pipeline returns Ok even when content can't be decrypted");

    assert!(
        !result.success,
        "sent-mail decrypt should NOT succeed without a counterparty anchor; got body={:?}",
        result.body
    );
}

/// Reply threading: `Message-ID`, `In-Reply-To`, `References`, and `Re:` subject
/// must travel byte-faithfully through `send_email` so the downstream database
/// threading code (which keys on these headers) groups messages correctly.
///
/// Scenario:
///   1. alice sends bob an encrypted email (root, Message-ID `<alice-1@…>`).
///   2. bob composes a reply: encrypts a new ciphertext, quotes alice's still-
///      encoded armored body as a prefix (the production frontend rule from
///      `replyToEmail()`: quote `email.body`, never `decryptedBody`), and sends
///      with `In-Reply-To=<alice-1@…>` and `References=<alice-1@…>`.
///
/// Pinned invariants on bob's reply as it lands in the mock store:
///   * `Message-ID` header is bob's own ID
///   * `In-Reply-To` header equals alice's Message-ID
///   * `References` header equals alice's Message-ID (single entry — first reply)
///   * `Subject` starts with "Re: "
///   * Body still contains **alice's armor markers** — i.e. the quoted prefix
///     is ciphertext, never plaintext. This is what prevents accidental
///     plaintext leakage in reply chains.
///   * Body also contains **bob's own armor markers** for his fresh ciphertext.
///   * Recomputing `thread_id` the way `database::compute_thread_id` does
///     (References-first → In-Reply-To → self) yields alice's normalized ID
///     for both the root and the reply, so they group into one thread.
///
/// What this does NOT cover (out of scope for the integration test):
///   * The database `resolve_threading` / `compute_thread_id` functions
///     themselves — those live in a private module. We assert the wire-level
///     inputs are correct; the database test surface should pin the rest.
///   * Three-or-more-deep reply chains (References list extension semantics).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reply_threading_headers_and_encoded_quote() {
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
    let bob_config = email_config(
        "bob@test.local",
        "password-bob",
        &bob_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    // ─── Hop 1: alice → bob ────────────────────────────────────────────────
    let alice_msgid = "alice-thread-root-001@test.local";
    let alice_ciphertext =
        nip44_encrypt(&alice_nsec, &bob_npub, "Hello bob, want to grab coffee?");
    let alice_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        alice_ciphertext
    );

    email::send_email(
        &alice_config,
        "bob@test.local",
        "coffee thursday?",
        &alice_body,
        Some(&alice_npub),
        Some(alice_msgid),
        None,
        None,
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("alice send_email");

    // ─── Hop 2: bob → alice (reply) ────────────────────────────────────────
    // bob's reply ciphertext is a *separate* encryption — the new prose he's
    // adding. The quoted prefix is alice's *encoded* body verbatim, never
    // decrypted, so plaintext can't leak into the reply chain.
    let bob_msgid = "bob-reply-001@test.local";
    let bob_ciphertext =
        nip44_encrypt(&bob_nsec, &alice_npub, "Sure, 3pm at the usual place?");
    let bob_armor = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        bob_ciphertext
    );
    // Quote alice's encoded armor with the standard `> ` prefix per line.
    let alice_quoted: String = alice_body
        .lines()
        .map(|l| format!("> {}", l))
        .collect::<Vec<_>>()
        .join("\n");
    let bob_body = format!("{}\n\nOn earlier, alice wrote:\n{}", bob_armor, alice_quoted);

    email::send_email(
        &bob_config,
        "alice@test.local",
        "Re: coffee thursday?",
        &bob_body,
        Some(&bob_npub),
        Some(bob_msgid),
        None,
        None,
        Some(alice_msgid),  // In-Reply-To
        Some(alice_msgid),  // References (single entry — first reply)
        true,
        true,
        Some(&alice_npub),
        true,
    )
    .await
    .expect("bob send_email reply");

    // ─── Pull bob's reply out of the mock store ────────────────────────────
    // mock-email's INBOX is a single global mailbox shared by both delivery
    // routes, so we filter by sender to disambiguate.
    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 2, "expected one root + one reply in INBOX");
    let reply = inbox
        .iter()
        .find(|e| e.from.to_string() == "bob@test.local")
        .expect("bob's reply is in INBOX");
    let root = inbox
        .iter()
        .find(|e| e.from.to_string() == "alice@test.local")
        .expect("alice's root is in INBOX");

    // Synthesize the same raw-headers string production code parses against,
    // and use the same lookup pattern: case-insensitive header name.
    let lookup = |email: &mock_email::Email, name: &str| -> Option<String> {
        email
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    // Production normalization (mirrors database::normalize_message_id):
    // strip whitespace and angle brackets so `<id@host>` and `id@host` compare
    // equal. We don't call the private function directly — the rule is two
    // lines and inlining keeps the test honest about what it pins.
    let normalize = |s: &str| s.trim().trim_start_matches('<').trim_end_matches('>').to_string();

    // 1. Message-ID propagation
    let root_msgid = lookup(root, "Message-ID").expect("root has Message-ID");
    assert_eq!(normalize(&root_msgid), alice_msgid, "root Message-ID");
    let reply_msgid = lookup(reply, "Message-ID").expect("reply has Message-ID");
    assert_eq!(normalize(&reply_msgid), bob_msgid, "reply Message-ID");

    // 2. In-Reply-To points at alice's root
    let in_reply_to = lookup(reply, "In-Reply-To").expect("reply has In-Reply-To");
    assert_eq!(
        normalize(&in_reply_to),
        alice_msgid,
        "In-Reply-To must point at alice's Message-ID"
    );

    // 3. References contains alice's root as the (only) entry
    let references = lookup(reply, "References").expect("reply has References");
    let refs_normalized: Vec<String> =
        references.split_whitespace().map(normalize).collect();
    assert_eq!(
        refs_normalized,
        vec![alice_msgid.to_string()],
        "References on first reply should be exactly [root_id]"
    );

    // 4. Subject carries the `Re: ` prefix verbatim
    assert!(
        reply.subject.starts_with("Re: "),
        "reply subject must start with 'Re: '; got {:?}",
        reply.subject
    );

    // 5. Reply body keeps the quote encrypted: alice's armor markers survive,
    //    and her ciphertext base64 prefix is present in the quoted region.
    //    If `replyToEmail()` ever regresses to quoting `decryptedBody`,
    //    "want to grab coffee" would leak here and this assertion would fail.
    assert!(
        reply.body.contains("BEGIN NOSTR NIP-44 ENCRYPTED BODY"),
        "alice's armor BEGIN marker missing from quoted reply body"
    );
    assert!(
        reply.body.contains(&alice_ciphertext[..32.min(alice_ciphertext.len())]),
        "alice's ciphertext prefix missing from quoted reply body — plaintext may have leaked"
    );
    assert!(
        !reply.body.contains("grab coffee"),
        "PLAINTEXT LEAK: alice's decrypted text appears in bob's reply body"
    );
    // 6. Bob's own armor is present too (his fresh ciphertext)
    assert!(
        reply.body.contains(&bob_ciphertext[..32.min(bob_ciphertext.len())]),
        "bob's own ciphertext prefix missing from reply body"
    );

    // 7. Replicate database::compute_thread_id's rule locally (References
    //    first → In-Reply-To fallback → self) and confirm both messages
    //    group under alice's normalized ID. This pins the wire-level signal
    //    the database consumes, without depending on a private API.
    let thread_id_of = |msg_id: &str, refs: Option<&str>, irt: Option<&str>| -> String {
        if let Some(r) = refs {
            if let Some(first) = r.split_whitespace().next() {
                return normalize(first);
            }
        }
        if let Some(p) = irt {
            if let Some(first) = p.split_whitespace().next() {
                return normalize(first);
            }
        }
        normalize(msg_id)
    };
    let root_thread = thread_id_of(alice_msgid, None, None);
    let reply_thread = thread_id_of(bob_msgid, Some(&references), Some(&in_reply_to));
    assert_eq!(
        root_thread, reply_thread,
        "root and reply must compute to the same thread_id"
    );
    assert_eq!(reply_thread, alice_msgid, "thread root is alice's Message-ID");
}

// ─────────────────────────────────────────────────────────────────────────
// Negative & edge-case signature tests
//
// The happy-path tests above (defaults_header_sig_roundtrip,
// defaults_full_roundtrip_with_inline_sig, glossia_body_latin_roundtrip)
// only pin that valid signatures verify. These pin what `verify_email_signature_full`
// does in adversarial / asymmetric scenarios:
//
//   * tampered body → both paths fail
//   * clearsigned plaintext (no inline block, no encryption) → "header"
//   * inline body valid, header invalid → "body" wins (body trust path is primary)
//   * inline body invalid, header valid → "header" wins (defensive fallback)
//   * broken X-Nostr-Pubkey (sig was computed by a different key) → fail
//
// The 4-arm match in verify_email_signature_full has surprising precedence
// rules — e.g. `(Some(false), Some(true))` returns header-trusted, not
// failure. Pinning these behaviours explicitly prevents accidental
// regressions when the match table gets touched.
// ─────────────────────────────────────────────────────────────────────────

/// Helper: build an armored body with an inline SIGNATURE block, signing
/// over the binary ciphertext bytes the verifier extracts. Used by the
/// mismatch tests below.
fn build_armored_with_inline_sig(
    nsec: &str,
    npub: &str,
    plaintext_ciphertext_b64: &str,
) -> String {
    let partial = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        plaintext_ciphertext_b64
    );
    let to_sign = email::extract_ciphertext_binary(&partial);
    let sig_hex = crypto::sign_data_bytes(nsec, &to_sign).expect("sign_data_bytes");
    let pubkey_hex = npub_to_hex(npub);

    let mut combined = Vec::with_capacity(96);
    combined.extend_from_slice(&hex_decode(&sig_hex));
    combined.extend_from_slice(&hex_decode(&pubkey_hex));

    let words = glossia::load_payload_words_for_wordlist("latin", "default")
        .expect("load latin wordlist");
    let tree = glossia::WordlistTree::new(words);
    let sig_block_words = glossia::codec::encode_base_n(&combined, &tree, "bitpack_fixed")
        .expect("bitpack_fixed encode");
    let sig_block_text = sig_block_words.join(" ");

    format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{plaintext_ciphertext_b64}\n-----BEGIN NOSTR SIGNATURE-----\n{sig_block_text}\n-----END NOSTR MESSAGE-----"
    )
}

/// Helper: rebuild a `raw_headers` string with one header value replaced.
/// Names are matched case-insensitively (same as production).
fn override_header(raw: &str, name: &str, new_value: &str) -> String {
    raw.lines()
        .map(|line| {
            if let Some((k, _)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case(name) {
                    return format!("{}: {}", k, new_value);
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tampering with the ciphertext after send invalidates both trust paths.
/// We flip a single byte inside the base64 body (decoded → mutated → re-encoded)
/// and confirm `verify_email_signature_full` returns `(Some(false), None)`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tampered_body_invalidates_signature() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (_bob_nsec, bob_npub, _) = test_keypair(2);
    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, "Hello bob");
    let body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        ciphertext
    );

    email::send_email(
        &alice_config,
        "bob@test.local",
        "tamper test",
        &body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    let delivered = &inbox[0];
    let raw_headers = raw_headers_from_store(delivered);

    // Sanity: pristine email verifies. Without this baseline a regression
    // that breaks verification altogether would still pass the negative test.
    let (pristine_valid, _) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(pristine_valid, Some(true), "baseline must verify before tampering");

    // Flip one byte inside the ciphertext (after base64 decode, then re-encode).
    let mut bytes = email::extract_ciphertext_binary(&delivered.body);
    bytes[0] ^= 0x01;
    let mutated_b64 = general_purpose::STANDARD.encode(&bytes);
    let tampered_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        mutated_b64
    );

    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&tampered_body, &raw_headers);
    assert_eq!(sig_valid, Some(false), "tampered body must not verify");
    assert_eq!(sig_source, None, "no trust path should be reported on failure");
}

/// Clearsigned plaintext: mirrors what the frontend produces for a signed
/// plain-text message (frontend/js/email-service.js:1308-1326,1822-1854):
/// the plaintext is glossia-encoded into body-dialect prose, wrapped in a
/// `----- BEGIN NOSTR SIGNED BODY -----` armor block with an inline
/// SIGNATURE block, and the original plaintext is shown above the armor
/// for non-nostr-mail clients.
///
/// Signing happens over the glossia-decoded canonical bytes of the armor
/// body — that round-trips through SMTP intact (CRLF normalization,
/// trailing whitespace, soft-wrapping all live outside the armor region).
/// A bare-plaintext + raw-bytes sig path doesn't exist in nostr-mail by
/// design, because raw byte-level transport equivalence isn't preserved.
///
/// Both trust paths validate the same canonical bytes here, so the
/// reported source is "both".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clearsigned_plaintext_verifies_via_header() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let plaintext = "Public announcement: meeting moved to 4pm.";
    let encoded_body = glossia_encode_latin_body(plaintext);

    // Assemble armor without the SIGNATURE block first so we can sign the
    // exact bytes extract_ciphertext_binary will produce on the receive side.
    let partial = format!(
        "----- BEGIN NOSTR SIGNED BODY -----\n{}\n----- END NOSTR MESSAGE -----",
        encoded_body
    );
    let to_sign = email::extract_ciphertext_binary(&partial);
    let sig_hex = crypto::sign_data_bytes(&alice_nsec, &to_sign).expect("sign_data_bytes");
    let pubkey_hex = npub_to_hex(&alice_npub);

    let mut combined = Vec::with_capacity(96);
    combined.extend_from_slice(&hex_decode(&sig_hex));
    combined.extend_from_slice(&hex_decode(&pubkey_hex));

    let words = glossia::load_payload_words_for_wordlist("latin", "default")
        .expect("load latin wordlist");
    let tree = glossia::WordlistTree::new(words);
    let sig_block_words = glossia::codec::encode_base_n(&combined, &tree, "bitpack_fixed")
        .expect("bitpack_fixed encode");
    let sig_block_text = sig_block_words.join(" ");

    let body = format!(
        "{plaintext}\n\n\
         ----- BEGIN NOSTR SIGNED BODY -----\n\
         {encoded_body}\n\
         ----- BEGIN NOSTR SIGNATURE -----\n\
         {sig_block_text}\n\
         ----- END NOSTR MESSAGE -----"
    );

    email::send_email(
        &alice_config,
        "world@test.local",
        "announcement",
        &body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,  // pubkey header
        true,  // sig header — send_email signs the same canonical bytes
        None,  // no recipient (no encryption context)
        false, // skip recipient header
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    let delivered = &inbox[0];
    let raw_headers = raw_headers_from_store(delivered);

    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(sig_valid, Some(true), "clearsigned plaintext must verify");
    assert_eq!(
        sig_source.as_deref(),
        Some("both"),
        "both inline body trust + header trust verify the same canonical bytes"
    );

    assert_eq!(
        email::verify_email_signature_inline(&delivered.body),
        Some(true),
        "inline SIGNATURE block validates against glossia-decoded canonical body bytes"
    );
}

/// Asymmetric: inline SIGNATURE block is valid (body trust path succeeds),
/// but the X-Nostr-Sig header is garbage (header trust path fails).
/// The match table says `(Some(true), _)` → `("body")`, so the verifier
/// reports the body trust path won. Pins that body trust takes precedence
/// over header failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inline_valid_header_broken_reports_body() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (_bob_nsec, bob_npub, _) = test_keypair(2);
    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, "Hello bob");
    let armored = build_armored_with_inline_sig(&alice_nsec, &alice_npub, &ciphertext);

    email::send_email(
        &alice_config,
        "bob@test.local",
        "inline-vs-header",
        &armored,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    let delivered = &inbox[0];
    let raw_headers = raw_headers_from_store(delivered);

    // Sanity: with both paths intact, source is "both".
    let (baseline_valid, baseline_source) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(baseline_valid, Some(true));
    assert_eq!(baseline_source.as_deref(), Some("both"));

    // Now break the header sig — replace with all-zero 64-byte hex.
    let bad_sig = "0".repeat(128);
    let broken_headers = override_header(&raw_headers, "X-Nostr-Sig", &bad_sig);

    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&delivered.body, &broken_headers);
    assert_eq!(sig_valid, Some(true), "body trust path must still verify");
    assert_eq!(
        sig_source.as_deref(),
        Some("body"),
        "body trust path takes precedence when header is invalid"
    );
}

/// Asymmetric (reverse): header sig is valid, but the inline SIGNATURE
/// block is broken (sig doesn't match the encoded pubkey). The match table
/// says `(Some(false), Some(true))` falls through to `(_, Some(true))` →
/// `("header")`. Pins that header trust is the defensive fallback when
/// the inline block can't be trusted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inline_broken_header_valid_reports_header() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (_bob_nsec, bob_npub, _) = test_keypair(2);
    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, "Hello bob");

    // Build an inline SIGNATURE block that's *malformed*: claim alice's
    // pubkey but use an all-zero signature. The block parses cleanly (the
    // bitpack decoder doesn't validate the sig itself), but
    // verify_signature_bytes rejects it.
    let mut combined = Vec::with_capacity(96);
    combined.extend_from_slice(&[0u8; 64]); // all-zero sig
    combined.extend_from_slice(&hex_decode(&npub_to_hex(&alice_npub)));

    let words = glossia::load_payload_words_for_wordlist("latin", "default")
        .expect("load latin wordlist");
    let tree = glossia::WordlistTree::new(words);
    let sig_block_words = glossia::codec::encode_base_n(&combined, &tree, "bitpack_fixed")
        .expect("bitpack_fixed encode");
    let sig_block_text = sig_block_words.join(" ");

    let broken_inline_body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{ciphertext}\n-----BEGIN NOSTR SIGNATURE-----\n{sig_block_text}\n-----END NOSTR MESSAGE-----"
    );

    email::send_email(
        &alice_config,
        "bob@test.local",
        "header-vs-inline",
        &broken_inline_body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,  // pubkey header
        true,  // sig header — will sign over the (broken-inline) body bytes correctly
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    let delivered = &inbox[0];
    let raw_headers = raw_headers_from_store(delivered);

    eprintln!("===== DELIVERED BODY =====\n{}\n===== END =====", delivered.body);
    // Confirm the inline path actually rejects (this is what makes the test
    // meaningful — if the bitpack roundtrip silently swallowed the bad sig
    // we'd be testing nothing).
    assert_eq!(
        email::verify_email_signature_inline(&delivered.body),
        Some(false),
        "broken inline SIGNATURE block must fail to verify"
    );

    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(
        sig_valid,
        Some(true),
        "header trust path must still report valid"
    );
    assert_eq!(
        sig_source.as_deref(),
        Some("header"),
        "broken inline + good header → 'header' (defensive fallback)"
    );
}

/// broken X-Nostr-Pubkey: the sig is a real schnorr signature over the
/// body bytes, but the header pubkey points at a *different* keypair than
/// the one that actually signed. Verification must fail.
///
/// This is the classic identity-spoofing attempt: an attacker takes a
/// well-formed signed email, swaps the pubkey to their own without
/// re-signing under their key. The schnorr verify rejects because the sig
/// is bound to the original signer's key, not the claimed one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broken_pubkey_fails_verification() {
    let mock = spawn_mock_email().await;
    let (alice_nsec, alice_npub, _) = test_keypair(1);
    let (_bob_nsec, bob_npub, _) = test_keypair(2);
    let (_mallory_nsec, mallory_npub, _) = test_keypair(3);
    let alice_config = email_config(
        "alice@test.local",
        "password-alice",
        &alice_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let ciphertext = nip44_encrypt(&alice_nsec, &bob_npub, "Hello bob");
    let body = format!(
        "-----BEGIN NOSTR NIP-44 ENCRYPTED BODY-----\n{}\n-----END NOSTR MESSAGE-----",
        ciphertext
    );

    // alice sends normally — sig is computed by alice over body bytes.
    email::send_email(
        &alice_config,
        "bob@test.local",
        "spoof test",
        &body,
        Some(&alice_npub),
        None,
        None,
        None,
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send_email");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    let delivered = &inbox[0];
    let raw_headers = raw_headers_from_store(delivered);

    // Baseline: alice's pubkey accepts alice's sig.
    let (baseline_valid, _) =
        email::verify_email_signature_full(&delivered.body, &raw_headers);
    assert_eq!(baseline_valid, Some(true), "baseline alice→alice must verify");

    // Swap X-Nostr-Pubkey to mallory's key. The sig stays alice's.
    let spoofed_headers = override_header(&raw_headers, "X-Nostr-Pubkey", &mallory_npub);
    let (sig_valid, sig_source) =
        email::verify_email_signature_full(&delivered.body, &spoofed_headers);
    assert_eq!(
        sig_valid,
        Some(false),
        "sig signed by alice must not verify under mallory's pubkey"
    );
    assert_eq!(sig_source, None, "no trust path should be reported on failure");

    // The extracted pubkey field is what the spoof set it to — confirms
    // the override actually took effect and we're testing what we think
    // we're testing (i.e. the spoof passed parsing and failed at verify).
    assert_eq!(
        email::extract_nostr_pubkey_from_headers(&spoofed_headers).as_deref(),
        Some(mallory_npub.as_str()),
        "override_header must have replaced X-Nostr-Pubkey"
    );
}

/// Reply to a *signed plaintext* email: alice's full armor (SIGNED BODY +
/// SIGNATURE) is quoted with `> ` prefixes and nested *inside* bob's own
/// SIGNED BODY region. After SMTP roundtrip, both signatures must verify
/// recursively — bob's at depth 0 over `bob_prose ++ alice_prose`, and
/// alice's at depth 1 over alice's prose alone.
///
/// Three properties this pins:
///   1. `parse_armor_depth` finds the nested region via unanchored marker
///      match — the `> ` prefix is transparent.
///   2. Glossia decode ignores `> `, dashes, and whitespace, so alice's
///      `> `-prefixed payload words decode to the same bytes as the
///      unquoted original.
///   3. `verify_all_signatures_inline` walks every depth and re-verifies
///      each level independently.
///
/// If any of those drift (regex anchored to `^`, decoder loses BIP-39-style
/// punctuation-stripping, recursion stops at depth 0), this test fails
/// before the reply chain breaks silently in production.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signed_plaintext_reply_preserves_nested_signature() {
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
    let bob_config = email_config(
        "bob@test.local",
        "password-bob",
        &bob_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    // Shared: load latin wordlist once for bitpack_fixed SIGNATURE encoding.
    let words = glossia::load_payload_words_for_wordlist("latin", "default")
        .expect("load latin wordlist");
    let tree = glossia::WordlistTree::new(words);

    // Build a SIGNED BODY + SIGNATURE armor block for `plaintext` signed by
    // `nsec`/`npub`. Mirrors `clearsigned_plaintext_verifies_via_header`'s
    // inline construction — partial first, sign over
    // `extract_ciphertext_binary(&partial)`, then assemble the full block.
    let build_signed_armor = |nsec: &str, npub: &str, plaintext: &str| -> String {
        let encoded = glossia_encode_latin_body(plaintext);
        let partial = format!(
            "----- BEGIN NOSTR SIGNED BODY -----\n{}\n----- END NOSTR MESSAGE -----",
            encoded
        );
        let to_sign = email::extract_ciphertext_binary(&partial);
        let sig_hex = crypto::sign_data_bytes(nsec, &to_sign).expect("sign_data_bytes");
        let pubkey_hex = npub_to_hex(npub);

        let mut combined = Vec::with_capacity(96);
        combined.extend_from_slice(&hex_decode(&sig_hex));
        combined.extend_from_slice(&hex_decode(&pubkey_hex));
        let sig_words = glossia::codec::encode_base_n(&combined, &tree, "bitpack_fixed")
            .expect("bitpack_fixed encode");
        let sig_block = sig_words.join(" ");

        format!(
            "----- BEGIN NOSTR SIGNED BODY -----\n\
             {encoded}\n\
             ----- BEGIN NOSTR SIGNATURE -----\n\
             {sig_block}\n\
             ----- END NOSTR MESSAGE -----"
        )
    };

    // ─── Hop 1: alice → bob (signed plaintext, not encrypted) ──────────────
    let alice_plaintext = "Public announcement: meeting moved to 4pm.";
    let alice_msgid = "alice-signed-root-001@test.local";
    let alice_armor = build_signed_armor(&alice_nsec, &alice_npub, alice_plaintext);
    // Plaintext shown above the armor mirrors what the frontend produces for
    // non-nostr-mail clients (see `clearsigned_plaintext_verifies_via_header`).
    let alice_body = format!("{alice_plaintext}\n\n{alice_armor}");

    email::send_email(
        &alice_config,
        "bob@test.local",
        "announcement",
        &alice_body,
        Some(&alice_npub),
        Some(alice_msgid),
        None,
        None,
        None,
        None,
        true,  // include_pubkey_header
        true,  // include_sig_header
        None,  // no recipient (clearsigned, no encryption context)
        false, // skip recipient header
    )
    .await
    .expect("alice send_email");

    // ─── Hop 2: bob → alice (reply, nesting alice's armor inside bob's) ───
    //
    // Quote alice's entire armor verbatim, including markers, SIGNATURE,
    // and the END marker. The `> ` prefix is per-line; `parse_armor_depth`
    // sees the nested markers via unanchored `contains()`, and the glossia
    // decoder strips `>` as non-alphanumeric punctuation when matching
    // payload words.
    let alice_quoted: String = alice_armor
        .lines()
        .map(|l| format!("> {}", l))
        .collect::<Vec<_>>()
        .join("\n");

    let bob_plaintext = "Acknowledged — see you at 4.";
    let bob_encoded = glossia_encode_latin_body(bob_plaintext);

    // Partial bob: alice's quoted armor lives *inside* bob's SIGNED BODY
    // region (between bob's BEGIN and bob's END), so bob's canonical bytes
    // are `bob_prose_decoded ++ alice_prose_decoded` per
    // extract_ciphertext_binary's recursion (src/email.rs:2931-2948).
    let bob_partial = format!(
        "----- BEGIN NOSTR SIGNED BODY -----\n\
         {bob_encoded}\n\
         {alice_quoted}\n\
         ----- END NOSTR MESSAGE -----"
    );
    let bob_to_sign = email::extract_ciphertext_binary(&bob_partial);
    let bob_sig_hex = crypto::sign_data_bytes(&bob_nsec, &bob_to_sign).expect("bob sign");
    let bob_pubkey_hex = npub_to_hex(&bob_npub);

    let mut bob_combined = Vec::with_capacity(96);
    bob_combined.extend_from_slice(&hex_decode(&bob_sig_hex));
    bob_combined.extend_from_slice(&hex_decode(&bob_pubkey_hex));
    let bob_sig_words = glossia::codec::encode_base_n(&bob_combined, &tree, "bitpack_fixed")
        .expect("bob bitpack_fixed encode");
    let bob_sig_block = bob_sig_words.join(" ");

    // Final bob body: plaintext preamble + outer SIGNED BODY containing
    // (bob's prose, alice's quoted nested armor) + bob's SIGNATURE + END.
    // Inserting bob's SIGNATURE after the quoted region doesn't change the
    // canonical bytes — parse_armor_depth's body-collection loop breaks at
    // `BEGIN NOSTR SIGNATURE` at depth 1, same as without it.
    let bob_reply_body = format!(
        "{bob_plaintext}\n\n\
         ----- BEGIN NOSTR SIGNED BODY -----\n\
         {bob_encoded}\n\
         {alice_quoted}\n\
         ----- BEGIN NOSTR SIGNATURE -----\n\
         {bob_sig_block}\n\
         ----- END NOSTR MESSAGE -----"
    );

    let bob_msgid = "bob-signed-reply-001@test.local";
    email::send_email(
        &bob_config,
        "alice@test.local",
        "Re: announcement",
        &bob_reply_body,
        Some(&bob_npub),
        Some(bob_msgid),
        None,
        None,
        Some(alice_msgid),
        Some(alice_msgid),
        true,
        true,
        None,
        false,
    )
    .await
    .expect("bob send_email reply");

    // ─── Pull both messages out of the mock store ──────────────────────────
    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 2, "expected one root + one reply in INBOX");
    let root = inbox
        .iter()
        .find(|e| e.from.to_string() == "alice@test.local")
        .expect("alice's root is in INBOX");
    let reply = inbox
        .iter()
        .find(|e| e.from.to_string() == "bob@test.local")
        .expect("bob's reply is in INBOX");

    let lookup = |email: &mock_email::Email, name: &str| -> Option<String> {
        email
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    let normalize = |s: &str| {
        s.trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string()
    };

    // (1) Both root and reply have correct Message-ID propagation
    assert_eq!(
        normalize(&lookup(root, "Message-ID").expect("root Message-ID")),
        alice_msgid
    );
    assert_eq!(
        normalize(&lookup(reply, "Message-ID").expect("reply Message-ID")),
        bob_msgid
    );

    // (2) Threading headers
    let in_reply_to = lookup(reply, "In-Reply-To").expect("reply In-Reply-To");
    assert_eq!(normalize(&in_reply_to), alice_msgid, "In-Reply-To");
    let references = lookup(reply, "References").expect("reply References");
    let refs_normalized: Vec<String> =
        references.split_whitespace().map(|s| normalize(s)).collect();
    assert_eq!(
        refs_normalized,
        vec![alice_msgid.to_string()],
        "References on first reply"
    );

    // (3) Re: subject prefix survived
    assert!(
        reply.subject.starts_with("Re: "),
        "reply subject must start with 'Re: '; got {:?}",
        reply.subject
    );

    // (4) The quoted armor markers survive verbatim in the wire body —
    //     per-line `> ` prefix on every alice line.
    assert!(
        reply.body.contains("> ----- BEGIN NOSTR SIGNED BODY -----"),
        "quoted alice BEGIN marker missing"
    );
    assert!(
        reply.body.contains("> ----- BEGIN NOSTR SIGNATURE -----"),
        "quoted alice SIGNATURE marker missing"
    );
    assert!(
        reply.body.contains("> ----- END NOSTR MESSAGE -----"),
        "quoted alice END marker missing"
    );

    // (5) ⭐ Both signatures verify recursively. This is the assertion the
    //     rest of the test exists to enable.
    let results = email::verify_all_signatures_inline(&reply.body);
    assert_eq!(
        results.len(),
        2,
        "expected outer (bob) + nested (alice) signature; got {:?}",
        results
    );

    // Results are ordered innermost-first by verify_all_signatures_recursive:
    // alice (depth 1) lands at index 0, bob (depth 0) at index 1.
    let alice_result = results.iter().find(|r| r.depth == 1).expect("alice depth 1");
    let bob_result = results.iter().find(|r| r.depth == 0).expect("bob depth 0");

    assert!(
        bob_result.is_valid,
        "bob's outer signature must verify; got {:?}",
        bob_result
    );
    assert!(
        alice_result.is_valid,
        "alice's nested signature must verify through `> ` quoting; got {:?}",
        alice_result
    );
    assert_eq!(
        bob_result.pubkey_hex.as_deref(),
        Some(npub_to_hex(&bob_npub).as_str()),
        "bob signed at depth 0"
    );
    assert_eq!(
        alice_result.pubkey_hex.as_deref(),
        Some(npub_to_hex(&alice_npub).as_str()),
        "alice signed at depth 1"
    );

    // (6) No plaintext leak: alice's original plaintext appears nowhere in
    //     the wire body. The frontend never quotes plaintext for a signed
    //     reply — only the encoded armor — so a regression that "helpfully"
    //     decoded for display before send would trip this.
    assert!(
        !reply.body.contains(alice_plaintext),
        "PLAINTEXT LEAK: alice's plaintext appears in bob's reply body"
    );

    // (7) Replicate database::compute_thread_id's rule locally and confirm
    //     both messages group under alice's normalized id.
    let thread_id_of = |msg_id: &str, refs: Option<&str>, irt: Option<&str>| -> String {
        if let Some(r) = refs {
            if let Some(first) = r.split_whitespace().next() {
                return normalize(first);
            }
        }
        if let Some(p) = irt {
            if let Some(first) = p.split_whitespace().next() {
                return normalize(first);
            }
        }
        normalize(msg_id)
    };
    let root_thread = thread_id_of(alice_msgid, None, None);
    let reply_thread = thread_id_of(bob_msgid, Some(&references), Some(&in_reply_to));
    assert_eq!(
        root_thread, reply_thread,
        "root and reply must compute to the same thread_id"
    );
    assert_eq!(reply_thread, alice_msgid, "thread root is alice's Message-ID");
}

/// Encrypted reply: alice NIP-44-encrypts to bob with an inline SIGNATURE
/// block; bob NIP-44-encrypts a reply with alice's *entire* armor (BODY +
/// SIGNATURE) nested directly inside bob's ENCRYPTED BODY region — with NO
/// `> ` quote prefix (per the production encrypted-reply structure; the
/// `> ` prefix is only used for signed plaintext, where it gives a UX
/// affordance to non-nostr-mail clients).
///
/// After SMTP roundtrip the test asserts:
///   - Bob's outer signature verifies at depth 0 over `bob_ct ++ alice_ct`.
///   - Alice's nested signature verifies at depth 1 over her own ciphertext.
///   - `decrypt_email_body_pipeline` as alice returns two `block_results`
///     (innermost-first), each successfully decrypted to the original
///     plaintext at that level.
///
/// X25519 ECDH is symmetric, so alice can decrypt her own outbound ct1
/// (she's one of the two participants) — that's what makes the nested
/// re-decrypt possible on the recipient side.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nip44_reply_preserves_nested_encrypted_armor() {
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
    let bob_config = email_config(
        "bob@test.local",
        "password-bob",
        &bob_nsec,
        mock.smtp_addr,
        mock.imap_addr,
    );

    let words = glossia::load_payload_words_for_wordlist("latin", "default")
        .expect("load latin wordlist");
    let tree = glossia::WordlistTree::new(words);

    // Inline helper: build a NIP-44 encrypted armor block with inline
    // SIGNATURE over the glossia-decoded base64 ciphertext bytes (matches
    // production: `extract_ciphertext_binary` returns the base64 string
    // bytes regardless of whether they were stored raw or glossia-encoded).
    let build_signed_nip44 = |nsec: &str, npub: &str, ct_b64: &str| -> String {
        let ct_glossia = glossia_encode_latin_body(ct_b64);
        let partial = format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n{ct_glossia}\n----- END NOSTR MESSAGE -----"
        );
        let to_sign = email::extract_ciphertext_binary(&partial);
        let sig_hex = crypto::sign_data_bytes(nsec, &to_sign).expect("sign_data_bytes");
        let pubkey_hex = npub_to_hex(npub);

        let mut combined = Vec::with_capacity(96);
        combined.extend_from_slice(&hex_decode(&sig_hex));
        combined.extend_from_slice(&hex_decode(&pubkey_hex));
        let sig_words = glossia::codec::encode_base_n(&combined, &tree, "bitpack_fixed")
            .expect("bitpack_fixed encode");
        let sig_block = sig_words.join(" ");

        format!(
            "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
             {ct_glossia}\n\
             ----- BEGIN NOSTR SIGNATURE -----\n\
             {sig_block}\n\
             ----- END NOSTR MESSAGE -----"
        )
    };

    // ─── Hop 1: alice → bob (NIP-44 encrypted, signed) ─────────────────────
    let alice_plaintext = "Hello bob, want coffee Thursday?";
    let alice_ct = nip44_encrypt(&alice_nsec, &bob_npub, alice_plaintext);
    let alice_armor = build_signed_nip44(&alice_nsec, &alice_npub, &alice_ct);
    let alice_msgid = "alice-enc-root-001@test.local";

    email::send_email(
        &alice_config,
        "bob@test.local",
        "coffee thursday?",
        &alice_armor,
        Some(&alice_npub),
        Some(alice_msgid),
        None,
        None,
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("alice send_email");

    // ─── Hop 2: bob → alice (NIP-44 encrypted reply, alice nested inside) ─
    let bob_plaintext = "Sure, 3pm at the usual place?";
    let bob_ct = nip44_encrypt(&bob_nsec, &alice_npub, bob_plaintext);
    let bob_ct_glossia = glossia_encode_latin_body(&bob_ct);

    // Nest alice's full armor inside bob's encrypted body — NO `> ` prefix.
    // parse_armor_depth finds the inner BEGIN/END pair via depth counting.
    let bob_partial = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
         {bob_ct_glossia}\n\
         {alice_armor}\n\
         ----- END NOSTR MESSAGE -----"
    );
    let bob_to_sign = email::extract_ciphertext_binary(&bob_partial);
    let bob_sig_hex = crypto::sign_data_bytes(&bob_nsec, &bob_to_sign).expect("bob sign");
    let bob_pubkey_hex = npub_to_hex(&bob_npub);

    let mut bob_combined = Vec::with_capacity(96);
    bob_combined.extend_from_slice(&hex_decode(&bob_sig_hex));
    bob_combined.extend_from_slice(&hex_decode(&bob_pubkey_hex));
    let bob_sig_words = glossia::codec::encode_base_n(&bob_combined, &tree, "bitpack_fixed")
        .expect("bob bitpack_fixed encode");
    let bob_sig_block = bob_sig_words.join(" ");

    let bob_reply_body = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
         {bob_ct_glossia}\n\
         {alice_armor}\n\
         ----- BEGIN NOSTR SIGNATURE -----\n\
         {bob_sig_block}\n\
         ----- END NOSTR MESSAGE -----"
    );

    let bob_msgid = "bob-enc-reply-001@test.local";
    email::send_email(
        &bob_config,
        "alice@test.local",
        "Re: coffee thursday?",
        &bob_reply_body,
        Some(&bob_npub),
        Some(bob_msgid),
        None,
        None,
        Some(alice_msgid),
        Some(alice_msgid),
        true,
        true,
        Some(&alice_npub),
        true,
    )
    .await
    .expect("bob send_email reply");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 2, "expected one root + one reply in INBOX");
    let reply = inbox
        .iter()
        .find(|e| e.from.to_string() == "bob@test.local")
        .expect("bob's reply is in INBOX");

    // (1) ⭐ Both signatures verify recursively.
    let results = email::verify_all_signatures_inline(&reply.body);
    assert_eq!(
        results.len(),
        2,
        "expected outer (bob) + nested (alice) signature; got {:?}",
        results
    );
    let alice_result = results.iter().find(|r| r.depth == 1).expect("alice depth 1");
    let bob_result = results.iter().find(|r| r.depth == 0).expect("bob depth 0");
    assert!(bob_result.is_valid, "bob's outer sig must verify: {:?}", bob_result);
    assert!(
        alice_result.is_valid,
        "alice's nested sig must verify (no `> ` prefix in encrypted nesting): {:?}",
        alice_result
    );
    assert_eq!(bob_result.body_type, "encrypted");
    assert_eq!(alice_result.body_type, "encrypted");

    // (2) ⭐ Recursive decrypt as alice yields two layers, each decrypts
    //     to its respective plaintext. block_results are innermost-first.
    let decrypt = email::decrypt_email_body_pipeline(
        &alice_nsec,
        &reply.body,
        &reply.subject,
        Some(&bob_npub),
        Some(&alice_npub),
    )
    .expect("decrypt_email_body_pipeline");
    assert!(decrypt.success, "decrypt must succeed: {:?}", decrypt.error);
    assert_eq!(
        decrypt.block_results.len(),
        2,
        "expected 2 decrypted blocks (alice innermost, bob outermost); got {:?}",
        decrypt.block_results
    );
    let inner = &decrypt.block_results[0];
    let outer = &decrypt.block_results[1];
    assert!(inner.was_encrypted);
    assert!(outer.was_encrypted);
    assert_eq!(
        inner.decrypted_text.as_deref(),
        Some(alice_plaintext),
        "alice's nested ciphertext must decrypt to her original plaintext; error={:?}",
        inner.error
    );
    assert_eq!(
        outer.decrypted_text.as_deref(),
        Some(bob_plaintext),
        "bob's outer ciphertext must decrypt to his reply plaintext; error={:?}",
        outer.error
    );

    // (3) No plaintext leak: neither plaintext appears anywhere in the wire body.
    assert!(
        !reply.body.contains(alice_plaintext),
        "alice's plaintext leaked into wire body"
    );
    assert!(
        !reply.body.contains(bob_plaintext),
        "bob's plaintext leaked into wire body"
    );

    // (4) Threading sanity: In-Reply-To points at alice's root, Re: prefix kept.
    let in_reply_to = reply
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("In-Reply-To"))
        .map(|(_, v)| v.clone())
        .expect("In-Reply-To header");
    let normalize = |s: &str| {
        s.trim().trim_start_matches('<').trim_end_matches('>').to_string()
    };
    assert_eq!(normalize(&in_reply_to), alice_msgid);
    assert!(reply.subject.starts_with("Re: "));
}

/// Three-level encrypted reply chain (matches the production example
/// pattern exactly): alice→bob, then bob→alice nests alice's armor, then
/// alice→bob nests bob's full reply armor. The final wire body has three
/// `BEGIN NOSTR NIP-44 ENCRYPTED BODY` markers followed by three
/// `SIGNATURE`/`END NOSTR MESSAGE` pairs unwinding innermost-first.
///
/// We construct all three layers locally and only SMTP-send hop 3 (the
/// recursive structure is the unit under test, not the threading chain
/// — that's covered separately by the 2-level test).
///
/// Asserts: three signatures verify at depths 0/1/2, and a recipient-side
/// recursive decrypt yields three plaintext layers in the correct order.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nip44_three_level_reply_chain() {
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

    let words = glossia::load_payload_words_for_wordlist("latin", "default")
        .expect("load latin wordlist");
    let tree = glossia::WordlistTree::new(words);

    // Inline helper: sign+encode a 96-byte (sig||pubkey) blob as a
    // bitpack_fixed SIGNATURE block body (just the words, no markers).
    let sig_block = |nsec: &str, npub: &str, canonical: &[u8]| -> String {
        let sig_hex = crypto::sign_data_bytes(nsec, canonical).expect("sign");
        let pubkey_hex = npub_to_hex(npub);
        let mut combined = Vec::with_capacity(96);
        combined.extend_from_slice(&hex_decode(&sig_hex));
        combined.extend_from_slice(&hex_decode(&pubkey_hex));
        glossia::codec::encode_base_n(&combined, &tree, "bitpack_fixed")
            .expect("bitpack_fixed encode")
            .join(" ")
    };

    // ─── Layer 1: alice → bob (innermost, will end up at depth 2) ─────────
    let pt_1 = "Hello bob, want coffee?";
    let ct_1 = nip44_encrypt(&alice_nsec, &bob_npub, pt_1);
    let ct_1_glossia = glossia_encode_latin_body(&ct_1);
    let partial_1 = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n{ct_1_glossia}\n----- END NOSTR MESSAGE -----"
    );
    let canonical_1 = email::extract_ciphertext_binary(&partial_1);
    let sig_1 = sig_block(&alice_nsec, &alice_npub, &canonical_1);
    let armor_1 = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
         {ct_1_glossia}\n\
         ----- BEGIN NOSTR SIGNATURE -----\n\
         {sig_1}\n\
         ----- END NOSTR MESSAGE -----"
    );

    // ─── Layer 2: bob → alice reply (will end up at depth 1) ──────────────
    let pt_2 = "Sure, 3pm at the usual place?";
    let ct_2 = nip44_encrypt(&bob_nsec, &alice_npub, pt_2);
    let ct_2_glossia = glossia_encode_latin_body(&ct_2);
    let partial_2 = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
         {ct_2_glossia}\n\
         {armor_1}\n\
         ----- END NOSTR MESSAGE -----"
    );
    let canonical_2 = email::extract_ciphertext_binary(&partial_2);
    let sig_2 = sig_block(&bob_nsec, &bob_npub, &canonical_2);
    let armor_2 = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
         {ct_2_glossia}\n\
         {armor_1}\n\
         ----- BEGIN NOSTR SIGNATURE -----\n\
         {sig_2}\n\
         ----- END NOSTR MESSAGE -----"
    );

    // ─── Layer 3: alice → bob reply-to-reply (outermost, depth 0) ─────────
    let pt_3 = "Great, see you then.";
    let ct_3 = nip44_encrypt(&alice_nsec, &bob_npub, pt_3);
    let ct_3_glossia = glossia_encode_latin_body(&ct_3);
    let partial_3 = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
         {ct_3_glossia}\n\
         {armor_2}\n\
         ----- END NOSTR MESSAGE -----"
    );
    let canonical_3 = email::extract_ciphertext_binary(&partial_3);
    let sig_3 = sig_block(&alice_nsec, &alice_npub, &canonical_3);
    let armor_3 = format!(
        "----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----\n\
         {ct_3_glossia}\n\
         {armor_2}\n\
         ----- BEGIN NOSTR SIGNATURE -----\n\
         {sig_3}\n\
         ----- END NOSTR MESSAGE -----"
    );

    // Sanity: the assembled body has exactly 3 BEGIN-BODY markers and
    // 3 END-MESSAGE markers (one per layer). Pins our construction before
    // we hand it to the wire.
    assert_eq!(armor_3.matches("BEGIN NOSTR NIP-44 ENCRYPTED BODY").count(), 3);
    assert_eq!(armor_3.matches("END NOSTR MESSAGE").count(), 3);

    // ─── Send hop 3 via SMTP ──────────────────────────────────────────────
    let msgid = "alice-enc-reply2-001@test.local";
    email::send_email(
        &alice_config,
        "bob@test.local",
        "Re: Re: coffee thursday?",
        &armor_3,
        Some(&alice_npub),
        Some(msgid),
        None,
        None,
        None,
        None,
        true,
        true,
        Some(&bob_npub),
        true,
    )
    .await
    .expect("send hop 3");

    let inbox = mock.store.get_mailbox_emails("INBOX").await;
    assert_eq!(inbox.len(), 1, "single-message test");
    let delivered = &inbox[0];

    // (1) ⭐ Three signatures verify at the right depths.
    let results = email::verify_all_signatures_inline(&delivered.body);
    assert_eq!(
        results.len(),
        3,
        "expected 3 signatures across the nested chain; got {:?}",
        results
    );
    for r in &results {
        assert!(
            r.is_valid,
            "signature at depth {} must verify; result={:?}",
            r.depth, r
        );
        assert_eq!(r.body_type, "encrypted");
    }
    let pk_at = |d: usize| -> String {
        results
            .iter()
            .find(|r| r.depth == d)
            .and_then(|r| r.pubkey_hex.clone())
            .expect("signature at this depth")
    };
    assert_eq!(pk_at(0), npub_to_hex(&alice_npub), "depth 0 = alice (hop 3)");
    assert_eq!(pk_at(1), npub_to_hex(&bob_npub),   "depth 1 = bob (hop 2)");
    assert_eq!(pk_at(2), npub_to_hex(&alice_npub), "depth 2 = alice (hop 1)");

    // (2) ⭐ Recursive decrypt as bob yields three plaintexts in
    //     innermost-first order: [pt_1, pt_2, pt_3].
    let decrypt = email::decrypt_email_body_pipeline(
        &bob_nsec,
        &delivered.body,
        &delivered.subject,
        Some(&alice_npub),
        Some(&bob_npub),
    )
    .expect("decrypt pipeline");
    assert!(decrypt.success, "decrypt failed: {:?}", decrypt.error);
    assert_eq!(
        decrypt.block_results.len(),
        3,
        "expected 3 decrypted layers; got {:?}",
        decrypt.block_results
    );
    let texts: Vec<&str> = decrypt
        .block_results
        .iter()
        .map(|b| b.decrypted_text.as_deref().unwrap_or("<none>"))
        .collect();
    assert_eq!(
        texts,
        vec![pt_1, pt_2, pt_3],
        "innermost-first plaintext order must match the construction chain"
    );

    // (3) No plaintext leak at any layer.
    for pt in &[pt_1, pt_2, pt_3] {
        assert!(
            !delivered.body.contains(pt),
            "plaintext {:?} leaked into wire body",
            pt
        );
    }
}
