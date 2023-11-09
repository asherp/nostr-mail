from threading import Thread
from kivy.properties import ListProperty
from kivymd.app import MDApp
import tempfile
import keyring
from kivy.network.urlrequest import UrlRequest

from relays import RelayScreen
from settings import SettingsScreen
from kivy.clock import Clock

from util import KEYRING_GROUP, Logger, NostrRelayManager
import os


class Main(MDApp):
    def build(self):
        self.theme_cls.theme_style = "Dark"
        # Don't initialize relay_manager here

    def on_start(self):
        # Initialize relay_manager when the app starts, ensuring everything is set up
        self.relay_manager = NostrRelayManager.get_instance()

if __name__ == "__main__":
    try:
        Main().run()
    except KeyboardInterrupt:
        print("Program exited by user.")


