# nostr-mail-crypto

WebAssembly crypto library for [nostr-mail](https://github.com/asherp/nostr-mail). Provides client-side NIP-04/NIP-44 encryption, Schnorr signatures, key management, and AES-256-GCM settings encryption -- all running in the browser with zero server trust.

## Building

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/):

```bash
wasm-pack build --target web
```

This produces a `pkg/` directory containing the `.wasm` binary and JS/TS bindings.

## Usage (JavaScript / TypeScript)

```js
import init, {
  init as initCrypto,
  generate_keypair,
  encrypt_message,
  decrypt_message,
  detect_encryption_format,
  get_public_key_from_private,
  validate_private_key,
  validate_public_key,
  sign_data,
  verify_signature,
  encrypt_setting_value,
  decrypt_setting_value,
} from "./pkg/nostr_mail_crypto.js";

// Initialize WASM module and panic hook
await init();
initCrypto();

// Generate a keypair
const kp = generate_keypair();
console.log(kp.private_key); // nsec1...
console.log(kp.public_key);  // npub1...

// Encrypt (NIP-44 by default)
const encrypted = encrypt_message(sender_nsec, recipient_npub, "Hello!", null);

// Decrypt (auto-detects NIP-04 vs NIP-44)
const decrypted = decrypt_message(recipient_nsec, sender_npub, encrypted);

// Detect format
detect_encryption_format(encrypted); // "nip04" | "nip44" | "unknown"

// Schnorr sign/verify
const sig = sign_data(nsec, "data to sign");
const valid = verify_signature(npub, sig, "data to sign"); // true

// Settings encryption (AES-256-GCM, key derived from nsec)
const enc = encrypt_setting_value(nsec, "smtp_password=hunter2");
const dec = decrypt_setting_value(nsec, enc);
```

## API

| Function | Returns | Description |
|---|---|---|
| `init()` | `void` | Set up panic hook for debugging |
| `generate_keypair()` | `WasmKeyPair` | Generate secp256k1 keypair (nsec/npub) |
| `get_public_key_from_private(nsec)` | `string` | Derive npub from nsec |
| `validate_private_key(nsec)` | `boolean` | Check if nsec is valid |
| `validate_public_key(npub)` | `boolean` | Check if npub is valid |
| `encrypt_message(nsec, npub, msg, algo?)` | `string` | NIP-04 or NIP-44 encryption |
| `decrypt_message(nsec, npub, ciphertext)` | `string` | Auto-detect and decrypt |
| `detect_encryption_format(content)` | `string` | `"nip04"`, `"nip44"`, or `"unknown"` |
| `sign_data(nsec, data)` | `string` | Schnorr signature (hex) |
| `verify_signature(npub, sig, data)` | `boolean` | Verify Schnorr signature |
| `encrypt_setting_value(nsec, value)` | `string` | AES-256-GCM encrypt |
| `decrypt_setting_value(nsec, encrypted)` | `string` | AES-256-GCM decrypt |

## Testing

```bash
# Browser tests (requires Chrome or Firefox)
wasm-pack test --headless --chrome

# Native tests (no browser needed)
cargo test
```

## License

Same license as the parent nostr-mail project.
