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
| Interpreted text roles (e.g. `` :ref:`label` ``) | 🔶 Partial | `:ref:`, `:program:`, `:term:`, and the domain roles `:func:` (plus `:py:func:`/`:c:func:`), `:py:mod:`, `:py:data:`/`:py:const:`, `:py:meth:`, `:py:class:`, `:py:attr:`, and `:py:exc:` are implemented. |
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
| `.. index::` (`single`/`pair`/`triple`/comma-shorthand entries, `!main` marker) | ✅ Implemented | Parsed in `crates/parser/src/index_directive.rs`; `pair`/`triple` are expanded into plain `single`-equivalent entries at parse time, mirroring Sphinx's own behavior. Produces no visible HTML at its own position (just an anchor); entries feed the general index page — see "General Index" below. |
| `.. index::` `see`/`seealso` entry types | 🔶 Partial | Parsed into the AST (`IndexEntry::See`/`SeeAlso`) but not yet surfaced in the rendered general index — they redirect the reader to another entry rather than linking to content, which needs different rendering than a plain linkable entry. |
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
| `py` domain `data` object type (`.. py:data::`) | ✅ Implemented | Cross-referenced via `:py:data:`/`:data:` *and* `:py:const:`/`:const:` — real Sphinx has no separate `py:const` directive; `const` is just an alternate role spelling that resolves to the same `PyObjectType::Data` as `data` (`ObjectType::from_role_name` maps both `"data"` and `"const"` to `Self::Py(PyObjectType::Data)`), so both roles link to one `.. py:data::` definition. Python-only, like `mod` — no `c:data`/`c:const`. |
| `.. py:data::` options: `:type:`, `:value:` | ✅ Implemented | Fields directly on `DomainObjectBody::PyData`, parsed by `extract_data_options` (mirrors `extract_module_options`) and rendered inline as leading `<dd>` paragraphs, same rationale as `py:module`'s options. |
| `py` domain `method` object type (`.. py:method::`) | ✅ Implemented | Cross-referenced via `:py:meth:`/`:meth:` (Python-only, like `mod`/`data` — no `c:method`). Nesting a `py:method` (or any domain object) inside a `py:class` body automatically qualifies its cross-reference name with the enclosing class's name (see below); a `py:method` with no enclosing `py:class` is indexed under its own bare/manually-dotted name, same as `py:function`. |
| `.. py:method::` options: `:classmethod:`, `:staticmethod:`, `:abstractmethod:`, `:async:` | ✅ Implemented | Boolean fields on `DomainObjectBody::PyMethod`, parsed by `extract_method_options` (mirrors `extract_module_options`'s flag-scanning loop) and rendered as `<em class="property">` prefix labels before the signature in a fixed `abstractmethod`/`async`/`classmethod`/`staticmethod` order, independent of the order the author wrote them in. `:property:`/`:no-index:` are out of scope (`property` would want its own future `py:property` directive; there's no indexing feature for `no-index` to affect). |
| `py` domain `class` object type (`.. py:class::`) | ✅ Implemented | Cross-referenced via `:py:class:`/`:class:` (Python-only — no `c:class`). Shares `PyFunction`'s shape (`{ signature, is_final, body }`); `extract_object_name` already strips an optional base-class list (`Greeter(Base)` → `Greeter`) so it needed no changes. `:final:` renders a `final` prefix label before the `class` label (same mechanism as `py:method`'s modifiers); other real-Sphinx options (`:canonical:`, `:no-index:`, etc.) are out of scope. |
| `py` domain `attribute` object type (`.. py:attribute::`) | ✅ Implemented | Cross-referenced via `:py:attr:`/`:attr:`. Python-only, like `mod`/`data` — no `c:attribute` (real Sphinx's C domain has no attribute-object equivalent either). Like `py:method`, benefits from nesting-based qualification when documented inside a `py:class` body (see `examples/domains.rst`'s `Greeter.name` attribute). |
| `.. py:attribute::` options: `:type:`, `:value:`, `:canonical:` | ✅ Implemented | Fields directly on `DomainObjectBody::PyAttribute`, parsed by `extract_attribute_options` (mirrors `extract_data_options`) and rendered inline as leading `<dd>` paragraphs. `:canonical:` (real Sphinx: the fully-qualified name of where the attribute is actually defined, for re-exports) is rendered as metadata only — no alias/cross-reference-redirect semantics, matching the minimal-semantics treatment every other domain option gets here (there's no `:noindex:` anywhere either). |
| `py` domain `exception` object type (`.. py:exception::`) | ✅ Implemented | Cross-referenced via `:py:exc:`/`:exc:` (Python-only — no `c:exception`). Shares `py:class`'s shape (`{ signature, is_final, body }`) and reuses `extract_class_options` as-is, since real Sphinx gives `py:exception` the exact same signature grammar and `:final:` option as `py:class` (base-class list included, e.g. `InvalidNameError(GreeterError)`). Real Sphinx's `:single-line-parameter-list:`/`:single-line-type-parameter-list:` options are signature-line-wrapping hints only — out of scope, like `py:class`'s `:canonical:`/`:no-index:`, since rusty-sphinx does no multi-line signature wrapping at all. |
| Nesting-based cross-reference qualification (`py:method`/any domain object nested inside a `py:class`/`py:exception` body) | ✅ Implemented | `index_nodes` (analyzer) and `render_domain_object` (renderer, via `RenderCtx::class_stack`) both track the nearest enclosing `py:class`/`py:exception`'s own qualified name and prefix it onto anything nested inside that body, via the shared `rusty_sphinx_ast::qualify_name` helper — this is what keeps the analyzer's index key and the renderer's anchor `id` from ever drifting apart. Composes across arbitrarily many nested classes (`Outer.Inner.method`). `py:exception` joins `py:class` here since exceptions are classes in Python; deliberately not generalized to every domain object type: nesting a `py:function` inside a `py:module`'s body is unaffected and still indexed under its own bare name, preserving prior behavior — real Sphinx doesn't qualify through modules this way either. |
| Default domain resolution for bare (unprefixed) directives/roles | ✅ Implemented | The default domain is a `rusty_sphinx_library` Bazel attribute (`default_domain`, default `"py"`), resolved once at parse time — not a `rusty_sphinx.toml` setting, since it describes the library's content rather than the consuming site. |
| Object name extraction from C-style pointer return types (e.g. `char *foo(void)`) | 🔶 Partial | `extract_object_name` naively yields `"*foo"`; real C declarator parsing is out of scope. |
| Additional object types (`struct`, `member`, …) beyond `function`/`module`/`data`/`method`/`class`/`attribute`, and domains beyond `py`/`c` | ❌ Not implemented | `Directive::DomainObject` wraps `DomainObjectBody`, a sum type with one variant per concrete object type (`PyFunction`, `PyModule`, `PyData`, `PyMethod`, `PyClass`, `PyAttribute`, `CFunction`), each carrying only its own fields — deliberately *not* one shared struct with every domain's options unioned together. `ObjectType`/`PyObjectType`/`CObjectType` remain a separate, lightweight tag vocabulary used by cross-reference roles (which never carry options). Adding a new object type means: a new `DomainObjectBody` variant + accessor arms (`crates/ast/src/domain_object_body.rs`), a new `PyObjectType`/`CObjectType` variant plus a `from_role_name` arm (`crates/ast/src/py_object_type.rs`/`object_type.rs`), a new `parse_*` function (`crates/parser/src/domains.rs`), a new cross-reference role regex + handler (`crates/parser/src/inline.rs`, if the object type has one), and a new match arm in `render_domain_object` (`crates/renderer/src/directives.rs`) — more touch points than the old fully-generic design, traded for no object type ever carrying another's irrelevant options. |
| `preview` subcommand default-domain awareness | 🔶 Partial | `preview` accepts `--default-domain` but always defaults to `py` today; the VS Code extension will eventually need to supply the enclosing library's actual Bazel attribute value. |
| Explicit `` :role:`title <target>` `` override for domain-object roles (`:func:`, `:mod:`, `:data:`, `:const:`, `:meth:`, `:class:`, `:attr:`) | ❌ Not implemented | Already supported for `:term:` via its own `<...>` parsing in `handle_inline_match`; domain-object roles only support the `!`/`~` target-prefix modifiers so far (see Inline Markup section above), not an explicit display-text override. |
| Cross-referencing domain objects (and targets/glossary terms) nested inside tables, lists, or admonition/version-change/seealso bodies | ✅ Implemented | `analyzer::analyze()` recurses into `Node::Table` cells, `Node::BulletList`/`Node::DefinitionList` items, directive bodies, and nested `DomainObjectBody` bodies via `index_nodes()`, not just top-level `doc.nodes` — matching the recursion `render_nodes` already does. Real Sphinx projects commonly nest `.. data::` definitions inside a grid table cell per row (e.g. CPython's `curses.rst` attribute tables); before this, such definitions parsed and rendered correctly but were invisible to the cross-reference index, so `:const:`/`:data:` references to them always rendered as broken links despite the target anchor existing on the page. See the "`py:data` Definitions Inside a Table" example in `examples/domains.rst`. |
| Automatic general-index entries for domain object definitions | ✅ Implemented | Every `Directive::DomainObject` definition automatically registers a `GenIndexEntry` (`crates/analyzer/src/lib.rs`), alongside its `domain_objects` cross-reference key, using its anchor id verbatim so genindex links resolve to the same `<dt id="...">` a `:func:`/`:class:`/etc. role would. Entry text is a simplified `"{qualified_name} ({object_type})"` (e.g. `"Greeter.greet (method)"`), not real Sphinx's fuller `"greet() (in module greetings)"` phrasing — a deliberate simplification, since rusty-sphinx has no module-context tracking for this beyond the existing `py:class` nesting/qualification. |

## General Index

| Feature | Status | Notes |
|---------|--------|-------|
| Site-wide `genindex.html` page | ✅ Implemented | `crates/renderer/src/genindex.rs::render_genindex` groups every `ProjectIndex.genindex_entries` (from `.. index::` directives and automatic domain-object entries, see above) alphabetically by first letter, with a "Symbols" bucket (non-alphabetic-leading terms) sorted first, subentries nested per primary term, multiple locations for one entry/subentry listed as multiple links, and `main` (`!`-prefixed) entries bolded. Always generated by `rusty_sphinx_site` (`rules/site.bzl`'s unconditional Phase 2.5) — matches real Sphinx's on-by-default `genindex.html`, not opt-in. Uses rusty-sphinx's own `<h2>`/nested-`<ul>` markup, not literal DOM/CSS parity with Sphinx's theme. |
| "Index" sidebar link | ✅ Implemented | `render_page()` (`crates/renderer/src/page.rs`) exposes a `genindex_href` template variable, computed via the existing `css_relative_path` helper (since `genindex.html` always lives at the site root, like `default.css`) whenever the project's merged index has any `genindex_entries` — sites with none get no dead link. Wired into `templates/default.html` and `examples/custom_template.html`. |

## Document Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Sections and document tree (nested sections) | 🔶 Partial | Heading levels are detected; no explicit section nesting in the AST |
| Transitions (`----`) | ✅ Implemented | A line of 4+ repeated punctuation chars with blank lines on both sides, rendered as `<hr />`. Diagnostics are emitted if a transition begins/ends the document or immediately follows another transition; no section-level check since sections aren't yet nested in the AST |
| Comments (`.. comment text`) | ✅ Implemented | Parsed as `Node::Comment`; produces no HTML output |
| Substitution definitions (`.. |name| replace::`) | ❌ Not implemented | — |
| Footnotes and citations | ❌ Not implemented | — |
