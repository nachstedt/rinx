Glossary
========

This page demonstrates the ``.. glossary::`` directive and the ``:term:`` role.

.. glossary::

   environment
      A structure where information about all documents under the root is
      saved, used for cross-referencing.

   builder
      A class (or function) that takes parsed documents and performs an
      output action, such as rendering HTML.

   configuration directory
   configuration folder
      The directory containing the ``conf.py`` (or ``rusty_sphinx.toml``)
      file. Multiple terms can share the same definition.

Sorted Glossary
---------------

Using the ``:sorted:`` option, terms are displayed alphabetically regardless
of the order they appear in the source.

.. glossary::
   :sorted:

   Python
      The programming language in which Sphinx is originally implemented.

   Rust
      A systems programming language focused on safety, speed, and
      concurrency. Rusty-Sphinx is implemented in Rust.

   reStructuredText
      The markup language used as input by Sphinx and Rusty-Sphinx,
      commonly abbreviated as RST.
