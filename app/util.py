from nostr.key import PrivateKey, PublicKey
from kivy.logger import Logger
import sqlite3
import keyring
from nostr.filter import Filter, Filters
from nostr.event import Event, EventKind
from nostr.message_type import ClientMessageType
from nostr.relay_manager import RelayManager
from websocket import WebSocketConnectionClosedException
from sqlitedict import SqliteDict
import asyncio
import datetime
from kivymd.app import MDApp
import json
import re
import secrets
import ssl
import os
import time
from kivy.config import Config
from sqlitedict import SqliteDict
import asyncio

# Set the directory where you want to store the log files
log_directory = os.path.join(os.path.dirname(__file__), 'logs')
if not os.path.exists(log_directory):
    os.makedirs(log_directory)

# Configure the Kivy logger
Config.set('kivy', 'log_dir', log_directory)
Config.set('kivy', 'log_name', 'kivy_%y-%m-%d_%_.txt')



KEYRING_GROUP = 'nostrmail'
DATABASE_PATH = 'nostrmail.sqlite'
DEFAULT_RELAYS = [
    "wss://nostr-pub.wellorder.net",
    "wss://relay.damus.io",
    'wss://brb.io',
    'wss://nostr.mom']


class NostrRelayManager:
    _instance = None

    @classmethod
    def get_instance(cls):
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def __init__(self):
        if self._instance is not None:
            raise Exception("This class is a singleton!")
        self.relay_manager = None
        self.connected = False
        NostrRelayManager._instance = self
        self.init_manager()


    def init_manager(self):
        relays = self.load_relays_from_db()
        self.relay_manager = RelayManager()
        Logger.info('connecting relays')
        for relay_url in relays:
            Logger.info(f'adding relay: {relay_url}')
            self.relay_manager.add_relay(relay_url)
        Logger.info(f'relays connected: {self.relay_manager.relays}')


    def load_relays_from_db(self):
        with SqliteDict(DATABASE_PATH, tablename='relays') as db:
            relays = db.get('relays', DEFAULT_RELAYS)
        return relays

    def get_events(self, pub_key_hex, kind='text', returns='content'):
        """fetch events of any kind for pub_key_hex"""
        relay_manager = self.relay_manager

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
        
        subscription_id = secrets.token_hex(4)
        request = [ClientMessageType.REQUEST, subscription_id]
        request.extend(filters.to_json_array())


        relay_manager.add_subscription(subscription_id, filters)

        message = json.dumps(request)

        try:
            relay_manager.publish_message(message)
        except WebSocketConnectionClosedException:
            Logger.warning('connection was closed, reopening..')
            relay_manager.open_connections()
            time.sleep(1.5)
            relay_manager.publish_message(message)

        time.sleep(1) # allow the messages to send
        Logger.info(f'message should have sent {kind}')
        Logger.info(f'found events {relay_manager.message_pool.has_events()}')

        events = []
        while relay_manager.message_pool.has_events():
            event_msg = relay_manager.message_pool.get_event()
            Logger.info(f'found event {event_msg}')
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
        try:
            relay_manager.close_subscription(subscription_id)
        except KeyError:
            Logger.warning(f'subscription id {subscription_id} not found in relay manager:')
            for relay in relay_manager.relays.values():
                Logger.info(relay.subscriptions)

        return events

    def load_profile_data(self, pub_key_hex=None):
        # Access the Manager instance with the relays from the relay_manager attribute
        if pub_key_hex is None:
            pub_key_hex = load_user_pub_key()

        profile_events = self.get_events(pub_key_hex, 'meta')

        Logger.info(f'profile events: {profile_events}')
        if len(profile_events) > 0:
            profile = profile_events[0]
            return profile
        else:
            Logger.warning(f'no profile events found for {pub_key_hex}')

    def publish_profile(self, profile_dict):
        relay_manager = self.relay_manager

        priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
        if not priv_key_nsec:
            Logger.error("Private key not found in keyring.")
            raise IOError("Expected private key in keyring")
        priv_key = PrivateKey.from_nsec(priv_key_nsec)

        event_profile = Event(priv_key.public_key.hex(),
                              json.dumps(profile_dict),
                              kind=EventKind.SET_METADATA)
        priv_key.sign_event(event_profile)
        
        # check signature
        assert event_profile.verify()

        try:
            relay_manager.publish_message(event_profile.to_message())
        except WebSocketConnectionClosedException:
            Logger.warning('connection was closed, reopening..')
            relay_manager.open_connections()
            time.sleep(1.5)
            relay_manager.publish_message(event_profile)

        print('waiting 1 sec to send')
        time.sleep(1) # allow the messages to send

        return event_profile.signature



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
        raise PreventUpdate
    try:
        pub_key_hex = PrivateKey.from_nsec(priv_key_nsec).public_key.hex()
    except:
        Logger.error(f'strange priv key ----> {priv_key_nsec} <----')
        raise IOError(f'something wrong with priv key, check nsec format: nsec1..')
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

def get_screen(screen_name):
    """loads screen from kivy app context"""
    app = MDApp.get_running_app()
    screen = app.root.ids.screen_manager.get_screen(screen_name)
    return screen

def get_encryption_iv(msg):
    """extract the iv from an ecnrypted blob"""
    return msg.split('?iv=')[-1].strip('==')



# async def subscribe_and_fetch(manager, subscription_id, event_filter):
#     Logger.debug(f"Subscription ID: {subscription_id}")
#     Logger.debug(f"Filter: {event_filter}")

#     queue = await manager.subscribe(subscription_id, event_filter)
#     Logger.debug("Subscribed to DMs.")

#     dm_events = []
#     end_time = asyncio.get_event_loop().time() + 10.0  # 10 seconds from now
#     Logger.debug("Starting to fetch DMs with a timeout of 10 seconds.")

#     unique_ids = set()
#     while asyncio.get_event_loop().time() < end_time:
#         try:
#             event = await asyncio.wait_for(queue.get(), timeout=1.0)
#             if event.verify() and event.id not in unique_ids:
#                 dm_events.append(event.to_json_object())
#                 unique_ids.add(event.id)
#         except asyncio.TimeoutError:
#             Logger.debug("Timeout occurred while waiting for DMs.")
#             break

#     return dm_events

# async def load_dms(relay_manager_instance, pub_key_hex=None):
#     if pub_key_hex is None:
#         pub_key_hex = load_user_pub_key()
#     Logger.debug(f"Public Key Hex: {pub_key_hex}")

#     manager = relay_manager_instance.relay_manager

#     if not any(relay.connected for relay in manager.relays):
#         Logger.error("Failed to connect to any relays.")
#         return None

#     filter_1 = {
#         "authors": [pub_key_hex],
#         "kinds": [EventKind.ENCRYPTED_DIRECT_MESSAGE]}
#     filter_2 = {
#         'pubkey_refs': [pub_key_hex],
#         "kinds": [EventKind.ENCRYPTED_DIRECT_MESSAGE]}

#     subscription_id_1 = secrets.token_hex(4)
#     subscription_id_2 = secrets.token_hex(4)

#     results = await asyncio.gather(
#         subscribe_and_fetch(manager, subscription_id_1, filter_1),
#         # subscribe_and_fetch(manager, subscription_id_2, filter_2)
#     )

#     # Flatten list of results into one list and return
#     dm_events = [event for sublist in results for event in sublist]
    
#     if dm_events:
#         Logger.info(f"Total DMs fetched: {len(dm_events)}")
#         return dm_events
#     else:
#         Logger.warn("No DM events found after fetching.")
#         return None





