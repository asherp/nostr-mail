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
import sys
```

```python
sys.path.append('../app')
```

```python
from util import NostrRelayManager
```

```python
relay_manager = NostrRelayManager(timeout=2)
```

```python
from pynostr.relay_list import RelayList
```

```python
relay_manager.relays
```

```python
relay_list = RelayList()
```

```python
relay_list.append_url_list?
```

```python
from pynostr.message_pool import EventMessageStore
from pynostr.utils import get_public_key, get_relay_list, get_timestamp
import datetime
import uuid
from pynostr.filters import FiltersList, Filters
from pynostr.event import Event, EventKind


# get_relay_list can be used to fetch over 500 relays

identity = public_key_bob

events = EventMessageStore()
events_by_relay = {}
unix_timestamp = get_timestamp(days=7)
now = datetime.datetime.utcnow()

filters = FiltersList(
    [Filters(authors=[identity.hex()], kinds=[EventKind.SET_METADATA], limit=1)]
)
subscription_id = uuid.uuid1().hex
relay_manager.add_subscription_on_all_relays(subscription_id, filters)
relay_manager.run_sync()

event_messages = relay_manager.message_pool.get_all_events()
events.add_event(event_messages)
```

```python
events
```

```python
for url in relay_list.get_url_list():

    event_list = events.get_events_by_url(url)
    if len(event_list) == 0:
        continue
    oldest_timestamp = now
    events_by_relay[url] = {"timestamp": None, "metadata": None}
    m = Metadata.from_event(event_list[0].event)
    events_by_relay[url]["timestamp"] = event_list[0].event.date_time()
    events_by_relay[url]["metadata"] = m
```

```python
profile_data = relay_manager.load_profile_data()
```

```python
profile_data
```

```python
from pynostr.filters import FiltersList, Filters
from pynostr.event import EventKind
```

```python
dir(EventKind)
```

```python
from pynostr.relay_manager import RelayManager
from pynostr.filters import FiltersList, Filters
from pynostr.event import EventKind
import time
import uuid

relay_manager = RelayManager(timeout=2)
relay_manager.add_relay("wss://nostr-pub.wellorder.net")
relay_manager.add_relay("wss://relay.damus.io")
filters = FiltersList([Filters(kinds=[EventKind.TEXT_NOTE], limit=100)])
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

## Profiles


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
