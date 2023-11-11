from kivy.app import App
from kivy.uix.rst import RstDocument
from kivy.uix.scrollview import ScrollView


content = """
=================
Complex RST Example
=================

Introduction
============

This is an introduction to a **complex** reStructuredText document. reStructuredText is a powerful tool for textual markup.

Sections and Subsections
------------------------

Main Section
^^^^^^^^^^^^
This is a main section. You can add more content here.

Subsection
^^^^^^^^^^
This is a subsection. Subsections are useful for breaking down content into manageable parts.

Lists and Enumeration
---------------------

1. First item in a numbered list
2. Second item
   - Bullet point 1
   - Bullet point 2
     - Sub-bullet point


Images
------
.. image:: https://upload.wikimedia.org/wikipedia/en/c/c5/Bob_the_builder.jpg
   :align: left

Code Blocks
-----------

Here is an example of a code block:

.. code-block:: python

    def hello_world():
        print("Hello, world!")

Tables
------

+------------+------------+-----------+
| Header 1   | Header 2   | Header 3  |
+============+============+===========+
| Item 1     | Item 2     | Item 3    |
+------------+------------+-----------+
| Item 4     | Item 5     | Item 6    |
+------------+------------+-----------+

Links and References
--------------------

Visit the `Kivy Website <https://kivy.org>`_ for more information.

Conclusion
==========

This is the conclusion of the complex reStructuredText example.


"""

class RstApp(App):
    def build(self):

        # Create a RstDocument and set its text
        rst_document = RstDocument(text=content)

        # Use a ScrollView to make the RstDocument scrollable
        scroll_view = ScrollView()
        scroll_view.add_widget(rst_document)

        return scroll_view

if __name__ == '__main__':
    RstApp().run()
