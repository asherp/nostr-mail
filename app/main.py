from threading import Thread
from sqlitedict import SqliteDict
from kivy.network.urlrequest import UrlRequest
from kivy.properties import ListProperty, BooleanProperty
from kivy.logger import Logger
from kivy.core.image import Image as CoreImage
from kivymd.app import MDApp
from kivymd.uix.screen import MDScreen
from kivymd.uix.dialog import MDDialog
from kivymd.uix.boxlayout import MDBoxLayout  # Make sure to import MDBoxLayout
from kivymd.uix.label import MDLabel
from kivy.uix.boxlayout import BoxLayout
from kivymd.uix.list import OneLineAvatarListItem, ImageLeftWidget, OneLineListItem
from kivymd.uix.textfield import MDTextField
from kivymd.uix.button import MDFlatButton

import tempfile
import keyring
from util import get_nostr_pub_key
from aionostr.util import NIP19_PREFIXES, from_nip19, to_nip19

KEYRING_GROUP = 'nostrmail'
DEFAULT_RELAYS = [
    "wss://nostr-pub.wellorder.net",
    "wss://relay.damus.io",
    'wss://brb.io',
    'wss://nostr.mom']

class EditableListItem(OneLineListItem):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.on_release = self.open_edit_dialog  # Sets the on_release to the dialog opener

    def open_edit_dialog(self):
        app = MDApp.get_running_app()
        relay_screen = app.root.ids.screen_manager.get_screen('relay_screen')
        Logger.info(f'editing current relay url: {self.text}')
        relay_screen.open_edit_dialog(self.text)

class RelayScreen(MDScreen):
    relays = ListProperty()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        Logger.info("Initializing RelayScreen")
        self.load_relays_from_db()


    def save_relays_to_db(self):
        with SqliteDict('nostrmail.sqlite', autocommit=True) as db:
            # Convert the ListProperty to a plain list before saving.
            db['relays'] = list(self.relays)
            Logger.info("Relays saved to database.")

    def load_relays_from_db(self):
        with SqliteDict('nostrmail.sqlite') as db:
            # Directly assign the list to self.relays
            self.relays = db.get('relays', DEFAULT_RELAYS)
            Logger.info("Relays loaded from database.")

    def load_default_relays(self):
        '''Load default relays if they are not already present.'''
        for default_relay in DEFAULT_RELAYS:
            if default_relay not in self.relays:
                self.relays.append(default_relay)
        self.on_pre_enter()  # Refresh the list
        self.save_relays_to_db()  # Save the updated list to the database

    def is_valid_relay_url(self, url):
        return url.startswith("wss://") and not url.endswith('.')


    def on_pre_enter(self, *args):
        self.ids.relay_list.clear_widgets()
        for relay in self.relays:
            self.ids.relay_list.add_widget(EditableListItem(text=relay))


    def on_save_button_release(self, relay_url):
        if self.current_content_cls is not None:
            self.save_relay_url(relay_url, self.current_content_cls.text)
        else:
            Logger.error('current_content_cls is None')
        self.dialog.dismiss()


    def open_edit_dialog(self, relay_url):
        '''Open a dialog to edit the selected relay URL.'''
        self.current_content_cls = MDTextField(text=relay_url)
        self.error_label = MDLabel(text='', color=(1, 0, 0, 1))  # Red color for error messages
        self.dialog_box = MDBoxLayout(orientation='vertical', children=[self.current_content_cls, self.error_label])
        self.dialog = MDDialog(
            # title="Edit Relay URL",
            type="custom",
            content_cls=self.dialog_box,
            buttons=[
                MDFlatButton(
                    text="CANCEL",
                    on_release=self.close_dialog
                ),
                MDFlatButton(
                    text="DELETE",
                    on_release=lambda *_: self.delete_relay_url(relay_url)
                ),
                MDFlatButton(
                    text="SAVE",
                    on_release=lambda *_: self.on_save_button_release(relay_url)
                ),
            ],
        )
        self.dialog.open()


    def delete_relay_url(self, relay_url):
        '''Delete the selected relay URL.'''
        if relay_url in self.relays:
            self.relays.remove(relay_url)
            self.on_pre_enter()  # Refresh the list
            self.save_relays_to_db()  # Save the updated list to the database
        self.close_dialog()

    def add_relay(self):
        '''Open a dialog to add a new relay URL.'''
        self.dialog = MDDialog(
            title="Add Relay URL",
            type="custom",
            content_cls=MDTextField(hint_text="wss://new.example.com"),
            buttons=[
                MDFlatButton(
                    text="CANCEL",
                    on_release=self.close_dialog
                ),
                MDFlatButton(
                    text="SAVE",
                    on_release=lambda *_: self.save_new_relay_url(self.dialog.content_cls.text)
                ),
            ],
        )
        self.dialog.open()

    def save_new_relay_url(self, new_url):
        '''Save the new relay URL.'''
        if self.is_valid_relay_url(new_url):
            self.relays.append(new_url)
            self.on_pre_enter()  # Refresh the list
            self.save_relays_to_db()  # Save the updated list to the database
        else:
            self.error_label.text = "The relay URL must start with wss://"
        self.close_dialog()

    def save_relay_url(self, old_url, new_url):
        '''Save the edited relay URL.'''
        if self.is_valid_relay_url(new_url):
            if old_url in self.relays:
                index = self.relays.index(old_url)
                self.relays[index] = new_url
                self.on_pre_enter()  # Refresh the list
                self.save_relays_to_db()  # Save the updated list to the database
        else:
            self.error_label.text = "The relay URL must start with wss://"

    def close_dialog(self, *args):
        '''Close the edit dialog.'''
        Logger.info(f'Closing dialog: {self.dialog}')
        if self.dialog is not None:
            self.dialog.dismiss()
        else:
            Logger.error('No dialog to close')



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
    def load_profile_data(self):
        # first fetch our hex key base on private key
        priv_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
        pub_key_hex = get_nostr_pub_key(priv_key)
        raise NotImplementedError('not finished yet')
        profile_query = to_nip19(ntype='nprofile', payload=pub_key_hex, relays=relays)
        Logger.info(profile_query)
        

    def on_enter(self, *args):
        Logger.info('PROFILE SCREEN ENTERED')
        # Load profile data from your data source here
        profile_data = self.load_profile_data()
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

class ContactsScreen(MDScreen):
    pass

class Main(MDApp):
    avatars = ListProperty([])

    def on_start(self):
        self.load_priv_key()
        self.load_credentials()
        self.root.ids.relay_screen.load_relays_from_db()


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
