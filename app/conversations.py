from kivymd.uix.screen import MDScreen
from ui import Logger
from kivy.clock import Clock
from kivymd.app import MDApp
from util import DEFAULT_RELAYS, Logger, KEYRING_GROUP, get_convs
from ui import get_screen
from kivy.lang import Builder
from kivymd.uix.list import OneLineListItem
from nostr.key import PrivateKey, PublicKey
from kivy.clock import mainthread
import keyring
from util import load_user_pub_key, get_nostr_pub_key, NostrRelayManager
from util import fetch_profile_from_db, save_profile_to_db
import json
from collections import defaultdict
from kivy.uix.anchorlayout import AnchorLayout
from widgets import LRListItem


Builder.load_file('conversations.kv')


class ConversationsScreen(MDScreen):

    def on_enter(self):
        Logger.info("ConversationsScreen: Entered the conversations screen.")
        # load DMs when the screen is entered
        self.load_direct_messages()

    def load_direct_messages(self, refresh=False):
        # fetch dms from database
        relay_manager = MDApp.get_running_app().relay_manager
        try:
            Logger.debug("ConversationsScreen: Loading DMs...")
            dms = relay_manager.get_dms(refresh=refresh)

            if len(dms) > 0:
                Logger.info("ConversationsScreen: DMs loaded successfully.")
                self.schedule_update_ui(dms)
            else:
                Logger.warning("ConversationsScreen: No DMs to load.")
        except Exception as e:
            Logger.error(f"ConversationsScreen: Error loading DMs - {e}")

    def on_refresh_press(self):
        # Your refresh logic here
        Logger.info("Conversations refresh button pressed")
        # For example, you might call the method that updates the UI:
        self.load_direct_messages(refresh=True)


    def load_profile(self, pub_key_hex):
        profile_data = fetch_profile_from_db(pub_key_hex)
        if profile_data is None:
            relay_manager = MDApp.get_running_app().relay_manager
            profile_data = relay_manager.fetch_profile_data(pub_key_hex=pub_key_hex, kind='profile')
            save_profile_to_db(pub_key_hex=pub_key_hex, profile_data=profile_data)
        return profile_data


    @mainthread
    def schedule_update_ui(self, dms):
        # Clear the list before updating to avoid duplication
        self.ids.dm_list.clear_widgets()
        
        # Log the count of DMs to be added
        Logger.info(f'ConversationsScreen: Updating UI with {len(dms)} convos.')
        pub_key_hex = load_user_pub_key()

        # Loop through the decrypted DMs and add them to the list
        for conv_id, msgs in dms.items():
            msg_list = list(msgs.items()) # iv: {**msg} -> (iv, {**msg})
            for iv, msg in sorted(msg_list, key=lambda m: m[1]['time']):
                author = msg['from_pubkey']
                profile_data = self.load_profile(author)
                if profile_data is not None:
                    image_source = profile_data['picture']
                if msg['from_pubkey'] == pub_key_hex:
                    list_item = LRListItem(msg['decrypted'], image_source=image_source, item_type='right')
                else:
                    list_item = LRListItem(msg['decrypted'], image_source=image_source, item_type='left')
                self.ids.dm_list.add_widget(list_item)

        Logger.info('ConversationsScreen: UI updated with decrypted DMs.')


def test_load_dms():
    Logger.setLevel('DEBUG')

    priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key_nsec:
        Logger.error("Private key not found in keyring.")
        return

    priv_key = PrivateKey.from_nsec(priv_key_nsec)
    pub_key_hex = priv_key.public_key.hex()

    relay_manager = NostrRelayManager()
    relay_manager.connect()

    dms = load_dms(relay_manager)
    
    for dm in dms:
        Logger.info(dm['created_at'])
        for k,v in dm.items():
            if k != 'created_at':
                Logger.info(f'\t{k}:{v}')

if __name__ == '__main__':
    try:
        test_load_dms()
    except KeyboardInterrupt:
        print("Program exited by user.")

