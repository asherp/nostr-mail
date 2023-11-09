from kivymd.uix.screen import MDScreen
from util import Logger
from kivy.clock import Clock
import asyncio
from kivymd.app import MDApp
from util import load_dms, DEFAULT_RELAYS, Logger, get_screen, KEYRING_GROUP
from kivy.lang import Builder
from kivymd.uix.list import OneLineListItem
from nostr.key import PrivateKey, PublicKey
from kivy.clock import mainthread
import keyring
from util import load_user_pub_key, get_nostr_pub_key, NostrRelayManager
import json


Builder.load_file('conversations.kv')

class ConversationsScreen(MDScreen):

    def on_enter(self):
        Logger.info("ConversationsScreen: Entered the conversations screen.")
        # Asynchronously load DMs when the screen is entered
        asyncio.ensure_future(self.load_direct_messages())


    async def load_direct_messages(self):
        # Fetch the relays from the RelayScreen

        relay_manager = MDApp.get_running_app().relay_manager
        try:
            Logger.debug("ConversationsScreen: Loading DMs...")
            dm_events = await load_dms(relay_manager, pub_key_hex=None)
            if dm_events:
                Logger.info("ConversationsScreen: DMs loaded successfully.")
                # Call the coroutine to decrypt and update the UI
                await self.update_ui_with_dms(dm_events)
            else:
                Logger.warning("ConversationsScreen: No DMs to load.")
        except Exception as e:
            Logger.error(f"ConversationsScreen: Error loading DMs - {e}")

    async def update_ui_with_dms(self, dm_events):
        # Fetch the user's private key
        priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
        if not priv_key_nsec:
            Logger.error("Private key not found in keyring.")
            return
        priv_key = PrivateKey.from_nsec(priv_key_nsec)
        pub_key_hex = priv_key.public_key.hex()
        # Decrypt the DMs
        dms = []
        for e in dm_events:
            if e is None:
                continue
            else:
                dm = dict(
                    valid=True,
                    created_at=e['created_at'],
                    event_id=e['id'],
                    author=e['pubkey'],
                    content=e['content'],
                    **dict(e['tags']))
            try:
                decrypted = priv_key.decrypt_message(dm['content'], dm['p'])
            except Exception as e:
                try:
                    decrypted = priv_key.decrypt_message(dm['content'], dm['author'])
                except Exception as e:
                    decrypted = json.dumps(dm, indent=4)
                dm['decrypted'] = decrypted
            dms.append(dm)

        # Call the UI update function on the main thread
        self.schedule_update_ui(dms)

    @mainthread
    def schedule_update_ui(self, dms):
        # Clear the list before updating to avoid duplication
        self.ids.dm_list.clear_widgets()
        
        # Log the count of DMs to be added
        Logger.info(f'ConversationsScreen: Updating UI with {len(dms)} DMs.')

        # Loop through the decrypted DMs and add them to the list
        for dm in dms:
            list_item = self.create_list_item(dm['decrypted'])
            self.ids.dm_list.add_widget(list_item)
        Logger.info('ConversationsScreen: UI updated with decrypted DMs.')

    def create_list_item(self, dm):
        # This method creates and returns a List Item for a DM
        item = OneLineListItem(text=dm)
        Logger.debug(f"ConversationsScreen: Created list item with content: {dm}")
        return item

async def test_load_dms():
    Logger.setLevel('DEBUG')

    priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key_nsec:
        Logger.error("Private key not found in keyring.")
        return

    priv_key = PrivateKey.from_nsec(priv_key_nsec)
    pub_key_hex = priv_key.public_key.hex()

    relay_manager = NostrRelayManager()
    await relay_manager.connect()

    dms = await load_dms(relay_manager)
    
    for dm in dms:
        Logger.info(dm['created_at'])
        for k,v in dm.items():
            if k != 'created_at':
                Logger.info(f'\t{k}:{v}')

if __name__ == '__main__':
    try:
        asyncio.run(test_load_dms())
    except KeyboardInterrupt:
        print("Program exited by user.")

