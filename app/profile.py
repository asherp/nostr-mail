from kivy.logger import Logger
from kivymd.uix.screen import MDScreen

import json
import keyring
from util import get_nostr_pub_key
# from aionostr.util import to_nip19
from kivymd.app import MDApp
import aionostr
import asyncio
from sqlitedict import SqliteDict
from util import DEFAULT_RELAYS, DATABASE_PATH, KEYRING_GROUP
# from aionostr.relay import Manager
# from aionostr.event import EventKind
# from aionostr.event import Event
from util import get_current_unix_timestamp, get_priv_key_hex
from util import load_profile_data, get_screen, save_profile_to_relays
from kivy.clock import Clock
from kivy.clock import mainthread
from kivy.lang import Builder
from kivy.app import App


Builder.load_file('profile.kv')


class ProfileScreen(MDScreen):
    def on_enter(self, *args):
        Logger.info('PROFILE SCREEN ENTERED')
        # Schedule the load_profile_data coroutine to be executed using the shared relay_manager
        asyncio.ensure_future(self.async_populate_profile())

    async def async_populate_profile(self):
        # Assuming MDApp.get_running_app().relay_manager is the shared relay_manager instance
        relay_manager = MDApp.get_running_app().relay_manager

        profile_data = await load_profile_data(relay_manager)
        if profile_data is not None:
            self.populate_profile(profile_data)
        else:
            Logger.error("Unable to populate profile. Profile data is None.")


    def populate_profile(self, profile_data):
        if profile_data is not None:
            # Parse the JSON string in the 'content' key of the profile data
            content = json.loads(profile_data.get('content', '{}'))  # Default to empty dict if 'content' is not found

            self.ids.display_name.text = content.get('display_name', '')
            self.ids.name.text = content.get('name', '')
            self.ids.picture_url.text = content.get('picture', '')
            self.ids.about.text = content.get('about', '')
            self.ids.email.text = content.get('email', '')

            # Handle id and pubkey
            self.profile_id = profile_data.get('id', '')
            self.public_key = profile_data.get('pubkey', '')
        else:
            Logger.error("Profile data is None, cannot populate profile.")



    def save_profile(self):
        Logger.info('ProfileScreen: Starting to save profile data...')
        asyncio.ensure_future(self.schedule_and_check())

    async def schedule_and_check(self):
        # Here you should use MDApp instead of App if you're using KivyMD
        relay_manager = MDApp.get_running_app().relay_manager.relay_manager
        success = await save_profile_to_relays(self.get_profile_content(), relay_manager)
        if success:
            self.schedule_on_success()
        else:
            self.schedule_on_failure()


    def get_profile_content(self):
        return {
            "display_name": self.ids.display_name.text,
            "name": self.ids.name.text,
            "picture": self.ids.picture_url.text,
            "about": self.ids.about.text,
            "email": self.ids.email.text
        }

    @mainthread
    def schedule_on_success(self):
        Clock.schedule_once(lambda dt: self.on_success())

    @mainthread
    def schedule_on_failure(self):
        Clock.schedule_once(lambda dt: self.on_failure())

    def on_success(self):
        self.ids.status_message.text = "Profile saved successfully!"
        Logger.info('ProfileScreen: Profile saved successfully.')

    def on_failure(self):
        self.ids.status_message.text = "Failed to save profile."
        Logger.error('ProfileScreen: Failed to save profile.')


if __name__ == "__main__":
    # Initialize the database connection to fetch the relay information
    with SqliteDict(DATABASE_PATH) as db:
        stored_relays = db.get('relays', DEFAULT_RELAYS)

    # Initialize the Manager with the stored relays
    relay_manager = Manager(relays=stored_relays)

    # Run the async function in the asyncio event loop
    async def main():
        # Start the manager (attempt to connect all relays)
        await relay_manager.connect()

        # Check if connected to any relays
        if not any(relay.connected for relay in relay_manager.relays):
            print("Failed to connect to any relays.")
            return

        try:
            # Once connected, run the load_profile_data function with the relay_manager
            profile_data = await load_profile_data(relay_manager)

            # Once you have profile_data, print or manipulate it as needed
            if profile_data:
                print(json.dumps(profile_data, indent=4))
            else:
                print("No profile data could be loaded.")
        finally:
            # Disconnect all relays
            await relay_manager.close()

    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("Program exited by user.")

