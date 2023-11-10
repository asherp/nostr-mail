from threading import Thread
from kivy.properties import ListProperty
from kivymd.app import MDApp
import tempfile
import keyring
from kivy.network.urlrequest import UrlRequest
import ssl
from relays import RelayScreen
from settings import SettingsScreen
from profile import ProfileScreen
from conversations import ConversationsScreen
from kivy.clock import Clock

from util import KEYRING_GROUP, Logger, NostrRelayManager
import os


class Main(MDApp):
    def build(self):
        self.theme_cls.theme_style = "Dark"

    def on_start(self):
        # Use the Kivy Clock to schedule the relay_manager initialization
        Clock.schedule_once(self.init_relay_manager, 0)

    def init_relay_manager(self, dt):
        # Initialize relay_manager and open connections
        self.relay_manager = NostrRelayManager.get_instance()
        self.relay_manager.manager.open_connections()

if __name__ == "__main__":
    try:
        Main().run()
    except KeyboardInterrupt:
        print("Program exited by user.")


