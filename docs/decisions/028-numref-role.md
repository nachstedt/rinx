# 28. The `:numref:` role and `numfig` numbering

## Status

Accepted.

## Context

`` :numref:`label` `` links a figure, table, code block or section and shows
its number, such as `Fig. 2` or `Section 1.3`. It depends on Sphinx's `numfig`
numbering, which this build did not have: nothing numbered a figure, and no
caption showed a number. Until now the role was parsed as nothing. The
catch-all entity-role pattern matched it and left the markup as text.

The behaviour was captured from `sphinx-build` 9.1 on a three-document project
covering every form, with and without a `:numbered:` toctree, at
`numfig_secnum_depth` 0, 1 and 2, and with `numfig` off. Sphinx's own
`assign_figure_numbers` was read for the walk. Some findings contradicted the
first plan:

- **Every captioned figure, table and code block is numbered, labelled or
  not.** docutils gives each one an id, and Sphinx skips only uncaptioned
  ones. So an unlabelled figure still takes a number and moves the count on.
- **A label on an uncaptioned element is an "undefined label"**, not an
  unnumbered one, when the element is a code block, because an uncaptioned
  code block is not enumerable at all. An uncaptioned figure or table is
  enumerable but gets no number.
- **Numbers count across documents in toctree order.** A child document is
  numbered at the point where its toctree stands, between the parent's own
  figures. A `:numbered:` chapter restarts the count under its number, cut to
  `numfig_secnum_depth` components.
- **A section is numbered whether `numfig` is on or not.** Only figures,
  tables and code blocks need it.
- **Which formatting style applies is a substring test.** Sphinx uses Python's
  `str.format` when the title contains `{name}` or the bare word `number`, and
  `%` otherwise. So `only number` links with that text, and `see this` fails
  with "not all arguments converted".
- **`numfig_format` goes through `%` alone when it is written in front of a
  caption.** A `{number}` format there crashes Sphinx's HTML writer.

## Decision

### 1. The AST keeps the question, and a parsed title

`InlineNode::NumberReference` holds the label as written and the explicit
title, already parsed into a `NumberFormat`. `:std:numref:` produces the same
node. Which element the label names, and its number, are project facts, so
they are resolved while rendering, as for `:ref:` and `:doc:`.

### 2. A title Sphinx could not apply is reported while parsing

`NumberFormat` reproduces Sphinx's style test exactly, and refuses what Python
would raise on: no slot, two `%s`, an unknown field, an unbalanced brace. It
does so where the author's text is read. The inline scan has nowhere to report
a problem, so a refused role becomes an intermediate
`InlineNode::RefusedNumberReference`. A whole-document pass reports it at the
role's span and lowers it to the unlinked text Sphinx shows. This is the same
shape `SubstitutionReference` already had. The pass reuses the substitution
resolver's traversal over every inline list, which was extracted into
`parser/blocks/inline_lists.rs` so the two cannot reach different content.
`:external:numref:` is refused the same way, since an `objects.inv` holds no
numbers.

### 3. One function decides which elements are numbered

`rinx_ast::enumerable_elements` walks a document and returns its captioned
figures, tables and code blocks in order, each with its labels. The analyzer
stores numbers by an element's position in that list. The renderer calls the
same function on the same document and finds each caption's number by the
directive's address. Neither side counts elements on its own, so a container
one of them fails to descend into cannot shift every number after it.
`walk_nodes` gained a sibling-aware form, `walk_nodes_with_siblings`, for
this: an element's labels are the `.. _x:` targets directly before it.

### 4. Input merges, numbers are recomputed

The index splits this the way it splits toctrees from section numbers:

- `numbering_steps` is each document's elements and top-level toctrees in
  order. It is per document and merges.
- `numref_targets` maps each label to an element's position or a section id.
  It is per document and merges.
- `element_numbers` holds the numbers the walk assigned. It is project-wide
  and recomputed by a new `build_project_index` phase that runs after section
  numbering, because a figure's number carries its section's number.

The walk is a port of Sphinx's own. The live preview re-runs it after merging
the edited document, since it is cheap and a figure added in the editor would
otherwise shift every number after it. Section numbers stay stale there until
the next build, as before.

### 5. Configuration follows Sphinx's names and defaults

`rinx.toml` gains `numfig`, `numfig_secnum_depth` and a `[numfig_format]`
table. `numfig` is **off by default**, as in Sphinx, so a site that does not
ask for it renders exactly as before. `numfig_secnum_depth` is read by the
index action and the other two by the renderer. Every format is a
`NumberFormat` validated on load, and `{name}` is refused there: the same text
goes in front of the caption it would name.

### 6. Every unresolved `:numref:` is reported, in Sphinx's order

The renderer checks in this order:

1. A label naming nothing numberable is a broken link, `link.broken-numref`.
2. `numfig` off for anything but a section is `numref.disabled`.
3. An element or section without a number is `numref.unnumbered`.
4. A `{name}` with no caption is `numref.no-caption`.

Every case except the broken link shows the written text unlinked, as Sphinx
does. All of them travel as `BrokenLink`s, so `.. noqa:` and `strict_links`
treat them alike. The live preview drops `numref.unnumbered` when it has no
index, since nothing can have a number then.

## Consequences

- `docs/compatibility.rst` records these deviations:
  - A refused title is reported while parsing.
  - An unknown label is drawn as a broken link.
  - A `.. sectnum::` number is shown.
  - `{number}` is accepted in `numfig_format`, and `{name}` is refused there.
  - `:external:numref:` is refused.
- Turning `numfig` on changes every captioned element's HTML across the site,
  and so re-renders every page. That is expected: the numbers are
  project-wide.
- A `:numref:` inside a `replace` substitution that is used twice is reported
  once per use, because the reporting pass runs after substitutions are
  spliced in.
