from threading import Thread
from kivy.network.urlrequest import UrlRequest
from kivy.properties import ListProperty, BooleanProperty
from kivy.logger import Logger
from kivy.core.image import Image as CoreImage
from kivymd.app import MDApp
from kivymd.uix.screen import MDScreen
from kivymd.uix.list import OneLineAvatarListItem, ImageLeftWidget
import tempfile
import keyring
from util import get_nostr_pub_key

class SettingsScreen(MDScreen):
    error = BooleanProperty(False)

    def save_priv_key(self):
        priv_key = self.ids.priv_key_input.text
        try:
            pub_key = get_nostr_pub_key(priv_key)
            self.update_pub_key_output(pub_key)

            priv_key_stored = keyring.get_password('nostrmail', 'priv_key')
            if priv_key != priv_key_stored:
                keyring.set_password('nostrmail', 'priv_key', priv_key)
                Logger.info('priv key stored')
                self.ids.priv_key_input.helper_text = 'Nostr Priv Key (changed)'
            else:
                Logger.info('priv key unchanged')
                self.ids.priv_key_input.helper_text = 'Nostr Priv Key'

        except Exception as e:
            self.ids.pub_key_output.text = ''
            self.ids.pub_key_output.helper_text = "Failed to generate public key." + str(e)
            self.error = True

    def update_pub_key_output(self, pub_key):
        self.ids.pub_key_output.text = pub_key
        self.ids.pub_key_output.helper_text = "Nostr Public Key"





class ComposeScreen(MDScreen):
    pass


class ProfileScreen(MDScreen):
    pass

class ContactsScreen(MDScreen):
    pass

class Main(MDApp):
    avatars = ListProperty([])

    def on_start(self):
        self.load_priv_key()

    def load_priv_key(self):
        priv_key = keyring.get_password('nostrmail', 'priv_key')
        settings_screen = self.root.ids.settings_screen
        settings_screen.ids.priv_key_input.text = priv_key
        settings_screen.save_priv_key()
        Logger.info('password for nostrmail loaded')


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
