# 31. Collapsed sidebar navigation

## Status

Accepted.

## Context

The sidebar expands every root document's toctrees on every page
([ADR-005](005-toctree-model.md)) and the default template folds the branches
off the current path with `<details>`. Folding hides the tree from the reader,
but every byte of it is still written into every page, so a site's sidebar
grows with the square of its page count.

On CPython's documentation — 529 pages — that made the rendered site
**612 MB**: `library/os.path.html` was 1.18 MB, of which 1.10 MB was
`<nav class="sidebar-nav">` and 77 KB the page's own content. Publishing the
benchmark sites next to the documentation on GitHub Pages, which caps a site
at 1 GB, was impossible at that size, and so is any real project of that size.

Sphinx has the same problem and the same answer: `toctree(collapse=True)`,
which its themes turn on by default (`collapse_navigation` in
sphinx_rtd_theme, `globaltoc_collapse` in alabaster). It keeps every top-level
entry and nests only the entries on the path to the current page.

## Decision

`rinx.toml` gains `collapse_navigation`, **on by default**, read into a
`SidebarTree` (`CurrentBranch` or `Full`) as the config is loaded, so no later
phase re-inspects a boolean.

With `CurrentBranch`, the sidebar keeps every top-level entry, and keeps the
children only of the current page and its ancestors — so the ancestors'
siblings are listed, and the current page's own children are, but theirs are
not. That is exactly where Sphinx's `_toctree_collapse` stops: it removes the
nested list of every item not marked `current`.

It is a **post-pass** over the tree `sidebar_entries` already builds
(`renderer/src/nav/collapse.rs`), not a change to the walk in `expand.rs`.
That walk is shared with an in-page `.. toctree::`, which never collapses, and
keeping the two on one walk is what ADR-005 bought. The cost is that the
sidebar is still *expanded* in full before being pruned, so rendering time is
unchanged; only the output shrinks.

The template needed no change: an entry with no children already renders as a
leaf without a `<details>` toggle.

## Consequences

- CPython's rendered site went from 612 MB to **59 MB**, and
  `library/os.path.html` from 1.18 MB to 88 KB. Its corpus build took 19.8 s.
- A reader can no longer unfold a distant branch in place; they follow its
  link instead, as in any Sphinx theme. `collapse_navigation = false` restores
  the folded full tree, which suits a small site.
- Rendering time still grows with the square of the page count. If that
  becomes the bottleneck, the walk itself can stop descending off the current
  branch — at the cost of giving the sidebar a walk of its own.
