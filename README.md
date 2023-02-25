
## Nostr-mail

A simple email encryption tool based on secp256 key pairs.

How it works:

Nostr-mail encrypts content using a symetric key derived from a combination of the sender's private key and the receiver's public key.

Both sender and receiver derive a shared secret known only to them, which is used to protect their communications.

This application can use any email server for delivery.




### Requirements

* secp256k1 https://pypi.org/project/secp256k1/
* nostr https://pypi.org/project/nostr/


### secp256k1

This library is maintained by rustyrussell https://github.com/rustyrussell/secp256k1-py 
Check that we can import

```python
from secp256k1 import PrivateKey, PublicKey
```

```python
privkey = PrivateKey()
privkey_der = privkey.serialize()
assert privkey.deserialize(privkey_der) == privkey.private_key
```

```python
privkey_der
```

### priv/pub key generation

```python
from nostr.key import PrivateKey

private_key = PrivateKey()
public_key = private_key.public_key
print(f"Private key: {private_key.bech32()}")
print(f"Public key: {public_key.bech32()}")
```

```python
import json
import ssl
import time
from nostr.relay_manager import RelayManager

relay_manager = RelayManager()
relay_manager.add_relay("wss://nostr-pub.wellorder.net")
relay_manager.add_relay("wss://relay.damus.io")
relay_manager.add_relay("wss://relay.oldcity-bitcoiners.info")
relay_manager.open_connections({"cert_reqs": ssl.CERT_NONE}) # NOTE: This disables ssl certificate verification
time.sleep(1.25) # allow the connections to open

while relay_manager.message_pool.has_notices():
    notice_msg = relay_manager.message_pool.get_notice()
    print(notice_msg.content)

relay_manager.close_connections()
```

```python
node_hello = 'npub1k9tkawv6ga6ptz3jl30pjzh68hk5mgvl28al5zc6r0myy849wvaq38a70g'
node_hello_hex = 'b1576eb99a4774158a32fc5e190afa3ded4da19f51fbfa0b1a1bf6421ea5733a'
```

```python
import json
import ssl
import time
from nostr.filter import Filter, Filters
from nostr.event import Event, EventKind
from nostr.relay_manager import RelayManager
from nostr.message_type import ClientMessageType
```

## Text events
From [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) there are three kinds of events

* 0: set_metadata: the content is set to a stringified JSON object {name: <username>, about: <string>, picture: <url, string>} describing the user who created the event. A relay may delete past set_metadata events once it gets a new one for the same pubkey.
* 1: text_note: the content is set to the plaintext content of a note (anything the user wants to say). Markdown links ([]() stuff) are not plaintext.
* 2: recommend_server: the content is set to the URL (e.g., wss://somerelay.com) of a relay the event creator wants to recommend to its followers.

```python
def get_events(author_hex, kind='text'):
    events = []
    if kind == 'text':
        kinds = [EventKind.TEXT_NOTE]
    elif kind == 'meta':
        kinds = [EventKind.SET_METADATA]
    else:
        raise NotImplementedError(f'{kind} events not supported')
    filters = Filters([Filter(authors=[author_hex], kinds=kinds)])
    subscription_id = "some random str"
    request = [ClientMessageType.REQUEST, subscription_id]
    request.extend(filters.to_json_array())

    relay_manager = RelayManager()
    relay_manager.add_relay("wss://nostr-pub.wellorder.net")
    relay_manager.add_relay("wss://relay.damus.io")
    relay_manager.add_subscription(subscription_id, filters)
    relay_manager.open_connections({"cert_reqs": ssl.CERT_NONE}) # NOTE: This disables ssl certificate verification
    time.sleep(1.25) # allow the connections to open

    message = json.dumps(request)
    relay_manager.publish_message(message)
    time.sleep(1) # allow the messages to send

    while relay_manager.message_pool.has_events():
        event_msg = relay_manager.message_pool.get_event()
        if kind == 'meta':
            content = json.loads(event_msg.event.content)
        else:
            content = event_msg.event.content
        events.append(content)

    relay_manager.close_connections()
    return events
```

```python
text_hello = get_events(node_hello_hex, 'text')
text_hello
```

```python
meta_hello = get_events(node_hello_hex, 'meta')
meta_hello
```

```python
import pandas as pd
```

```python
pd.DataFrame(meta_hello)
```

hello is nip05 compliant, so we should be able to a get request to his server to verify his pub key

```python
import requests
```

### Nip05 validation

```python
from json import JSONDecodeError

def validate_nip05(hex_name):
    meta = get_events(hex_name, 'meta')
    nip05 = meta[0].get('nip05')
    if nip05 is None:
        return False
    # construct get request
    if '@' in nip05:
        username, tld = nip05.split('@')
    else:
        return False
    
    url = f'https://{tld}/.well-known/nostr.json?name={username}'
    result = requests.get(url)
    try:
        nip05_data = json.loads(result.content.decode('utf-8'))
    except JSONDecodeError:
        raise NameError('Cannot decode nip05 json')
    if 'names' in nip05_data:
        names = nip05_data['names']
        # reverse lookup
        pubs = {pub_key: name for name, pub_key in names.items()}
        if hex_name in pubs:
            return pubs[hex_name]
        else:
            raise NameError(f'{hex_name} not among registered pub keys: {pubs}')
    else:
        raise NameError('nip05 data does not contain names')
    
    return result
```

```python
JSONDecodeError?
```

```python
validate_nip05(node_hello_hex) # returns the name of this user according to their .com
```

```python
validate_nip05(hodl_hex)
```

Hello Jessica's nip05 contains several other names, so this acts as a p2p registry!

```python
hrh_hex = 'ae4efa502bf1cb7dd63343ae4bc7bd0a599c5251c686c9ebd3b5f0d4f841a939'
```

```python
try:
    validate_nip05(hrh_hex) # this one doesn't validate
except NameError as m:
    print(m)
```

## Shared secret


First we'll create two key pairs, one for the sender and one for the receiver

```python
from nostr.key import PrivateKey

priv_key1 = PrivateKey()
pub_key1 = priv_key1.public_key
print(f"Private key: {priv_key1.bech32()}")
print(f"Public key: {pub_key1.bech32()}")
```

```python
priv_key2 = PrivateKey()
pub_key2 = priv_key2.public_key
print(f"Private key: {priv_key1.bech32()}")
print(f"Public key: {pub_key1.bech32()}")
```

```python
assert priv_key1.compute_shared_secret(pub_key2.hex()) == priv_key2.compute_shared_secret(pub_key1.hex())

print('shared secret validated!')
```

## Encryption


We use [Fernet encryption](https://cryptography.io/en/latest/fernet/#fernet-symmetric-encryption) available from the cryptography package. Fernet encryption is a form of symmetric encryption, meaning the same key may be used to encrypt and decrypt a message.

```python
from cryptography.fernet import Fernet, InvalidToken
import base64

def get_fernet(key_str):
    if isinstance(key_str, str):
        fernet_key = base64.urlsafe_b64encode(bytes(key_str.ljust(32).encode()))
    else:
        fernet_key = base64.urlsafe_b64encode(key_str)
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

## Mock email flow

```python
sender_priv = PrivateKey()
sender_pub = sender_priv.public_key.hex()

email_msg = """
    Well, hello there!

    This is a decrypted message!
"""

receiver_priv = PrivateKey()
receiver_pub = receiver_priv.public_key.hex()

sender_secret = sender_priv.compute_shared_secret(receiver_pub)
sender_secret # will match receiver secret
```

```python
encrypted_email = encrypt(email_msg, sender_secret)
encrypted_email
```

```python
receiver_secret = receiver_priv.compute_shared_secret(sender_pub)

# this works because the receiver_secret matches the sender_secret (hence, shared secret)
decrypted_email = decrypt(encrypted_email, receiver_secret)
print(decrypted_email)
```

## TOTP

We may use a different key for each message by concatonating the shared secret with a time stamp and hashing the result. This is known as a [time-based on-time password](https://en.wikipedia.org/wiki/Time-based_one-time_password) (TOTP) and is familiar to anyone who has used [google authenticator](https://googleauthenticator.net/). The time used would be the time the email was sent. The epoch needs to be large enough for the mail servers to route the message.

It might also help to use the latest block hash as the time stamp.

This approach may provide some additional security benefit, such as mitigating replay attacks or preventing emails from being sent from the future.

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
def hash_concat(key, value):
    """concatonates a message with a value and returns the hash"""
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
                      hash_concat(sender_secret, epoch_value)),
              hash_concat(receiver_secret, epoch_value)) # 
    )
```

```python

```
