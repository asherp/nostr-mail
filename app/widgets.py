from kivymd.uix.boxlayout import MDBoxLayout
from kivymd.uix.list import OneLineListItem, ImageLeftWidget, ImageRightWidget

class LRListItem(MDBoxLayout):
    def __init__(self, text, image_source='', item_type='left', image_size=("38dp", "38dp"), **kwargs):
        super().__init__(**kwargs)
        self.size_hint_y = None
        self.height = "48dp"

        if item_type == 'left':
            image_widget = ImageLeftWidget(
                source=image_source,
                size_hint=(None, None),
                size=image_size,
                pos_hint={"center_y": 0.5}
            )
            self.add_widget(image_widget)
            self.add_widget(OneLineListItem(text=text))

        elif item_type == 'right':
            self.add_widget(OneLineListItem(text=text))
            image_widget = ImageRightWidget(
                source=image_source,
                size_hint=(None, None),
                size=image_size,
                pos_hint={"center_y": 0.5}
            )
            self.add_widget(image_widget)