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
import secrets
from datetime import datetime
from util import load_profile_data



class ProfileScreen(MDScreen):
    def on_enter(self, *args):
        Logger.info('PROFILE SCREEN ENTERED')
        # Schedule the load_profile_data coroutine to be executed.
        asyncio.ensure_future(self.async_populate_profile())

    async def async_populate_profile(self):
        app = MDApp.get_running_app()
        relay_screen = app.root.ids.screen_manager.get_screen('relay_screen')

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
        # This method will be called by the UI
        Logger.info('Saving profile data...')
        asyncio.ensure_future(self.save_profile_to_relays())


    async def save_profile_to_relays(self):
        # Collect the data from the input fields
        content = {
            "display_name": self.ids.display_name.text,
            "name": self.ids.name.text,
            "picture": self.ids.picture_url.text,
            "about": self.ids.about.text,
            "email": self.ids.email.text
        }
        
        # Convert the dictionary to a JSON string
        content_json = json.dumps(content)
        
        # Fetch the user's private key
        private_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
        if not private_key:
            Logger.error("Private key not found in keyring.")
            return
        
        # Create the profile event using the Event class
        profile_event = Event(
            pubkey=self.public_key,
            created_at=get_current_unix_timestamp(),  # or simply use `int(time.time())`
            kind=EventKind.SET_METADATA,  # Replace with the correct kind number for a profile event
            tags=[],  # Add any tags if necessary, for example, [['p', 'profile']]
            content=content_json
        )

        # Sign the event with the user's private key
        profile_event.sign(private_key)

        # Fetch the relays from the RelayScreen
        app = MDApp.get_running_app()
        relay_screen = app.root.ids.screen_manager.get_screen('relay_screen')
        relays = relay_screen.relays
        
        # Create a relay pool
        relay_pool = app.root.ids.relay_manager  # Assuming you have a relay manager in your app root
        
        # Start the relay pool
        await relay_pool.start()

        # Post the event to the relay pool
        try:
            response = await relay_pool.add_event(profile_event, check_response=True)
            # Check if the response is successful
            if response and response[1] == profile_event.id:  # assuming successful response returns the event ID
                Logger.info(f"Profile event published to relays with event id {profile_event.id}.")
            else:
                Logger.error(f"Failed to publish profile event. Response: {response}")
        except Exception as e:
            Logger.error(f"Failed to publish profile event: {e}")
        finally:
            # Stop the relay pool once done
            await relay_pool.stop()






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
