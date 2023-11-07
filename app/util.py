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


KEYRING_GROUP = 'nostrmail'
DATABASE_PATH = 'nostrmail.sqlite'
DEFAULT_RELAYS = [
    "wss://nostr-pub.wellorder.net",
    "wss://relay.damus.io",
    'wss://brb.io',
    'wss://nostr.mom']


def load_user_pub_key():
    priv_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key:
        Logger.error("Private key not found in keyring.")
        raise IOError("Expected private key in keyring")
    pub_key_hex = get_nostr_pub_key(priv_key)
    return pub_key_hex


async def load_profile_data(relays, pub_key_hex=None):
    if pub_key_hex is None:
        pub_key_hex = load_user_pub_key()

    # Initialize the Manager with the list of relays
    manager = Manager(relays=relays)

    # Start the manager (attempt to connect all relays)
    await manager.connect()

    # Check if connected to any relays
    if not any(relay.connected for relay in manager.relays):
        Logger.error("Failed to connect to any relays.")
        return None

    # Construct the profile query using EventKind for metadata
    filter_ = {"authors": [pub_key_hex], "kinds": [EventKind.SET_METADATA]}
    subscription_id = "some_random_str"  # Generate a unique ID for the subscription

    try:
        # Use the manager to subscribe to the relays with the filter
        queue = await manager.subscribe(subscription_id, filter_)

        # Use the manager to get profile events
        event = await queue.get()
        if event:
            profile_dict = event.to_json_object()
            return profile_dict

    except Exception as e:
        Logger.error(f"Error querying for profile: {e}")

    finally:
        # Ensure to unsubscribe before closing to clean up properly
        await manager.unsubscribe(subscription_id)
        # Disconnect all relays
        await manager.close()

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


async def save_profile_to_relays(content):
    # Collect the data from the input fields
    
    # Convert the dictionary to a JSON string
    content_json = json.dumps(content)
    
    # Fetch the user's private key in nsec format
    priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key_nsec:
        Logger.error("Private key not found in keyring.")
        return
    
    pub_key = get_nostr_pub_key(priv_key_nsec)
    priv_key_hex = get_priv_key_hex(priv_key_nsec)

    profile_event = Event(
        pubkey=pub_key,
        created_at=get_current_unix_timestamp(),  # or simply use `int(time.time())`
        kind=EventKind.SET_METADATA,  # Replace with the correct kind number for a profile event
        tags=[],  # Add any tags if necessary, for example, [['p', 'profile']]
        content=content_json
    )

    # Sign the event with the user's private key
    profile_event.sign(priv_key_hex)

    profile_screen = get_screen('profile_screen')

    # Fetch the relays from the RelayScreen
    relay_screen = get_screen('relay_screen')

    relays = relay_screen.relays

    # Initialize the Manager with the list of relays
    manager = Manager(relays=relays)
    
    await manager.connect()

    try:
        response = await manager.add_event(profile_event, check_response=True)
        success_count = parse_responses(response)
        Logger.info(f'response: {response}')
        if success_count > 0:
            Logger.info(f"Profile event published to {success_count} relay(s) with event id {profile_event.id}.")
            return True
        else:
            Logger.error(f"Failed to publish profile event to any relay. Response: {response}")
            return False
    except Exception as e:
        Logger.error(f"Failed to publish profile event: {e}")
        return False
    finally:
        await manager.close()

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


