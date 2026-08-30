//! Cryptographic DKIM verification (RFC 6376).
//!
//! This is the counterpart to the `Authentication-Results` parsing in
//! [`crate::email`]. That path *reads a claim* our provider made; this path
//! *checks the signature ourselves*. The difference matters: a parsed header is
//! only as trustworthy as our ability to tell which hop wrote it, whereas a
//! verified signature is evidence independent of any header.
//!
//! Verification needs a DNS TXT lookup (`<selector>._domainkey.<domain>`) to
//! fetch the signing key, which brings two constraints the rest of this module
//! is shaped around:
//!
//! 1. **It can fail for reasons that are not forgery.** Offline, captive
//!    portal, resolver misconfigured, DNS timeout. Those produce
//!    [`DkimVerdict::Unavailable`], never [`DkimVerdict::Fail`] — callers drop
//!    messages on a failed transport verdict, and "the train was in a tunnel"
//!    must not delete someone's mail.
//! 2. **It is async, and our IMAP loop is not.** `MessageAuthenticator` is
//!    async, but `verify_transport_authentication` and its callers are plain
//!    sync functions running on the blocking IMAP thread. See [`block_on`].

use mail_auth::{AuthenticatedMessage, DkimResult, MessageAuthenticator};
use std::sync::OnceLock;

/// Result of verifying a message's DKIM signatures ourselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DkimVerdict {
    /// A signature verified cryptographically *and* its `d=` aligns with the
    /// `From:` domain. This is the only variant that should raise trust.
    PassAligned { domain: String },
    /// A signature verified, but no verifying signature aligned with `From:`.
    /// Someone signed this message; it was not the domain it claims to be from.
    PassNotAligned { signed_by: String, from_domain: String },
    /// Signatures are present and none verified — bad body hash, bad signature,
    /// revoked or missing key. Real evidence of tampering or forgery.
    Fail { reason: String },
    /// No `DKIM-Signature` header at all. Common and not itself suspicious.
    NoSignature,
    /// We could not perform the check. Explicitly *not* a failure.
    Unavailable { reason: String },
}

impl DkimVerdict {
    /// True only for a verified, aligned signature.
    pub fn is_trustworthy(&self) -> bool {
        matches!(self, DkimVerdict::PassAligned { .. })
    }

    /// True when we obtained real evidence against the message, as opposed to
    /// merely being unable to check it.
    pub fn is_evidence_of_forgery(&self) -> bool {
        matches!(self, DkimVerdict::Fail { .. } | DkimVerdict::PassNotAligned { .. })
    }
}

/// Shared resolver. `None` if construction failed (no `/etc/resolv.conf`, which
/// is the normal case on Android) — we degrade to Unavailable rather than
/// retrying a resolver that will not appear.
fn authenticator() -> Option<&'static MessageAuthenticator> {
    static AUTHENTICATOR: OnceLock<Option<MessageAuthenticator>> = OnceLock::new();
    AUTHENTICATOR
        .get_or_init(|| match MessageAuthenticator::new_system_conf() {
            Ok(resolver) => Some(resolver),
            Err(e) => {
                crate::debug_log!(
                    "[RUST] dkim: no system DNS configuration ({e}); DKIM verification disabled. \
                     Transport auth falls back to reading Authentication-Results."
                );
                None
            }
        })
        .as_ref()
}

/// Dedicated runtime for DNS. Separate from the app's runtime so that blocking
/// on it cannot starve application tasks.
fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| crate::debug_log!("[RUST] dkim: could not build DNS runtime: {e}"))
                .ok()
        })
        .as_ref()
}

/// Drive a future to completion from a synchronous caller.
///
/// `Runtime::block_on` panics if it is called while a runtime context is
/// already active on the thread, and we cannot prove our callers are clear of
/// one — `should_rescue_message` runs inside the IMAP loop, which may itself be
/// invoked from a Tauri command on a runtime worker. Running the block on a
/// freshly spawned thread sidesteps the question entirely: a new thread never
/// carries a runtime context.
fn block_on<T, F>(future: F) -> Option<T>
where
    T: Send,
    F: std::future::Future<Output = T> + Send,
{
    let runtime = runtime()?;
    std::thread::scope(|scope| scope.spawn(|| runtime.block_on(future)).join().ok())
}

/// Cheap scan for a `DKIM-Signature` header, to skip the expensive path on
/// unsigned mail.
///
/// Only scans the header block: a body can contain the literal text
/// "DKIM-Signature:" (a quoted reply, or a bounce carrying the original
/// message), and treating that as a signature would send us down the slow path
/// for nothing. A false positive is only a wasted lookup, never a wrong
/// verdict, but the header block is where the answer actually lives.
fn has_dkim_signature(raw_message: &[u8]) -> bool {
    const NEEDLE: &[u8] = b"dkim-signature:";

    // Headers end at the first blank line (CRLF CRLF, or LF LF from a
    // lenient server).
    let header_end = raw_message
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 2)
        .or_else(|| raw_message.windows(2).position(|w| w == b"\n\n").map(|i| i + 1))
        .unwrap_or(raw_message.len());

    raw_message[..header_end]
        .windows(NEEDLE.len())
        .any(|w| w.eq_ignore_ascii_case(NEEDLE))
}

/// Verify the DKIM signatures on a raw RFC 5322 message and check alignment
/// against `from_domain`.
pub fn verify_dkim(raw_message: &[u8], from_domain: &str) -> DkimVerdict {
    // Most mail we see on the fallback path is unsigned. Checking for the
    // header first avoids parsing the message and spawning a thread per
    // message during an IMAP sync, for a result that is already known.
    if !has_dkim_signature(raw_message) {
        return DkimVerdict::NoSignature;
    }

    let Some(authenticator) = authenticator() else {
        return DkimVerdict::Unavailable {
            reason: "no system DNS resolver available".to_string(),
        };
    };

    let Some(message) = AuthenticatedMessage::parse(raw_message) else {
        return DkimVerdict::Unavailable {
            reason: "message could not be parsed for DKIM".to_string(),
        };
    };

    let Some(outputs) = block_on(authenticator.verify_dkim(&message)) else {
        return DkimVerdict::Unavailable {
            reason: "DKIM lookup could not be scheduled".to_string(),
        };
    };

    interpret(&outputs, from_domain)
}

/// Turn mail-auth's per-signature outputs into a single verdict.
///
/// Split out from [`verify_dkim`] so the decision logic is testable without a
/// resolver; the DNS-backed path is covered separately with a seeded cache.
fn interpret(outputs: &[mail_auth::DkimOutput<'_>], from_domain: &str) -> DkimVerdict {
    if outputs.is_empty() {
        return DkimVerdict::NoSignature;
    }

    let from_domain = from_domain.to_lowercase();
    let mut signed_by: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut transient = false;

    for output in outputs {
        let signature_domain = output.signature().map(|s| s.d.to_lowercase());

        match output.result() {
            DkimResult::Pass => {
                let Some(domain) = signature_domain else { continue };

                // RFC 6376 §3.5 `l=`: a signature may cover only the first N
                // bytes of the body, which lets anyone append arbitrary content
                // to a validly-signed message. Treat a length-limited signature
                // as no signature rather than as trust.
                if output.signature().is_some_and(|s| s.l > 0) {
                    failures.push(format!(
                        "signature from {domain} covers only part of the body (l= tag); refusing to trust it"
                    ));
                    continue;
                }

                if is_aligned(&domain, &from_domain) {
                    return DkimVerdict::PassAligned { domain };
                }
                signed_by.push(domain);
            }
            // Temporary conditions are not evidence about the message.
            DkimResult::TempError(e) => {
                transient = true;
                failures.push(format!("temporary DKIM error: {e}"));
            }
            DkimResult::Fail(e) | DkimResult::PermError(e) => {
                failures.push(match &signature_domain {
                    Some(d) => format!("signature from {d} did not verify: {e}"),
                    None => format!("signature did not verify: {e}"),
                });
            }
            DkimResult::Neutral(e) => failures.push(format!("DKIM neutral: {e}")),
            DkimResult::None => {}
        }
    }

    if let Some(domain) = signed_by.into_iter().next() {
        return DkimVerdict::PassNotAligned {
            signed_by: domain,
            from_domain,
        };
    }

    if failures.is_empty() {
        return DkimVerdict::NoSignature;
    }

    // A transient error anywhere means we cannot claim to have disproved the
    // message — report inability, not forgery.
    if transient {
        DkimVerdict::Unavailable {
            reason: failures.join("; "),
        }
    } else {
        DkimVerdict::Fail {
            reason: failures.join("; "),
        }
    }
}

/// DKIM alignment: does the signing domain speak for the `From:` domain?
///
/// Strict equality, plus the case where the signing domain is a parent of the
/// `From:` domain (`d=example.com` signing `from=mail.example.com`), which is
/// how organizations legitimately sign subdomain mail.
///
/// Deliberately *not* full DMARC relaxed alignment: that compares
/// organizational domains, which requires the Public Suffix List to do safely.
/// Without a PSL, treating any shared suffix as alignment would let a signature
/// from `co.uk` validate mail from `bank.co.uk`. Erring strict costs us some
/// true positives, which fall through to `PassNotAligned` rather than to trust.
fn is_aligned(signing_domain: &str, from_domain: &str) -> bool {
    if signing_domain == from_domain {
        return true;
    }
    // Require a label boundary so "evilexample.com" does not align "example.com".
    from_domain
        .strip_suffix(signing_domain)
        .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alignment_exact_match() {
        assert!(is_aligned("example.com", "example.com"));
    }

    #[test]
    fn test_alignment_accepts_parent_domain_signing_subdomain() {
        // d=example.com legitimately signs mail from mail.example.com
        assert!(is_aligned("example.com", "mail.example.com"));
    }

    #[test]
    fn test_alignment_rejects_suffix_without_label_boundary() {
        // The classic near-miss: evilexample.com must not align example.com.
        assert!(!is_aligned("example.com", "evilexample.com"));
    }

    #[test]
    fn test_alignment_rejects_subdomain_signing_parent() {
        // A signature from a subdomain does not speak for the parent domain.
        assert!(!is_aligned("mail.example.com", "example.com"));
    }

    #[test]
    fn test_alignment_rejects_unrelated_domain() {
        assert!(!is_aligned("attacker.example", "bank.example"));
    }

    #[test]
    fn test_verdict_trust_predicates() {
        let pass = DkimVerdict::PassAligned {
            domain: "example.com".to_string(),
        };
        assert!(pass.is_trustworthy());
        assert!(!pass.is_evidence_of_forgery());

        let unavailable = DkimVerdict::Unavailable {
            reason: "offline".to_string(),
        };
        assert!(!unavailable.is_trustworthy());
        // Being unable to check must never read as evidence against the message.
        assert!(!unavailable.is_evidence_of_forgery());

        let no_sig = DkimVerdict::NoSignature;
        assert!(!no_sig.is_trustworthy());
        assert!(!no_sig.is_evidence_of_forgery());

        let fail = DkimVerdict::Fail {
            reason: "bad body hash".to_string(),
        };
        assert!(!fail.is_trustworthy());
        assert!(fail.is_evidence_of_forgery());
    }

    #[test]
    fn test_has_dkim_signature_detects_header() {
        assert!(has_dkim_signature(
            b"DKIM-Signature: v=1; a=rsa-sha256\r\nFrom: a@b.example\r\n\r\nbody\r\n"
        ));
        // Header names are case-insensitive.
        assert!(has_dkim_signature(
            b"dkim-signature: v=1\r\nFrom: a@b.example\r\n\r\nbody\r\n"
        ));
    }

    #[test]
    fn test_has_dkim_signature_ignores_body_mentions() {
        // A quoted reply or a bounce can carry the literal text in the body.
        // Scanning only the header block keeps us off the slow path.
        assert!(!has_dkim_signature(
            b"From: a@b.example\r\n\r\nthey said DKIM-Signature: v=1; a=rsa-sha256\r\n"
        ));
    }

    #[test]
    fn test_has_dkim_signature_handles_lf_only_separator() {
        // Lenient servers hand back bare LF rather than CRLF.
        assert!(has_dkim_signature(
            b"DKIM-Signature: v=1\nFrom: a@b.example\n\nbody\n"
        ));
        assert!(!has_dkim_signature(
            b"From: a@b.example\n\nDKIM-Signature: v=1\n"
        ));
    }

    #[test]
    fn test_has_dkim_signature_on_headers_only_message() {
        // No blank line at all: the whole buffer is headers.
        assert!(has_dkim_signature(b"DKIM-Signature: v=1\r\nFrom: a@b.example\r\n"));
        assert!(!has_dkim_signature(b"From: a@b.example\r\n"));
        assert!(!has_dkim_signature(b""));
    }

    #[test]
    fn test_interpret_no_outputs_is_no_signature() {
        assert_eq!(interpret(&[], "example.com"), DkimVerdict::NoSignature);
    }

    // ---- End-to-end verification against a real signed message ----
    //
    // The message and keys below are the RFC 8463 §A test vector, carrying both
    // an Ed25519 and an RSA signature over the same body. DNS is seeded from a
    // DummyCaches, so these exercise real canonicalization, real body hashing
    // and real signature verification with no network.

    const RFC8463_ED25519_KEY: &str =
        "v=DKIM1; k=ed25519; p=11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
    const RFC8463_RSA_KEY: &str = "v=DKIM1; k=rsa; p=MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDkHlOQoBTzWRiGs5V6NpP3idY6Wk08a5qhdR6wy5bdOKb2jLQiY/J16JYi0Qvx/byYzCNb3W91y3FutACDfzwQ/BC/e/8uBsCR+yz1Lxj+PL6lHvqMKrM3rG4hstT5QjvHO9PzoxZyVYLzBfO2EeC3Ip3G+2kryOTIKT+l/K4w3QIDAQAB";

    /// RFC 8463 §A test vector: one body, signed twice (Ed25519 and RSA).
    const RFC8463_MESSAGE: &str = concat!(
        "DKIM-Signature: v=1; a=ed25519-sha256; c=relaxed/relaxed;\n",
        " d=football.example.com; i=@football.example.com;\n",
        " q=dns/txt; s=brisbane; t=1528637909; h=from : to :\n",
        " subject : date : message-id : from : subject : date;\n",
        " bh=2jUSOH9NhtVGCQWNr9BrIAPreKQjO6Sn7XIkfJVOzv8=;\n",
        " b=/gCrinpcQOoIfuHNQIbq4pgh9kyIK3AQUdt9OdqQehSwhEIug4D11Bus\n",
        " Fa3bT3FY5OsU7ZbnKELq+eXdp1Q1Dw==\n",
        "DKIM-Signature: v=1; a=rsa-sha256; c=relaxed/relaxed;\n",
        " d=football.example.com; i=@football.example.com;\n",
        " q=dns/txt; s=test; t=1528637909; h=from : to : subject :\n",
        " date : message-id : from : subject : date;\n",
        " bh=2jUSOH9NhtVGCQWNr9BrIAPreKQjO6Sn7XIkfJVOzv8=;\n",
        " b=F45dVWDfMbQDGHJFlXUNB2HKfbCeLRyhDXgFpEL8GwpsRe0IeIixNTe3\n",
        " DhCVlUrSjV4BwcVcOF6+FF3Zo9Rpo1tFOeS9mPYQTnGdaSGsgeefOsk2Jz\n",
        " dA+L10TeYt9BgDfQNZtKdN1WO//KgIqXP7OdEFE4LjFYNcUxZQ4FADY+8=\n",
        "From: Joe SixPack <joe@football.example.com>\n",
        "To: Suzie Q <suzie@shopping.example.net>\n",
        "Subject: Is dinner ready?\n",
        "Date: Fri, 11 Jul 2003 21:00:37 -0700 (PDT)\n",
        "Message-ID: <20030712040037.46341.5F8J@football.example.com>\n",
        "\n",
        "Hi.\n",
        "\n",
        "We lost the game.  Are you hungry yet?\n",
        "\n",
        "Joe.\n",
    );

    use mail_auth::common::parse::TxtRecordParser;
    use mail_auth::common::verify::DomainKey;
    use mail_auth::{Parameters, ResolverCache, Txt};
    use std::borrow::Borrow;
    use std::collections::HashMap;
    use std::hash::Hash;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    /// Minimal in-memory TXT cache so tests resolve DKIM keys without a network.
    ///
    /// mail-auth checks the supplied cache before going to DNS, so seeding this
    /// makes verification fully offline and deterministic. The crate's own
    /// DummyCaches is `#[cfg(test)]`-gated and not reachable from here, but
    /// `ResolverCache` is public, so we bring our own.
    #[derive(Default)]
    struct TxtCache(Mutex<HashMap<Box<str>, Txt>>);

    impl ResolverCache<Box<str>, Txt> for TxtCache {
        fn get<Q>(&self, name: &Q) -> Option<Txt>
        where
            Box<str>: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            self.0.lock().unwrap().get(name).cloned()
        }

        fn remove<Q>(&self, name: &Q) -> Option<Txt>
        where
            Box<str>: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            self.0.lock().unwrap().remove(name)
        }

        fn insert(&self, key: Box<str>, value: Txt, _valid_until: Instant) {
            self.0.lock().unwrap().insert(key, value);
        }
    }

    fn seeded_cache(records: &[(&str, &str)]) -> TxtCache {
        let cache = TxtCache::default();
        for (name, value) in records {
            let key = DomainKey::parse(value.as_bytes()).expect("test key should parse");
            cache
                .0
                .lock()
                .unwrap()
                .insert(name.to_string().into_boxed_str(), Txt::DomainKey(Arc::new(key)));
        }
        cache
    }

    /// Run real DKIM verification against seeded DNS and return our verdict.
    fn verify_offline(raw: &str, from_domain: &str, records: &[(&str, &str)]) -> DkimVerdict {
        let raw = raw.replace('\n', "\r\n");
        let cache = seeded_cache(records);
        let authenticator = MessageAuthenticator::new_system_conf()
            .expect("resolver construction; DNS answers come from the seeded cache");
        let message = AuthenticatedMessage::parse(raw.as_bytes()).expect("vector should parse");

        let outputs = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(
                authenticator.verify_dkim(Parameters::new(&message).with_txt_cache(&cache)),
            );

        interpret(&outputs, from_domain)
    }

    fn rfc8463_records() -> Vec<(&'static str, &'static str)> {
        vec![
            ("brisbane._domainkey.football.example.com.", RFC8463_ED25519_KEY),
            ("test._domainkey.football.example.com.", RFC8463_RSA_KEY),
        ]
    }

    #[test]
    fn test_verifies_real_signature_with_aligned_domain() {
        let verdict = verify_offline(
            RFC8463_MESSAGE,
            "football.example.com",
            &rfc8463_records(),
        );

        assert_eq!(
            verdict,
            DkimVerdict::PassAligned { domain: "football.example.com".to_string() },
            "the RFC 8463 vector must verify and align"
        );
        assert!(verdict.is_trustworthy());
    }

    #[test]
    fn test_tampered_body_does_not_verify() {
        // Flip the body. The signature and body hash are now wrong, which is
        // exactly what DKIM exists to catch — and what reading a provider's
        // Authentication-Results header can never catch on its own.
        let tampered = RFC8463_MESSAGE.replace(
            "We lost the game.  Are you hungry yet?",
            "We won the game.  Send me your bank details.",
        );
        assert_ne!(tampered, RFC8463_MESSAGE, "the replacement must actually apply");

        let verdict = verify_offline(&tampered, "football.example.com", &rfc8463_records());

        assert!(!verdict.is_trustworthy(),
            "a modified body must not verify; got {verdict:?}");
        assert!(verdict.is_evidence_of_forgery(),
            "a broken body hash is real evidence, not an inability to check; got {verdict:?}");
    }

    #[test]
    fn test_valid_signature_from_unaligned_domain_is_not_trusted() {
        // The signature verifies, but the From: domain is someone else's. This
        // is the spoofing case: anyone can validly sign their own mail while
        // claiming a From: they do not control.
        let verdict = verify_offline(
            RFC8463_MESSAGE,
            "bank.example",
            &rfc8463_records(),
        );

        assert_eq!(
            verdict,
            DkimVerdict::PassNotAligned {
                signed_by: "football.example.com".to_string(),
                from_domain: "bank.example".to_string(),
            },
            "a verified signature from a non-aligned domain must not confer trust"
        );
        assert!(!verdict.is_trustworthy());
    }

    #[test]
    fn test_nonexistent_key_is_treated_as_failure() {
        // NXDOMAIN for the selector is a *definitive* answer: the key the
        // signature names does not exist, so the signature cannot be validated.
        // RFC 6376 §6.1.2 treats that as PERMFAIL, and so do we — this is
        // deliberately different from being unable to reach DNS at all.
        let verdict = verify_offline(RFC8463_MESSAGE, "football.example.com", &[]);

        assert!(!verdict.is_trustworthy(), "got {verdict:?}");
        assert!(
            matches!(verdict, DkimVerdict::Fail { .. }),
            "a signature naming a key that does not exist is a failure; got {verdict:?}"
        );
    }

    #[test]
    fn test_dns_error_is_unavailable_not_failure() {
        // The case that must never drop mail: the resolver itself errored, so
        // we learned nothing about the message. A user on a train must not have
        // their inbox filtered as though every sender were forging mail.
        //
        // Relies on mail-auth's `test` feature, where a domain containing
        // `_dns_error.` makes the resolver return a transient DNS error instead
        // of querying the network — so this stays hermetic in CI.
        let message = RFC8463_MESSAGE
            .replace("d=football.example.com;", "d=_dns_error.example.com;")
            .replace("From: Joe SixPack <joe@football.example.com>",
                     "From: Joe SixPack <joe@_dns_error.example.com>");

        let verdict = verify_offline(&message, "_dns_error.example.com", &[]);

        assert!(!verdict.is_trustworthy(), "got {verdict:?}");
        assert!(
            matches!(verdict, DkimVerdict::Unavailable { .. }),
            "a resolver error is an inability to check, not proof of forgery; got {verdict:?}"
        );
        assert!(
            !verdict.is_evidence_of_forgery(),
            "callers drop mail on evidence of forgery; a DNS outage must not qualify"
        );
    }

    #[test]
    fn test_message_without_signature_reports_no_signature() {
        let unsigned = concat!(
            "From: Joe <joe@football.example.com>\n",
            "Subject: no dkim here\n",
            "\n",
            "body\n",
        );

        let verdict = verify_offline(unsigned, "football.example.com", &rfc8463_records());

        assert_eq!(verdict, DkimVerdict::NoSignature);
        assert!(!verdict.is_evidence_of_forgery(),
            "absence of a signature is not evidence against the message");
    }
}
