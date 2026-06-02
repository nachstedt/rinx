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

## Inline Markup

| Feature | Status | Notes |
|---------|--------|-------|
| Bold (`**text**`) | ❌ Not implemented | Emitted as plain text |
| Italic (`*text*`) | ❌ Not implemented | Emitted as plain text |
| Inline code (`` `text` ``) | ❌ Not implemented | Emitted as plain text |
| Interpreted text roles (e.g. `` :ref:`label` ``) | 🔶 Partial | `:ref:` (resolves to references) and `:program:` (renders as `<strong class="program">`) are implemented. |
| Hyperlink references (`link text <URL>`_) | ❌ Not implemented | — |
| Anonymous hyperlinks (`link text <URL>`__) | ❌ Not implemented | — |

## Block-Level Elements

| Feature | Status | Notes |
|---------|--------|-------|
| Bullet lists (`-`, `*`, `+`) | ❌ Not implemented | — |
| Enumerated lists (`1.`, `a.`, `i.`, …) | ❌ Not implemented | — |
| Definition lists | ❌ Not implemented | — |
| Field lists (`:field: value`) | ❌ Not implemented | — |
| Option lists | ❌ Not implemented | — |
| Block quotes (indented paragraphs without a directive) | ❌ Not implemented | — |
| Line blocks (`| line`) | ❌ Not implemented | — |

## Directives

| Feature | Status | Notes |
|---------|--------|-------|
| `.. toctree::` | ✅ Implemented | Rendered as an `<ul>` list of links |
| `.. code-block::` | 🔶 Partial | Parsed into AST as `Directive::Unknown`; not rendered to HTML |
| `.. note::`, `.. warning::`, `.. tip::` (admonitions) | ❌ Not implemented | Parsed as `Directive::Unknown`; no HTML output |
| `.. image::` | ❌ Not implemented | — |
| `.. include::` | ❌ Not implemented | — |
| `.. literalinclude::` | ❌ Not implemented | — |
| `.. automodule::` / `.. autofunction::` (autodoc) | ❌ Not implemented | Sphinx extension; out of scope for core parser |

## Document Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Sections and document tree (nested sections) | 🔶 Partial | Heading levels are detected; no explicit section nesting in the AST |
| Transitions (`----`) | ❌ Not implemented | A line of 4+ punctuation chars with blank lines on both sides; currently parsed as a heading |
| Comments (`.. comment text`) | ❌ Not implemented | Treated as an unknown directive |
| Substitution definitions (`.. |name| replace::`) | ❌ Not implemented | — |
| Footnotes and citations | ❌ Not implemented | — |
