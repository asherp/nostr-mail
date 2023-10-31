from threading import Thread
from kivy.network.urlrequest import UrlRequest
from kivy.properties import ListProperty
from kivy.logger import Logger
from kivy.core.image import Image as CoreImage
from kivymd.app import MDApp
from kivymd.uix.screen import MDScreen
from kivymd.uix.list import OneLineAvatarListItem, ImageLeftWidget
import tempfile



class ComposeScreen(MDScreen):
    pass


class SettingsScreen(MDScreen):
    pass


class Main(MDApp):
    avatars = ListProperty([])

    def build(self):
        self.theme_cls.theme_style = "Dark"
        self.load_images()

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


    def compose_email(self, *args):
        Logger.info("Compose button clicked")
        # ... your code for handling compose action ...

if __name__ == "__main__":
    Main().run()
