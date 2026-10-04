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
  ``:class:`` names are added to the rendered ``<code>``.
- The ``:sub:``/``:subscript:`` and ``:sup:``/``:superscript:`` roles, whose
  text is set below or above the line. Escaped spaces join one to its word:

  .. code-block:: rst

     .. role:: chem(sub)

     Water is H\ :sub:`2`\ O, E = mc\ :sup:`2`, and glucose
     C\ :chem:`6`\ H\ :chem:`12`\ O\ :chem:`6`.

  A role derived from one takes ``:class:``, and without it has its own name
  as its class. These four and ``code`` are the only base roles: any other,
  or none, is reported as ``role.unsupported-base``, and a name rinx already
  gives a role as ``role.builtin-name``.
- Interpreted text: text in single backquotes with no role, read as the
  *default role*. That is ``title-reference`` (``:title-reference:``,
  ``:title:`` or ``:t:``), shown as a citation, unless a library's
  ``default_role`` or a ``.. default-role::`` chooses another — any role rinx
  knows. A role may also be written after the text:

  .. code-block:: rst

     `Dune` is a novel; water is H\ `2`:sub:\ O.

     .. default-role:: any

     See `the syntax page <syntax>`.

  ``.. default-role::`` applies to the rest of the document; without an
  argument it restores ``title-reference``. A name no role answers to is
  reported as ``default-role.unknown-role``.

Not yet supported: footnotes and citations, field lists beyond a document's
leading ``:orphan:``, ``.. raw::``, ``.. class::``, ``.. role::`` with a base
other than ``code``, ``sub`` or ``sup``, ``.. topic::``,
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
- The ``:rfc:`` role, which links an IETF Request for Comments the same way:
  ``:rfc:`2324``` shows **RFC 2324**, links ``rfc2324.html`` below
  ``https://datatracker.ietf.org/doc/html/`` unless the site sets
  ``rfc_base_url``, and lists the mention under *RFC*. A ``section-``,
  ``appendix-`` or ``page-`` anchor is spelled out, so
  ``:rfc:`2324#section-2.3``` shows **RFC 2324 Section 2.3**. A target that
  is not a number is reported as ``rfc.invalid-number``.
- The ``:cve:`` and ``:cwe:`` roles, which link a Common Vulnerabilities and
  Exposures record and a Common Weakness Enumeration entry:
  ``:cve:`2024-3094``` shows **CVE 2024-3094** and ``:cwe:`787``` shows
  **CWE 787**, each listed in the general index under the registry's name.
  A ``:cve:`` target is the identifier's year and sequence number; anything
  else — including the full ``CVE-2024-3094``, whose prefix the link already
  adds — is reported as ``cve.invalid-id``. A ``:cwe:`` target that is not a
  number is reported as ``cwe.invalid-number``.
- docutils' ``:pep-reference:`` role, ``:pep:``'s plainer sibling:
  ``:pep-reference:`8``` links ``pep-0008`` below the same ``pep_base_url``
  and shows *PEP 8*, with no general index entry, no bold and no trailing
  slash. Its target is a number from 0 to 9999 and nothing else — no title,
  no ``#`` anchor; anything else is shown as written and reported as
  ``pep-reference.invalid-number``.
- docutils' ``:rfc-reference:`` role, ``:rfc:``'s plainer sibling:
  ``:rfc-reference:`2822#section-3``` links ``rfc2822.html#section-3`` below
  the same ``rfc_base_url`` and shows *RFC 2822*, with no general index entry
  and no bold. Its target is a number of at least 1 with an optional ``#``
  section; anything else is reported as ``rfc-reference.invalid-number``.
- Admonitions, ``.. seealso::``, ``.. versionadded::``,
  ``.. versionchanged::`` and ``.. deprecated::``. As in Sphinx, text after
  the ``::`` starts the body — ``.. seealso:: :pep:`634``` is a complete
  directive — except for the generic ``.. admonition::``, whose argument is
  its title, and a version change's version, which is its first word.
- ``.. code-block::``, ``.. highlight::`` and ``.. literalinclude::``, all
  highlighted at build time.
- ``.. math::``, rendered to MathML at build time.
- ``.. glossary::`` and ``.. index::``, whose ``see:`` and ``seealso:``
  entries the general index lists as an unlinked "see …" or "see also …"
  under their term.
- The ``:index:`` role, which indexes the place it is written and shows its
  text there: ``:index:`interpreter``` is one ``single`` entry — commas
  included, and with ``;`` nesting a subentry — and a leading ``!`` marks it
  main without being shown. With an explicit title the target is one
  ``.. index::`` line, as in ``:index:`loops <pair: loop; statement>```. An
  entry its type cannot split is reported under the role's own codes, such as
  ``index-role.invalid-pair``, and its text is still shown.
- The doctest directives of ``sphinx.ext.doctest``, executed by
  ``bazel test`` (see :ref:`rules`).
- The Python domain (``py:function``, ``py:class``, ``py:method``,
  ``py:module``, … and their roles), the C domain (``c:function``,
  ``c:struct``, ``c:macro``, …) and the standard domain's ``.. program::`` and
  ``.. option::``. A module's ``:synopsis:`` is shown in the Python Module
  Index, which a site enables with ``domain_indices`` (see :ref:`rules`),
  and as the tooltip of every ``:mod:`` link. Every object takes
  ``:no-index:`` (for a second description of something documented
  elsewhere), ``:no-index-entry:`` and ``:no-contents-entry:``, a signature
  may wrap with a trailing backslash, and an option the object does not take
  is reported as ``directive.unknown-option``.
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
names for the block that follows it. :ref:`diagnostics` lists every code.
