from kivy.properties import ListProperty

from kivymd.app import MDApp
from kivymd.uix.screen import MDScreen
from kivymd.uix.dialog import MDDialog
from kivymd.uix.button import MDFlatButton
from kivymd.uix.list import OneLineListItem
from kivymd.uix.textfield import MDTextField
from kivymd.uix.boxlayout import MDBoxLayout
from kivymd.uix.label import MDLabel

import keyring
from sqlitedict import SqliteDict
from util import Logger
from util import get_nostr_pub_key, DEFAULT_RELAYS, DATABASE_PATH
from kivy.lang import Builder
# from aionostr.relay import Manager

Builder.load_file('relays.kv')


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
        # self.relay_manager = None
        Logger.info("Initializing RelayScreen")
        # self.load_relays_from_db()


    # def load_relays_from_db(self):
    #     with SqliteDict(DATABASE_PATH, tablename='relays') as db:
    #         # Directly assign the list to self.relays
    #         self.relays = db.get('relays', DEFAULT_RELAYS)
    #         # Initialize the NostrRelayManager with the loaded relays
    #         self.relay_manager = Manager(self.relays)
    #         Logger.info("Relays loaded from database and relay manager initialized.")

    def save_relays_to_db(self):
        with SqliteDict(DATABASE_PATH, tablename='relays', autocommit=True) as db:
            # Convert the ListProperty to a plain list before saving.
            db['relays'] = list(self.relays)
            Logger.info("Relays saved to database.")


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
        self.dialog_box = MDBoxLayout(
            children=[self.current_content_cls, self.error_label],
            orientation="vertical",
            spacing="12dp",
            size_hint_y=None,
            height="120dp")
        self.dialog = MDDialog(
            title="Edit Relay URL",
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

