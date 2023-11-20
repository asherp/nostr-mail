from kivy.properties import BooleanProperty, StringProperty
from kivymd.uix.screen import MDScreen

import keyring
from util import get_nostr_pub_key, KEYRING_GROUP
from kivy.lang import Builder
from ui import Logger
from redmail import EmailSender
import imaplib
from kivymd.uix.snackbar import Snackbar


Builder.load_file('settings.kv')

Logger.info(f'Using keyring backend: {keyring.get_keyring()}')


class SettingsScreen(MDScreen):
    error = BooleanProperty(False)
    priv_key = StringProperty('')  # Use a StringProperty for the private key
    current_snackbar = None

    def __init__(self, **kw):
        super().__init__(**kw)
        self.fbind('priv_key', self.on_priv_key)

    def on_priv_key(self, instance, value):
        # Called whenever priv_key changes
        pub_key = get_nostr_pub_key(value)
        self.ids.pub_key.text = pub_key  # Assuming you have a pub_key id in your kv


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


    def test_email_connection(self):

        smtp_port_text=self.ids.smtp_port.text
        imap_port_text=self.ids.imap_port.text

        Logger.info('test_email_connection triggered')
        # Validate smtp_port
        try:
            smtp_port = int(smtp_port_text) if smtp_port_text else None
        except ValueError:
            self.update_status("Invalid SMTP port.")
            return  # Stop execution if the port is invalid

        try:
            imap_port = int(imap_port_text) if imap_port_text else None
        except ValueError:
            self.update_status("Invalid IMAP port.")
            return  # Stop execution if the port is invalid

        credentials = dict(
            email_address=self.ids.email_address.text,
            email_password=self.ids.email_password.text,
            imap_host=self.ids.imap_host.text,
            imap_port=imap_port,
            smtp_host=self.ids.smtp_host.text,
            smtp_port=smtp_port,
            )

        if not all(credentials.values()):
            for k, v in credentials.items():
                if (len(str(v)) == 0) | (v is None):
                    self.update_status(f"{k}: {v} invalid")
                    return
            self.update_status(f'something wrong with inputs')
            Logger.info('something wrong with inputs')
            return

        Logger.info('initializing EmailSender')

        # Initialize the EmailSender with the user's settings
        email_sender = EmailSender(
            host=credentials['smtp_host'],
            port=credentials['smtp_port'],
            username=credentials['email_address'],
            password=credentials['email_password'],
            use_starttls=True,  # or use_ssl=True, based on your email provider
        )
        Logger.info('testing smtp (send) connection')
        # Test the connection

        # Attempt to log in to the email server
        try:
            email_sender.connect()
            self.update_status("Connection successful! We can send email.")
            Logger.info('connection test successful')
            email_sender.close()
        except Exception as e:
            self.update_status(f"SMTP connection failed: {e}")
            return

        try:
            mail = imaplib.IMAP4_SSL(host=credentials['imap_host'])
        except:
            self.update_status(f"Invalid IMAP host: {credentials['imap_host']}")

        Logger.info(f"logging in to {credentials['email_address']}")

        try:
            response, msg = mail.login(credentials['email_address'], credentials['email_password'])
            if response == 'OK':
                Logger.info('connected')
                self.update_status("Logged in successfully! We can receive emails.")
                mail.select('Inbox')
                mail.logout()
            else:
                Logger.info('could not connect')
                self.update_status("IMAP login failed.")
        except:
            self.update_status(f"IMAP connection failed: {e}")

        


    def update_status(self, message):
        if self.current_snackbar:
            self.current_snackbar.text = message  # Update the text of the existing snackbar
            if not self.current_snackbar.is_open:
                self.current_snackbar.open()
        else:
            self.current_snackbar = Snackbar(text=message)
            self.current_snackbar.open()
            self.current_snackbar.bind(on_dismiss=lambda *x: setattr(self, 'current_snackbar', None))

    def reset_snackbar(self, instance):
        self.snackbar = None  # Reset reference when Snackbar is dismissed

    def show_alert(self, message):
        # This method would update the UI with an alert message
        # You can use a Snackbar, Modal, or any other way you prefer to notify the user
        Logger.info(message)  # Placeholder for UI update
