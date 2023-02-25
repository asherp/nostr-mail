
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

```python
result = requests.get('https://safier.com/.well-known/nostr.json?name=xenofun')
```

```python
json.loads(result.content.decode('utf-8'))
```

The above response holds several other names, so this acts as a p2p registry!

```python
hodl_hex = '1afe0c74e3d7784eba93a5e3fa554a6eeb01928d12739ae8ba4832786808e36d'
```

```python
pd.DataFrame(get_events(hodl_hex, 'meta'))
```

```python

```
