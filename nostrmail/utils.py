from nostr.key import PrivateKey

import json
import ssl
import time
from nostr.relay_manager import RelayManager
from nostr.key import PrivateKey

import json
import ssl
import time
from nostr.filter import Filter, Filters
from nostr.event import Event, EventKind
from nostr.relay_manager import RelayManager
from nostr.message_type import ClientMessageType
import requests
from omegaconf import OmegaConf
import pandas as pd
import os
from diskcache import FanoutCache
from cryptography.hazmat.primitives import hashes
import base64

cache = FanoutCache('cache', size_limit=1e6) # 1Mb

nostr_contacts = os.environ['NOSTR_CONTACTS']

relays = OmegaConf.load(nostr_contacts).relays


def get_events(pub_key_hex, kind='text', relays=relays, returns='content'):
    relay_manager = RelayManager()

    for relay in relays:
        relay_manager.add_relay(relay)

    events = []
    if kind == 'text':
        kinds = [EventKind.TEXT_NOTE]
        filter_ = Filter(authors=[pub_key_hex], kinds=kinds)
    elif kind == 'meta':
        kinds = [EventKind.SET_METADATA]
        filter_ = Filter(authors=[pub_key_hex], kinds=kinds)
    elif kind == 'dm':
        kinds = [EventKind.ENCRYPTED_DIRECT_MESSAGE]
        filter_ = Filter(pubkey_refs=[pub_key_hex], kinds=kinds)
    else:
        raise NotImplementedError(f'{kind} events not supported')
    filters = Filters([filter_])
    subscription_id = "some random str"
    request = [ClientMessageType.REQUEST, subscription_id]
    request.extend(filters.to_json_array())


    relay_manager.add_subscription(subscription_id, filters)
    relay_manager.open_connections({"cert_reqs": ssl.CERT_NONE}) # NOTE: This disables ssl certificate verification
    time.sleep(1.25) # allow the connections to open

    message = json.dumps(request)
    relay_manager.publish_message(message)
    time.sleep(1) # allow the messages to send

    while relay_manager.message_pool.has_events():
        event_msg = relay_manager.message_pool.get_event()
        if returns == 'content':
            if kind == 'meta':
                content = json.loads(event_msg.event.content)
            else:
                content = event_msg.event.content
        elif returns == 'event':
            content = event_msg.event
        else:
            raise NotImplementedError(f"{returns} returns option not supported, options are 'event' or 'content'")
        events.append(content)

    relay_manager.close_connections()
    return events

def publish_direct_message(priv_key, receiver_pub_key_hex, clear_text, event_id=None):
    dm_encrypted = priv_key.encrypt_message(clear_text, receiver_pub_key_hex)
    dm_encrypted

    relay_manager = RelayManager()

    for relay in relays:
        relay_manager.add_relay(relay)
    relay_manager.open_connections({"cert_reqs": ssl.CERT_NONE}) # NOTE: This disables ssl certificate verification
    print('waiting 1 sec to open')
    time.sleep(1)
    
    if event_id is None:
        tags=[['p', receiver_pub_key_hex]]
    else:
        tags=[['p', receiver_pub_key_hex], ['e', event_id]]


    dm_event = Event(priv_key.public_key.hex(),
                 dm_encrypted,
                 kind=EventKind.ENCRYPTED_DIRECT_MESSAGE,
                 tags=tags,
                )
    priv_key.sign_event(dm_event)

    assert dm_event.verify()

    relay_manager.publish_event(dm_event)
    print('waiting 1 sec to send')
    time.sleep(1) # allow the messages to send

    relay_manager.close_connections()
    return dm_event.signature

def get_dms(pub_key_hex):
    """Get all dms for this pub key
    Returns list of dict objects storing metadata for each dm
    Note: if a dm signature does not pass, the event is markded with valid=False
    """
    dms = []
    dm_events = get_events(pub_key_hex, kind='dm', returns='event')
    for e in dm_events:
        # check signature first
        if not e.verify():
            dm = dict(valid=False, event_id=e.id)
        else:
            dm = dict(
                valid=True,
                time=pd.Timestamp(e.created_at, unit='s'),
                event_id=e.id,
                author=e.public_key,
                content=e.content,
                **dict(e.tags))
            assert dm['p'] == pub_key_hex # recipient matches
        dms.append(dm)
    return dms

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


def load_contacts(contacts_file=nostr_contacts):
    cfg = OmegaConf.load(contacts_file)
    return OmegaConf.to_container(cfg.contacts)


def publish_profile(priv_key, profile_dict):
    relay_manager = RelayManager()

    for relay in relays:
        relay_manager.add_relay(relay)
    relay_manager.open_connections({"cert_reqs": ssl.CERT_NONE}) # NOTE: This disables ssl certificate verification
    print('waiting 1.5 sec to open')
    time.sleep(1.5)
    
    event_profile = Event(priv_key.public_key.hex(),
                          json.dumps(profile_dict),
                          kind=EventKind.SET_METADATA)
    priv_key.sign_event(event_profile)
    
    # check signature
    assert event_profile.verify()
    relay_manager.publish_event(event_profile)
    print('waiting 1 sec to send')
    time.sleep(1) # allow the messages to send

    relay_manager.close_connections()
    return event_profile.signature

@cache.memoize(tag='profiles') #Todo add temporal caching/refresh button
def load_user_profile(pub_key_hex):
    print(f'fetching profile {pub_key_hex}')
    profile_events = get_events(pub_key_hex, 'meta')
    if len(profile_events) > 0:
        return profile_events[0]

def sha256(message):
    if message is None:
        return ''
    digest = hashes.Hash(hashes.SHA256())
    digest.update(message.encode())
    digest.update(b"123")
    return base64.urlsafe_b64encode(digest.finalize()).decode('ascii')

