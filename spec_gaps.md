# RST / Sphinx Specification Gaps

This file tracks known deviations and missing features relative to the full
[reStructuredText specification](https://docutils.sourceforge.io/rst.html) and
[Sphinx documentation system](https://www.sphinx-doc.org/).

It serves as a living checklist: entries should be updated or removed when
the corresponding feature is implemented.

## Heading Adornments

| Feature | Status | Notes |
|---------|--------|-------|
| Underline-only headings | ✅ Implemented | Level determined by first-encounter order of the adornment character within a document |
| Overlined headings (`===` above **and** below the text) | ❌ Not implemented | Treated as a distinct adornment from an underline-only heading using the same character; currently parsed as two consecutive paragraphs |
| Per-document level reset vs. project-wide level tracking | 🔶 Partial | Levels reset per `parse()` call; Sphinx accumulates heading hierarchy across files linked via `toctree` |
| Inline markup/roles inside heading text (e.g. `` :mod:`x` ``, `**bold**`) | ✅ Implemented | `Node::Heading.text` is `Vec<InlineNode>`, parsed via the same `parse_inline_text` paragraphs use, and rendered via the same `render_inline`. Nav-sidebar labels and the page `<title>` still need a plain `String`, so those are derived via `rusty_sphinx_ast::inline_plain_text`, which flattens markup/links down to their visible text (only for H1 headings, which feed `document_titles`). |

## Inline Markup

| Feature | Status | Notes |
|---------|--------|-------|
| Bold (`**text**`) | ❌ Not implemented | Emitted as plain text |
| Italic (`*text*`) | ❌ Not implemented | Emitted as plain text |
| Inline code (`` `text` ``) | ❌ Not implemented | Emitted as plain text |
| Interpreted text roles (e.g. `` :ref:`label` ``) | 🔶 Partial | `:ref:`, `:program:`, `:term:`, and the domain roles `:func:` (plus `:py:func:`/`:c:func:`) and `:py:mod:` are implemented. |
| Domain-object role target modifiers `!target` (suppress link) and `~target` (shorten display to last dotted component) | ✅ Implemented | Applies to `:func:`/`:mod:` (and any future domain role) via `InlineNode::DomainObjectReference`'s `name`/`display`/`link` fields, parsed by the shared `parse_domain_object_target` helper in `crates/parser/src/inline.rs`. The separate `` :role:`title <target>` `` explicit-title override (already supported for `:term:`) is **not** yet supported for domain-object roles — tracked as a follow-up below. |
| Hyperlink references (`link text <URL>`_) | ❌ Not implemented | — |
| Anonymous hyperlinks (`link text <URL>`__) | ❌ Not implemented | — |
| Smart typography (`---` → em dash, `--` → en dash, `...` → ellipsis) | ✅ Implemented | Mirrors docutils' `smartquotes` transform (on by default in Sphinx); always on, no config toggle. Applied in `crates/parser/src/typography.rs::apply_smart_typography`, hooked into `parse_inline_text`'s plain-text runs and `find_inline_markup`'s `Emphasis`/`Strong` content, so it covers paragraphs, headings, and definition-list terms (all of which route through `parse_inline_text`), but not inline literals/`:program:`/code blocks. Glossary term strings (`GlossaryEntry.terms: Vec<String>`) bypass `parse_inline_text` and are not yet covered — same pre-existing gap as glossary not supporting inline markup in terms. |

## Block-Level Elements

| Feature | Status | Notes |
|---------|--------|-------|
| Bullet lists (`-`, `*`, `+`) | ✅ Implemented | Rendered as `<ul>`/`<li>`; nested lists and multi-paragraph items supported via recursion into `parse_blocks` (`crates/parser/src/bullet_list.rs`) |
| Enumerated lists (`1.`, `a.`, `i.`, …) | ❌ Not implemented | — |
| Definition lists | ✅ Implemented | A non-blank line immediately followed (no blank line) by a more-indented non-blank line starts an entry; rendered as `<dl><dt>term</dt><dd>definition</dd></dl>` (`crates/parser/src/definition_list.rs`, `Node::DefinitionList`). Term text is `Vec<InlineNode>`, so roles like `:mod:`/`:ref:` inside terms resolve normally — this is what the CPython benchmark's `seealso` blocks use. Term **classifiers** (`term : classifier`) and glossary's "multiple terms share one definition" quirk are not supported by this generic construct (glossary keeps its own separate implementation, see `crates/parser/src/glossary.rs`) |
| Field lists (`:field: value`) | ❌ Not implemented | — |
| Option lists | ❌ Not implemented | — |
| Literal blocks (``::`` paragraph ending or standalone ``::``) | ✅ Implemented | Rendered as ``<pre><code>...</code></pre>`` |
| Block quotes (indented paragraphs without a directive) | ❌ Not implemented | — |
| Line blocks (`| line`) | ❌ Not implemented | — |

## Tables

| Feature | Status | Notes |
|---------|--------|-------|
| Grid tables (`+---+` / `|` / `=` ASCII art) | ✅ Implemented | `crates/parser/src/table.rs::try_parse_grid_table` → `Node::Table { header_rows, body_rows }`; rendered as `<table>`/`<thead>`/`<tbody>`/`<tr>`/`<th\|td>`. Supports the optional `=` header/body divider and both **column spans** (dropped `\|`) and **row spans** (dropped `-` divider), emitted as HTML `colspan`/`rowspan`. Also supports **hierarchical/nested headers** that introduce a finer column split partway through the table (e.g. a header's second line splitting one top-border column into two sub-columns, as in CPython's `apiabiversion.rst`) — column boundaries are derived from the union of every `+` found anywhere in the table, not just the top border, since a `+` is unambiguously a column marker wherever it appears; a row-block that predates a given split reads as an implicit colspan across the not-yet-split columns. One consequence: a stray/typo `+` not aligned with any other row's structure is silently absorbed as a new column boundary rather than rejected (matches real Sphinx/docutils' own ambiguity here). Every line's overall *width* must still match the top border exactly — a width mismatch is reported as a diagnostic and the block falls back to a paragraph (parser stays error-resilient). Cell contents are parsed as full block-level RST ("a miniature document"). Supporting this required making heading adornment detection reject mixed-punctuation lines like `+---+---+` (only a single repeated punctuation char is a valid section adornment, per spec), so a header-less grid table is no longer mistaken for an overline heading. |
| Simple tables (whitespace-column `===` style) | ❌ Not implemented | — |
| `.. csv-table::` / `.. list-table::` directives | ❌ Not implemented | — |
| `.. table::` (table title/caption/options) | ❌ Not implemented | — |

## Directives

| Feature | Status | Notes |
|---------|--------|-------|
| `.. toctree::` | ✅ Implemented | Rendered as an `<ul>` list of links |
| `.. code-block::` | ✅ Implemented | Rendered as ``<pre><code class="language-...">...</code></pre>`` |
| `.. note::`, `.. warning::`, `.. tip::` (admonitions) | ❌ Not implemented | Parsed as `Directive::Unknown`; no HTML output |
| `.. image::` | ❌ Not implemented | — |
| `.. include::` | ❌ Not implemented | — |
| `.. literalinclude::` | ❌ Not implemented | — |
| `.. automodule::` / `.. autofunction::` (autodoc) | ❌ Not implemented | Sphinx extension; out of scope for core parser |

## Domains

| Feature | Status | Notes |
|---------|--------|-------|
| `py`/`c` domains, `function` object type (`.. py:function::`, `.. c:function::`) | ✅ Implemented | Cross-referenced via `:func:`/`:py:func:`/`:c:func:`. Each domain owns an independent object-type vocabulary (`PyObjectType`, `CObjectType`) — no shared `ObjectType` enum across domains. |
| `py` domain `module` object type (`.. py:module::`) | ✅ Implemented | Cross-referenced via `:py:mod:` (and the bare `:mod:` role, resolved via `default_domain`); unlike `:func:` there is no `:c:mod:` form, since real Sphinx's `mod` role has no C-domain analog. |
| `.. py:module::` options: `:platform:`, `:synopsis:`, `:deprecated:` | ✅ Implemented | Fields directly on `DomainObjectBody::PyModule` (not shared with other object types) and rendered inline as leading `<dd>` paragraphs, since rusty-sphinx has no separate module-index page (real Sphinx's `synopsis`/`platform` mainly populate that page rather than the module's own rendered output). |
| Default domain resolution for bare (unprefixed) directives/roles | ✅ Implemented | The default domain is a `rusty_sphinx_library` Bazel attribute (`default_domain`, default `"py"`), resolved once at parse time — not a `rusty_sphinx.toml` setting, since it describes the library's content rather than the consuming site. |
| Object name extraction from C-style pointer return types (e.g. `char *foo(void)`) | 🔶 Partial | `extract_object_name` naively yields `"*foo"`; real C declarator parsing is out of scope. |
| Additional object types (`class`, `struct`, `member`, …) beyond `function`/`module`, and domains beyond `py`/`c` | ❌ Not implemented | `Directive::DomainObject` wraps `DomainObjectBody`, a sum type with one variant per concrete object type (`PyFunction`, `PyModule`, `CFunction`), each carrying only its own fields — deliberately *not* one shared struct with every domain's options unioned together. `ObjectType`/`PyObjectType`/`CObjectType` remain a separate, lightweight tag vocabulary used by cross-reference roles (which never carry options). Adding a new object type means: a new `DomainObjectBody` variant + accessor arms (`crates/ast/src/lib.rs`), a new `parse_*` function (`crates/parser/src/domains.rs`), and a new match arm in `render_domain_object` (`crates/renderer/src/directives.rs`) — more touch points than the old fully-generic design, traded for no object type ever carrying another's irrelevant options. |
| `preview` subcommand default-domain awareness | 🔶 Partial | `preview` accepts `--default-domain` but always defaults to `py` today; the VS Code extension will eventually need to supply the enclosing library's actual Bazel attribute value. |
| Explicit `` :role:`title <target>` `` override for domain-object roles (`:func:`, `:mod:`) | ❌ Not implemented | Already supported for `:term:` via its own `<...>` parsing in `handle_inline_match`; domain-object roles only support the `!`/`~` target-prefix modifiers so far (see Inline Markup section above), not an explicit display-text override. |

## Document Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Sections and document tree (nested sections) | 🔶 Partial | Heading levels are detected; no explicit section nesting in the AST |
| Transitions (`----`) | ✅ Implemented | A line of 4+ repeated punctuation chars with blank lines on both sides, rendered as `<hr />`. Diagnostics are emitted if a transition begins/ends the document or immediately follows another transition; no section-level check since sections aren't yet nested in the AST |
| Comments (`.. comment text`) | ✅ Implemented | Parsed as `Node::Comment`; produces no HTML output |
| Substitution definitions (`.. |name| replace::`) | ❌ Not implemented | — |
| Footnotes and citations | ❌ Not implemented | — |
