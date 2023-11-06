from kivymd.uix.screen import MDScreen

class ComposeScreen(MDScreen):

    def compose_email(self, *args):
        Logger.info("Compose button clicked")
        # ... your code for handling compose action ...