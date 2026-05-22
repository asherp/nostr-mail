// TEST-ONLY KEYS AND HELPERS — fixtures derived from single-byte seeds.
// These keypairs (nsec1qqqqqq…, nsec1qzqzpq…, etc.) are obviously fake;
// they MUST NOT be pasted into a real settings.json.

#![allow(dead_code)]

use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::sync::Arc;
use std::time::Duration;

use mock_email::{EmailStore, ImapServer, SmtpServer};
use nostr_sdk::prelude::*;

use nostr_mail_lib::types::EmailConfig;

/// Bind an ephemeral port and immediately release it. There is an unavoidable
/// TOCTOU race here, but it matches the pattern used in
/// mock-email/tests/integration_tests.rs and is fine for our single-process tests.
pub fn find_available_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

pub struct MockEmail {
    pub store: Arc<EmailStore>,
    pub smtp_addr: SocketAddr,
    pub imap_addr: SocketAddr,
}

pub async fn spawn_mock_email() -> MockEmail {
    let store = Arc::new(EmailStore::new());
    store.init().await;

    let smtp_addr = SocketAddr::from(([127, 0, 0, 1], find_available_port()));
    let imap_addr = SocketAddr::from(([127, 0, 0, 1], find_available_port()));

    let smtp = SmtpServer::new(smtp_addr, store.clone());
    let imap = ImapServer::new(imap_addr, store.clone());

    tokio::spawn(async move {
        let _ = smtp.start().await;
    });
    tokio::spawn(async move {
        let _ = imap.start().await;
    });

    // Give the listeners a moment to bind before tests dial them.
    tokio::time::sleep(Duration::from_millis(150)).await;

    MockEmail {
        store,
        smtp_addr,
        imap_addr,
    }
}

/// Deterministic test keypair derived from a single byte seed.
/// Returns (nsec, npub, Keys). seed=0 is rejected (zero is not a valid secp256k1 secret).
pub fn test_keypair(seed: u8) -> (String, String, Keys) {
    assert!(seed != 0, "secp256k1 secret keys cannot be zero");
    let bytes = [seed; 32];
    let sk = SecretKey::from_slice(&bytes).expect("non-zero 32-byte secret is valid");
    let keys = Keys::new(sk.clone());
    let nsec = sk.to_bech32().expect("nsec encode");
    let npub = keys.public_key().to_bech32().expect("npub encode");
    (nsec, npub, keys)
}

pub fn email_config(
    address: &str,
    password: &str,
    nsec: &str,
    smtp: SocketAddr,
    imap: SocketAddr,
) -> EmailConfig {
    EmailConfig {
        email_address: address.to_string(),
        password: password.to_string(),
        smtp_host: smtp.ip().to_string(),
        smtp_port: smtp.port(),
        imap_host: imap.ip().to_string(),
        imap_port: imap.port(),
        use_tls: false,
        private_key: Some(nsec.to_string()),
    }
}

/// Convert npub → hex pubkey (32 bytes hex-encoded).
pub fn npub_to_hex(npub: &str) -> String {
    let pk = PublicKey::from_bech32(npub).expect("valid npub");
    pk.to_hex()
}

/// Convert hex pubkey → npub.
pub fn hex_to_npub(hex: &str) -> String {
    let pk = PublicKey::from_hex(hex).expect("valid pubkey hex");
    pk.to_bech32().expect("npub encode")
}
