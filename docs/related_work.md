Here's other directions we've looked at for potential use in NostrMail.

## NIP-05

!!! note
    This is not required by NostrMail. This section is just kept for pedagogical purposes.


[nip-05](https://github.com/nostr-protocol/nips/blob/master/05.md) is a Pub key validation standard based on control over a domain. Most email users will not have their own servers, however, so NostrMail clients should not require it.

HelloJessica is nip05 compliant, so we should be able to a get request to his server to verify his pub key

```python
from nostrmail.utils import validate_nip05
```

```python
validate_nip05(node_hello_hex) # returns the name of this user according to their .com
```

Hello Jessica's NIP-05 json actually includes several other names, so this doubles as a PGP registry. However, `nip-02` [provides a similar solution](https://github.com/nostr-protocol/nips/blob/master/02.md) to registration.


## NIP-02

[NIP-02](https://github.com/nostr-protocol/nips/blob/master/02.md) Supports contacts lists, comprised of pub keys, petnames, and preferred relays. Users may be found by walking through the network of contacts. This is desirable for nostrmail, where we want to easily look up an email address through dot notation. For instance, an email to `carol.bob.alice` means `find carol in my contacts`, then `find bob in carol's contacts`, then `find alice in bob's contacts`.


## Fernet encryption

!!! note
    NostrMail does not use this method. We decided to use the same scheme as NOSTr DMs to reduce the workload on other implementations.

We could have used [Fernet encryption](https://cryptography.io/en/latest/fernet/#fernet-symmetric-encryption) available from the cryptography package. Fernet encryption is a form of symmetric encryption, meaning the same key may be used to encrypt and decrypt a message.

```python
from cryptography.fernet import Fernet, InvalidToken
import base64

def get_fernet(key):
    if isinstance(key, str):
        fernet_key = base64.urlsafe_b64encode(bytes(key.ljust(32).encode()))
    else:
        fernet_key = base64.urlsafe_b64encode(key)
    return Fernet(fernet_key)


def encrypt(message, key):
    f = get_fernet(key)
    token = f.encrypt(message.encode())

    encrypted_msg = token.decode('ascii')

    return encrypted_msg

def decrypt(message, key):
    f = get_fernet(key)
    decrypted_msg = f.decrypt(message.encode()).decode('ascii')

    return decrypted_msg
```

```python
decrypt(encrypt('hello world', 'yowzah'), 'yowzah')
```

While apparently simpler, there are a few drawbacks that make this untennable:

* There's no `iv` token as with AES, so you have to directly match the dm with the subject when looking up emails
* All NostrMail clients would have to implement fernet encryption in addition to AES for dms


## TOTP

We may use a different key for each message by concatonating the shared secret with a time stamp and hashing the result. This is known as a [time-based on-time password](https://en.wikipedia.org/wiki/Time-based_one-time_password) (TOTP) and should already be familiar to anyone who has used [google authenticator](https://googleauthenticator.net/). The time used would be the time the email was sent. The epoch needs to be large enough for the mail servers to route the message.

It might also help to use the latest block hash as the time stamp.

This approach may provide some additional security benefit, such as mitigating replay attacks or preventing emails from being sent from the future or something.

```python
from cryptography.hazmat.primitives import hashes
```

```python
def sha256(message):
    digest = hashes.Hash(hashes.SHA256())
    digest.update(message.encode())
    digest.update(b"123")
    return digest.finalize()
```

```python
import base64
base64.urlsafe_b64encode(sha256('hey')).decode('ascii')
```

```python
sha256('hey')
```

```python
def hash_concat(key, value):
    """concatonates a message with a value and returns the hash
    key - a binary
    """
    key_str = base64.urlsafe_b64encode(key).decode('ascii')
    return sha256(key_str + str(value))
```

Using the most recent bitcoin block

```python
latest_block_hash = '000000000000000000065a582c53ef20e5ae37b74844b31bfcbd82f4c515fdb2'
```

```python
epoch_value = latest_block_hash
assert sender_secret == receiver_secret

print(decrypt(encrypt(email_msg,
                      hash_concat(sender_secret, latest_block_hash)),
              hash_concat(receiver_secret, epoch_value)) # 
    )
```


