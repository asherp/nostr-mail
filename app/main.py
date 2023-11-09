from threading import Thread
from kivy.properties import ListProperty
from kivymd.app import MDApp
import tempfile
import keyring
from kivy.network.urlrequest import UrlRequest

from relays import RelayScreen
from settings import SettingsScreen
# from compose import ComposeScreen
# from profile import ProfileScreen
# from conversations import ConversationsScreen
# from contacts import ContactsScreen
# import asyncio
from kivy.clock import Clock

from util import KEYRING_GROUP, Logger, NostrRelayManager
import os


class Main(MDApp):
    def build(self):
        self.theme_cls.theme_style = "Dark"
        self.relay_manager = NostrRelayManager.get_instance()


if __name__ == "__main__":
    try:
        Main().run()
    except KeyboardInterrupt:
        print("Program exited by user.")


