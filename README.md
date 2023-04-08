
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


### priv/pub key generation

Use this to set your private key as an environment variable. Store it in your local .env so the container can pick it up on load.

```python
from nostr.key import PrivateKey

private_key = PrivateKey()
public_key = private_key.public_key
print(f"Private key: {private_key.bech32()}")
print(f"Public key: {public_key.bech32()}")
```

<!-- #region -->
Copy and the above private key into `.env` at the root of this repo.

```sh
NOSTR_PRIV_KEY=<priv key here>
```
<!-- #endregion -->

```python
import os
```

```python
try:
    priv_key = os.environ['NOSTR_PRIV_KEY']
except KeyError:
    raise KeyError('Please set environment variable NOSTR_PRIV_KEY')
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

## Text events
From [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) there are three kinds of events

* 0: set_metadata: the content is set to a stringified JSON object {name: <username>, about: <string>, picture: <url, string>} describing the user who created the event. A relay may delete past set_metadata events once it gets a new one for the same pubkey.
* 1: text_note: the content is set to the plaintext content of a note (anything the user wants to say). Markdown links ([]() stuff) are not plaintext.
* 2: recommend_server: the content is set to the URL (e.g., wss://somerelay.com) of a relay the event creator wants to recommend to its followers.

```python
from nostrmail.utils import get_events
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
from nostrmail.utils import validate_nip05
```

```python
from json import JSONDecodeError
```

```python
validate_nip05(node_hello_hex) # returns the name of this user according to their .com
```

Hello Jessica's nip05 contains several other names, so this acts as a p2p registry!

```python
hrh_hex = 'ae4efa502bf1cb7dd63343ae4bc7bd0a599c5251c686c9ebd3b5f0d4f841a939'
```

```python
bytes(b"\x02")
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


nostr-python already uses AES 256 (?) encryption. More on the encryption scheme can be found here https://github.com/jeffthibault/python-nostr/blob/37cb66ba2d3968b2d75cc8ad71c3550415ca47fe/nostr/key.py#L69

<!-- #region -->
```python
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
import secrets
iv = secrets.token_bytes(16)
cipher = Cipher(algorithms.AES(self.compute_shared_secret(public_key_hex)), modes.CBC(iv))
```
<!-- #endregion -->

```python
help(priv_key1.encrypt_message)
```

```python
help(priv_key2.decrypt_message)
```

```python
clear_message = 'hello there'

encrypted_msg = priv_key1.encrypt_message(clear_message, pub_key2.hex())
encrypted_msg
```

```python
assert priv_key2.decrypt_message(encrypted_msg, pub_key1.hex()) == clear_message
```

This approach uses the key pairs alone. There is no timing information included.


## Fernet encryption


We could also use [Fernet encryption](https://cryptography.io/en/latest/fernet/#fernet-symmetric-encryption) available from the cryptography package. Fernet encryption is a form of symmetric encryption, meaning the same key may be used to encrypt and decrypt a message.

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

<!-- #region -->
`python-nostr/key.py`
```python
def compute_shared_secret(self, public_key_hex: str) -> bytes:
        pk = secp256k1.PublicKey(bytes.fromhex("02" + public_key_hex), True)
        return pk.ecdh(self.raw_secret, hashfn=copy_x)
```

The shared secret is the result of applying Eliptic Curve Diffe Hellman, so it should return a point on the elliptic curve (which is just another public key)
<!-- #endregion -->

```python
sender_secret # can turn into hex encoded str
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

We may use a different key for each message by concatonating the shared secret with a time stamp and hashing the result. This is known as a [time-based on-time password](https://en.wikipedia.org/wiki/Time-based_one-time_password) (TOTP) and should already be familiar to anyone who has used [google authenticator](https://googleauthenticator.net/). The time used would be the time the email was sent. The epoch needs to be large enough for the mail servers to route the message.

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

### Try connecting to Damus

```python
import json 
import ssl
import time
from nostr.event import Event
from nostr.relay_manager import RelayManager
from nostr.message_type import ClientMessageType
from nostr.key import PrivateKey

relay_manager = RelayManager()
relay_manager.add_relay("wss://nostr-pub.wellorder.net")
relay_manager.add_relay("wss://relay.damus.io")
relay_manager.open_connections({"cert_reqs": ssl.CERT_NONE}) # NOTE: This disables ssl certificate verification
time.sleep(1.25) # allow the connections to open
```

```python
event = Event(pub_key_hex, "Hello there")
```

```python
priv_key.sign_event(event)
```

```python
assert event.verify() # checks signature on event
```

```python
relay_manager.publish_event(event)
time.sleep(1) # allow the messages to send

relay_manager.close_connections()
```

### fetch event for your pub key

```python
from nostrmail.utils import get_events
```

```python
get_events(pub_key_hex)
```

```python
from nostr.key import mine_vanity_key
```

## Address book

```python
from omegaconf import OmegaConf
import pandas as pd
import dash_bootstrap_components as dbc
```

```python
from nostrmail.utils import load_contacts
```

```python
load_contacts()
```

```python
def update_contacts_table(url):
    contacts = load_contacts()
    table = dbc.Table.from_dataframe(contacts, index=True)
    return table.children
```

## create user profile

```python
try:
    priv_key_str = os.environ['NOSTR_PRIV_KEY']
except KeyError:
    raise KeyError('Please set environment variable NOSTR_PRIV_KEY')
```

```python
priv_key = PrivateKey.from_nsec(priv_key_str)
assert priv_key.bech32() == priv_key_str
```

```python
from nostrmail.utils import get_events, load_current_user
from nostr.key import PrivateKey
import os
```

```python

```

## Generate Alice profile

```python
import os
```

```python
from nostr.event import EventKind
from nostr.key import PrivateKey
from nostr.event import Event
from nostr.relay_manager import RelayManager
import json
import ssl

alice_priv_key_str = os.environ['PRIV_KEY_ALICE']
alice_email = os.environ['EMAIL_ALICE']
alice_priv_key = PrivateKey.from_nsec(alice_priv_key_str)
assert alice_priv_key.bech32() == alice_priv_key_str
```

```python
import time
```

```python
from nostrmail.utils import relays, publish_profile
```

```python
alice_profile = dict(display_name='Alice',
              name='alice',
              picture='https://encrypted-tbn0.gstatic.com/images?q=tbn:ANd9GcTV-0rZbgnQcRbDqbk0hHPLHPyHpqLJ8xkriA&usqp=CAU',
              about='my name is Alice..',
              email=alice_email)
```

```python
sig = publish_profile(alice_priv_key, alice_profile)
```

```python
sig
```

Verify that profile was published

```python
from nostrmail.utils import get_events
```

```python
alice_profile_remote = get_events(alice_priv_key.public_key.hex(), 'meta')[0]
```

```python
assert alice_profile_remote['email'] == alice_email
```

```python
alice_profile_remote
```

### Publish Bob's profile

```python
bob_priv_key_str = os.environ['PRIV_KEY_BOB']
bob_email = os.environ['EMAIL_BOB']
bob_priv_key = PrivateKey.from_nsec(bob_priv_key_str)
assert bob_priv_key.bech32() == bob_priv_key_str
```

```python
bob_priv_key.public_key.hex()
```

```python
bob_profile = dict(display_name='Bob',
              name='bob',
              picture='https://cdnb.artstation.com/p/assets/images/images/030/065/923/large/in-house-media-bobgundisplay.jpg?1599501909',
              about="I am the one they call Bob",
              email=bob_email)
```

```python
sig = publish_profile(bob_priv_key, bob_profile)
```

```python
bob_profile_remote = get_events(bob_priv_key.public_key.hex(), 'meta')[0]
```

```python
assert bob_profile_remote['email'] == bob_email
```

```python
bob_profile_remote
```

```python
bob_priv_key.public_key.hex()
```

## Direct Message

Test delivery of the email subject via dm. The standard is defined in https://github.com/nostr-protocol/nips/blob/master/04.md

* text is encrypted with `base64-encoded, aes-256-cbc` using the x-coordinate if the shared point between sender/receiver
* content includes an initialization vector `"content": "<encrypted_text>?iv=<initialization_vector>"`
* `tags` MUST contain an entry identifying the receiver of the message in the form `["p", "<pubkey, as a hex string>"]`.
* `tags` MAY contain an entry identifying the previous message in a conversation or a message we are explicitly replying to, in the form `["e", "<event_id>"]`.

```python
from nostr.key import PrivateKey
import os
alice_priv_key_str = os.environ['PRIV_KEY_ALICE']
alice_email = os.environ['EMAIL_ADDRESS_ALICE']
alice_priv_key = PrivateKey.from_nsec(alice_priv_key_str)
assert alice_priv_key.bech32() == alice_priv_key_str

bob_priv_key_str = os.environ['PRIV_KEY_BOB']
bob_email = os.environ['EMAIL_ADDRESS_BOB']
bob_priv_key = PrivateKey.from_nsec(bob_priv_key_str)
assert bob_priv_key.bech32() == bob_priv_key_str
```

Confirm that we can create a valid priv key from the one provided

```python
from nostrmail.utils import relays, publish_direct_message
```

```python
# publish_direct_message(alice_priv_key, bob_priv_key.public_key.hex(), "hi ho bob!")
```

```python
from nostrmail.utils import get_events
```

```python
txt_events = get_events(bob_priv_key.public_key.hex(), kind='dm', returns='event')
```

```python
for e in txt_events:
    print(e.content, e.tags)
```

```python
bob_priv_key.public_key.hex()
```

```python
bob_priv_key.decrypt_message(e.content, alice_priv_key.public_key.hex())
```

```python
# publish_direct_message(bob_priv_key, alice_priv_key.public_key.hex(), 'hullo, hullo!', e.id)
```

```python
from nostrmail.utils import get_dms, get_convs
```

```python
import pandas as pd
```

```python
alice_dms = get_dms(alice_priv_key.public_key.hex())
```

```python
alice_priv_key.public_key.hex()
```

```python
dms = pd.DataFrame(alice_dms)
dms['conv'] = get_convs(dms)
```

```python
dms
```

```python
pd.DataFrame(alice_dms).set_index('time').sort_index(ascending=False)
```

```python
bob_dms = get_dms(bob_priv_key.public_key.hex())
```

```python
bob_dms_df = pd.DataFrame(bob_dms)
```

```python
bob_dms_df['convs'] = get_convs(bob_dms_df)
```

```python
bob_dms_df
```

```python
bob_dms
```

```python
def get_encryption_iv(msg):
    """extract the iv from an ecnrypted blob"""
    return msg.split('?iv=')[-1].strip('==')
```

```python
for id_, _ in pd.DataFrame(bob_dms).iterrows():
    print(get_encryption_iv(_.content), alice_priv_key.decrypt_message(_.content, bob_priv_key.public_key.hex()))
```

```python
alice_priv_key.decrypt_message(_.content, bob_priv_key.public_key.hex())
```

```python
# from nostr.event import EncryptedDirectMessage # this isn't available as of nostr==0.0.2

# dm = EncryptedDirectMessage(
#   recipient_pubkey=recipient_pubkey,
#   cleartext_content="Secret message!"
# )
# private_key.sign_event(dm)
# relay_manager.publish_event(dm)
```

### Contacts

There's a nip for contacts!
https://github.com/nostr-protocol/nips/blob/master/02.md e.g. frank.david.erin 


## search email by subject

```python
import imaplib
import email
```

```python
import os
```

```python
email_imap = os.environ['IMAP_HOST']
```

```python
email_username = os.environ['EMAIL_ADDRESS']
```

```python
email_password = os.environ['EMAIL_PASSWORD']
```

```python
# Set up connection to IMAP server
mail = imaplib.IMAP4_SSL(email_imap)
```

```python
if not email_is_logged_in(mail):
    print('logging in')
    mail.login(email_username, email_password)
```

```python
mail.login(email_username, email_password)
```

```python
email_is_logged_in(mail)
```

```python
if not email_is_logged_in(mail):
    print('logging in')
    mail.login(email_username, email_password)
```

```python
from dash import html
```

```python
mail.select('Inbox')
```

```python
# email_body = find_email_by_subject(mail, 'bVpH/kND9hb1p83A0saXYw')
email_body = find_email_by_subject(mail, 'r2e7cDJR6dqDgShm6w')

email_body
```

```python
type(email_body)
```

```python
check_if_email_logged_in(mail)
```

```python
from dash import dcc
```

```python
dcc.Markdown?
```

```python
print(alice_priv_key.decrypt_message(email_body, bob_priv_key.public_key.hex()))
```

```python
imaplib.IMAP4_SSL?
```

```python
# Close the mailbox and logout from the IMAP server
mail.close()
mail.logout()
```

```python
assert not email_is_logged_in(mail)
```

## Filters

```python
from nostr.filter import Filter, Filters
```

```python
Filters?
```

```python
Filter?
```

```python
%load_ext autoreload
%autoreload 2
```

```python

```
