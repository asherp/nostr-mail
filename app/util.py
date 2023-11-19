import asyncio
import datetime
import json
import logging
import os
import re
import secrets
import sqlite3
import ssl
import time
import uuid
from contextlib import contextmanager
from websocket import WebSocketConnectionClosedException

import keyring
from pynostr.key import PrivateKey, PublicKey
from pynostr.filters import FiltersList, Filters
from pynostr.event import Event, EventKind
from pynostr.message_type import ClientMessageType
from pynostr.relay_manager import RelayManager
from pynostr.relay_list import RelayList
from pynostr.message_pool import EventMessageStore
from pynostr.metadata import Metadata
from pynostr.utils import get_public_key, get_timestamp
from sqlitedict import SqliteDict
from rich.table import Table
from json import JSONDecodeError
from collections import defaultdict





KEYRING_GROUP = 'nostrmail'
DATABASE_PATH = 'nostrmail.sqlite'
DEFAULT_RELAYS = [
    "wss://nostr-pub.wellorder.net",
    "wss://relay.damus.io",
    'wss://brb.io',
    'wss://nostr.mom']


# Initialize a standard logger; it can be replaced with a Kivy logger if needed.
Logger = logging.getLogger(__name__)

def setup_kivy_logger():
    global Logger
    from ui import Logger as KivyLogger
    Logger = KivyLogger


class NostrRelayManager(RelayManager):
    _instance = None

    @classmethod
    def get_instance(cls, **kwargs):
        if cls._instance is None:
            cls._instance = cls(**kwargs)
        return cls._instance

    def __init__(self, logger=None, **kwargs):
        if self._instance is not None:
            raise Exception("This class is a singleton!")
        if logger is None:
            import logging
            logging.basicConfig(level=logging.INFO)
            logger = logging.getLogger()
        self.logger = logger
        super().__init__(**kwargs)  # Initialize parent class
        NostrRelayManager._instance = self
        self.init_manager()

    def get_relay_list(self):
        relay_list = RelayList()
        relay_list.append_url_list(list(self.relays))
        return relay_list

    def init_manager(self):
        relays = self.load_relays_from_db()
        self.logger.info('connecting relays')
        for relay_url in relays:
            self.logger.info(f'adding relay: {relay_url}')
            self.add_relay(relay_url)  # Call method from RelayManager
        self.logger.info(f'relays connected: {self.relays}')  # Access attribute from RelayManager


    def load_relays_from_db(self):
        with SqliteDict(DATABASE_PATH, tablename='relays') as db:
            relays = db.get('relays', DEFAULT_RELAYS)
        return relays


    def add_subscription(self, id, filters: Filters):
        super().add_subscription_on_all_relays(id, filters)
        self.logger.info(f"Subscription added with ID {id}")


    @contextmanager
    def temporary_subscription(self, filters):
        subscription_id = secrets.token_hex(4)
        self.add_subscription(subscription_id, filters)
        try:
            yield subscription_id
        finally:
            self.close_subscription_on_all_relays(subscription_id)

    def publish_message(self, message):
        try:
            super().publish_message(message)
            self.logger.info(f"Message published: {message}")
        except WebSocketConnectionClosedException as e:
            self.logger.warning(f"WebSocket connection closed: {e}")
            self.run_sync()
        except Exception as e:
            self.logger.error(f"Error in publishing message: {e}")

    def fetch_profile_data(self, pub_key_hex=None, kind='profile'):
        """fetches profile data following example from pynostr.examples.show_metadata"""

        # Access the Manager instance with the relays from the relay_manager attribute
        if pub_key_hex is None:
            pub_key_hex = load_user_pub_key()

        relay_list = self.get_relay_list()

        print(f"Checking {len(relay_list.data)} relays...")

        relay_list.update_relay_information(timeout=0.5)
        relay_list.drop_empty_metadata()

        self.add_relay_list(relay_list)

        events = EventMessageStore()
        events_by_relay = {}
        unix_timestamp = get_timestamp(days=7)
        now = datetime.datetime.utcnow()

        if kind == 'profile':
            filters = FiltersList(
                [Filters(authors=[pub_key_hex], kinds=[EventKind.SET_METADATA], limit=1)]
            )
        elif kind == 'dm':
            kinds=[EventKind.ENCRYPTED_DIRECT_MESSAGE]
            filter_to_pub_key = Filters(pubkey_refs=[pub_key_hex], kinds=kinds)
            filter_from_pub_key = Filters(authors=[pub_key_hex], kinds=kinds)
            filters = FiltersList([filter_to_pub_key, filter_from_pub_key])

        subscription_id = uuid.uuid1().hex
        self.add_subscription_on_all_relays(subscription_id, filters)
        self.run_sync()

        event_messages = self.message_pool.get_all_events()
        events.add_event(event_messages)

        if kind == 'dm':
            # decryption handled downstream
            #extract event from EventMessageStore
            return [_.event for _ in events]

        for url in relay_list.get_url_list():
            event_list = events.get_events_by_url(url)
            if len(event_list) == 0:
                continue
            try:
                print(f'{url} events: {len(event_list)}')
                events_by_relay[url] = {
                    "timestamp": event_list[0].event.date_time(),
                    "metadata": Metadata.from_event(event_list[0].event).metadata_to_dict()
                }
            except JSONDecodeError:
                self.logger.error(f'something wrong with event {event_list[0].event}')
                continue

        relay_list_sorted = sorted(events_by_relay.items(), key=lambda item: item[1]["timestamp"])

        result = {
            'latest_metadata_url': relay_list_sorted[-1][0],
            'latest_metadata': relay_list_sorted[-1][1]["metadata"]
        }

        if 'identities' in relay_list_sorted[-1][1]["metadata"]:
            identities_info = []
            for identity in relay_list_sorted[-1][1]["metadata"]['identities']:
                identities_info.append({
                    'claim_type': identity.claim_type,
                    'identity': identity.identity,
                    'proof': identity.proof
                })
            result['identities_info'] = identities_info

        profile_data = result['latest_metadata']

        return profile_data


    def get_events(self, pub_key_hex, kind='text', returns='content'):
        """fetch events of any kind for pub_key_hex"""

        # Check if the events are already in the cache
        # if so, return them
        # events = Cache.get(kind, pub_key_hex)
        # if events is not None:
        #     return events

        if kind == 'text':
            kinds = [EventKind.TEXT_NOTE]
            filter_ = Filters(authors=[pub_key_hex], kinds=kinds)
            filters = FiltersList([filter_])
        elif kind == 'meta':
            kinds = [EventKind.SET_METADATA]
            filter_ = Filters(authors=[pub_key_hex], kinds=kinds)
            filters = FiltersList([filter_])
        elif kind == 'dm':
            kinds = [EventKind.ENCRYPTED_DIRECT_MESSAGE]
            filter_to_pub_key = Filters(pubkey_refs=[pub_key_hex], kinds=kinds)
            filter_from_pub_key = Filters(authors=[pub_key_hex], kinds=kinds)
            filters = FiltersList([filter_to_pub_key, filter_from_pub_key])
        else:
            raise NotImplementedError(f'{kind} events not supported')
        
        with self.temporary_subscription(filters) as subscription_id:
            self.logger.info(f"Temporary subscription created with ID {subscription_id}")

            request = [ClientMessageType.REQUEST, subscription_id]
            request.extend(filters.to_json_array())
            message = json.dumps(request)

            try:
                self.publish_message(message)
            except WebSocketConnectionClosedException:
                self.logger.warning('connection was closed, reopening..')
                self.open_connections()
                time.sleep(1.5)
                self.publish_message(message)

            time.sleep(1) # allow the messages to send
            self.logger.info(f'message should have sent {kind}')
            self.logger.info(f'found events {self.message_pool.has_events()}')

            events = []
            while self.message_pool.has_events():
                event_msg = self.message_pool.get_event()
                self.logger.info(f"Processing event: {event_msg}")
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

            # Cache.append(kind, pub_key_hex, events)
            self.logger.info(f"Events fetched for {pub_key_hex}: {events}")
            return events


    def __del__(self):
        self.close_subscription_on_all_relays()
        self.close_all_relay_connections()

    def wait_and_publish_message(self, message):
        try:
            self.publish_message(message)
        except WebSocketConnectionClosedException as e:
            self.logger.error(f"Failed to publish message: {e}")

    def load_profile_data(self, pub_key_hex=None):
        # Access the Manager instance with the relays from the relay_manager attribute
        if pub_key_hex is None:
            pub_key_hex = load_user_pub_key()

        profile_events = self.get_events(pub_key_hex, 'meta')

        self.logger.info(f'profile events: {profile_events}')
        if len(profile_events) > 0:
            profile = profile_events[0]
            return profile
        else:
            self.logger.warning(f'no profile events found for {pub_key_hex}')

    def publish_profile(self, profile_dict):

        priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
        if not priv_key_nsec:
            self.logger.error("Private key not found in keyring.")
            raise IOError("Expected private key in keyring")
        priv_key = PrivateKey.from_nsec(priv_key_nsec)

        event_profile = Event(pubkey=priv_key.public_key.hex(),
                              content=json.dumps(profile_dict),
                              kind=EventKind.SET_METADATA)
        event_profile.sign(get_priv_key_hex(priv_key_nsec))

        # check signature
        assert event_profile.verify()

        try:
            for relay_url, relay in self.relays.items():
                if relay.policy.should_write:
                    print(f'will publish to {relay_url}')
                if not relay.is_connected:
                    print(f'connecting to {relay_url}')
                    relay.connect()
                    time.sleep(1)
                    print(f'relay connected: {relay.is_connected}')
            self.publish_event(event_profile)
        except WebSocketConnectionClosedException:
            self.logger.warning('connection was closed, reopening..')
            self.open_connections()
            time.sleep(1.5)
            self.publish_event(event_profile)

        self.run_sync()

        print('waiting 1 sec to send')
        time.sleep(1) # allow the messages to send

        return event_profile.sig

    def get_dms(self, refresh=True):
        """Get all dms for this pub key
        Returns list of dict objects storing metadata for each dm
        Note: if a dm signature does not pass, the event is markded with valid=False
        """
        priv_key = load_user_priv_key()
        pub_key_hex = priv_key.public_key.hex()
        events = fetch_event_from_db('dm')

        if refresh:
            events_ = self.fetch_profile_data(pub_key_hex, kind='dm')
            for event in events_:
                save_event_to_db('dm', **event.to_dict())
                events.add_event(event)

        # this step removes any duplicate events
        decrypted_events = decrypt_dm_events(events, priv_key)

        return decrypted_events

def load_user_priv_key():
    priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key_nsec:
        Logger.error("Private key not found in keyring.")
        raise IOError("Expected private key in keyring")
    priv_key = PrivateKey.from_nsec(priv_key_nsec)
    return priv_key

def load_user_pub_key():
    priv_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key:
        Logger.error("Private key not found in keyring.")
        raise IOError("Expected private key in keyring")
    pub_key_hex = get_nostr_pub_key(priv_key)
    return pub_key_hex


def parse_responses(response):
    success_count = 0
    Logger.debug(f'parse_responses received: {response}, type: {type(response)}')
    Logger.handlers[0].flush()

    for task in response:
        if isinstance(task, set):
            continue

        if not task.done():
            continue  # Skip tasks that are not done yet
        
        try:
            result = task.result()
            if isinstance(result, tuple) and result[0] == "OK":
                # If the response is a tuple starting with "OK", consider it a success.
                success_count += 1
        except Exception as e:
            # Log exceptions from tasks.
            Logger.error(f"Task resulted in an error: {e}")

    return success_count


def save_profile_to_relays(content, relay_manager):
    # Convert the dictionary to a JSON string
    content_json = json.dumps(content)
    
    # Fetch the user's private key in nsec format
    priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key_nsec:
        Logger.error("Private key not found in keyring.")
        return False
    
    pub_key = get_nostr_pub_key(priv_key_nsec)
    priv_key_hex = get_priv_key_hex(priv_key_nsec)

    profile_event = Event(
        pubkey=pub_key,
        created_at=get_current_unix_timestamp(),
        kind=EventKind.SET_METADATA,
        tags=[],
        content=content_json
    )

    # Sign the event with the user's private key
    profile_event.sign(priv_key_hex)

    # Use the connected relay_manager to add the event
    try:
        response = relay_manager.publish_event(profile_event, check_response=True)
        Logger.debug(f'Raw response from relay_manager.add_event: {response}')
        success_count = parse_responses(response)
        Logger.debug(f'Parsed response count: {success_count}')
        if success_count > 0:
            Logger.info(f"Profile event published to {success_count} relay(s) with event id {profile_event.id}.")
            return True
        else:
            Logger.error(f"Failed to publish profile event to any relay. Parsed response: {success_count}, Raw response: {response}")
            return False
    except Exception as e:
        Logger.error(f"Failed to publish profile event: {e}")
        return False



def get_priv_key_hex(priv_key_nsec):
    priv_key = PrivateKey.from_nsec(priv_key_nsec)
    return priv_key.hex()

def get_nostr_pub_key(priv_key_nsec):
    """given priv key in nsec format, returns pub key in hex format"""
    if priv_key_nsec is None:
        return 'no priv key provided'
    try:
        pub_key_hex = PrivateKey.from_nsec(priv_key_nsec).public_key.hex()
    except:
        Logger.error(f'strange priv key ----> {priv_key_nsec} <----')
        return 'priv key does not match nsec format'
    return pub_key_hex


def save_setting(section, key, value, settings_file='address_book.yaml'):

    # Load data from a YAML file
    with open('address_book.yaml', 'r') as f:
        data = yaml.load(f, Loader=yaml.FullLoader)

    data[section][key] = value

    with open('address_book.yaml', 'w') as f:
        yaml.dump(data, f)

def get_setting(section, key, settings_file='address_book.yaml'):
    # Load data from a YAML file
    with open('address_book.yaml', 'r') as f:
        data = yaml.load(f, Loader=yaml.FullLoader)
    return data[section][key]    


def get_current_unix_timestamp():
    return int(datetime.datetime.now().timestamp())


def get_encryption_iv(msg):
    """extract the iv from an ecnrypted blob"""
    return msg.split('?iv=')[-1].strip('==')



def get_convs(dms):
    """assign conversation tuples to each dm
    
    dms - pd.DataFrame of dms
    
    """
    convs = []
    for _, e in dms.iterrows():
        convs.append(tuple(sorted((e.author, e.p))))
    return convs


def decrypt_dm_events(events, priv_key):
    dms = defaultdict(dict)
    for event in events:
        from_pubkey = event.pubkey
        to_pubkey = dict(event.tags)['p']
        conv_key = tuple(sorted([from_pubkey, to_pubkey]))
        if priv_key.public_key.hex() == from_pubkey:
            decrypted = priv_key.decrypt_message(event.content, to_pubkey)
        else:
            decrypted = priv_key.decrypt_message(event.content, from_pubkey)
        iv = get_encryption_iv(event.content)
        dms[conv_key][iv] = dict(decrypted=decrypted, time=event.date_time(), from_pubkey=from_pubkey)
    return dms

def save_event_to_db(table_name, id=None, **event):
    """inserts event into database with id matching nostr id"""
    with SqliteDict(DATABASE_PATH, tablename=table_name, autocommit=True) as db:
        db[id] = event

def fetch_event_from_db(table_name, id=None):
    """fetch rows from db

    if id is None:
        return table dict of {id: **event}
    else:
        return table_name[id]
    """

    with SqliteDict(DATABASE_PATH, tablename=table_name, autocommit=True) as db:
        if id is None:
            events = EventMessageStore()
            for event_id, event in dict(db).items():
                events.add_event(event=Event.from_dict(dict(id=event_id, **event)))
            return events
        else:
            return Event.from_dict(dict(id=id, **db[id]))


def save_profile_to_db(pub_key_hex=None, profile_data=None):
    if pub_key_hex is None:
        pub_key_hex = load_user_pub_key()

    with SqliteDict(DATABASE_PATH, tablename='profiles', autocommit=True) as db:
        db[pub_key_hex] = profile_data

def fetch_profile_from_db(pub_key_hex=None):
    if pub_key_hex is None:
        pub_key_hex = load_user_pub_key()
    with SqliteDict(DATABASE_PATH, tablename='profiles', autocommit=True) as db:
        return db.get(pub_key_hex)


