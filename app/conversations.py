from kivymd.uix.screen import MDScreen
from util import Logger
from kivy.clock import Clock
import asyncio

from util import load_dms, DEFAULT_RELAYS, Logger, get_screen
from kivy.lang import Builder


Builder.load_file('conversations.kv')

class ConversationsScreen(MDScreen):

    def on_enter(self):
        Logger.info("ConversationsScreen: Entered the conversations screen.")
        # Asynchronously load DMs when the screen is entered
        asyncio.ensure_future(self.load_direct_messages())

    async def load_direct_messages(self):
        # Fetch the relays from the RelayScreen
        relay_screen = get_screen('relay_screen')
        relays = relay_screen.relays
        try:
            Logger.debug("ConversationsScreen: Loading DMs...")
            dms = await load_dms(relays=relays, pub_key_hex=None)
            if dms:
                Logger.info("ConversationsScreen: DMs loaded successfully.")
                # Schedule the update of the DMs on the main thread
                Clock.schedule_once(lambda dt: self.update_ui_with_dms(dms))
            else:
                Logger.warning("ConversationsScreen: No DMs to load.")
        except Exception as e:
            Logger.error(f"ConversationsScreen: Error loading DMs - {e}")

    def update_ui_with_dms(self, dms):
        # Update the UI with the loaded DMs
        Logger.debug("ConversationsScreen: Updating UI with DMs.")
        dm_list = self.ids.dm_list
        dm_list.clear_widgets()
        for dm in dms:
            item = self.create_list_item(dm)
            dm_list.add_widget(item)

    def create_list_item(self, dm):
        # This method creates a List Item for a DM
        from kivymd.uix.list import OneLineListItem
        item = OneLineListItem(text=dm['content'])
        Logger.debug(f"ConversationsScreen: Created list item with content: {dm['content']}")
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

