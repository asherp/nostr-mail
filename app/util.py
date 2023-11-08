from aionostr.util import PrivateKey, PublicKey
from kivy.logger import Logger
import sqlite3
import keyring
from aionostr.relay import Manager
from aionostr.event import EventKind
from aionostr.event import Event
import datetime
from kivymd.app import MDApp
import json
import re
import secrets
import os
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
        else:
            self.relay_manager = None
            self.connected = False
            NostrRelayManager._instance = self
            self.init_manager()

    async def connect(self):
        # Assuming each relay in the manager has an async 'connect' method
        if self.relay_manager:
            # Connect to each relay managed by the relay_manager
            await asyncio.gather(*(relay.connect() for relay in self.relay_manager.relays))
            self.connected = all(relay.connected for relay in self.relay_manager.relays)
        else:
            Logger.error("Relay manager not initialized.")

    def init_manager(self):
        # Load relays from the database
        self.relay_manager = self.load_relays_from_db()

    def load_relays_from_db(self):
        with SqliteDict(DATABASE_PATH, tablename='relays') as db:
            relays = db.get('relays', DEFAULT_RELAYS)
        return Manager(relays=relays)


def load_user_pub_key():
    priv_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key:
        Logger.error("Private key not found in keyring.")
        raise IOError("Expected private key in keyring")
    pub_key_hex = get_nostr_pub_key(priv_key)
    return pub_key_hex



async def load_profile_data(relay_manager_instance, pub_key_hex=None):
    # Access the Manager instance with the relays from the relay_manager attribute
    manager = relay_manager_instance.relay_manager

    if not any(relay.connected for relay in manager.relays):
        Logger.error("Failed to connect to any relays.")
        return None

    if pub_key_hex is None:
        pub_key_hex = load_user_pub_key()

    # Construct the profile query using EventKind for metadata
    filter_ = {"authors": [pub_key_hex], "kinds": [EventKind.SET_METADATA]}
    subscription_id = secrets.token_hex(4)  # Generate a unique ID for the subscription

    try:
        # Use the manager instance to subscribe to the relays with the filter
        queue = await manager.subscribe(subscription_id, filter_)

        # Use the manager instance to get profile events
        event = await queue.get()
        if event:
            profile_dict = event.to_json_object()
            return profile_dict

    except Exception as e:
        Logger.error(f"Error querying for profile: {e}")

    finally:
        # Ensure to unsubscribe before closing to clean up properly
        await manager.unsubscribe(subscription_id)
        # No need to disconnect if using a shared manager, unless it's the end of its lifecycle

    Logger.warn("No profile events found.")
    return None



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


async def save_profile_to_relays(content, relay_manager):
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
        response = await relay_manager.add_event(profile_event, check_response=True)
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


async def load_dms(relay_manager_instance, pub_key_hex=None):
    if pub_key_hex is None:
        pub_key_hex = load_user_pub_key()
    Logger.debug(f"Public Key Hex: {pub_key_hex}")

    manager = relay_manager_instance.relay_manager

    if not any(relay.connected for relay in manager.relays):
        Logger.error("Failed to connect to any relays.")
        return None

    filter_ = {"authors": [pub_key_hex], "kinds": [EventKind.ENCRYPTED_DIRECT_MESSAGE]}
    subscription_id = secrets.token_hex(4)
    Logger.debug(f"Subscription ID: {subscription_id}")
    Logger.debug(f"Filter: {filter_}")

    try:
        queue = await manager.subscribe(subscription_id, filter_)
        Logger.debug("Subscribed to DMs.")

        # List to hold all DM events
        dm_events = []
        # Set a timeout for DM fetching to avoid infinite waiting
        end_time = asyncio.get_event_loop().time() + 10.0  # 10 seconds from now
        Logger.debug(f"Starting to fetch DMs with a timeout of 10 seconds.")

        # Fetch all available DMs until timeout
        while asyncio.get_event_loop().time() < end_time:
            try:
                # Wait for a DM with a 1-second timeout
                event = await asyncio.wait_for(queue.get(), timeout=1.0)
                dm_events.append(event)
            except asyncio.TimeoutError:
                Logger.debug("Timeout occurred while waiting for DMs.")
                break

        if dm_events:
            Logger.info(f"Total DMs fetched: {len(dm_events)}")
            return dm_events
        else:
            Logger.warn("No DM events found after fetching.")
            return None

    except Exception as e:
        Logger.error(f"Error querying for DMs: {e}")
        import traceback
        Logger.error(traceback.format_exc())

    finally:
        Logger.debug("Unsubscribing from DMs.")
        await manager.unsubscribe(subscription_id)
        # No need to close the manager if it's shared and will be used later on.

    Logger.warn("No DM events found at the end of the function.")
    return None



