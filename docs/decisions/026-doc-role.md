# 26. The `:doc:` role: a document name, resolved while rendering

## Status

Accepted.

## Context

`` :doc:`name` `` links a whole page. It is the most common way a Sphinx
project links one page from another, and until now this build parsed it as
nothing at all: the catch-all entity-role pattern matched it, found no schema
role called `doc`, and left the markup as text. Our own guides were written
without it for that reason.

Most of what it needs already existed because of `:any:` (ADR-025), which
counts a document among the kinds it searches: `rinx_toctree::resolve_docname`
applied a toctree's naming rule to the target, and `:any:`'s renderer drew
the hit. Four questions were left: where the lookup belongs, how a page with no
title is found, what `!` shows, and whether a bare `:doc:` falls back to other
sites' inventories. The answers were checked against `sphinx-build` 9.1 on a
two-site project exercising every form.

## Decision

### 1. The AST records the name as written

The parser produces an `InlineNode::DocReference` holding the explicit title,
the target, the `!` and the `:external:` markup. `:std:doc:` is the same node.
The target stays relative, because resolving it needs the referencing
document's path and the index, both of which the renderer has — the same
reason `:ref:` and `:any:` keep theirs.

### 2. Every document is in the index, titled or not

`ProjectIndex::document_titles` holds only documents that open with a heading,
so a `:doc:` to a page without one would have been reported as broken. Sphinx
links it and shows `<no title>`. The index therefore gains `documents`, every
analyzed document's path, per-document so it merges on the preview path.
`ProjectIndex::find_document` is the one lookup and also accepts a document
found only in `document_titles`, so an index written before the field existed
still resolves. `:any:` switched to it, which means it now finds titleless pages
as well, and the written `objects.inv` lists them.

The toctree's document universe (`renderer::nav::expand`) still reads
`document_titles`. Sphinx gives a titleless toctree entry no link either, so
this was left for its own change.

### 3. One lookup and one link for both roles

`renderer::resolution::resolve_document` resolves a name, and
`inline::doc_reference::write_doc_link` draws the link:
`<a class="reference internal"><span class="doc">` around the explicit title,
else the document's title, else `<no title>`. `:any:`'s document hit calls
both. This keeps ADR-025 §3's rule that a hit renders exactly as its own role
would.

### 4. Inventories only when asked, with the name as written

Sphinx's `intersphinx_disabled_reftypes` defaults to `['std:doc']`, so a bare
`:doc:` never leaves its own site. A name like `index` is too generic to guess
at in someone else's inventory. This build does the same: only
`:external:doc:` and `:external+name:doc:` search inventories, and they skip
this site entirely. The target is looked up exactly as written. Sphinx 9.1
neither joins it to the referencing document's directory nor strips a leading
`/`, because another site's document names start at that site's root.

The inventory pruning (ADR-023) keeps every name a `:doc:` writes.

### 5. `!` shows everything after it

In Sphinx 9.1, `` :doc:`!Notes <notes>` `` shows `Notes <notes>`. A disabled
`XRefRole` renders its whole text, and no title is split off something that
won't be a link. This build does the same, as unlinked
`<span class="xref std std-doc">` text.

## Consequences

- `docs/compatibility.rst` records three deviations:
  - A leading `/` resolves from the Bazel workspace root, because that is where every document's path starts. This matches toctree entries here.
  - A target ending in `.rst` is found. This is inherited from the toctree naming rule; Sphinx finds nothing.
  - An unknown document is a broken link reported as `link.broken-doc`. Sphinx shows it as text.
- The user guides may now use `:doc:`.
- `:any:`'s `!` form still splits a title, where Sphinx shows the whole text.
  That is ADR-025's role and is not changed here.
