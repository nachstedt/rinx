.. _admonitions:

Admonitions Showcase
=====================

This page demonstrates all supported admonition types in Rinx.

Standard Admonitions
--------------------

.. note::
   This is a standard note. It uses the default "Note" title.

.. warning::
   This is a warning! Be careful when performing this action.

.. danger::
   This is a danger admonition. Something very bad might happen!

.. tip::
   Here is a helpful tip for using the system more efficiently.

.. important::
   This is an important piece of information that you should not miss.

.. caution::
   Proceed with caution.

.. hint::
   Did you know you can use recursive blocks inside admonitions?

      This paragraph is indented further and belongs to the same admonition.

.. attention::
   Please pay attention to the details.

.. error::
   An error has occurred.

Generic Admonitions
-------------------

.. admonition:: Custom Title

   This is a generic admonition with a custom title provided as an argument.

Content on the Directive Line
-----------------------------

Every admonition but the generic one reads text after the ``::`` as the first
line of its body:

.. note:: A one-line note.

.. warning:: A warning starting on the directive line,
   continued below it.
   :collapsible: open

   Options may follow that first block, and the body after a blank line.

Collapsible Admonitions
-----------------------

.. note::
   :collapsible:
   
   This note is collapsible and closed by default.

.. warning::
   :collapsible: open
   
   This warning is collapsible but starts in the open state.

.. admonition:: Collapsible Custom Admonition
   :collapsible:
   
   You can also make generic admonitions collapsible.

Nested Content
--------------

.. note::
   Admonitions can contain other blocks:
   
   * Bullet lists
   * Multiple paragraphs

   **A bold line**

   stands in for a heading: a section title may not stand inside an
   admonition, and one written there is reported, as
   :ref:`headings-misplaced` shows.
