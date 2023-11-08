from kivymd.uix.screen import MDScreen
from util import Logger
from kivy.clock import Clock
import asyncio
from kivymd.app import MDApp
from util import load_dms, DEFAULT_RELAYS, Logger, get_screen, KEYRING_GROUP
from kivy.lang import Builder
from kivymd.uix.list import OneLineListItem
from aionostr.util import PrivateKey
from kivy.clock import mainthread
import keyring


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
            dms = await load_dms(relay_manager, pub_key_hex=None)
            if dms:
                Logger.info("ConversationsScreen: DMs loaded successfully.")
                # Call the coroutine to decrypt and update the UI
                await self.update_ui_with_dms(dms)
            else:
                Logger.warning("ConversationsScreen: No DMs to load.")
        except Exception as e:
            Logger.error(f"ConversationsScreen: Error loading DMs - {e}")

    async def update_ui_with_dms(self, encrypted_dms):
        # Fetch the user's private key
        priv_key_nsec = keyring.get_password(KEYRING_GROUP, 'priv_key')
        if not priv_key_nsec:
            Logger.error("Private key not found in keyring.")
            return
        priv_key = PrivateKey.from_nsec(priv_key_nsec)

        # Decrypt the DMs
        decrypted_dms = []
        for dm in encrypted_dms:
            try: # priv_key.decrypt_message(encrypted_message, pub_key_hex)
                decrypted_content = priv_key.decrypt_message(dm['content'], dm['pubkey'])
                dm['decrypted_content'] = decrypted_content
                Logger.info(f'decrypted content: {decrypted_content}')
                decrypted_dms.append(dm.copy())
            except Exception as e:
                Logger.error(f"Error decrypting message from {dm['pubkey']}: {e}")

        # Call the UI update function on the main thread
        self.schedule_update_ui(decrypted_dms)

    @mainthread
    def schedule_update_ui(self, decrypted_dms):
        # Clear the list before updating to avoid duplication
        self.ids.dm_list.clear_widgets()
        
        # Log the count of DMs to be added
        Logger.info(f'ConversationsScreen: Updating UI with {len(decrypted_dms)} DMs.')

        # Loop through the decrypted DMs and add them to the list
        for dm in decrypted_dms:
            list_item = self.create_list_item(dm)
            self.ids.dm_list.add_widget(list_item)
        Logger.info('ConversationsScreen: UI updated with decrypted DMs.')

    def create_list_item(self, dm):
        # This method creates and returns a List Item for a DM
        item = OneLineListItem(text=dm['decrypted_content'])
        Logger.debug(f"ConversationsScreen: Created list item with content: {dm['decrypted_content']}")
        return item

async def test_load_dms():
    Logger.setLevel('DEBUG')
    dms = await load_dms(DEFAULT_RELAYS)
    Logger.info(dms)


if __name__ == '__main__':
    try:
        asyncio.run(test_load_dms())
    except KeyboardInterrupt:
        print("Program exited by user.")

