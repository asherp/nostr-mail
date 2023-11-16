from kivy.logger import Logger
from kivymd.uix.screen import MDScreen

import json
import keyring
from util import get_nostr_pub_key
from kivymd.app import MDApp
import aionostr
import asyncio
from sqlitedict import SqliteDict
from util import DEFAULT_RELAYS, DATABASE_PATH, KEYRING_GROUP
from util import get_current_unix_timestamp, get_priv_key_hex
from util import save_profile_to_relays
from kivy.clock import Clock
from kivy.clock import mainthread
from kivy.lang import Builder
from kivy.app import App
from threading import Thread
from util import NostrRelayManager
from ui import get_screen, Logger

Builder.load_file('profile.kv')


class ProfileScreen(MDScreen):

    def on_enter(self, *args):
        super(ProfileScreen, self).on_enter(*args)
        self.populate_profile()

    def populate_profile(self):
        # Schedule the profile data loading in the main loop
        Clock.schedule_once(lambda dt: self.load_profile_data())

    def load_profile_data(self):
        # Here, ensure that the relay_manager's load_profile_data method
        # is non-blocking or handles long operations properly
        relay_manager = MDApp.get_running_app().relay_manager
        profile_data = relay_manager.load_profile_data()  # Ensure this is non-blocking
        if profile_data:
            self.update_profile_ui(profile_data)

    @mainthread
    def update_profile_ui(self, profile_data):
        # Add logging to check the data is correct
        Logger.info('ProfileScreen: Updating profile UI with data: {}'.format(profile_data))

        # Now update the UI elements
        try:
            self.ids.display_name.text = profile_data.get('display_name', '')
            self.ids.name.text = profile_data.get('name', '')
            self.ids.picture_url.text = profile_data.get('picture', '')
            self.ids.about.text = profile_data.get('about', '')
            self.ids.email.text = profile_data.get('email', '')
            
            # Handle id and pubkey
            self.profile_id = profile_data.get('id', '')
            self.public_key = profile_data.get('pubkey', '')
        except Exception as e:
            Logger.error('ProfileScreen: Error updating UI: {}'.format(e))


    def get_profile_content(self):
        return {
            "display_name": self.ids.display_name.text,
            "name": self.ids.name.text,
            "picture": self.ids.picture_url.text,
            "about": self.ids.about.text,
            "email": self.ids.email.text
        }

    def save_profile(self):
        Logger.info('ProfileScreen: Starting to save profile data...')
        relay_manager = MDApp.get_running_app().relay_manager
        signature = relay_manager.publish_profile(self.get_profile_content())

        if signature:
            self.on_success()
        else:
            self.on_failure()

    def refresh_profile(self):
        # Logic to refresh the profile data
        self.populate_profile()


    def on_success(self):
        self.ids.status_message.text = "Profile saved successfully!"
        Logger.info('ProfileScreen: Profile saved successfully.')

    def on_failure(self):
        self.ids.status_message.text = "Failed to save profile."
        Logger.error('ProfileScreen: Failed to save profile.')


if __name__ == "__main__":
    # Initialize the Manager with the stored relays
    relay_manager = NostrRelayManager(timeout=2)

    # Run the async function in the asyncio event loop
    profile_data = relay_manager.load_profile_data()

    # Once you have profile_data, print or manipulate it as needed
    if profile_data:
        print(json.dumps(profile_data, indent=4))
    else:
        print("No profile data could be loaded.")

