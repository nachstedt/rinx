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
| Overlined headings (`===` above **and** below the text) | ❌ | — |
| Per-document level reset vs. project-wide level tracking | 🔶 | Levels reset per file; Sphinx accumulates heading hierarchy across files linked via `toctree` |
| Inline markup/roles inside heading text (e.g. `` :mod:`x` ``, `**bold**`) | ✅ | — |

## Inline Markup

| Feature | Status | Notes |
|---------|--------|-------|
| Bold (`**text**`) | ❌ | Rendered as plain text rather than erroring |
| Italic (`*text*`) | ❌ | Rendered as plain text rather than erroring |
| Inline code (`` `text` ``) | ❌ | Rendered as plain text rather than erroring |
| Interpreted text roles (e.g. `` :ref:`label` ``) | 🔶 | `:ref:`, `:program:`, `:term:`, and the domain roles `:func:`, `:mod:`, `:data:`/`:const:`, `:meth:`, `:class:`, `:attr:`, `:exc:`, `:macro:`, `:struct:`/`:union:`, and `:type:` (with `py:`/`c:` prefixed forms) are implemented |
| Domain-object role target modifiers: `!target`, `~target`, `.target`, trailing `target()` | ✅ | — |
| Hyperlink references (`link text <URL>`_) | ❌ | — |
| Anonymous hyperlinks (`link text <URL>`__) | ❌ | — |
| Smart typography (`---` → em dash, `--` → en dash, `...` → ellipsis) | ✅ | Not applied inside inline literals, `:program:`, code blocks, or glossary terms |

## Block-Level Elements

| Feature | Status | Notes |
|---------|--------|-------|
| Bullet lists (`-`, `*`, `+`) | ✅ | — |
| Enumerated lists (`1.`, `a.`, `i.`, …) | ❌ | — |
| Definition lists | ✅ | Term classifiers (`term : classifier`) not supported; glossary's "multiple terms share one definition" quirk uses a separate implementation |
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
| `.. toctree::` | ✅ | — |
| `.. code-block::` | ✅ | — |
| `.. index::` (`single`/`pair`/`triple`/comma-shorthand entries, `!main` marker) | ✅ | — |
| `.. index::` `see`/`seealso` entry types | 🔶 | Parsed but not yet surfaced in the rendered general index |
| `.. note::`, `.. warning::`, `.. tip::` (admonitions) | ❌ | — |
| `.. image::` | ❌ | — |
| `.. include::` | ❌ | — |
| `.. literalinclude::` | ❌ | — |
| `.. doctest::`, `.. testcode::`, `.. testoutput::`, `.. testsetup::`, `.. testcleanup::` (`sphinx.ext.doctest`) | 🔶 | Parsed and rendered but not executed as part of the site build — execution is a separate, opt-in `bazel test` target (see `docs/decisions/002-doctest-execution.md`). `:pyversion:` is parsed but deliberately not evaluated at parse time |
| Doctest blocks (a text block starting with `>>> `, no directive) | ✅ | Unlike the `.. doctest::` directive family above, these are executed by default, matching Sphinx |
| `.. automodule::` / `.. autofunction::` (autodoc) | ❌ | Out of scope — would require importing user code to generate content |

## Domains

| Feature | Status | Notes |
|---------|--------|-------|
| `py`/`c` domains, `function` object type (`.. py:function::`, `.. c:function::`) | ✅ | — |
| `py` domain `module` object type (`.. py:module::`) | ✅ | — |
| `.. py:module::` options: `:platform:`, `:synopsis:`, `:deprecated:` | ✅ | — |
| `py` domain `data` object type (`.. py:data::`) | ✅ | — |
| `c` domain `data` cross-reference role (`:c:data:`/`:c:var:`/`:c:member:`) | ✅ | A `:c:macro:`/`:c:data:` naming collision resolves via alias with a soft type-mismatch warning rather than a broken link, matching real Sphinx's own permissive behavior here |
| Multiple signatures per definition directive (several argument lines under one directive) | ✅ | — |
| `.. py:data::` options: `:type:`, `:value:` | ✅ | — |
| `py` domain `method` object type (`.. py:method::`) | ✅ | — |
| `.. py:method::` options: `:classmethod:`, `:staticmethod:`, `:abstractmethod:`, `:async:` | ✅ | `:property:`/`:no-index:` not supported |
| `:module:` option on every `py:*` object-description directive | ✅ | — |
| `.. classmethod::` / `.. staticmethod::` legacy directive aliases | ✅ | — |
| `py` domain `class` object type (`.. py:class::`) | ✅ | `:canonical:`/`:no-index:` and other real-Sphinx options not supported |
| `py` domain `attribute` object type (`.. py:attribute::`) | ✅ | — |
| `.. py:attribute::` options: `:type:`, `:value:`, `:canonical:` | ✅ | `:canonical:` is rendered as plain metadata only — no alias/redirect semantics |
| `py` domain `exception` object type (`.. py:exception::`) | ✅ | Signature-line-wrapping options (`:single-line-parameter-list:`, etc.) not supported — no multi-line signature wrapping exists |
| `c` domain `macro` object type (`.. c:macro::`) | ✅ | Aliased with `function` for cross-referencing, matching real Sphinx's own macro/function ambiguity |
| Nesting-based cross-reference qualification (`py:method`/any domain object nested inside a `py:class`/`py:exception` body) | ✅ | — |
| Module-based cross-reference qualification (`py:function`/`py:class`/etc. documented as a sibling after `.. py:module::`) | ✅ | Still unresolved: a role whose requested type doesn't match the definition's (e.g. `:meth:`~random.seed`` pointing at a `py:function`) — real Sphinx also warns about this case |
| Target resolution order (leading-dot `refspecific` targets, and the suffix fallback) | ✅ | Two deliberate deviations from real Sphinx: (1) the requested object type is checked in every search tier, not just dot-prefixed targets, so a role like `:func:`Thread.run`` is reported rather than silently linked to a method; (2) an ambiguous suffix match does not resolve at all, where Sphinx warns but links the first candidate |
| `py` domain role-target aliasing (`:exc:`/`:class:` resolving against either a `.. class::` or `.. exception::` definition of the same name) | ✅ | Real Sphinx's third alias, `:obj:`, is not supported — only `class`/`exception` alias each other |
| Default domain resolution for bare (unprefixed) directives/roles | ✅ | — |
| Object name extraction from C declarations (e.g. `char *foo(void)`, `int (*Py_tracefunc)(PyObject *obj)`) | ✅ | Multiple declarators per signature, K&R definitions, a real expression grammar in array sizes/initializers, and macro wrappers like `PyAPI_FUNC(...)` are not supported — falls back to a name-extraction heuristic and emits a diagnostic when the declaration doesn't parse |
| `c:struct`/`c:union`/`c:member` (+ `c:var` directive-name alias), and their common object-description options | ✅ | `:no-contents-entry:` is parsed but has no rendering effect (no local contents/TOC listing exists). `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `c` domain `type` object type (`.. c:type::`) | ✅ | — |
| `c` domain namespace directives (`.. c:namespace::`, `.. c:namespace-push::`, `.. c:namespace-pop::`) | ✅ | One deliberate deviation: an unmatched `c:namespace-push` inside a body is contained by that body's close rather than leaking out to a later unrelated pop |
| `preview` subcommand default-domain awareness | 🔶 | Always defaults to `py`; does not yet read the enclosing library's actual `default_domain` attribute |
| Explicit `` :role:`title <target>` `` override for domain-object roles | ✅ | — |
| Cross-referencing domain objects (and targets/glossary terms) nested inside tables, lists, or admonition/version-change/seealso bodies | ✅ | — |
| Automatic general-index entries for domain object definitions | ✅ | Entry text uses a simplified format (e.g. `"Greeter.greet (method)"`) rather than Sphinx's fuller `"coroutine() (in module types)"` phrasing |
| `.. py:currentmodule::` (sets module context for subsequent cross-referencing without documenting a module itself) | ✅ | — |
| `std` domain `cmdoption` object type (`.. option::`, legacy alias `.. cmdoption::`) | ✅ | Cross-reference keys are case-insensitive, so e.g. `-X` and `-x` would collide (pre-existing limitation, not std-domain-specific) |
| `.. program::` (sets the "current program" context `.. option::`/`:option:` qualify against) | ✅ | — |
| `:option:` cross-reference role | ✅ | — |

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
| Substitution definitions (`.. |name| replace::`) | ❌ | — |
| Footnotes and citations | ❌ | — |
