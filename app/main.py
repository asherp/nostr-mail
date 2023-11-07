from threading import Thread
from kivy.properties import ListProperty
from kivy.logger import Logger
from kivymd.app import MDApp
import tempfile
import keyring
from kivy.network.urlrequest import UrlRequest

from relays import RelayScreen
from settings import SettingsScreen
from compose import ComposeScreen
from profile import ProfileScreen
from contacts import ContactsScreen
import asyncio
from kivy.clock import Clock
from util import KEYRING_GROUP
from kivy.lang import Builder
import os
from kivy.config import Config

# Set the directory where you want to store the log files
log_directory = os.path.join(os.path.dirname(__file__), 'logs')
if not os.path.exists(log_directory):
    os.makedirs(log_directory)

# Configure the Kivy logger
Config.set('kivy', 'log_dir', log_directory)
Config.set('kivy', 'log_name', 'kivy_%y-%m-%d_%_.txt')


Builder.load_file('relay.kv')
Builder.load_file('profile.kv')
Builder.load_file('settings.kv')
Builder.load_file('contacts.kv')
Builder.load_file('compose.kv')

class Main(MDApp):

    def build(self):
        self.theme_cls.theme_style = "Dark"


if __name__ == "__main__":
    try:
        asyncio.run(Main().async_run())
    except KeyboardInterrupt:
        print("Program exited by user.")

