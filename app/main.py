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
    avatars = ListProperty([])

    def build(self):
        self.theme_cls.theme_style = "Dark"
        Clock.schedule_once(self.schedule_async_data)

    def schedule_async_data(self, *args):
        # Access the profile screen using the screen manager
        profile_screen = self.root.ids.screen_manager.get_screen('profile_screen')
        # Schedule the async_populate_profile coroutine to be executed.
        asyncio.create_task(profile_screen.async_populate_profile())

    def load_images(self):
        urls = [
            "https://upload.wikimedia.org/wikipedia/commons/a/ac/Default_pfp.jpg",
        ]
        for url in urls:
            Thread(target=self.fetch_avatar, args=(url,)).start()

    def fetch_avatar(self, url):
        def on_success(request, result):
            with tempfile.NamedTemporaryFile(delete=False, suffix='.jpg') as temp_file:
                temp_file.write(result)
                self.avatars.append(temp_file.name)  # store the file path instead of CoreImage
                Logger.info(f'Temporary file created at: {temp_file.name}')

        def on_failure(request, result):
            Logger.error(f"Failed to load image from {url}")

        UrlRequest(url, on_success=on_success, on_failure=on_failure, on_error=on_failure)

if __name__ == "__main__":
    loop = asyncio.get_event_loop()
    loop.run_until_complete(Main().async_run())
    loop.close()

