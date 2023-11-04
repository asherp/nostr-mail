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

KEYRING_GROUP = 'nostrmail'

class SettingsScreen(MDScreen):
    error = BooleanProperty(False)

    def save_settings(self):
        self.save_priv_key()
        self.save_email_credentials()


    def save_priv_key(self):
        priv_key = self.ids.priv_key.text
        try:
            pub_key = get_nostr_pub_key(priv_key)
            self.update_pub_key(pub_key)

            priv_key_stored = keyring.get_password(KEYRING_GROUP, 'priv_key')
            if priv_key != priv_key_stored:
                keyring.set_password(KEYRING_GROUP, 'priv_key', priv_key)
                Logger.info('priv key stored')
                self.ids.priv_key.helper_text = 'Nostr Priv Key (changed)'
            else:
                Logger.info('priv key unchanged')
                self.ids.priv_key.helper_text = 'Nostr Priv Key'

        except Exception as e:
            self.ids.pub_key.text = ''
            self.ids.pub_key.helper_text = "Failed to generate public key." + str(e)
            self.error = True


    def save_email_credentials(self):
        credential_names = [
            'email_address',
            'email_password',
            'imap_host',
            'imap_port',
            'smtp_host',
            'smtp_port']

        # Use a for loop to save the credentials
        for name in credential_names:
            self.save_credential(name, self.ids[name].text)


    def save_credential(self, credential_type, credential_value):
        stored_credential = keyring.get_password(KEYRING_GROUP, credential_type)
        if credential_value != stored_credential:
            keyring.set_password(KEYRING_GROUP, credential_type, credential_value)
            Logger.info(f'{credential_type} updated.')
        else:
            Logger.info(f'{credential_type} unchanged.')



    def update_pub_key(self, pub_key):
        self.ids.pub_key.text = pub_key
        self.ids.pub_key.helper_text = "Nostr Public Key"





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
        self.load_credentials()

    def load_priv_key(self):
        priv_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
        settings_screen = self.root.ids.settings_screen
        if priv_key is not None:
            settings_screen.ids.priv_key.text = priv_key
            settings_screen.save_priv_key()
            Logger.info('priv_key for nostrmail loaded')
        else:
            Logger.info('priv_key for nostrmail not found')


    def load_credentials(self):
        self.load_credential('email_address', 'email_address')
        self.load_credential('email_password', 'email_password')

    def load_credential(self, credential_type, text_field_id):
        credential_value = keyring.get_password(KEYRING_GROUP, credential_type)
        settings_screen = self.root.ids.settings_screen
        if credential_value is not None:
            getattr(settings_screen.ids, text_field_id).text = credential_value
            Logger.info(f'{credential_type} for nostrmail loaded')
        else:
            Logger.info(f'{credential_type} for nostrmail not found')



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
