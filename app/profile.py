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
from aionostr.relay import RelayPool



class ProfileScreen(MDScreen):
    def on_enter(self, *args):
        Logger.info('PROFILE SCREEN ENTERED')
        # Schedule the load_profile_data coroutine to be executed.
        asyncio.ensure_future(self.async_populate_profile())

    async def async_populate_profile(self):
        profile_data = await self.load_profile_data()
        self.populate_profile(profile_data)


    async def load_profile_data(self):
        # first fetch our hex key base on private key
        priv_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
        pub_key_hex = get_nostr_pub_key(priv_key)
        app = MDApp.get_running_app()
        relay_screen = app.root.ids.screen_manager.get_screen('relay_screen')
        
        # Now you have access to the relays list
        relays = relay_screen.relays

        for relay in relays:
            profile_query = to_nip19(ntype='nprofile', payload=pub_key_hex, relays=[relay])
            Logger.info(f'profile_query: {profile_query}')
            profile_events = await aionostr.get_anything(profile_query)
            profile_event = profile_events[0]
            profile_dict = profile_event.to_json_object()
            return profile_dict


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

        ### or use aionostr.add_event??
        # event_id = await add_event(
        #     relays,
        #     event=event,
        #     pubkey=pubkey,
        #     private_key=private_key,
        #     created_at=int(created),
        #     kind=kind,
        #     content=content,
        #     tags=tags,
        #     direct_message=dm,
        # )
        # click.echo(event_id)
        # click.echo(to_nip19('nevent', event_id, relays))

        # Create the profile event
        profile_event = {
            # ... [populate the event dictionary with appropriate keys and values]
        }
        
        # Fetch the relays from the RelayScreen
        app = MDApp.get_running_app()
        relay_screen = app.root.ids.screen_manager.get_screen('relay_screen')
        relays = relay_screen.relays
        
        # Create a relay pool
        relay_pool = RelayPool(relays=relays)

        # Start the relay pool
        await relay_pool.start()

        # Post the event to the relay pool
        try:
            await relay_pool.publish(profile_event)
            Logger.info(f"Profile event published to relays.")
        except Exception as e:
            Logger.error(f"Failed to publish profile event: {e}")
        finally:
            # Stop the relay pool once done
            await relay_pool.stop()



async def load_profile_data(self):
    # Retrieve the private key from the keyring
    priv_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
    if not priv_key:
        Logger.error("Private key not found in keyring.")
        return None  # Fail gracefully without throwing an exception
    
    pub_key_hex = get_nostr_pub_key(priv_key)
    app = MDApp.get_running_app()
    relay_screen = app.root.ids.screen_manager.get_screen('relay_screen')

    # Now you have access to the relays list
    relays = relay_screen.relays

    for relay in relays:
        try:
            profile_query = to_nip19(ntype='nprofile', payload=pub_key_hex, relays=[relay])
            Logger.info(f'Trying profile query on relay: {relay}')
            profile_events = await aionostr.get_anything(profile_query)
            if profile_events:
                profile_event = profile_events[0]
                profile_dict = profile_event.to_json_object()
                return profile_dict
            else:
                Logger.warn(f"No profile events found on relay: {relay}")
        except Exception as e:
            Logger.error(f"Error querying relay {relay}: {e}")

    Logger.error("All relays failed to load profile data.")
    return None



if __name__ == "__main__":
    # Run the async function in the asyncio event loop
    profile_data = asyncio.run(load_profile_data())

    # Once you have profile_data, you can print or manipulate it as needed
    if profile_data:
        print(json.dumps(profile_data, indent=4))
    else:
        print("No profile data could be loaded.")
