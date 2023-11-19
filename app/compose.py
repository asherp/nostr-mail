from kivymd.uix.screen import MDScreen
from kivy.lang import Builder


Builder.load_file('compose.kv')

class ComposeScreen(MDScreen):

    def compose_email(self, *args):
        Logger.info("Compose button clicked")
        # ... your code for handling compose action ...


if __name__ == "__main__":
    print('loading email')