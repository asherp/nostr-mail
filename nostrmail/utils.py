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
import email

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
        filters = Filters([filter_])
    elif kind == 'meta':
        kinds = [EventKind.SET_METADATA]
        filter_ = Filter(authors=[pub_key_hex], kinds=kinds)
        filters = Filters([filter_])
    elif kind == 'dm':
        kinds = [EventKind.ENCRYPTED_DIRECT_MESSAGE]
        filter_to_pub_key = Filter(pubkey_refs=[pub_key_hex], kinds=kinds)
        filter_from_pub_key = Filter(authors=[pub_key_hex], kinds=kinds)
        filters = Filters([filter_to_pub_key, filter_from_pub_key])
    else:
        raise NotImplementedError(f'{kind} events not supported')
    
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

def publish_direct_message(priv_key, receiver_pub_key_hex, clear_text=None, dm_encrypted=None, event_id=None):
    """publish a direct message sent from priv_key to receiver_pub_key_hex

    clear_text will be encrypted using a shared secret between sender and receiver
    dm_encrypted is optional - if supplied, clear_text is ignored. if not supplied, clear_text is required
    """

    if dm_encrypted is None:
        if clear_text is None:
            raise IOError('Must provide clear_text if dm is not precomputed')
        else:
            dm_encrypted = priv_key.encrypt_message(clear_text, receiver_pub_key_hex)
    else:
        # assumes the dm was precomputed and receiver can decrypt it
        pass

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
            if dm['p'] == pub_key_hex:
                pass
            elif dm['author'] == pub_key_hex:
                pass
            else:
                raise AssertionError('pub key not associated with dm')
        dms.append(dm)
    return dms

def get_convs(dms):
    """assign conversation tuples to each dm
    
    dms - pd.DataFrame of dms
    
    """
    convs = []
    for _, e in dms.iterrows():
        convs.append(tuple(sorted((e.author, e.p))))
    return convs

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
def load_user_profile(pub_key_hex, cache_val=0):
    print(f'fetching profile {pub_key_hex}')
    profile_events = get_events(pub_key_hex, 'meta')
    if len(profile_events) > 0:
        profile = profile_events[0]
        return profile

def sha256(message):
    if message is None:
        return ''
    digest = hashes.Hash(hashes.SHA256())
    digest.update(message.encode())
    digest.update(b"123")
    return base64.urlsafe_b64encode(digest.finalize()).decode('ascii')

def email_is_logged_in(mail):
    try:
        return 'OK' in mail.noop()
    except:
        return False

def get_encryption_iv(msg):
    """extract the iv from an ecnrypted blob"""
    return msg.split('?iv=')[-1].strip('==')

def find_email_by_subject(mail, subject):
    # Search for emails matching a specific subject
    result, data = mail.search(None, f'SUBJECT "{subject}"')
    
    # Process the list of message IDs returned by the search
    for num in data[0].split():
        # Fetch the email message by ID
        result, data = mail.fetch(num, '(RFC822)')
        raw_email = data[0][1]
        # Convert raw email data into a Python email object
        email_message = email.message_from_bytes(raw_email)
        # Extract the email subject and print it
        subject = email_message['Subject'].strip()
#         print(f"Email subject: {subject}")
    
        # Extract the email body and print it
        if email_message.is_multipart():
#             print('found multipart')
            for part in email_message.walk():
                content_type = part.get_content_type()
                if content_type == 'text/plain':
                    email_body = part.get_payload(decode=True).decode()
                    break
        else:
#             print('normal email')
            email_body = email_message.get_payload(decode=True).decode()
#         print(f"Email body: {email_body}")
        return email_body.strip()


@cache.memoize(typed=True, tag='block_height')
def get_block_hash(block_height):
    result = requests.get(f'https://blockstream.info/api/block-height/{block_height}').content.decode('utf-8')
    return result


@cache.memoize(typed=True, tag='blocks')
def get_block_info(block_height=None, block_hash=None):
    if block_hash is None:
        if block_height is not None:
            block_hash = get_block_hash(block_height)
        else:
            raise IOError('block_height or block_hash required')
    if 'Block not found' in block_hash:
        # this needs to raise an error to prevent cache from storing it
        raise ValueError('Block not found')
    print(f'getting block {block_hash}')
    result = requests.get(f'https://blockstream.info/api/block/{block_hash}')
    return result.json()

def get_latest_block_hash():
    block_hash = requests.get('https://blockstream.info/api/blocks/tip/hash').content.decode('utf-8')
    return block_hash

