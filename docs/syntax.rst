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
- The ``:code:`` role, and roles derived from it with ``.. role::``, which
  give inline code a language to be highlighted as:

  .. code-block:: rst

     .. role:: python(code)
        :language: python

     Call :python:`print("hi")`, or write :code:`x = 1` unhighlighted.

  A role applies from its definition to the end of the document, and its
  ``:class:`` names are added to the rendered ``<code>``. Only ``code`` can be
  a base role: any other, or none, is reported as ``role.unsupported-base``,
  and a name rinx already gives a role as ``role.builtin-name``.

Not yet supported: footnotes and citations, field lists beyond a document's
leading ``:orphan:``, ``.. raw::``, ``.. class::``, ``.. role::`` with a base
other than ``code``, ``.. default-role::``, ``.. topic::``,
``.. sidebar::``, ``.. rubric::`` and ``.. parsed-literal::``.

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
- The ``:download:`` role, which links a file and copies it into the site's
  ``_downloads/`` directory. The file is named as a ``:doc:`` target is,
  relative to the page or from the workspace root with a leading ``/``, and is
  declared in the library's ``downloads`` attribute (see :ref:`rules`); a URL
  is linked as written and never copied.
- The ``:numref:`` role, which links a figure, table, code block or section
  and shows its number: ``Fig. 2``, ``Table 1.3``. Figures, tables and code
  blocks are numbered only when the site sets ``numfig = true`` (see
  :ref:`site-config`), which also writes each number in front of its caption.
  Only captioned ones are numbered, counting across the site in toctree order
  and starting again under each chapter of a ``:numbered:`` toctree. A section
  shows the number a ``:numbered:`` toctree gave it. An explicit title is the
  format: ``:numref:`Figure {number}: {name} <fig-label>``` shows the number
  and the caption, and ``%s`` works as well as ``{number}``.
- The ``:pep:`` role, which links a Python Enhancement Proposal —
  ``:pep:`8``` shows **PEP 8** — and lists the mention in the general index
  under *Python Enhancement Proposals*. ``:pep:`8#naming``` links a section of
  the PEP, and ``:pep:`Style guide <8>``` shows a title of its own. Links go
  to ``https://peps.python.org/`` unless the site sets ``pep_base_url`` (see
  :ref:`site-config`); a target that is not a number is shown as written and
  reported as ``pep.invalid-number``.
- docutils' ``:pep-reference:`` role, ``:pep:``'s plainer sibling:
  ``:pep-reference:`8``` links ``pep-0008`` below the same ``pep_base_url``
  and shows *PEP 8*, with no general index entry, no bold and no trailing
  slash. Its target is a number from 0 to 9999 and nothing else — no title,
  no ``#`` anchor; anything else is shown as written and reported as
  ``pep-reference.invalid-number``.
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
  ``.. option::``. A module's ``:synopsis:`` is shown in the Python Module
  Index, which a site enables with ``domain_indices`` (see :ref:`rules`),
  and as the tooltip of every ``:mod:`` link.
- Linking to other sites through their ``objects.inv``, and writing one; see
  :ref:`intersphinx`.

Not yet supported: autodoc, ``.. only::``, and the ``:envvar:`` and
``:confval:`` objects.

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
