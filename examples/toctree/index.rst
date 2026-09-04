.. _toctree-index:

##################
Toctree Directives
##################

Every ``.. toctree::`` option and entry form, so the rendered site exercises
each of them.

The toctree below uses an explicit ``:caption:``, an ``Explicit Title <target>``
entry, a ``self`` entry pointing back at this page, and an external link — none
of which name a document the way a plain entry does.

.. toctree::
   :caption: Entry forms
   :maxdepth: 2

   Overview of this page <self>
   entry_forms
   Renamed in the sidebar <titles_only>
   https://www.sphinx-doc.org/

Numbering
=========

``:numbered:`` numbers everything it reaches, across document boundaries, and
the numbers appear both here and as prefixes on the target pages' headings.

.. toctree::
   :caption: Numbered chapters
   :numbered:

   numbered_one
   numbered_two

Depth limits
============

The same two documents again, at ``:maxdepth: 1`` — proving two toctrees in one
document render differently, which they could not before.

.. toctree::
   :maxdepth: 1

   titles_only
   entry_forms

``:titlesonly:`` drops the section entries instead of limiting their depth.

.. toctree::
   :titlesonly:

   titles_only

Globs and ordering
==================

``:glob:`` expands a pattern against the project's documents, sorted; a ``*``
does not cross a ``/``. ``:reversed:`` flips the order after expansion.

.. toctree::
   :glob:
   :reversed:

   globbed_*

Hidden
======

A ``:hidden:`` toctree renders nothing here but still contributes to the
sidebar, the reading order and the section numbering. Its ``:name:`` makes it a
reference target: :ref:`hidden-toc`.

.. toctree::
   :hidden:
   :name: hidden-toc

   hidden_page

``orphan.rst`` is deliberately listed by *no* toctree, and says so with an
``:orphan:`` field of its own — removing that field is what makes the build
warn about it.
