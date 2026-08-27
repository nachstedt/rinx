# RST / Sphinx Specification Gaps

This file tracks known deviations and missing features relative to the full
[reStructuredText specification](https://docutils.sourceforge.io/rst.html) and
[Sphinx documentation system](https://www.sphinx-doc.org/).

It serves as a living checklist: entries should be updated or removed when
the corresponding feature is implemented. Notes are only added when a
feature is partially implemented or behaves differently from real Sphinx —
not to describe how something is implemented.

Legend: ✅ implemented · 🔶 partial · ❌ not implemented

## Heading Adornments

| Feature | Status | Notes |
|---------|--------|-------|
| Underline-only headings | ✅ | — |
| Overlined headings (`===` above **and** below the text) | ✅ | Treated as a distinct adornment style from a same-character underline, so it can be assigned its own heading level |
| Per-document level reset vs. project-wide level tracking | 🔶 | Levels reset per file; Sphinx accumulates heading hierarchy across files linked via `toctree` |
| Inline markup/roles inside heading text (e.g. `` :mod:`x` ``, `**bold**`) | ✅ | — |

## Inline Markup

| Feature | Status | Notes |
|---------|--------|-------|
| Bold (`**text**`) | ✅ | — |
| Italic (`*text*`) | ✅ | — |
| Inline code (`` `text` ``) | ✅ | — |
| `` :ref:`label` `` cross-reference role | ✅ | — |
| `` :term:`glossary term` `` cross-reference role | ✅ | — |
| `` :program:`name` `` role | ✅ | Renders as highlighted text (`<strong class="program">`); not a cross-reference/lookup role |
| Domain-object role target modifiers: `!target`, `~target`, `.target`, trailing `target()` | ✅ | — |
| Hyperlink references (`link text <URL>`_) | ✅ | — |
| Anonymous hyperlinks (`link text <URL>`__) | ✅ | — |
| Smart typography (`---` → em dash, `--` → en dash, `...` → ellipsis) | ✅ | Not applied inside inline literals, `:program:`, code blocks, or glossary terms |
| `` :any:`target` `` cross-reference role (finds any cross-reference target) | ❌ | — |
| `` :doc:`page` `` cross-reference role (links to another document) | ❌ | — |
| `` :download:`file` `` role (links to a downloadable file) | ❌ | — |
| `` :numref:`target` `` cross-reference role (numbered reference to a figure/table/section) | ❌ | — |
| `` :code:`text` `` role (inline code with an optional highlighted language) | ❌ | — |
| `` :math:`...` `` / `` :eq:`label` `` roles (inline math / equation-number reference) | ❌ | — |
| `` :pep:`num` `` / `` :rfc:`num` `` roles (generate a URL to the PEP/RFC) | ❌ | — |
| `` :cve:`num` `` / `` :cwe:`num` `` roles (generate a URL to the CVE/CWE entry) | ❌ | — |
| `` :index:`term` `` role (inline index entry, distinct from the `.. index::` directive) | ❌ | — |
| `` :sub:`text` ``/`` :subscript:`text` ``, `` :sup:`text` ``/`` :superscript:`text` `` roles | ❌ | — |
| Default role for unadorned `` `text` `` (single backtick, no role prefix — resolves to title-reference in real docutils/Sphinx) | ❌ | — |
| Semantic markup roles: `:abbr:`, `:command:`, `:dfn:`, `:file:`, `:guilabel:`, `:kbd:`, `:mailheader:`, `:makevar:`, `:manpage:`, `:menuselection:`, `:mimetype:`, `:newsgroup:`, `:regexp:`, `:samp:` | ❌ | Real-Sphinx text-formatting-only roles (no cross-reference target); none render any HTML wrapper here |

Domain-object cross-reference roles (`:func:`, `:mod:`, `:meth:`, `:class:`, `:attr:`, `:exc:`, `:macro:`, `:struct:`, `:union:`, `:type:`, `:data:`/`:const:`/`:var:`/`:member:`) and `:option:` are tracked per-role under the domain they belong to in the `## Domains` section below, not in this table.

## Block-Level Elements

| Feature | Status | Notes |
|---------|--------|-------|
| Bullet lists (`-`, `*`, `+`) | ✅ | — |
| Enumerated lists (`1.`, `a.`, `i.`, …) | ❌ | — |
| Definition lists | 🔶 | Term classifiers (`term : classifier`) not supported; glossary's "multiple terms share one definition" quirk uses a separate implementation |
| Field lists (`:field: value`) | ❌ | — |
| Option lists | ❌ | — |
| Literal blocks (``::`` paragraph ending or standalone ``::``) | ✅ | — |
| Block quotes (indented paragraphs without a directive) | ❌ | — |
| Line blocks (`| line`) | ❌ | — |

## Tables

| Feature | Status | Notes |
|---------|--------|-------|
| Grid tables (`+---+` / `|` / `=` ASCII art) | ✅ | Column/row spans and hierarchical/nested headers supported |
| Simple tables (whitespace-column `===` style) | ❌ | — |
| `.. list-table::` | ✅ | `:name:` resolves to the document, not a fragment precisely at the table (same limitation as every other internal `:ref:` target) |
| `.. csv-table::` directive | ❌ | — |
| `.. table::` (table title/caption/options) | ❌ | — |

## Directives

| Feature | Status | Notes |
|---------|--------|-------|
| `.. toctree::` | 🔶 | `:numbered:`, `:caption:`, `:hidden:`, `:titlesonly:`, `:glob:`, `:reversed:`, `:includehidden:` are recognized but silently ignored — no effect on rendering |
| `.. code-block::` | 🔶 | Only the language argument is honored; Sphinx options like `:linenos:`, `:emphasize-lines:`, `:caption:`, `:dedent:` are not recognized — an option line would render literally as code text |
| `.. index::` (`single`/`pair`/`triple`/comma-shorthand entries, `!main` marker) | ✅ | — |
| `.. index::` `see`/`seealso` entry types | 🔶 | Parsed but not yet surfaced in the rendered general index |
| `.. note::`, `.. warning::`, `.. tip::` (admonitions) | ✅ | All docutils admonition kinds supported (`attention`, `caution`, `danger`, `error`, `hint`, `important`, `note`, `tip`, `warning`), plus generic `.. admonition:: Title` and a `:collapsible:` option |
| `.. seealso::` | ✅ | — |
| `.. versionadded::`, `.. versionchanged::`, `.. deprecated::` (version-change admonitions) | 🔶 | `.. versionremoved::` is not supported |
| `.. image::` | ❌ | — |
| `.. figure::` (image with a caption/legend) | ❌ | — |
| `.. include::` | ❌ | — |
| `.. literalinclude::` | ❌ | — |
| `.. doctest::`, `.. testcode::`, `.. testoutput::`, `.. testsetup::`, `.. testcleanup::` (`sphinx.ext.doctest`) | 🔶 | Parsed and rendered but not executed as part of the site build — execution is a separate, opt-in `bazel test` target (see `docs/decisions/002-doctest-execution.md`). `:pyversion:` is parsed but deliberately not evaluated at parse time |
| Doctest blocks (a text block starting with `>>> `, no directive) | ✅ | Unlike the `.. doctest::` directive family above, these are executed by default, matching Sphinx |
| `.. automodule::` / `.. autofunction::` (autodoc) | ❌ | Out of scope — would require importing user code to generate content |
| `.. contents::` (local/on-page table of contents) | ❌ | — |
| `.. sectnum::` / `.. section-numbering::` (automatic section numbering) | ❌ | — |
| `.. topic::` (self-contained boxed section) | ❌ | — |
| `.. sidebar::` (parallel mini-document alongside the main text) | ❌ | — |
| `.. rubric::` (informal heading outside the document's section structure) | ❌ | — |
| `.. epigraph::` / `.. highlights::` / `.. pull-quote::` (block-quote styling variants) | ❌ | — |
| `.. compound::` / `.. container::` (generic grouping directives, no inherent styling) | ❌ | — |
| `.. parsed-literal::` (literal block with inline markup parsed) | ❌ | — |
| `.. raw::` (pass content untouched to a specific output format) | ❌ | — |
| `.. class::` / `.. rst-class::` (sets a CSS class on the following element) | ❌ | — |
| `.. role::` / `.. default-role::` (define/select a custom interpreted text role) | ❌ | — |
| `.. meta::` (HTML `<meta>` tags) | ❌ | — |
| `.. centered::` (deprecated in real Sphinx in favor of `.. rst-class:: centered`) | ❌ | — |
| `.. hlist::` (compact multi-column bullet list) | ❌ | — |
| `.. highlight::` (sets the default `.. code-block::` language for following blocks) | ❌ | — |
| `.. math::` (display math block) | ❌ | — |
| `.. productionlist::` (formal grammar production rules) | ❌ | — |
| `.. only::` (conditionally includes content based on build tags) | ❌ | — |
| `.. sectionauthor::` / `.. moduleauthor::` / `.. codeauthor::` (author metadata; no visible output by default in real Sphinx either) | ❌ | — |
| `.. target-notes::` (footnote-style listing of link targets, for hardcopy output) | ❌ | — |

## Domains

### `py` domain

| Feature | Status | Notes |
|---------|--------|-------|
| `.. py:function::` (function object type) | ✅ | — |
| `:func:` cross-reference role | ✅ | Resolves to `.. py:function::`; the same role also resolves to `.. c:function::` in the `c` domain (see below) |
| `.. py:module::` (module object type) | ✅ | Options `:platform:`, `:synopsis:`, `:deprecated:` supported |
| `:mod:` cross-reference role | ✅ | Resolves to `.. py:module::` |
| `.. py:data::` (data object type) | ✅ | Options `:type:`, `:value:` supported |
| `:data:`/`:const:` cross-reference role | ✅ | Resolves to `.. py:data::` |
| `.. py:method::` (method object type) | 🔶 | Options `:classmethod:`, `:staticmethod:`, `:abstractmethod:`, `:async:` supported; `:property:`/`:no-index:` not supported |
| `:meth:` cross-reference role | ✅ | Resolves to `.. py:method::` |
| `.. classmethod::` / `.. staticmethod::` (legacy directive aliases for `.. py:method::`) | ✅ | — |
| `.. py:class::` (class object type) | 🔶 | `:canonical:`/`:no-index:` and other real-Sphinx options not supported |
| `:class:` cross-reference role | ✅ | Resolves to `.. py:class::` |
| `.. py:attribute::` (attribute object type) | ✅ | Options `:type:`, `:value:`, `:canonical:` supported; `:canonical:` is rendered as plain metadata only — no alias/redirect semantics |
| `:attr:` cross-reference role | ✅ | Resolves to `.. py:attribute::` |
| `.. py:exception::` (exception object type) | 🔶 | Signature-line-wrapping options (`:single-line-parameter-list:`, etc.) not supported — no multi-line signature wrapping exists |
| `:exc:` cross-reference role | ✅ | Resolves to `.. py:exception::` |
| `.. py:currentmodule::` (sets module context for subsequent cross-referencing without documenting a module itself) | ✅ | — |
| `:module:` option (available on every `py:*` object-description directive) | ✅ | — |
| `py` domain role-target aliasing (`:exc:`/`:class:` resolving against either a `.. class::` or `.. exception::` definition of the same name) | 🔶 | Real Sphinx's third alias, `:obj:`, is not supported — only `class`/`exception` alias each other |
| Nesting-based cross-reference qualification (object directives nested inside a `py:class`/`py:exception` body) | ✅ | — |
| Module-based cross-reference qualification (`py:function`/`py:class`/etc. documented as a sibling after `.. py:module::`) | ✅ | Still unresolved: a role whose requested type doesn't match the definition's (e.g. `:meth:`~random.seed`` pointing at a `py:function`) — real Sphinx also warns about this case |

### `c` domain

| Feature | Status | Notes |
|---------|--------|-------|
| `.. c:function::` (function object type) | ✅ | — |
| `:func:` cross-reference role | ✅ | Resolves to `.. c:function::`; the same role also resolves to `.. py:function::` in the `py` domain (see above) |
| `.. c:macro::` (macro object type) | ✅ | Aliased with `function` for cross-referencing, matching real Sphinx's own macro/function ambiguity |
| `:macro:` cross-reference role | ✅ | Resolves to `.. c:macro::` |
| `.. c:struct::` (struct object type) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect (no local contents/TOC listing exists). `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:struct:` cross-reference role | ✅ | Resolves to `.. c:struct::` |
| `.. c:union::` (union object type) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect (no local contents/TOC listing exists). `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:union:` cross-reference role | ✅ | Resolves to `.. c:union::` |
| `.. c:member::` (member object type) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect (no local contents/TOC listing exists). `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:member:` cross-reference role | ✅ | A `:c:macro:`/`:c:data:` naming collision resolves via alias with a soft type-mismatch warning rather than a broken link, matching real Sphinx's own permissive behavior here |
| `.. c:var::` (directive-name alias of `.. c:member::`) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect (no local contents/TOC listing exists). `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:var:` cross-reference role | ✅ | A `:c:macro:`/`:c:data:` naming collision resolves via alias with a soft type-mismatch warning rather than a broken link, matching real Sphinx's own permissive behavior here |
| `:data:` cross-reference role | ✅ | A `:c:macro:`/`:c:data:` naming collision resolves via alias with a soft type-mismatch warning rather than a broken link, matching real Sphinx's own permissive behavior here |
| `.. c:type::` (type object type) | ✅ | — |
| `:type:` cross-reference role | ✅ | Resolves to `.. c:type::` |
| `.. c:namespace::` (sets the active C namespace) | ✅ | — |
| `.. c:namespace-push::` (pushes a namespace onto the active stack) | ✅ | One deliberate deviation: an unmatched `c:namespace-push` inside a body is contained by that body's close rather than leaking out to a later unrelated pop |
| `.. c:namespace-pop::` (pops the most recently pushed namespace) | ✅ | See `.. c:namespace-push::`'s note for the one behavioral deviation in this family |
| Object name extraction from C declarations (shared by `c:function`/`c:macro` signature parsing, e.g. `char *foo(void)`, `int (*Py_tracefunc)(PyObject *obj)`) | 🔶 | Multiple declarators per signature, K&R definitions, a real expression grammar in array sizes/initializers, and macro wrappers like `PyAPI_FUNC(...)` are not supported — falls back to a name-extraction heuristic and emits a diagnostic when the declaration doesn't parse |

### `std` domain

| Feature | Status | Notes |
|---------|--------|-------|
| `.. option::` (cmdoption object type) | ✅ | Cross-reference keys are case-insensitive, so e.g. `-X` and `-x` would collide (pre-existing limitation, not std-domain-specific) |
| `.. cmdoption::` (legacy alias of `.. option::`) | ✅ | Cross-reference keys are case-insensitive, so e.g. `-X` and `-x` would collide (pre-existing limitation, not std-domain-specific) |
| `.. program::` (sets the "current program" context `.. option::`/`:option:` qualify against) | ✅ | — |
| `:option:` cross-reference role | ✅ | — |
| `.. envvar::` (environment-variable object type) | ❌ | — |
| `:envvar:` cross-reference role | ❌ | — |
| `.. confval::` (configuration-value object type) | ❌ | — |
| `:confval:` cross-reference role | ❌ | — |
| `.. describe::` (generic formatted description; no index entry or cross-reference target) | ❌ | — |
| `.. object::` (generic object description; no cross-reference target) | ❌ | — |
| `:token:` cross-reference role (references a grammar token defined by `.. productionlist::`) | ❌ | — |

### Cross-domain behavior

| Feature | Status | Notes |
|---------|--------|-------|
| Multiple signatures per definition directive (several argument lines under one directive) | ✅ | — |
| Default domain resolution for bare (unprefixed) directives/roles | ✅ | — |
| Target resolution order (leading-dot `refspecific` targets, and the suffix fallback) | ✅ | Two deliberate deviations from real Sphinx: (1) the requested object type is checked in every search tier, not just dot-prefixed targets, so a role like `:func:`Thread.run`` is reported rather than silently linked to a method; (2) an ambiguous suffix match does not resolve at all, where Sphinx warns but links the first candidate |
| Explicit `` :role:`title <target>` `` override for domain-object roles | ✅ | — |
| Cross-referencing domain objects (and targets/glossary terms) nested inside tables, lists, or admonition/version-change/seealso bodies | ✅ | — |
| Automatic general-index entries for domain object definitions | ✅ | Entry text uses a simplified format (e.g. `"Greeter.greet (method)"`) rather than Sphinx's fuller `"coroutine() (in module types)"` phrasing |
| `preview` subcommand default-domain awareness | 🔶 | `preview` accepts a `--default-domain` flag identical to `parse`'s, but the VS Code extension never queries the enclosing `rusty_sphinx_library`'s `default_domain` Bazel attribute or passes it through, so editor-driven previews always default to `py` |

## General Index

| Feature | Status | Notes |
|---------|--------|-------|
| Site-wide `genindex.html` page | ✅ | Own markup, not visually identical to Sphinx's default theme |
| "Index" sidebar link | ✅ | — |

## Document Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Sections and document tree (nested sections) | 🔶 | Heading levels are detected; no explicit section nesting in the AST |
| Transitions (`----`) | ✅ | No section-boundary validation, since sections aren't nested in the AST yet |
| Comments (`.. comment text`) | ✅ | — |
| Substitution definitions (`.. |name| replace::`) | ❌ | Includes the `.. unicode::` and `.. date::` substitution-definition helper directives |
| Footnotes and citations | ❌ | — |
