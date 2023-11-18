from threading import Thread
from kivy.properties import ListProperty
from kivymd.app import MDApp
import tempfile
import keyring
from kivy.network.urlrequest import UrlRequest
import ssl
from util import RelayManager
from relays import RelayScreen
from settings import SettingsScreen
from profile import ProfileScreen
from conversations import ConversationsScreen
from kivy.clock import Clock
from util import DATABASE_PATH, DEFAULT_RELAYS
from util import KEYRING_GROUP, NostrRelayManager, SqliteDict
from ui import Logger, ErrorPopup
import os
import sys
from kivy.core.window import Window 



class MainApp(MDApp):

    def close_application(self): 
        # closing application 
        Logger.info('closing relay connections')
        self.relay_manager.close_connections()

    def build(self):
        self.theme_cls.theme_style = "Dark"

    def on_start(self):
        # Use the Kivy Clock to schedule the relay_manager initialization
        Clock.schedule_once(self.init_relay_manager, 0)

    def on_stop(self):
        # Code to safely close relay connections
        Logger.info('closing connections')
        Clock.schedule_once(self.close_application, 0)
        Logger.info('connections closed')

    def init_relay_manager(self, dt):
        # Initialize relay_manager and open connections
        self.relay_manager = NostrRelayManager.get_instance(logger=Logger)


if __name__ == "__main__":
    try:
        MainApp().run()
    except KeyboardInterrupt:
        Logger.info("Program exited by user.")

    except Exception as e:
        Logger.info(f"An error occurred: {e}")




