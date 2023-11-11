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
from kivy.uix.popup import Popup
from kivy.uix.label import Label
from kivy.uix.button import Button
from kivy.uix.boxlayout import BoxLayout
from kivy.cache import Cache
from contextlib import contextmanager

Cache.register('text', limit=100, timeout=60)
Cache.register('meta', limit=100, timeout=60)
Cache.register('dm', limit=100, timeout=60)


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


class ErrorPopup(Popup):
    def __init__(self, error_message, **kwargs):
        super().__init__(**kwargs)
        self.title = 'An Error Occurred'
        self.size_hint = (0.5, 0.5)

        layout = BoxLayout(orientation='vertical')
        layout.add_widget(Label(text=error_message))

        close_button = Button(text='Close')
        close_button.bind(on_press=self.dismiss)
        layout.add_widget(close_button)

        self.content = layout
        self.bind(on_dismiss=self.on_close)

    def on_close(self, instance):
        MDApp.get_running_app().stop()


class NostrRelayManager(RelayManager):
    _instance = None

    @classmethod
    def get_instance(cls):
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def __init__(self):
        if self._instance is not None:
            raise Exception("This class is a singleton!")
        super().__init__()  # Initialize parent class
        NostrRelayManager._instance = self
        self.init_manager()

    def init_manager(self):
        relays = self.load_relays_from_db()
        Logger.info('connecting relays')
        for relay_url in relays:
            Logger.info(f'adding relay: {relay_url}')
            self.add_relay(relay_url)  # Call method from RelayManager
        Logger.info(f'relays connected: {self.relays}')  # Access attribute from RelayManager


    def load_relays_from_db(self):
        with SqliteDict(DATABASE_PATH, tablename='relays') as db:
            relays = db.get('relays', DEFAULT_RELAYS)
        return relays


    def add_subscription(self, id, filters: Filters):
        super().add_subscription(id, filters)
        Logger.info(f"Subscription added with ID {id}")


    def close_subscription(self, id: str):
        try:
            super().close_subscription(id)
            Logger.info(f"Subscription closed with ID {id}")
        except KeyError:
            Logger.warning(f"Attempted to close non-existing subscription with ID {id}")

    @contextmanager
    def temporary_subscription(self, filters):
        subscription_id = secrets.token_hex(4)
        self.add_subscription(subscription_id, filters)
        try:
            yield subscription_id
        finally:
            self.close_subscription(subscription_id)

    def publish_message(self, message):
        try:
            super().publish_message(message)
            Logger.info(f"Message published: {message}")
        except WebSocketConnectionClosedException as e:
            Logger.warning(f"WebSocket connection closed: {e}")
            self.open_connections()
        except Exception as e:
            Logger.error(f"Error in publishing message: {e}")

    def get_events(self, pub_key_hex, kind='text', returns='content'):
        """fetch events of any kind for pub_key_hex"""

        # Check if the events are already in the cache
        # if so, return them
        # events = Cache.get(kind, pub_key_hex)
        # if events is not None:
        #     return events

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
        
        with self.temporary_subscription(filters) as subscription_id:
            Logger.info(f"Temporary subscription created with ID {subscription_id}")

            request = [ClientMessageType.REQUEST, subscription_id]
            request.extend(filters.to_json_array())
            message = json.dumps(request)

            try:
                self.publish_message(message)
            except WebSocketConnectionClosedException:
                Logger.warning('connection was closed, reopening..')
                self.open_connections()
                time.sleep(1.5)
                self.publish_message(message)

            time.sleep(1) # allow the messages to send
            Logger.info(f'message should have sent {kind}')
            Logger.info(f'found events {self.message_pool.has_events()}')

            events = []
            while self.message_pool.has_events():
                event_msg = self.message_pool.get_event()
                Logger.info(f"Processing event: {event_msg}")
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

            Cache.append(kind, pub_key_hex, events)
            Logger.info(f"Events fetched for {pub_key_hex}: {events}")
            return events

    def close_all_subscriptions(self):
        for subscription_id in list(self.relays.keys()):
            try:
                self.close_subscription(subscription_id)
            except KeyError as e:
                Logger.error(f"Failed to close subscription {subscription_id}: {e}")

    def __del__(self):
        self.close_all_subscriptions()
        self.close_connections()

    def wait_and_publish_message(self, message):
        try:
            self.publish_message(message)
        except WebSocketConnectionClosedException as e:
            Logger.error(f"Failed to publish message: {e}")

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
            self.publish_message(event_profile.to_message())
        except WebSocketConnectionClosedException:
            Logger.warning('connection was closed, reopening..')
            self.open_connections()
            time.sleep(1.5)
            self.publish_message(event_profile)

        print('waiting 1 sec to send')
        time.sleep(1) # allow the messages to send

        return event_profile.signature

    def get_dms(self):
        """Get all dms for this pub key
        Returns list of dict objects storing metadata for each dm
        Note: if a dm signature does not pass, the event is markded with valid=False
        """
        priv_key = load_user_priv_key()
        pub_key_hex = priv_key.public_key.hex()

        dms = []
        dm_events = self.get_events(pub_key_hex, kind='dm', returns='event')
        for e in dm_events:
            # check signature first
            if not e.verify():
                continue
            else:
                dm = dict(
                    valid=True,
                    time=e.created_at,
                    event_id=e.id,
                    author=e.public_key,
                    content=e.content,
                    **dict(e.tags))
                dm['decrypted'] = 'could not decrypt'
                if dm['author'] == pub_key_hex: # sent from the user
                    dm['decrypted'] = priv_key.decrypt_message(dm['content'], dm['p'])
                else: # sent to the user
                    dm['decrypted'] = priv_key.decrypt_message(dm['content'], dm['author'])
            dms.append(dm)
        return dms

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

def get_screen(screen_name):
    """loads screen from kivy app context"""
    app = MDApp.get_running_app()
    screen = app.root.ids.screen_manager.get_screen(screen_name)
    return screen

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


