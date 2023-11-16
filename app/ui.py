from kivy.logger import Logger
from kivy.config import Config
from kivy.uix.popup import Popup
from kivy.uix.label import Label
from kivy.uix.button import Button
from kivy.uix.boxlayout import BoxLayout
from kivy.cache import Cache
from kivymd.app import MDApp
import os

# Kivy Cache setup
Cache.register('text', limit=100, timeout=60)
Cache.register('meta', limit=100, timeout=60)
Cache.register('dm', limit=100, timeout=60)

# Set the directory where you want to store the log files
def configure_logger():
    log_directory = os.path.join(os.path.dirname(__file__), 'logs')
    if not os.path.exists(log_directory):
        os.makedirs(log_directory)
    
    # Configure the Kivy logger
    Config.set('kivy', 'log_dir', log_directory)
    Config.set('kivy', 'log_name', 'kivy_%y-%m-%d_%_.txt')
    Config.write()

configure_logger()

class ErrorPopup(Popup):
    def __init__(self, error_message, **kwargs):
        super().__init__(**kwargs)
        self.title = 'An Error Occurred'
        self.size_hint = (None, None)
        self.size = (400, 400)

        layout = BoxLayout(orientation='vertical')
        layout.add_widget(Label(text=error_message))

        close_button = Button(text='Close')
        close_button.bind(on_press=self.dismiss)
        layout.add_widget(close_button)

        self.content = layout

def get_screen(screen_name):
    """loads screen from kivy app context"""
    app = MDApp.get_running_app()
    screen = app.root.ids.screen_manager.get_screen(screen_name)
    return screen

