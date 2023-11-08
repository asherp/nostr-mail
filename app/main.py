from threading import Thread
from kivy.properties import ListProperty
from kivymd.app import MDApp
import tempfile
import keyring
from kivy.network.urlrequest import UrlRequest

from relays import RelayScreen
from settings import SettingsScreen
from compose import ComposeScreen
from profile import ProfileScreen
from conversations import ConversationsScreen
from contacts import ContactsScreen
import asyncio
from kivy.clock import Clock

from util import KEYRING_GROUP, Logger, NostrRelayManager
import os



class Main(MDApp):

    def build(self):
        self.theme_cls.theme_style = "Dark"
        self.relay_manager = NostrRelayManager.get_instance()
        # This line schedules the connect method to run as soon as the event loop starts.
        Clock.schedule_once(lambda dt: asyncio.create_task(self.connect_to_relays()))

    async def connect_to_relays(self):
        # Ensure that the relay_manager has been initialized
        if self.relay_manager.relay_manager:
            # Attempt to connect to each relay
            await asyncio.gather(*(relay.connect() for relay in self.relay_manager.relay_manager.relays))
            # Check if all relays are connected
            self.relay_manager.connected = all(relay.connected for relay in self.relay_manager.relay_manager.relays)
        else:
            Logger.error("Relay manager not initialized.")

if __name__ == "__main__":
    try:
        asyncio.run(Main().async_run())
    except KeyboardInterrupt:
        print("Program exited by user.")


