# NIP-44 Encryption

Nostr-mail uses **NIP-44** as the default encryption standard for all encrypted communications.

## What is NIP-44?

NIP-44 is a modern encryption standard for Nostr that provides:
- **Better Security**: Improved cryptographic properties compared to NIP-04
- **Versioning**: Supports multiple encryption versions for future improvements
- **Standardization**: Widely adopted across Nostr clients

## NIP-44 vs NIP-04

| Feature | NIP-44 | NIP-04 |
|---------|--------|--------|
| Security | Modern, secure | Legacy, known issues |
| Versioning | Supported | Not supported |
| Adoption | Growing | Legacy |
| Recommendation | ✓ Use | ✗ Avoid |

## Encryption Details

- **Algorithm**: NIP-44 encryption using shared secret derived from keypairs
- **Default**: NIP-44 is the default for all new installations
- **Backward Compatibility**: NIP-04 is supported for reading legacy messages
- **Attachments**: Hybrid encryption (AES-256 for files, NIP-44 for keys)

## How It Works

1. **Shared Secret**: Derived from sender's private key and receiver's public key using ECDH
2. **Encryption**: Message encrypted using NIP-44 standard
3. **Decryption**: Receiver uses their private key and sender's public key to derive the same shared secret
4. **Versioning**: NIP-44 supports version numbers for future algorithm improvements

## Security Notes

- NIP-44 provides better security than NIP-04
- All new messages use NIP-44 by default
- Legacy NIP-04 messages can still be decrypted
- Consider migrating old conversations to NIP-44
- Private keys never leave the local device
- Encryption happens in the Rust backend

## Configuration

You can select the encryption algorithm in Settings → Nostr Settings → Encryption Algorithm:
- **NIP-44 (Recommended)**: Modern, secure encryption standard
- **NIP-04 (Legacy)**: Older encryption standard with known issues

## Migration from NIP-04

If you have existing NIP-04 encrypted messages:
1. They will continue to work (backward compatibility)
2. New messages will use NIP-44 by default
3. Consider re-encrypting important conversations with NIP-44

## Technical Details

For more information, see the [NIP-44 specification](https://github.com/nostr-protocol/nips/blob/master/44.md).

### Implementation

- **Backend**: Rust implementation using `nostr-sdk` crate
- **Default Version**: Uses default NIP-44 version
- **Key Derivation**: ECDH shared secret derivation
- **Message Format**: Base64 encoded encrypted content

### Hybrid Encryption for Attachments

Large file attachments use hybrid encryption:
1. Generate random AES-256 key
2. Encrypt file with AES-256
3. Encrypt AES-256 key with NIP-44
4. Send encrypted file + encrypted key

This provides:
- Efficient encryption for large files
- Strong security via NIP-44 for keys
- Standard encryption for file data
