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
| Interpreted text roles (e.g. `` :ref:`label` ``) | 🔶 Partial | `:ref:`, `:program:`, `:term:`, and the domain role `:func:` (plus `:py:func:`/`:c:func:`) are implemented. |
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
| Literal blocks (``::`` paragraph ending or standalone ``::``) | ✅ Implemented | Rendered as ``<pre><code>...</code></pre>`` |
| Block quotes (indented paragraphs without a directive) | ❌ Not implemented | — |
| Line blocks (`| line`) | ❌ Not implemented | — |

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
| Default domain resolution for bare (unprefixed) directives/roles | ✅ Implemented | The default domain is a `rusty_sphinx_library` Bazel attribute (`default_domain`, default `"py"`), resolved once at parse time — not a `rusty_sphinx.toml` setting, since it describes the library's content rather than the consuming site. |
| Object name extraction from C-style pointer return types (e.g. `char *foo(void)`) | 🔶 Partial | `extract_object_name` naively yields `"*foo"`; real C declarator parsing is out of scope. |
| Additional object types (`class`, `struct`, `member`, …) and domains beyond `py`/`c` | ❌ Not implemented | The enum-per-domain design (`ObjectType::Py(PyObjectType)` / `ObjectType::C(CObjectType)`) is meant to extend via new enum arms without touching other domains. |
| `preview` subcommand default-domain awareness | 🔶 Partial | `preview` accepts `--default-domain` but always defaults to `py` today; the VS Code extension will eventually need to supply the enclosing library's actual Bazel attribute value. |

## Document Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Sections and document tree (nested sections) | 🔶 Partial | Heading levels are detected; no explicit section nesting in the AST |
| Transitions (`----`) | ❌ Not implemented | A line of 4+ punctuation chars with blank lines on both sides; currently parsed as a heading |
| Comments (`.. comment text`) | ✅ Implemented | Parsed as `Node::Comment`; produces no HTML output |
| Substitution definitions (`.. |name| replace::`) | ❌ Not implemented | — |
| Footnotes and citations | ❌ Not implemented | — |
