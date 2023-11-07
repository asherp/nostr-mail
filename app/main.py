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



class Main(MDApp):

    def build(self):
        self.theme_cls.theme_style = "Dark"
        Clock.schedule_once(self.schedule_async_data)

    def schedule_async_data(self, *args):
        # Access the profile screen using the screen manager
        profile_screen = self.root.ids.screen_manager.get_screen('profile_screen')
        # Schedule the async_populate_profile coroutine to be executed.
        asyncio.create_task(profile_screen.async_populate_profile())


if __name__ == "__main__":
    try:
        asyncio.run(Main().async_run())
    except KeyboardInterrupt:
        print("Program exited by user.")

