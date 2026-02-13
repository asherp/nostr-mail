//! WASM browser tests for nostr-mail-crypto.
//!
//! Run with: wasm-pack test --headless --chrome  (or --firefox / --node)

use wasm_bindgen_test::*;
use nostr_mail_crypto::*;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_init() {
    // Should not panic
    init();
    // Calling twice should also be fine (set_once is idempotent)
    init();
}

// ---------------------------------------------------------------------------
// Key Generation
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_keypair_generation() {
    let kp = generate_keypair().unwrap();
    let priv_key = kp.private_key();
    let pub_key = kp.public_key();

    assert!(!priv_key.is_empty(), "private key should not be empty");
    assert!(!pub_key.is_empty(), "public key should not be empty");

    // nsec / npub prefixes
    assert!(priv_key.starts_with("nsec1"), "private key should start with nsec1");
    assert!(pub_key.starts_with("npub1"), "public key should start with npub1");
}

// ---------------------------------------------------------------------------
// Public Key Derivation
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_public_key_derivation() {
    let kp = generate_keypair().unwrap();
    let derived = get_public_key_from_private(&kp.private_key()).unwrap();
    assert_eq!(kp.public_key(), derived, "derived public key should match");
}

// ---------------------------------------------------------------------------
// Key Validation
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_validate_private_key_valid() {
    let kp = generate_keypair().unwrap();
    assert!(validate_private_key(&kp.private_key()));
}

#[wasm_bindgen_test]
fn test_validate_private_key_invalid() {
    assert!(!validate_private_key("not-a-valid-nsec"));
    assert!(!validate_private_key(""));
}

#[wasm_bindgen_test]
fn test_validate_public_key_valid() {
    let kp = generate_keypair().unwrap();
    assert!(validate_public_key(&kp.public_key()));
}

#[wasm_bindgen_test]
fn test_validate_public_key_invalid() {
    assert!(!validate_public_key("not-a-valid-npub"));
    assert!(!validate_public_key(""));
}

// ---------------------------------------------------------------------------
// NIP-44 Encrypt / Decrypt Round-Trip
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_nip44_encrypt_decrypt() {
    let sender = generate_keypair().unwrap();
    let recipient = generate_keypair().unwrap();

    let plaintext = "Hello from NIP-44 in WASM!";
    let encrypted = encrypt_message(
        &sender.private_key(),
        &recipient.public_key(),
        plaintext,
        Some("nip44".into()),
    )
    .unwrap();

    assert!(!encrypted.is_empty());
    assert_ne!(encrypted, plaintext, "ciphertext should differ from plaintext");

    let decrypted = decrypt_message(
        &recipient.private_key(),
        &sender.public_key(),
        &encrypted,
    )
    .unwrap();

    assert_eq!(decrypted, plaintext);
}

// ---------------------------------------------------------------------------
// NIP-04 Encrypt / Decrypt Round-Trip
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_nip04_encrypt_decrypt() {
    let sender = generate_keypair().unwrap();
    let recipient = generate_keypair().unwrap();

    let plaintext = "Hello from NIP-04 in WASM!";
    let encrypted = encrypt_message(
        &sender.private_key(),
        &recipient.public_key(),
        plaintext,
        Some("nip04".into()),
    )
    .unwrap();

    assert!(!encrypted.is_empty());
    // NIP-04 format contains ?iv=
    assert!(encrypted.contains("?iv="), "NIP-04 ciphertext should contain ?iv= separator");

    let decrypted = decrypt_message(
        &recipient.private_key(),
        &sender.public_key(),
        &encrypted,
    )
    .unwrap();

    assert_eq!(decrypted, plaintext);
}

// ---------------------------------------------------------------------------
// Default Algorithm (should be NIP-44)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_default_algorithm_is_nip44() {
    let sender = generate_keypair().unwrap();
    let recipient = generate_keypair().unwrap();

    let plaintext = "Default algorithm test";
    // Pass None for algorithm -- should default to nip44
    let encrypted = encrypt_message(
        &sender.private_key(),
        &recipient.public_key(),
        plaintext,
        None,
    )
    .unwrap();

    // NIP-44 ciphertext should NOT contain ?iv=
    assert!(!encrypted.contains("?iv="), "NIP-44 (default) should not contain ?iv=");

    let decrypted = decrypt_message(
        &recipient.private_key(),
        &sender.public_key(),
        &encrypted,
    )
    .unwrap();

    assert_eq!(decrypted, plaintext);
}

// ---------------------------------------------------------------------------
// Detect Encryption Format
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_detect_nip04_format() {
    let result = detect_encryption_format("dGVzdA==?iv=dGVzdA==");
    assert_eq!(result, "nip04");
}

#[wasm_bindgen_test]
fn test_detect_nip04_format_real() {
    // Generate a real NIP-04 ciphertext and check detection
    let sender = generate_keypair().unwrap();
    let recipient = generate_keypair().unwrap();
    let encrypted = encrypt_message(
        &sender.private_key(),
        &recipient.public_key(),
        "test",
        Some("nip04".into()),
    )
    .unwrap();
    assert_eq!(detect_encryption_format(&encrypted), "nip04");
}

#[wasm_bindgen_test]
fn test_detect_empty() {
    assert_eq!(detect_encryption_format(""), "unknown");
}

#[wasm_bindgen_test]
fn test_detect_plain_text() {
    assert_eq!(detect_encryption_format("just some plain text"), "unknown");
}

// ---------------------------------------------------------------------------
// Schnorr Sign / Verify Round-Trip
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_sign_verify() {
    let kp = generate_keypair().unwrap();
    let data = "Message to sign in WASM";

    let signature = sign_data(&kp.private_key(), data).unwrap();
    assert!(!signature.is_empty());
    // Schnorr signature is 64 bytes = 128 hex chars
    assert_eq!(signature.len(), 128, "signature should be 128 hex characters");

    let valid = verify_signature(&kp.public_key(), &signature, data).unwrap();
    assert!(valid, "signature should be valid");
}

#[wasm_bindgen_test]
fn test_sign_verify_tampered_data() {
    let kp = generate_keypair().unwrap();
    let data = "Original message";
    let signature = sign_data(&kp.private_key(), data).unwrap();

    let valid = verify_signature(&kp.public_key(), &signature, "Tampered message").unwrap();
    assert!(!valid, "signature should be invalid for tampered data");
}

#[wasm_bindgen_test]
fn test_sign_verify_wrong_key() {
    let kp1 = generate_keypair().unwrap();
    let kp2 = generate_keypair().unwrap();
    let data = "Message signed by kp1";
    let signature = sign_data(&kp1.private_key(), data).unwrap();

    let valid = verify_signature(&kp2.public_key(), &signature, data).unwrap();
    assert!(!valid, "signature should be invalid for wrong public key");
}

// ---------------------------------------------------------------------------
// Settings Encrypt / Decrypt Round-Trip
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_settings_encrypt_decrypt() {
    let kp = generate_keypair().unwrap();
    let value = "smtp_password=hunter2";

    let encrypted = encrypt_setting_value(&kp.private_key(), value).unwrap();
    assert!(!encrypted.is_empty());
    assert_ne!(encrypted, value, "encrypted should differ from plaintext");

    let decrypted = decrypt_setting_value(&kp.private_key(), &encrypted).unwrap();
    assert_eq!(decrypted, value);
}

#[wasm_bindgen_test]
fn test_settings_empty_value() {
    let kp = generate_keypair().unwrap();

    let encrypted = encrypt_setting_value(&kp.private_key(), "").unwrap();
    assert_eq!(encrypted, "", "encrypting empty string should return empty string");

    let decrypted = decrypt_setting_value(&kp.private_key(), "").unwrap();
    assert_eq!(decrypted, "", "decrypting empty string should return empty string");
}

#[wasm_bindgen_test]
fn test_settings_wrong_key_fails() {
    let kp1 = generate_keypair().unwrap();
    let kp2 = generate_keypair().unwrap();
    let value = "secret-setting";

    let encrypted = encrypt_setting_value(&kp1.private_key(), value).unwrap();
    // Decrypting with a different key should fail
    let result = decrypt_setting_value(&kp2.private_key(), &encrypted);
    assert!(result.is_err(), "decrypting with wrong key should fail");
}

// ---------------------------------------------------------------------------
// Cross-compatibility: encrypt with one algo, detect format
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_nip44_detect_after_encrypt() {
    let sender = generate_keypair().unwrap();
    let recipient = generate_keypair().unwrap();
    let encrypted = encrypt_message(
        &sender.private_key(),
        &recipient.public_key(),
        "detect me",
        Some("nip44".into()),
    )
    .unwrap();

    let format = detect_encryption_format(&encrypted);
    // NIP-44 ciphertext should be detected as nip44 (base64 with version byte)
    assert!(
        format == "nip44" || format == "unknown",
        "NIP-44 ciphertext should be detected as nip44 or unknown, got: {}",
        format
    );
}
