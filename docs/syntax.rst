.. _syntax:

Supported syntax
================

rinx implements the parts of reStructuredText and Sphinx that documentation
projects use most, and grows from there. This page is an overview;
:ref:`compatibility` lists every construct with its deviations. The `example site <../example-site/examples/index.html>`__ shows each construct
rendered.

A construct rinx does not know is never dropped silently. An unknown directive
is reported as a warning and drawn as a visible error block quoting its source.

reStructuredText
----------------

Supported:

- Sections (underlined and overlined headings), paragraphs, transitions and
  comments.
- Inline markup: emphasis, strong, inline literals, hyperlinks (named and
  anonymous), backslash escapes, and smart quotes and dashes.
- Bullet, enumerated, definition and option lists, line blocks, block quotes,
  literal blocks and doctest blocks.
- Grid and simple tables, ``.. table::``, ``.. list-table::`` and
  ``.. csv-table::`` (the last including ``:file:``).
- Substitutions, ``.. include::``, ``.. image::``, ``.. figure::``,
  ``.. contents::`` and ``.. sectnum::``.

Not yet supported: footnotes and citations, field lists beyond a document's
leading ``:orphan:``, ``.. raw::``, ``.. class::``, ``.. role::``,
``.. topic::``, ``.. sidebar::``, ``.. rubric::`` and
``.. parsed-literal::``.

Sphinx
------

Supported:

- ``.. toctree::`` (including ``:glob:``, ``:numbered:``, ``:maxdepth:`` and
  ``:hidden:``), with navigation, previous/next links and the general index.
- The ``:ref:``, ``:doc:``, ``:term:`` and ``:program:`` roles, ``:math:``
  and ``:eq:``, and ``:any:``, which searches labels, documents, terms, options,
  equations and domain objects at once and reports a target naming several
  of them as ``link.ambiguous-any`` rather than guessing. A ``:doc:`` names a
  page relative to the one it is written on (``:doc:`../install```), or from
  the Bazel workspace root with a leading ``/`` (``:doc:`/docs/install```), as
  a toctree entry does.
- Admonitions, ``.. seealso::``, ``.. versionadded::``,
  ``.. versionchanged::`` and ``.. deprecated::``.
- ``.. code-block::``, ``.. highlight::`` and ``.. literalinclude::``, all
  highlighted at build time.
- ``.. math::``, rendered to MathML at build time.
- ``.. glossary::`` and ``.. index::``.
- The doctest directives of ``sphinx.ext.doctest``, executed by
  ``bazel test`` (see :ref:`rules`).
- The Python domain (``py:function``, ``py:class``, ``py:method``,
  ``py:module``, … and their roles), the C domain (``c:function``,
  ``c:struct``, ``c:macro``, …) and the standard domain's ``.. program::`` and
  ``.. option::``.
- Linking to other sites through their ``objects.inv``, and writing one; see
  :ref:`intersphinx`.

Not yet supported: the ``:numref:``, ``:download:`` and ``:code:`` roles,
autodoc, ``.. only::``, and the ``:envvar:`` and ``:confval:`` objects.

Extensions
----------

These directives come from popular Sphinx extensions and are always
available. rinx has no ``extensions =`` setting.

- **sphinx-design:** ``.. dropdown::``, ``.. grid::`` and
  ``.. button-link::``.
- **sphinx-needs:** project-defined entities with ``needtable``,
  ``needflow``, ``needsequence``, ``needpie``, ``needbar``, ``needimport`` and
  ``needextend``, under their own names and rinx's. See :ref:`entities`.
- **sphinxcontrib-plantuml:** ``.. uml::`` and ``.. plantuml::``, with
  ``diagrams = True`` on the library.
- **sphinx-simplepdf:** ``.. if-builder::``.
- Jinja-templated sources, which a Sphinx project gets from a ``source-read``
  hook, with ``jinja = True`` on the library.

Diagnostics
-----------

Every warning names its file, line and column, and carries a stable code such
as ``link.broken-ref``. A ``.. noqa: <code>`` comment silences the codes it
names for the block that follows it.
