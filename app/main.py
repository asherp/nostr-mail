from kivy.lang import Builder
from kivymd.app import MDApp
from kivymd.uix.screen import MDScreen

class ProfileScreen(MDScreen):
    pass

class ContactsScreen(MDScreen):
    pass

class ComposeScreen(MDScreen):
    pass

class InboxScreen(MDScreen):
    pass

class SettingsScreen(MDScreen):
    pass

class MainApp(MDApp):

    def build(self):
        self.theme_cls.theme_style = "Dark"  # Set the theme style to Dark
        return Builder.load_file('main.kv')

    def show_screen(self, screen_name):
        self.root.ids.screen_manager.current = screen_name

if __name__ == '__main__':
    MainApp().run()
