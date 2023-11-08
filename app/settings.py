from kivy.properties import BooleanProperty, StringProperty
from kivy.logger import Logger
from kivymd.uix.screen import MDScreen

import keyring
from util import get_nostr_pub_key, KEYRING_GROUP
from kivy.lang import Builder

Builder.load_file('settings.kv')

Logger.info(f'Using keyring backend: {keyring.get_keyring()}')


class SettingsScreen(MDScreen):
    error = BooleanProperty(False)
    priv_key = StringProperty('')  # Use a StringProperty for the private key

    def on_enter(self):
        # This will be called when the screen is about to be displayed
        self.load_priv_key()
        self.load_email_credentials()

    def load_priv_key(self):
        try:
            # Retrieve the private key from keyring
            priv_key_value = keyring.get_password(KEYRING_GROUP, 'priv_key')
            if priv_key_value:
                self.priv_key = priv_key_value  # Set the StringProperty to the retrieved value
                self.update_pub_key(get_nostr_pub_key(self.priv_key))
                Logger.info('Private key loaded from keyring.')
            else:
                Logger.warn('Private key not found in keyring.')
                self.priv_key = ''  # Ensure the property is cleared if no key is found
        except Exception as e:
            Logger.error(f'Error loading private key: {e}')
            self.error = True


    def toggle_priv_key_visibility(self):
        # This toggles the visibility of the private key
        self.ids.priv_key.password = not self.ids.priv_key.password
        # Change the icon accordingly
        if self.ids.priv_key.password:
            self.ids.priv_key.right_action_items = [['eye-off', lambda x: self.toggle_priv_key_visibility()]]
        else:
            self.ids.priv_key.right_action_items = [['eye', lambda x: self.toggle_priv_key_visibility()]]


    def save_priv_key(self):
        try:
            if self.priv_key:
                pub_key = get_nostr_pub_key(self.priv_key)
                self.update_pub_key(pub_key)
                stored_key = keyring.get_password(KEYRING_GROUP, 'priv_key')
                if self.priv_key != stored_key:
                    keyring.set_password(KEYRING_GROUP, 'priv_key', self.priv_key)
                    Logger.info('Private key stored in keyring.')
                else:
                    Logger.info('Private key unchanged in keyring.')
            else:
                Logger.warn('No private key provided to save.')
        except Exception as e:
            Logger.error(f'Error saving private key: {e}')
            self.error = True

    def update_pub_key(self, pub_key):
        self.ids.pub_key.text = pub_key
        self.ids.pub_key.helper_text = "Nostr Public Key"


    def save_settings(self):
        self.save_priv_key()
        self.save_email_credentials()


    def load_email_credentials(self):
        credential_fields = {
            'email_address': self.ids.email_address,
            'email_password': self.ids.email_password,
            'imap_host': self.ids.imap_host,
            'imap_port': self.ids.imap_port,
            'smtp_host': self.ids.smtp_host,
            'smtp_port': self.ids.smtp_port,
        }
        for cred_type, field in credential_fields.items():
            value = keyring.get_password(KEYRING_GROUP, cred_type)
            if value is not None:
                field.text = value
                Logger.info(f'{cred_type} loaded into settings.')
            else:
                Logger.warn(f'{cred_type} not found in keyring.')

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

