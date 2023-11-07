from kivy.logger import Logger
from kivymd.uix.screen import MDScreen

import json
import keyring
from util import get_nostr_pub_key
from aionostr.util import to_nip19
from kivymd.app import MDApp
import aionostr
import asyncio
from sqlitedict import SqliteDict
from util import DEFAULT_RELAYS, DATABASE_PATH, KEYRING_GROUP
from aionostr.relay import Manager
from aionostr.event import EventKind
from aionostr.event import Event
from util import get_current_unix_timestamp, get_priv_key_hex
from util import load_profile_data, get_screen, save_profile_to_relays
from kivy.clock import Clock


class ProfileScreen(MDScreen):
    def on_enter(self, *args):
        Logger.info('PROFILE SCREEN ENTERED')
        # Schedule the load_profile_data coroutine to be executed.
        asyncio.ensure_future(self.async_populate_profile())

    async def async_populate_profile(self):
        relay_screen = get_screen('relay_screen')

        profile_data = await load_profile_data(relays=relay_screen.relays)

        self.populate_profile(profile_data)


    def populate_profile(self, profile_data):
        # Parse the JSON string in the 'content' key of the profile data
        content = json.loads(profile_data['content'])

        self.ids.display_name.text = content.get('display_name', '')
        self.ids.name.text = content.get('name', '')
        self.ids.picture_url.text = content.get('picture', '')
        self.ids.about.text = content.get('about', '')
        self.ids.email.text = content.get('email', '')

        # You might also want to store the id and pubkey somewhere or use it in some way
        # For example:
        self.profile_id = profile_data.get('id', '')
        self.public_key = profile_data.get('pubkey', '')


    def save_profile(self):
        # Extract the profile data from the UI elements
        content = {
            "display_name": self.ids.display_name.text,
            "name": self.ids.name.text,
            "picture": self.ids.picture_url.text,
            "about": self.ids.about.text,
            "email": self.ids.email.text
        }

        # Log the initiation of profile save
        Logger.info('ProfileScreen: Starting to save profile data...')

        async def schedule_and_check():
            success = await save_profile_to_relays(content)
            if success:
                Clock.schedule_once(lambda dt: self.on_success())
            else:
                Clock.schedule_once(lambda dt: self.on_failure())


        # Start the asynchronous operation
        asyncio.ensure_future(schedule_and_check())

    def on_success(self):
        # Update the status message on the UI
        self.ids.status_message.text = "Profile saved successfully!"
        # Log the success message
        Logger.info('ProfileScreen: on_success triggered.')

    def on_failure(self):
        # Update the status message on the UI
        self.ids.status_message.text = "Failed to save profile."
        # Log the failure message
        Logger.error('ProfileScreen: on_failure triggered.')


if __name__ == "__main__":
    # Run the async function in the asyncio event loop
    try:
        profile_data = asyncio.run(load_profile_data(relays=DEFAULT_RELAYS))
    except KeyboardInterrupt:
        print("Program exited by user.")

    # Once you have profile_data, you can print or manipulate it as needed
    if profile_data:
        print(json.dumps(profile_data, indent=4))
    else:
        print("No profile data could be loaded.")
