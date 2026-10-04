.. _compatibility:

Compatibility status
====================

How rinx relates to `reStructuredText
<https://docutils.sourceforge.io/rst.html>`__, `Sphinx
<https://www.sphinx-doc.org/>`__ and the Sphinx extensions it supports,
construct by construct. :ref:`syntax` is the overview; this page is the full
list. Notes only name what is missing or behaves differently.

.. list-table::
   :header-rows: 1

   * - Status
     - Meaning
   * - ✅
     - Supported.
   * - ✅ ℹ️
     - Supported, with deliberate deviations listed in the notes.
   * - 🔶
     - Partially supported: the notes list what is still missing.
   * - ❌
     - Not supported.

Heading adornments
------------------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - Underline-only headings
     - ✅ ℹ️
     - - An underline shorter than four characters silently degrades to
         text; docutils reports it at info level.
   * - Overlined headings (``===`` above **and** below the text)
     - ✅ ℹ️
     - - An underline that differs from its overline is reported and still
         makes a heading; docutils drops the block with an error.
       - An overline with no underline, or two adornments with no title
         between them, are reported and stay text; docutils drops the block.
   * - Section titles inside nested content (block quotes, lists, tables,
       directive content)
     - ✅ ℹ️
     - - Reported, as docutils does, but kept as a heading that sets no level
         for the document; docutils drops it.
       - Allowed in Python and C object descriptions, as in Sphinx, and in
         ``.. include::`` and ``.. if-builder::`` content.
   * - Per-document level reset vs. project-wide level tracking
     - 🔶
     - - Heading levels are assigned per document.
   * - Inline markup and roles inside heading text
     - ✅
     -

Inline markup
-------------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - Bold (``**text**``) and italic (``*text*``)
     - ✅
     -
   * - Inline literals (double-backquoted text)
     - ✅
     -
   * - ``:ref:`` role
     - ✅ ℹ️
     - - A label on a figure, table or code block without a caption shows
         the label; Sphinx warns.
   * - ``:term:`` role
     - ✅
     -
   * - ``:program:`` role
     - ✅
     -
   * - Domain-object role target modifiers: ``!target``, ``~target``,
       ``.target``, trailing ``target()``
     - ✅
     -
   * - Named and anonymous hyperlink references
     - 🔶
     - - A named reference resolves within its own document, as in docutils:
         against its external targets, labels and section titles.
       - A name given two different targets in one document links nowhere, but
         is not yet reported as docutils' "Duplicate explicit target name".
       - Embedded relative URIs (``<#anchor>``), embedded aliases
         (``<name_>``) and link text wrapped over a line break are not
         recognised.
   * - Smart typography (``---``, ``--``, ``...``)
     - ✅
     -
   * - Inline-markup recognition rules (start/end-string context)
     - 🔶
     - - docutils' rule 5 (a marker inside a matching quote pair, e.g.
         ``"*"``) is not implemented.
   * - Backslash escapes
     - ✅
     -
   * - ``:math:`` role
     - ✅ ℹ️
     - - Rendered to MathML at build time, not by MathJax in the browser.
       - Stretchy brackets need a locally installed math font.
   * - ``:eq:`` role
     - ✅
     -
   * - ``:any:`` role
     - ✅ ℹ️
     - - A target naming several things is reported as
         ``link.ambiguous-any`` and not linked; Sphinx warns and links the
         first.
       - Glossary terms are found; Sphinx 9's ``:any:`` misses them.
       - A label above no heading is found, showing the label, as ``:ref:``
         does here; Sphinx's ``:any:`` does not find it.
       - Entities are found, since each is a label.
   * - ``:doc:`` role
     - ✅ ℹ️
     - - A leading ``/`` resolves from the Bazel workspace root, where every
         document's path starts, not from the directory holding the site's
         sources — as a toctree entry's does.
       - A target ending in ``.rst`` is found; Sphinx looks for a document
         named ``….rst`` and finds nothing.
       - An unknown document is drawn as a broken link and reported as
         ``link.broken-doc``; Sphinx shows the target as plain text.
   * - ``:download:`` role
     - ✅ ℹ️
     - - A file keeps its path from the Bazel workspace root under
         ``_downloads/``; Sphinx copies it to ``_downloads/<hash>/<name>``.
       - A leading ``/`` resolves from the Bazel workspace root, as
         ``:doc:``'s does.
       - A file not declared in the library's ``downloads`` fails the build
         as ``download.undeclared``; Sphinx warns and shows the text unlinked.
   * - ``:numref:`` role
     - ✅ ℹ️
     - - A title Sphinx could not apply, such as one with no ``%s`` or
         ``{number}``, is reported while parsing as
         ``numref.invalid-format``; Sphinx warns while resolving it, and
         crashes on an unbalanced brace.
       - An unknown label is drawn as a broken link and reported as
         ``link.broken-numref``; Sphinx shows the text.
       - A section numbered by ``.. sectnum::`` shows its number; Sphinx
         numbers only sections a ``:numbered:`` toctree reaches.
       - ``numfig_format`` accepts ``{number}``, which Sphinx cannot write
         in front of a caption, and refuses ``{name}`` when the site config
         loads.
       - ``:external:numref:`` is refused as ``numref.external``, since an
         ``objects.inv`` holds no numbers.
   * - ``:code:`` role
     - ✅ ℹ️
     - - Rendered as ``<code class="code">``, without docutils'
         ``docutils literal notranslate`` classes or the
         ``<span class="pre">`` around each word, as an inline literal is.
       - Highlighted by syntect's grammars rather than Pygments, as a code
         block is. A language no grammar highlights is reported as
         ``code-role.unknown-language``; Sphinx warns as
         ``misc.highlighting_failure``.
   * - ``:pep:`` role
     - ✅ ℹ️
     - - Only ASCII digits are a number: ``+8``, ``8_0`` and non-ASCII
         digits, which Python's ``int()`` accepts, are refused.
       - A target that is not a number is reported as ``pep.invalid-number``
         and shown as its source text; Sphinx reports an error and shows a
         ``problematic`` node.
       - The PEP index is ``rinx.toml``'s ``pep_base_url``, not a
         ``docutils.conf`` setting.
       - Role names are case-sensitive, so ``:PEP:`` is not recognized;
         docutils matches role names case-insensitively.
       - Its index anchor is numbered after the document's ``.. index::``
         anchors rather than in document order among them.
   * - docutils' ``:pep-reference:`` role
     - ✅ ℹ️
     - - Only ASCII digits are a number, as for ``:pep:``.
       - A target that is not a number from 0 to 9999 is reported as
         ``pep-reference.invalid-number`` and shown as its source text;
         docutils reports an error and shows a ``problematic`` node.
       - The PEP index is ``rinx.toml``'s ``pep_base_url``, and the page is
         always ``pep-%04d``: docutils' ``pep_file_url_template`` cannot be
         changed.
       - Role names are case-sensitive, so ``:PEP-Reference:`` is not
         recognized.
   * - ``:rfc:`` role
     - ✅ ℹ️
     - - Only ASCII digits are a number, as for ``:pep:``.
       - A target that is not a number is reported as ``rfc.invalid-number``
         and shown as its source text; Sphinx reports an error and shows a
         ``problematic`` node.
       - The RFC index is ``rinx.toml``'s ``rfc_base_url``, not a
         ``docutils.conf`` setting.
       - Role names are case-sensitive, so ``:RFC:`` is not recognized.
       - Its index anchor is numbered after the document's ``.. index::``
         anchors, as for ``:pep:``.
   * - ``:cve:`` role
     - ✅ ℹ️
     - - A target must be a year and a sequence number (``2024-3094``),
         optionally followed by ``#`` and an anchor; anything else is
         reported as ``cve.invalid-id`` and shown as its source text.
         Sphinx accepts any target, so a full ``CVE-2024-3094`` links
         ``id=CVE-CVE-2024-3094`` there without a warning; here it is
         refused with a hint to drop the prefix.
       - Role names are case-sensitive, and the index anchor is numbered as
         for ``:pep:``.
   * - ``:cwe:`` role
     - ✅ ℹ️
     - - Only ASCII digits are a number, as for ``:pep:``; anything else is
         reported as ``cwe.invalid-number`` and shown as its source text.
       - Role names are case-sensitive, and the index anchor is numbered as
         for ``:pep:``.
   * - docutils' ``:rfc-reference:`` role
     - ✅ ℹ️
     - - Only ASCII digits are a number, as for ``:pep:``.
       - A target that is not a number of at least 1 is reported as
         ``rfc-reference.invalid-number`` and shown as its source text;
         docutils reports an error and shows a ``problematic`` node.
       - The RFC index is ``rinx.toml``'s ``rfc_base_url``.
       - Role names are case-sensitive, so ``:RFC-Reference:`` is not
         recognized.
   * - ``:index:`` role
     - ✅ ℹ️
     - - Its index anchor is numbered after the document's ``.. index::``
         anchors, in document order among the registry roles' anchors, as
         for ``:pep:``.
       - An entry its type cannot split is reported as
         ``index-role.invalid-single``, ``index-role.invalid-pair``,
         ``index-role.invalid-triple``, ``index-role.invalid-see`` or
         ``index-role.invalid-seealso`` while parsing, and makes no entry;
         Sphinx warns only while building the index. The text is shown in
         both.
       - Role names are case-sensitive, so ``:Index:`` is not recognized.
   * - ``:sub:``/``:subscript:`` and ``:sup:``/``:superscript:`` roles
     - ✅ ℹ️
     - - Role names are case-sensitive, so ``:Sub:`` is not recognized.
       - A section title holding one shows it as plain text in the
         navigation and the page title, as for emphasis; Sphinx keeps the
         markup in its toctree.
   * - Default role (single-backquoted text without a role), and a role
       written after the text (```text`:role:``)
     - ✅ ℹ️
     - - Role names are case-sensitive, so ```x`:Sub:`` is not recognized.
       - A role written after the text that rinx does not know is left as
         written; docutils reports it.
       - An unknown library ``default_role`` fails the build; Sphinx warns.
   * - ``:title-reference:``/``:title:``/``:t:`` role
     - ✅ ℹ️
     - - Role names are case-sensitive, so ``:Title:`` is not recognized.
       - A section title holding one shows it as plain text in the
         navigation and the page title, as for emphasis.
   * - Semantic markup roles (``:abbr:``, ``:command:``, ``:dfn:``,
       ``:file:``, ``:guilabel:``, ``:kbd:``, ``:mailheader:``,
       ``:makevar:``, ``:manpage:``, ``:menuselection:``, ``:mimetype:``,
       ``:newsgroup:``, ``:regexp:``, ``:samp:``)
     - ❌
     -

Domain-object roles are listed under their domain in
:ref:`compatibility-domains` below.

Block-level elements
--------------------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - Bullet lists
     - ✅
     -
   * - Enumerated lists
     - ✅ ℹ️
     - - The prefix and suffix are rendered, so ``(a)`` and ``a.`` differ in
         the HTML; Sphinx drops them.
   * - Definition lists
     - 🔶
     - - Term classifiers (``term : classifier``) are not supported.
   * - Field lists
     - 🔶
     - - Only a document's leading field list is read, and only ``:orphan:``
         is interpreted.
       - Field lists are not rendered.
   * - Option lists
     - ✅
     -
   * - Literal blocks
     - ✅
     -
   * - Block quotes and attributions
     - ✅ ℹ️
     - - An indented comment does not nest into a block quote.
   * - Line blocks
     - ✅
     -

Tables
------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - Grid tables
     - ✅
     -
   * - Simple tables
     - ✅
     -
   * - ``.. table::``
     - ✅
     -
   * - ``.. list-table::``
     - ✅
     -
   * - ``.. csv-table::``
     - ✅ ℹ️
     - - ``:url:`` is not supported: the build never fetches.
       - ``:encoding:`` is UTF-8 only.
       - ``:delim:``, ``:quote:`` and ``:escape:`` must be ASCII.
       - A ``:file:`` must be declared in ``parse_data``.

Directives
----------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - ``.. toctree::``
     - ✅
     -
   * - ``.. code-block::``
     - ✅ ℹ️
     - - ``:caption:`` is plain text, not inline markup.
       - Session and traceback languages (``pycon``, ``console``, ``pytb``,
         …) render unhighlighted.
       - A few grammars (e.g. ``powershell``) fall back to plain text with
         ``code-block.highlight-failed``.
   * - ``.. code::``
     - ✅
     -
   * - ``.. highlight::``
     - ✅ ℹ️
     - - Applies to the rest of the document, not just the enclosing block.
   * - ``.. index::``
     - ✅
     -
   * - ``.. index::`` ``see``/``seealso`` entries
     - ✅
     -
   * - Admonitions (``.. note::``, ``.. warning::``, …,
       ``.. admonition::``)
     - ✅
     -
   * - ``.. seealso::``
     - ✅
     -
   * - ``.. versionadded::``, ``.. versionchanged::``,
       ``.. deprecated::``
     - ✅
     -
   * - ``.. versionremoved::``
     - ❌
     -
   * - ``.. image::``
     - ✅ ℹ️
     - - A ``:scale:`` without ``:width:`` or ``:height:`` is dropped with
         ``image.scale-no-dimensions``.
       - ``:loading: embed`` on an external URL links it instead.
       - The ``logo.*`` wildcard is not supported.
       - Images keep their source path under ``_images/`` instead of being
         flattened.
   * - ``.. figure::``
     - ✅ ℹ️
     - - As ``.. image::``.
       - ``:figwidth: image`` becomes CSS ``width: fit-content``.
   * - ``.. include::``
     - ✅ ℹ️
     - - ``:parser:`` is not supported.
       - ``:encoding:`` is UTF-8 only.
       - The file must be declared in ``parse_data``, and not in ``srcs``.
   * - ``.. literalinclude::``
     - ✅ ℹ️
     - - ``:pyobject:`` is refused; use ``:start-after:``/``:end-before:``.
       - ``:lineno-match:`` is refused with ``:diff:`` or a discontinuous
         ``:lines:``.
       - ``:encoding:`` is UTF-8 only.
       - The file must be declared in ``parse_data``.
   * - ``sphinx.ext.doctest`` directives (``.. doctest::``,
       ``.. testcode::``, …)
     - ✅ ℹ️
     - - Executed by the opt-in ``rinx_doctest_tests`` target, not by the
         site build (see :ref:`rules`).
   * - Doctest blocks (``>>>`` without a directive)
     - ✅
     -
   * - ``.. contents::``
     - ✅ ℹ️
     - - Cannot backlink sections that come before it.
   * - ``.. sectnum::`` / ``.. section-numbering::``
     - ✅ ℹ️
     - - ``:depth: 0`` means unlimited.
       - ``:start: 0`` is refused.
       - An unset ``:suffix:`` is empty, not a non-breaking space.
       - ``:prefix:`` and ``:suffix:`` are trimmed of whitespace.
   * - ``.. math::``
     - 🔶
     - - Multi-line equations use ``aligned`` instead of ``split`` (same
         layout).
       - ``math_number_all`` and ``math_numfig`` are not supported: only
         labelled equations are numbered, per document.
       - Under ``:nowrap:``, environments such as ``align`` restart their
         count in every directive.
   * - autodoc (``.. automodule::``, ``.. autofunction::``, …)
     - ❌
     - - Out of scope: it would import user code.
   * - ``.. topic::``, ``.. sidebar::``, ``.. rubric::``
     - ❌
     -
   * - ``.. epigraph::``, ``.. highlights::``, ``.. pull-quote::``
     - ❌
     -
   * - ``.. compound::``, ``.. container::``
     - ❌
     -
   * - ``.. parsed-literal::``
     - ❌
     -
   * - ``.. raw::``
     - ❌
     -
   * - ``.. class::`` / ``.. rst-class::``
     - ❌
     -
   * - ``.. role::``
     - 🔶
     - - Only a role derived from ``code`` (``.. role:: python(code)``), with
         ``:language:`` and ``:class:``, or from ``sub``, ``subscript``,
         ``sup`` or ``superscript`` (``.. role:: chem(sub)``), with
         ``:class:``. Any other base, or none, is reported as
         ``role.unsupported-base`` and the role is not defined.
       - A name rinx already gives a role (``:ref:``, ``:code:``, an entity
         role, …) is refused as ``role.builtin-name``; docutils lets a
         document replace it.
       - ``:class:`` names are normalized as docutils does, except that a
         non-ASCII letter is not reduced to its ASCII base first.
       - ``:language: none`` highlights nothing and adds no ``highlight``
         class; Sphinx adds ``highlight none``.
   * - ``.. default-role::``
     - ✅ ℹ️
     - - Built-in role names are case-sensitive, so
         ``.. default-role:: Any`` is reported as an unknown role.
   * - ``.. meta::``
     - ❌
     -
   * - ``.. centered::``
     - ❌
     -
   * - ``.. hlist::``
     - ❌
     -
   * - ``.. productionlist::``
     - ❌
     -
   * - ``.. only::``
     - ❌
     - - ``.. if-builder::`` covers the builder case.
   * - ``.. sectionauthor::``, ``.. moduleauthor::``, ``.. codeauthor::``
     - ❌
     -
   * - ``.. target-notes::``
     - ❌
     -

Extension directives
--------------------

rinx has no ``extensions =`` setting: a supported extension directive is
always available, and its name cannot be used by an entity schema.

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - ``.. dropdown::`` (sphinx-design)
     - ✅ ℹ️
     - - The bundled octicons are newer than sphinx-design's: ``commit`` is
         unknown, 57 newer names are accepted.
       - ``sd_custom_directives`` and per-directive option defaults are not
         supported.
   * - ``.. grid::`` / ``.. grid-item::`` (sphinx-design)
     - ✅ ℹ️
     - - ``sd_custom_directives`` and per-directive option defaults are not
         supported.
       - Breakpoints use Bootstrap's widths; sphinx-design's stylesheet is
         not bundled.
   * - ``.. button-link::`` (sphinx-design)
     - ✅ ℹ️
     - - No ``reference external`` classes on the link.
       - ``:ref-type:`` is refused.
       - ``:outline:`` without ``:color:`` warns.
       - A reference inside the label renders as its text.
       - The URL argument does not continue onto the next line.
   * - ``:octicon:`` role, material icons, ``.. button-ref::``,
       ``.. card::``, ``.. grid-item-card::``, ``.. tab-set::``
       (sphinx-design)
     - ❌
     -
   * - ``.. needtable::`` (sphinx-needs), as ``.. entity-table::``
     - ✅ ℹ️
     - - ``:style: datatables`` is refused.
       - ``:show_filters:``, ``:show_parts:``, ``:layout:`` and
         ``:filter-func:`` are not supported.
       - Rows are ordered by id unless ``:sort:`` is given.
   * - ``.. needflow::`` (sphinx-needs), as ``.. entity-flow::``
     - ✅ ℹ️
     - - Without ``:relations:``, every declared relation is drawn, not just
         ``links``.
       - Edges to entities outside the filter are not drawn.
       - ``:direction:`` is ``TB`` or ``LR`` only.
       - ``:show_filters:``, ``:show_legend:``, ``:highlight:``,
         ``:border_color:``, ``:filter-func:``, ``:engine:``, the
         ``:root_id:`` family and ``:tags:``/``:status:``/``:types:`` are
         refused.
       - Needs ``diagrams = True`` on the library.
   * - ``.. needsequence::`` (sphinx-needs), as ``.. entity-sequence::``
     - ✅ ℹ️
     - - ``:relations:`` is mandatory.
       - Several starts share one visited set: nothing is drawn twice.
       - Every lifeline is declared with its title.
       - An unknown start warns; the other starts are still drawn.
       - ``:show_filters:``, ``:show_legend:``, ``:show_link_names:``,
         ``:highlight:``, ``:filter-func:``, ``:sort_by:``, ``:export_id:``,
         ``:filter_warning:``, ``:height:``, ``:engine:`` and
         ``:tags:``/``:status:``/``:types:`` are refused.
       - Needs ``diagrams = True`` on the library.
   * - ``.. needpie::`` (sphinx-needs), as ``.. entity-pie::``
     - ✅ ℹ️
     - - ``:labels:`` of the wrong length warns; the wedges are still drawn.
       - ``:explode:``, ``:shadow:``, ``:style:`` and ``:filter-func:`` are
         refused.
   * - ``.. needbar::`` (sphinx-needs), as ``.. entity-bar::``
     - ✅ ℹ️
     - - A ragged row is padded with zeros and warns.
       - A label list of the wrong length keeps the data and warns.
       - A rotation that is not whole degrees is refused.
       - ``:style:`` and ``:status:``/``:tags:``/``:types:``/``:cypher:``
         are refused.
   * - ``.. needlist::`` (sphinx-needs)
     - ❌
     -
   * - ``.. needimport::`` (sphinx-needs)
     - ✅ ℹ️
     - - The file must be declared in ``parse_data``; a URL is refused.
       - ``needs_import_keys`` is the ``[import_keys]`` table of the entity
         schema; an undeclared key is refused.
       - ``:hide:``, ``:collapse:``, ``:layout:``, ``:style:``, ``:setup:``,
         ``:pre_template:`` and ``:post_template:`` are refused.
       - Named sections cannot be imported.
       - Warnings from imported prose carry no position.
   * - ``.. needextend::`` (sphinx-needs), as ``.. entity-update::``
     - ✅ ℹ️
     - - Renders a box by default; set ``show_entity_updates`` to hide it.
       - The original value and every change stay traceable.
       - Conflicting updates warn.
       - ``id``, ``type``, ``type_name``, ``docname``, ``title`` and
         sections cannot be changed.
   * - ``.. needservice::`` (sphinx-needs)
     - ❌
     - - Refused: the build never queries external services. Import a
         ``needs.json`` with ``.. needimport::`` instead.
   * - Dynamic functions (``[[copy('id')]]``, sphinx-needs)
     - ❌
     -
   * - ``.. uml::`` / ``.. plantuml::`` (sphinxcontrib-plantuml)
     - ✅ ℹ️
     - - Needs ``diagrams = True`` on the library.
       - Laid out by the ELK engine bundled in PlantUML, never a Graphviz
         ``dot`` on the host, so layouts differ from a Sphinx build's.
       - ``:scale:`` without ``:width:`` warns.
       - ``:save:`` is refused.
       - ``:config:`` names a preamble in ``rinx.toml``'s ``[uml_configs]``.
   * - ``.. needuml::`` / ``.. needarch::`` (sphinx-needs), as
       ``.. entity-diagram::`` / ``.. entity-arch::``
     - ✅ ℹ️
     - - As ``.. uml::``.
       - ``imports()`` takes an explicit id.
   * - ``.. if-builder::`` (sphinx-simplepdf)
     - ✅ ℹ️
     - - No ``<div class="docutils container">`` wrapper, so headings inside
         stay sections.
       - An unknown builder name warns.
       - An empty selected body warns.
   * - ``.. ifinclude::``, ``.. pdfinclude::`` (sphinx-simplepdf)
     - ❌
     -

.. _compatibility-domains:

Domains
-------

Every domain
~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - ``:no-index:``, ``:no-index-entry:`` and ``:no-contents-entry:``
       (and ``:noindex:``, ``:noindexentry:``, ``:nocontentsentry:``)
     - 🔶
     - - ``:no-contents-entry:`` has no effect, since rinx lists no objects
         in a local table of contents.
   * - ``:no-typesetting:``
     - ❌
     - - Warns as an unknown option.
   * - Signatures wrapped with a trailing backslash
     - ✅
     -
   * - An option an object does not take
     - ✅ ℹ️
     - - Warns, as Sphinx errors.

Python (``py``)
~~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - ``.. py:function::`` and ``:func:``
     - ✅
     -
   * - ``.. py:module::`` and ``:mod:``
     - ✅ ℹ️
     - - ``:synopsis:``, ``:platform:`` and ``:deprecated:`` are shown only
         in the Python Module Index and a ``:mod:`` link's tooltip, as in
         Sphinx; a synopsis wrapped over several lines is joined with a
         space, where Sphinx keeps the line break in the tooltip.
   * - Python Module Index (``py-modindex.html``)
     - ✅ ℹ️
     - - Opt-in with ``domain_indices = ["py-modindex"]`` on ``rinx_site``,
         where Sphinx writes it whenever a module is documented.
       - Submodules fold without JavaScript, behind a checkbox toggle.
       - ``:ref:`modindex``` and ``:ref:`py-modindex``` are broken links on
         a site without the index, where Sphinx links a page it never wrote.
   * - ``.. py:currentmodule::`` and the ``:module:`` option
     - ✅
     -
   * - ``.. py:data::`` and ``:data:``/``:const:``
     - ✅
     -
   * - ``.. py:method::`` and ``:meth:``
     - 🔶
     - - ``:property:`` and ``:final:`` are not supported.
   * - ``.. classmethod::`` / ``.. staticmethod::``
     - ✅
     -
   * - ``.. py:class::`` and ``:class:``
     - 🔶
     - - ``:canonical:`` and ``:annotation:`` are not supported.
   * - ``.. py:attribute::`` and ``:attr:``
     - 🔶
     - - ``:canonical:`` is shown but creates no alias.
   * - ``.. py:exception::`` and ``:exc:``
     - 🔶
     - - Signature wrapping options (``:single-line-parameter-list:``, …)
         are not supported.
   * - ``:exc:``/``:class:`` resolving to either kind
     - 🔶
     - - ``:obj:`` is not supported.
   * - Qualification by nesting and by ``.. py:module::``
     - ✅
     -

C (``c``)
~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - ``.. c:function::`` and ``:func:``
     - ✅
     -
   * - ``.. c:macro::`` and ``:macro:``
     - ✅
     -
   * - ``.. c:struct::``, ``.. c:union::``, ``.. c:member::``,
       ``.. c:var::`` and their roles
     - ✅
     -
   * - ``:data:`` role
     - ✅
     -
   * - ``.. c:type::`` and ``:type:``
     - ✅
     -
   * - ``.. c:enum::`` / ``.. c:enumerator::``
     - ❌
     -
   * - ``.. c:namespace::``, ``.. c:namespace-push::``,
       ``.. c:namespace-pop::``
     - ✅ ℹ️
     - - An unmatched push inside a body ends with that body.
   * - Name extraction from C declarations
     - 🔶
     - - Multiple declarators, K&R definitions, expressions in array sizes
         and macro wrappers such as ``PyAPI_FUNC(...)`` fall back to a
         heuristic with a warning.

Standard (``std``)
~~~~~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - ``.. option::`` / ``.. cmdoption::`` and ``:option:``
     - ✅ ℹ️
     - - Keys are case-insensitive, so ``-X`` and ``-x`` collide.
   * - ``.. program::``
     - ✅
     -
   * - ``.. glossary::`` and ``:term:``
     - ✅
     -
   * - ``.. envvar::`` and ``:envvar:``
     - ❌
     -
   * - ``.. confval::`` and ``:confval:``
     - ❌
     -
   * - ``.. describe::`` / ``.. object::``
     - ❌
     -
   * - ``:token:`` role
     - ❌
     -

Across domains
~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - Several signatures per directive
     - ✅
     -
   * - Default domain for unprefixed directives and roles
     - 🔶
     - - The VS Code preview always uses ``py``, ignoring the library's
         ``default_domain``.
   * - Target resolution order
     - ✅ ℹ️
     - - The requested object type is checked in every search tier.
       - An ambiguous suffix match does not resolve; Sphinx links the first.
   * - Explicit ``title <target>`` in domain roles
     - ✅
     -
   * - Targets nested in tables, lists and directive bodies
     - ✅
     -
   * - General-index entries for domain objects
     - ✅ ℹ️
     - - Simplified entry text, e.g. ``Greeter.greet (method)``.

Linking between sites (intersphinx)
-----------------------------------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - Writing ``objects.inv``
     - ✅ ℹ️
     - - Labels without a title are listed too.
       - Anchors are rinx's own.
   * - Reading inventories (``intersphinx_mapping``)
     - ✅ ℹ️
     - - An inventory is a pinned build input, never fetched (see
         :ref:`intersphinx`).
       - Version 1 inventories are refused.
   * - ``:external:`` / ``:external+name:``
     - ✅
     -

Document structure
------------------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - Nested sections
     - 🔶
     - - Sections are not nested in the document tree.
   * - Transitions
     - ✅ ℹ️
     - - Not validated against section boundaries.
   * - Comments
     - ✅
     -
   * - Substitutions
     - 🔶
     - - ``raw`` substitutions are not supported.
       - ``date`` is refused: it would make the build depend on the clock.
   * - Footnotes and citations
     - ❌
     -
   * - Jinja-templated sources (a ``source-read`` hook in ``conf.py``)
     - ✅ ℹ️
     - - Opt-in with ``jinja = True``; ``jinja_context`` replaces
         ``html_context``.
       - Whitespace control, computed template names and undefined values
         are refused.
       - Template names resolve from the source root.
   * - Site-wide general index (``genindex.html``)
     - ✅ ℹ️
     - - Own markup, not Sphinx's.

Diagnostics
-----------

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - ``path:line:column`` on every warning
     - ✅
     -
   * - Positions for ``.. math::`` LaTeX errors
     - 🔶
     - - Point at the body's first line, not the offending character.
   * - Positions inside ``.. csv-table::`` cells
     - 🔶
     - - Reported without a position.
   * - Stable diagnostic codes
     - ✅ ℹ️
     - - rinx extension.
   * - ``.. noqa:`` suppression comments
     - ✅ ℹ️
     - - rinx extension; ``noqa.unknown-code`` reports a mistyped code.
   * - Unknown directives
     - ✅ ℹ️
     - - A warning and a visible error block quoting the source; Sphinx
         errors.
   * - ``noqa.unused`` for a suppression that matched nothing
     - ❌
     -

Entities (rinx extension)
-------------------------

A project-declared construct vocabulary covering the ground of `sphinx-needs
<https://sphinx-needs.com/>`__; see :ref:`entities`.

.. list-table::
   :header-rows: 1
   :widths: 40 8 52

   * - Feature
     - Status
     - Notes
   * - User-declared entity types (``.. req::``, …)
     - ✅
     -
   * - Typed attributes
     - ✅
     -
   * - Named prose sections
     - ✅
     -
   * - Typed relations and derived back-links
     - ✅
     -
   * - Entity roles
     - ✅
     -
   * - Explicit, derived and generated ids
     - ✅
     -
   * - Value patterns (sphinx-needs' ``schemas.json`` ``pattern``)
     - ✅ ℹ️
     - - Rust ``regex`` dialect: no lookaround or backreferences.
       - Conditional and network rules are not converted.
   * - Filter strings
     - ✅ ℹ️
     - - Arithmetic, ordering comparisons, attribute access, calls other
         than ``startswith``/``endswith`` and ``filter_func`` are refused.
   * - JSON Schema for the entity schema file
     - ✅ ℹ️
     - - Checks structure only; the build checks references.
   * - Per-type presentation templates
     - ✅
     -
