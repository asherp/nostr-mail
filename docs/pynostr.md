# pynostr

This library originated as a fork of [python-nostr](https://github.com/jeffthibault/python-nostr) but is more stable.


## Pub/Priv keys

```python
from pynostr.key import PrivateKey

private_key = PrivateKey()
public_key = private_key.public_key
print(f"Private key: {private_key.bech32()}")
print(f"Public key: {public_key.bech32()}")
```

```python
import dotenv
```

```python
dotenv.load_dotenv('../.env')
```

```python
import os
```

```python
private_key_alice = PrivateKey.from_nsec(os.environ['PRIV_KEY_ALICE'])
public_key_alice = private_key_alice.public_key
public_key_alice.hex()
```

```python
private_key_bob = PrivateKey.from_nsec(os.environ['PRIV_KEY_BOB'])
public_key_bob = private_key_bob.public_key
public_key_bob.hex()
```

## RelayManager


This code needs to be run outside of jupyter's event loop

```python
import nest_asyncio
nest_asyncio.apply()
```

```python
cd ../app
```

```python
from util import NostrRelayManager
```

```python
relay_manager = NostrRelayManager(error_threshold=3, timeout=2)
```

```python
for url in relay_manager.relays:
    print(url)
```

## Profiles

```python
profile_data = relay_manager.fetch_profile_data(public_key_bob)

```

```python
profile_data
```

## Publishing

```python
import nest_asyncio
nest_asyncio.apply()
```

```python
import json
import ssl
import time
import uuid
from pynostr.event import Event
from pynostr.relay_manager import RelayManager
from pynostr.filters import FiltersList, Filters
from pynostr.message_type import ClientMessageType
from pynostr.key import PrivateKey

relay_manager = RelayManager(timeout=6)
relay_manager.add_relay("wss://nostr-pub.wellorder.net")
relay_manager.add_relay("wss://relay.damus.io")
private_key = PrivateKey.from_nsec('nsec1uugsmmryuzjvaltgh4spnjt3xcr48w8k4xtdg8cze6e4c9qnhdhqgp50k2')

filters = FiltersList([Filters(authors=[private_key.public_key.hex()], limit=100)])
subscription_id = uuid.uuid1().hex
relay_manager.add_subscription_on_all_relays(subscription_id, filters)

event = Event("Hello Nostr")
event.sign(private_key.hex())

relay_manager.publish_event(event)
relay_manager.run_sync()
print('event published')

time.sleep(5) # allow the messages to send
while relay_manager.message_pool.has_ok_notices():
    ok_msg = relay_manager.message_pool.get_ok_notice()
    print(ok_msg)
while relay_manager.message_pool.has_events():
    event_msg = relay_manager.message_pool.get_event()
    print(event_msg.event.to_dict())

```

```python
from pynostr.relay_manager import RelayManager
from pynostr.filters import FiltersList, Filters
from pynostr.event import EventKind
import time
import uuid
from pynostr.key import PrivateKey

private_key = PrivateKey.from_nsec('nsec1uugsmmryuzjvaltgh4spnjt3xcr48w8k4xtdg8cze6e4c9qnhdhqgp50k2')

relay_manager = RelayManager(timeout=2)
relay_manager.add_relay("wss://nostr-pub.wellorder.net")
relay_manager.add_relay("wss://relay.damus.io")
filters = FiltersList([Filters(authors=[private_key.public_key.hex()], kinds=[EventKind.TEXT_NOTE], limit=100)])
# filters = FiltersList([Filters(kinds=[EventKind.TEXT_NOTE], limit=100)])
subscription_id = uuid.uuid1().hex
relay_manager.add_subscription_on_all_relays(subscription_id, filters)
relay_manager.run_sync()
while relay_manager.message_pool.has_notices():
    notice_msg = relay_manager.message_pool.get_notice()
    print(notice_msg.content)
while relay_manager.message_pool.has_events():
    event_msg = relay_manager.message_pool.get_event()
    print(event_msg.event.content)
relay_manager.close_all_relay_connections()
```

```python

```
