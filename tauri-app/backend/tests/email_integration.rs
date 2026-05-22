mod common;

use common::*;
use nostr_mail_lib::{crypto, email};

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
    use base64::{engine::general_purpose, Engine as _};
    let ciphertext_bytes = email::extract_ciphertext_binary(&delivered.body);
    let ciphertext_b64 = general_purpose::STANDARD.encode(&ciphertext_bytes);
    let decrypted = crypto::decrypt_message(&bob_nsec, &alice_npub, &ciphertext_b64)
        .expect("nip44 decrypt");
    assert_eq!(decrypted, plaintext, "plaintext round-trip mismatch");
}
