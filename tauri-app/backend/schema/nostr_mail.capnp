@0xb7c5e3a1d4f29e80;

# Nostr-Mail Cap'n Proto Schema
# Covers the encrypted manifest, armor block structure, and email envelope.

# ── Encryption ────────────────────────────────────────────────────

# Which Nostr encryption protocol was used.
#
#   nip04 — AES-256-CBC, ciphertext format: base64?iv=base64
#   nip44 — XChaCha20-Poly1305, ciphertext format: version(1) + nonce(32) + padded_plaintext + MAC(32) → base64
enum NipVersion {
  nip04 @0;
  nip44 @1;
}

# ── Manifest ──────────────────────────────────────────────────────
# The manifest is used when a message body is >= 64 KiB or has
# attachments. It replaces the simple "NIP-encrypt the body" path
# with a two-layer scheme:
#
#   1. Body and each attachment are AES-256-GCM encrypted with
#      independent symmetric keys.
#   2. The manifest (this struct) is serialized, then NIP-encrypted
#      as a whole and placed inside the ASCII armor block.
#
# Because the manifest itself is NIP-encrypted, the AES keys inside
# it are effectively double-wrapped — readable only by the holder
# of the recipient's Nostr private key.
#
# Wire format (inside the email body's text/plain):
#
#   ----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----
#   <NIP-44 ciphertext of serialized Manifest>
#   ----- BEGIN NOSTR SIGNATURE -----
#   ...
#   ----- END NOSTR MESSAGE -----
#
# Attachments travel as separate MIME parts with opaque filenames
# (a1.dat, a2.dat, …) and application/octet-stream content type.
#
# ── Binary vs JSON detection ──
#
# After NIP decryption, the manifest payload is either:
#
#   JSON (legacy)  — first byte is 0x7B ('{')
#   Cap'n Proto    — first 4 bytes are 0x00000000 (single-segment table)
#
# The decoder checks the first byte to select the deserialization path.
# New messages SHOULD use Cap'n Proto; JSON is retained for reading
# older emails.

struct Manifest {
  body         @0 :EncryptedBlob;
  attachments  @1 :List(Attachment);
  version      @2 :UInt16;           # schema version (default 0 = initial)
}

# A single AES-256-GCM encrypted payload with its integrity hash
# and the symmetric key needed to decrypt it.
#
# Example (body encryption):
#
#   plaintext body → UTF-8 → base64 → encryptWithAES(base64, key)
#                                        ↓
#                                    ciphertext  (stored here)
#                                    SHA-256     (cipherSha256)
#                                    AES key     (keyWrap, plaintext)

struct EncryptedBlob {
  ciphertext   @0 :Data;    # AES-256-GCM encrypted payload (nonce prepended: 12-byte IV || ciphertext)
  cipherSha256 @1 :Data;    # 32-byte SHA-256 of ciphertext (integrity check before decryption)
  keyWrap      @2 :Data;    # raw AES-256 key bytes (safe because the Manifest is NIP-encrypted)
  cipherSize   @3 :UInt64;  # size of ciphertext in bytes (redundant with Data length, useful for streaming)
}

# Metadata for one encrypted attachment.
#
# The actual encrypted file bytes are NOT in the manifest — they
# travel as a separate MIME attachment named "<id>.dat". The
# manifest just carries the metadata and AES key needed to find
# and decrypt them.
#
# Attachments are padded to 64 KiB boundaries to avoid leaking
# the original file size.
#
# Example manifest entry for a PDF:
#
#   id:            "a1"
#   origFilename:  "invoice.pdf"
#   origMime:      "application/pdf"
#   cipherSha256:  <32 bytes>
#   cipherSize:    131072        (original 98 KB, padded to 128 KB)
#   keyWrap:       <32 bytes>    (AES-256 key)

struct Attachment {
  id           @0 :Text;    # opaque identifier, e.g. "a1"
  origFilename @1 :Text;    # original filename (hidden from MIME headers)
  origMime     @2 :Text;    # original MIME type (hidden from MIME headers)
  cipherSha256 @3 :Data;    # 32-byte SHA-256 of the encrypted file
  cipherSize   @4 :UInt64;  # size of the encrypted file in bytes
  keyWrap      @5 :Data;    # AES-256 key for this attachment
}

# ── Armor Blocks ──────────────────────────────────────────────────
# Parsed representation of a nostr-mail message extracted from the
# text/plain body of an email. The ASCII armor delimiters are the
# wire format; this struct is for processing after parsing.
#
# A message has a body (encrypted, signed, or plain), an optional
# signature or seal block. Quoted/reply messages are nested inside
# the body, because in the wire format the inner message appears
# within the outer body's armor region:
#
#   ----- BEGIN NOSTR NIP-04 ENCRYPTED BODY -----    ← outer body starts
#   <reply ciphertext>
#   ----- BEGIN NOSTR NIP-44 ENCRYPTED BODY -----    ← quoted, inside outer body
#   <original ciphertext>
#   ----- BEGIN NOSTR SIGNATURE -----
#   @OriginalAuthor
#   <sig + pubkey>
#   ----- END NOSTR MESSAGE -----                    ← closes inner
#   ----- BEGIN NOSTR SIGNATURE -----                ← outer sig, outside body
#   @ReplyAuthor
#   <sig + pubkey>
#   ----- END NOSTR MESSAGE -----                    ← closes outer
#
# Parsed as:
#
#   ArmorMessage {
#     body: encrypted {
#       nip: nip04,
#       ciphertext: <reply bytes>,
#       quoted: ArmorMessage {
#         body: encrypted {
#           nip: nip44,
#           ciphertext: <original bytes>,
#           quoted: null
#         },
#         signature: @OriginalAuthor
#       }
#     },
#     signature: @ReplyAuthor
#   }

struct ArmorMessage {
  body         @0 :Body;
  signature    @1 :SignatureBlock;  # present when signed
  seal         @2 :SealBlock;      # present when unsigned + has identity
}

# The body content of an armor block. Exactly one variant is set.
# The quoted field is present on all variants — in reply chains,
# the inner message is nested within the outer body's armor region
# (between the body content and the outer signature/seal).
#
#   encrypted — ciphertext from a BEGIN NOSTR NIP-XX ENCRYPTED BODY block
#   signed    — glossia-encoded plaintext from a BEGIN NOSTR SIGNED BODY block;
#               the raw plaintext also appears above the armor for non-nostr-mail clients,
#               but the glossia content is the canonical signed payload
#   plain     — unarmored body text (no cryptographic wrapping)

struct Body {
  quoted         @4 :ArmorMessage;   # nested original message (replies), inside this body's armor region
  encodedContent @5 :Text;           # raw armor body text as it appears in the email (glossia words, base64, or plaintext)
  union {
    encrypted :group {
      nip        @0 :NipVersion;
      ciphertext @1 :Data;         # raw NIP ciphertext bytes (decoded from glossia/base64)
    }
    signed :group {
      plaintext  @2 :Data;         # decoded plaintext bytes (from glossia encoding)
    }
    plain :group {
      text       @3 :Text;         # unarmored body
    }
  }
}

# Proof of authorship + identity. Appears inside a
# ----- BEGIN NOSTR SIGNATURE ----- block.
#
# The signature is a Schnorr signature (secp256k1) over
# SHA-256(decoded_body_bytes), where decoded_body_bytes are the
# canonical binary obtained by glossia-decoding or base64-decoding
# the armor block content.
#
# Example armor block:
#
#   ----- BEGIN NOSTR SIGNATURE -----
#   @Alice
#   <signature: glossia-encoded or hex — decoded to 64 bytes>
#   <pubkey: glossia-encoded, hex, or npub (bech32) — decoded to 32 bytes>
#   ----- END NOSTR MESSAGE -----
#
# Decoders MUST also accept the legacy combined format where sig + pubkey
# are encoded together as a single 96-byte glossia or hex payload.

struct SignatureBlock {
  profileName      @0 :Text;   # display name from the @ProfileName line
  signature        @1 :Data;   # 64-byte Schnorr signature
  pubkey           @2 :Data;   # 32-byte x-only secp256k1 public key
  encodedSigPubkey @3 :Text;   # raw encoded sig+pubkey content before decode (for display/fallback)
}

# Identity declaration without a signature. Appears inside a
# ----- BEGIN NOSTR SEAL ----- block for unsigned messages.
#
# Provides the sender's pubkey, which is needed for NIP-04/NIP-44
# decryption and survives email forwarding (unlike MIME headers).
#
# Example armor block:
#
#   ----- BEGIN NOSTR SEAL -----
#   @Bob
#   <pubkey: glossia-encoded, hex, or npub (bech32) — decoded to 32 bytes>
#   ----- END NOSTR SEAL -----

struct SealBlock {
  displayName  @0 :Text;   # display name from the @DisplayName line
  pubkey       @1 :Data;   # 32-byte x-only secp256k1 public key
}

# ── Email Envelope ────────────────────────────────────────────────
# Full email message as stored/processed by the app. Mirrors the
# Rust EmailMessage type. The body field contains the raw text/plain
# content (armor blocks and all); parsing into ArmorMessage happens
# at a higher layer.
#
# Trust model:
#   - from/to: SMTP headers, spoofable, zero trust
#   - senderPubkey: from X-Nostr-Pubkey header or in-body SEAL/SIGNATURE block
#   - signatureValid: true only if an in-body SIGNATURE block verified
#   - transportAuth: DMARC/DKIM verification of the sending domain

struct Email {
  id              @0  :Text;
  from            @1  :Text;
  to              @2  :Text;
  subject         @3  :Text;
  body            @4  :Text;       # raw text/plain (contains armor blocks)
  htmlBody        @5  :Text;       # rendering aid only, not source of truth
  date            @6  :Int64;      # unix timestamp (seconds)
  isRead          @7  :Bool;
  rawHeaders      @8  :Text;       # full MIME headers for debugging/fallback
  senderPubkey    @9  :Text;       # hex or npub from X-Nostr-Pubkey or SIGNATURE block
  recipientPubkey @10 :Text;       # hex or npub of intended recipient
  messageId       @11 :Text;       # MIME Message-ID header
  signatureValid  @12 :Bool;       # true if Schnorr signature verified
  hasSignature    @13 :Bool;       # distinguishes "verified false" from "no signature"
  transportAuth   @14 :TransportAuth;
  attachments     @15 :List(EmailAttachment);
  signatureSource @16 :Text;       # "body", "header", "both", or absent — which trust path verified
}

# Result of DMARC/DKIM verification on the email's sending domain.
# This is transport-level authentication — it verifies the domain,
# not the Nostr identity.

struct TransportAuth {
  verified @0 :Bool;
  method   @1 :TransportAuthMethod;
  reason   @2 :Text;               # human-readable explanation
}

# Which transport authentication method was used (if any).

enum TransportAuthMethod {
  dmarc @0;
  dkim  @1;
  none  @2;
}

# A single email attachment. For encrypted messages using the
# manifest path, the filename is opaque ("a1.dat"), contentType
# is "application/octet-stream", and the original metadata is
# stored in the Manifest's Attachment entries (not here).

struct EmailAttachment {
  filename         @0 :Text;
  contentType      @1 :Text;
  data             @2 :Data;       # raw bytes (not base64-encoded)
  size             @3 :UInt64;
  isEncrypted      @4 :Bool;
  encryptionMethod @5 :Text;       # "manifest_aes", etc.
  originalFilename @6 :Text;       # restored from manifest after decryption
  originalType     @7 :Text;       # restored from manifest after decryption
  originalSize     @8 :UInt64;     # pre-padding size
}
