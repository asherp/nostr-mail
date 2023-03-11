from nostr.key import PrivateKey

import json
import ssl
import time
from nostr.relay_manager import RelayManager

import json
import ssl
import time
from nostr.filter import Filter, Filters
from nostr.event import Event, EventKind
from nostr.relay_manager import RelayManager
from nostr.message_type import ClientMessageType
import requests

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
