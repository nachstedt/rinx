Definition Lists
=================

A term line immediately followed by a more-indented line starts a
definition list, rendered as ``<dl><dt>term</dt><dd>definition</dd></dl>``.

Basic Usage
-----------

Environment
   A structure where information about all documents under the root is
   saved, used for cross-referencing.

Builder
   A class (or function) that takes parsed documents and performs an
   output action, such as rendering HTML.

Multi-Paragraph Definition
---------------------------

A term's definition can span multiple paragraphs, as long as each stays
indented relative to the term:

Configuration
   The first paragraph of the definition.

   A second paragraph, still part of the same definition.

Inline Markup in Terms
-----------------------

Term text is parsed like any other inline text, so it may contain
cross-reference roles:

The :py:mod:`greetings` module
   Greeting utilities, cross-referenced from the term itself.

See :ref:`home-index`
   Links back to the site's home page.

Inside a ``seealso`` Directive
-------------------------------

Definition lists are a plain block-level construct, so they also work
nested inside directive bodies such as ``.. seealso::``:

.. seealso::

   The :py:mod:`greetings` module
      Utilities for greeting people, regardless of language settings.

   See :ref:`home-index`
      Tutorial material on getting started with the example site.
