.. _button-link-basics:

======
Basics
======

The argument is the URL and the content is the label:

.. button-link:: https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html

   The directives reference

A button with no content at all shows its target instead of an empty button,
which is what makes a one-line ``.. button-link::`` worth writing:

.. button-link:: https://docutils.sourceforge.io/rst.html

The label is parsed as inline markup — the only other place this build does
that with a directive's argument or content is a dropdown's title:

.. button-link:: https://example.com/config

   Read ``conf.py`` *carefully*

A label may run over several lines. They are joined the way docutils joins
them, so this is one label and not two:

.. button-link:: https://example.com/long-label

   Open the online editor
   in a new workspace
