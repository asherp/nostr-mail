# Glossia: Add `seal_nostr` ASCII Armor Dialect

## Problem

When glossia encoding is set to Hex (empty meta), the signature block gets proper ASCII armor via the existing `sig_nostr` dialect:

```
-----BEGIN NOSTR SIGNATURE-----
c8ca1f5e59bc5e915a8044beff139359...
-----END NOSTR SIGNATURE-----
```

But the pubkey/seal block bypasses glossia entirely and outputs a bare npub:

```
npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
```

This is needed because nostr-mail is moving to a dual-format email structure:
- **text/plain**: machine-readable ASCII-armored blocks (searchable by Gmail IMAP)
- **text/html**: human-readable glossia-encoded prose (Latin, BIP39, etc.)

The plaintext needs consistent ASCII armor for all blocks so Gmail's `X-GM-RAW` search can find nostr-mail emails by searching for markers like `BEGIN NOSTR SEAL`.

## Goal

Add a `seal_nostr` dialect to the CS grammar that produces:

```
-----BEGIN NOSTR SEAL-----
npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
-----END NOSTR SEAL-----
```

The payload is bech32-encoded (the npub itself), not hex.

## What to change

### 1. CS cover words — `languages/cs/cover.yaml`

Add a `SEAL` cover word with a new Modal refinement:

```yaml
SEAL:
  Modal: 1.0
  refinement: seal
```

This parallels how `SIGNATURE` uses `Modal` with refinement `sig`.

### 2. CS grammar — `languages/cs/grammar.yaml`

Add a `seal_nostr` dialect after `sig_nostr` (~line 290). It follows the same pattern as `sig_nostr` but uses `Modal[seal]` instead of `Modal[sig]`:

```yaml
    seal_nostr:
      # Nostr seal armor (bech32 npub).
      # Output: ----- BEGIN NOSTR SEAL -----\n<payload>\n----- END NOSTR SEAL -----
      payload_wordlist: "bech32"
      cover_wordlist: "default"
      payload_line_width: null
      rules:
        HEADER:
          lambda: "λn:(e->t). n"
          cfg_productions:
            - production: "Dot Aux[begin] Prefix[nostr] Modal[seal] Dot Conj"
              weight: 1.0
              lambda: "λn:(e->t). n"

        FOOTER:
          lambda: "λn:(e->t). n"
          cfg_productions:
            - production: "Conj Dot Aux[end] Prefix[nostr] Modal[seal] Dot"
              weight: 1.0
              lambda: "λn:(e->t). n"
```

Key differences from `sig_nostr`:
- `payload_wordlist: "bech32"` (not `base16`) — the payload is bech32 characters
- `payload_line_width: null` — npubs are short enough (~59 chars) to not need line wrapping
- `Modal[seal]` instead of `Modal[sig]`

### 3. Pipeline — `src/pipeline.rs`

**a)** `meta_payload` set (~line 346): add `"seal"` to the list of recognized meta words:

```rust
"bytes", "nostr", "pgp", "prose", "body", "subject", "spells", "sig", "seal",
```

**b)** `meta_word_to_dialect()` (~line 296): add the `seal` → `seal_nostr` mapping:

```rust
"sig" => Some("sig_nostr"),
"seal" => Some("seal_nostr"),
```

### 4. WASM display name — `src/wasm.rs`

Add the display name mapping (~line 480, near the `sig_nostr` entry):

```rust
("cs", "seal_nostr", "bech32") => "Nostr Seal".to_string(),
```

### 5. Grammar tests — `src/grammar.rs`

Add tests following the pattern of the existing `sig_nostr` tests (~line 1814):

```rust
#[test]
fn test_cs_seal_nostr_grammar_loads() {
    let grammar = Grammar::from_language_dialect("cs", "seal_nostr")
        .expect("Failed to load CS seal_nostr grammar");
    assert!(grammar.grammar_uses_pos(Pos::Prefix), "Seal Nostr should use Prefix (NOSTR)");
    assert!(grammar.grammar_uses_pos(Pos::Modal), "Seal Nostr should use Modal (SEAL)");
    assert!(!grammar.grammar_uses_pos(Pos::Cop), "Seal Nostr should not use Cop (ENCRYPTED)");
    assert!(!grammar.grammar_uses_pos(Pos::To), "Seal Nostr should not use To (MESSAGE)");
}

#[test]
fn test_cs_seal_nostr_produces_sequences() {
    let grammar = Grammar::from_language_dialect("cs", "seal_nostr")
        .expect("Failed to load CS seal_nostr grammar");
    let seqs = grammar.generate_sequences(None);
    assert_eq!(seqs.len(), 13,
        "Nostr seal should produce k=13 sequences (6 header + 1 body + 6 footer)");
}
```

## Input/Output

Once implemented, nostr-mail will call:

```
transcode(npubBech32Data, "encode into seal nostr")
```

Where `npubBech32Data` is the bech32 character data **after the `npub1` prefix** (the prefix is a cover word, not payload). This follows the same pattern as `crypto/nostr` where `npub1` is a `Prefix` cover word.

**Wait** — there's a subtlety here. The `seal_nostr` dialect lives in the `cs` language, but the input is bech32 characters (not hex). The `crypto/nostr` grammar already handles stripping/restoring the `npub1` prefix. The question is whether `seal_nostr` should:

**(A)** Accept the full `npub1...` string and treat the bech32 data chars as payload (the `npub1` prefix would NOT appear in output — the armor replaces it):

```
-----BEGIN NOSTR SEAL-----
7umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
-----END NOSTR SEAL-----
```

**(B)** Accept the full `npub1...` string as-is and embed it literally inside the armor:

```
-----BEGIN NOSTR SEAL-----
npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
-----END NOSTR SEAL-----
```

**Option B is better** for human readability and because the `npub1` prefix is needed for decoding. The simplest implementation: treat the full npub as raw payload characters (add `n`, `p`, `u`, `b`, `1` to the payload alphabet or use a raw/passthrough mode). Alternatively, the CS grammar could use a dedicated payload wordlist that includes the full bech32 charset plus the prefix characters.

Please decide on (A) vs (B) and implement accordingly. Option B is more useful since the decoded output is a complete, copy-pasteable npub.
