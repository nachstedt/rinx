# 48. The preview scrolls with the editor, as Markdown's does

## Status

Accepted.

## Context

Roadmap #34 asks that the preview follow the editor and lead back to the
source. VS Code's built-in Markdown preview already does this, and an author
moving between a Markdown and a reStructuredText file expects the two to
behave alike. Markdown's preview has four settings, all on by default:

- `scrollPreviewWithEditor`: scrolling the editor scrolls the preview so the
  editor's top visible line is at the preview's top;
- `scrollEditorWithPreview`: scrolling the preview scrolls the editor back;
- `markEditorSelection`: the block holding the cursor is marked with a bar;
- `doubleClickToSwitchToEditor`: a double-click on the preview puts the
  cursor on the line the block was written on.

The cursor itself does not scroll Markdown's preview; it only marks it. The
roadmap's "follows the cursor" means that marker.

Markdown's renderer puts each block's source line on its first element
(`data-line`), and the webview's `scroll-sync.ts` maps between those lines
and the page's vertical positions. A rinx page names nothing of its source:
since ADR-045 a `Document` holds no lines, only `BlockId`s into the
`SourceMap` the parse leaves beside it, and the build never sees that map.
Before this step only definitions carried a `BlockId`.

## Decision

### Every rendered block carries a `BlockId`

PR #407 gave a `BlockId` to every node that renders a block: a paragraph, a
list item (not the list, so that two items of one list are two positions), a
definition or option list item, a table, a literal or doctest block, a block
quote, a line block, a transition and a directive, at its marker line (which
moved `block` from three `Directive` variants up to `Node::Directive`). The
parser's block loop places each node over the lines it consumed; a construct
yielding several nodes places them itself.

Ordinals assigned by the renderer were rejected: the ids have to name lines,
which only the parse knows, and the renderer's later passes (anonymous
targets, substitutions, numbering) would otherwise have to keep a second
numbering in step with the parser's.

The cost, measured on CPython's 555 documents: the `.ast` files grow from
32.0 MB to 34.2 MB (+6.9 %), and parsing from 7.75 s to 7.92 s. The HTML of
669 documents (CPython, `docs/`, `examples/`) is byte-identical, and so is
every `.ast` when lines are added above its blocks.

### The page names a line only when asked

`PageAssets::source_lines` (an `Option<&SourceMap>`) asks the renderer to
put `data-source-line` — the 0-based line, as an editor counts — on the first
start tag each placed block writes, and `data-source-end-line` as well on a
literal or doctest block, whose lines show one to one. The attribute is
inserted after the tag's name once the block is written, so no block's
writer had to change. A tag a nested block marked already keeps its mark,
and a block written in another file (an `.. include::`d fragment,
`rst_prolog`) is left unmarked, since its lines are not the document's.

The build cannot ask: a `SourceMap` has no serde impl, so no build phase
holds one, and its pages are unchanged by construction. The language
server's `rinx/preview` request takes `sourceLines: true`, defaulting to
false, so the parity test comparing the preview with `rinx render` still
compares the build's page.

### The webview ports Markdown's scroll sync

`editors/vscode/src/scrollSync.ts` is shipped into the webview as its own
source, as the diagram drawer is (ADR-042). It reads the marked blocks in page
order, passing over a hidden one (in a closed dropdown) and one naming an
earlier line than a block before it (drawn ahead of where it was written),
and interpolates: between two blocks' tops, through a literal block's lines,
or into the last block. The four settings are `rinx.preview.*`, under
Markdown's names, read each time they apply so changing one needs no reload.

The editor's top line is Markdown's fractional line — the first visible line
plus how far into it the view starts — and scrolling the editor to a
fractional line reveals that line from as far into it, at the top. Each side
scrolling the other could loop, so a scroll made to follow the other side is
not reported back for 100 ms; Markdown counts such scrolls instead, which
swallows the reader's next scroll whenever one scrolled nowhere.

## Consequences

- A page with blocks from an `.. include::` interpolates across them, from
  the `.. include::`'s neighbours.
- A directive's first tag names its marker line, so scrolling through a long
  admonition interpolates between its marker and the blocks of its body,
  which carry their own lines.
- The marker is a bar left of the block, in a stylesheet the webview adopts,
  so it survives the page's head being swapped for the next page's.
- Only the VS Code extension uses the attributes; another client asking for
  `sourceLines` gets the same page.
