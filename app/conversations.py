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

    def load_direct_messages(self):
        # Fetch the relays from the RelayScreen

        relay_manager = MDApp.get_running_app().relay_manager
        try:
            Logger.debug("ConversationsScreen: Loading DMs...")
            dms = relay_manager.get_dms()
            if len(dms) > 0:
                Logger.info("ConversationsScreen: DMs loaded successfully.")
                # Call the coroutine to decrypt and update the UI
                self.schedule_update_ui(dms)
            else:
                Logger.warning("ConversationsScreen: No DMs to load.")
        except Exception as e:
            Logger.error(f"ConversationsScreen: Error loading DMs - {e}")

    @mainthread
    def schedule_update_ui(self, dms):
        # Clear the list before updating to avoid duplication
        self.ids.dm_list.clear_widgets()
        
        # Log the count of DMs to be added
        Logger.info(f'ConversationsScreen: Updating UI with {len(dms)} DMs.')

        def conversation_id(dm):
            """construct a unique pairing of pub keys"""
            return ''.join(sorted([dm['author'], dm['p']]))

        conversations = defaultdict(list)

        # Loop through the decrypted DMs and add them to the list
        for dm in dms:
            conversations[conversation_id(dm)].append(dm)

        for convo, msgs in conversations.items():
            for msg in sorted(msgs, key=lambda m: m['time']):
                list_item = LRListItem(msg['decrypted'])
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

