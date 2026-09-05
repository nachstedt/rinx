# 8. Source transclusion: `.. include::` and `.. literalinclude::`

## Status

Accepted.

## Context

Both directives name a file and put its contents into the page. They are among
the most-used directives in real Sphinx projects — shared boilerplate
fragments, and showing real source files instead of copy-pasted snippets — and
the CPython corpus the benchmark builds leans on both.

They are also the first constructs whose content comes from a *different file*
than the one being parsed. Three things in this codebase assumed one document
== one file: source positions, `.. noqa:` suppressions, and the Bazel
declaration of what a build action reads.

## Decision

### Inclusion happens while parsing

Both directives read their file in `rusty_sphinx_parser`, through the injected
`ParseFileLoader` seam on `ParseCtx` that `.. csv-table::`'s `:file:` already
used. The parser still performs no I/O of its own;
`crates/worker/src/commands/parse_files.rs` supplies the filesystem, and the
live-preview path gets the same behaviour for free.

Neither directive could be a "resolve later" AST node:

- `.. include::` inserts **reStructuredText**. Sections, hyperlink targets,
  `.. toctree::` entries and index entries inside it belong to the *including*
  document. Deferring would break section structure, outlines, section
  numbering and toctree validation at once.
- `.. literalinclude::` inserts **verbatim text** that `:dedent:`, `:lines:`,
  `:start-after:` and the rest transform before anyone sees it — exactly the
  transform `.. code-block::` already performs while parsing.

### An include is a transclusion, not a container

`.. include::` is the only directive that yields *several* nodes, which is why
`try_parse_directive` returns `Option<(usize, Vec<Node>)>`. A wrapper node
would have been cheaper and is wrong: it would nest the fragment's sections
inside itself, so a heading in a shared fragment would stop being a heading of
the page that included it.

For the same reason the include parser does **not** reset `adornment_order`.
An included heading takes its level from the including document's own adornment
sequence, which is what makes transclusion transparent.

### There is deliberately no cache firewall

The doctest plans (ADR 2) and the embedded image assets (ADR 7) both exist to
keep expensive downstream work from re-running when an input changes in a way
that does not matter. There is no equivalent here, and there should not be: an
included file's text genuinely *is* part of the including document's `.ast`, so
editing a fragment must re-parse and re-render every page that includes it. The
page really did change.

That is why `parse_data` files are ordinary parse-action inputs with no sidecar
in between.

### A span carries the file it was measured in

This is the part that took two attempts.

The first design put the file's path on `Diagnostic`. That handles parse-time
diagnostics and nothing else. **Render-time** diagnostics — a broken `:ref:`, a
`:math:` that will not convert, a code block whose grammar fails — take their
position from the `Span` stored on an AST node, in a different process, long
after the parse. Once an include splices a fragment's nodes into the document,
those spans hold the *fragment's* line numbers. A path recorded only on
parse-time diagnostics cannot fix that: a broken cross-reference in a shared
fragment would be reported against the including document's unrelated line 3.
Confidently wrong is worse than positionless.

So the file rides on `Span`:

```rust
pub struct Span {
    pub start: Position,
    pub end: Position,
    pub file: Option<FileId>,   // None = the document being processed
}
```

`FileId` is an interned `u32` indexing `Document::source_files`, not a path.
`Span` is `Copy` and is passed by value through every reporting signature in
the workspace; a `String` on it would end that, and an `Arc<str>` would too.
There is one span per inline node in an `.ast`, so the four bytes matter.

`Diagnostic` needed no new field at all — it already carries a `Span`.

Two invariants follow, and both are enforced rather than documented:

- **Every span is built in one place.** `ParseCtx::span` is the only
  constructor that stamps the file, and the crate's three span-building helpers
  (`ParseCtx::line_span`/`lines_span`, the simple-table context's, and the
  inline `SourceMap`'s) all route through it. A span that skipped it would carry
  a fragment's line numbers under the document's name.
- **A `.. noqa:` is file-scoped.** `Suppression` carries the matching
  `Option<FileId>`, and `Suppression::suppresses` compares it. Without that, a
  comment on line 3 of a document would silence a diagnostic from line 3 of a
  fragment it includes — a different place entirely, and one the author never
  looked at.

The worker's `WarningOrigin` (`commands/diagnostics.rs`) is the single place an
id becomes a path again:

```text
warning: shared/params.rst:7:3 (included from guide/api.rst): table.grid.no-columns: ...
```

One line, not two, because these are read with `grep` as often as with eyes.
The fragment is named first: it is the file the author has to open. An id with
no table entry drops the position rather than printing the document's path
against a foreign line number.

### `.. literalinclude::` adds no AST node

It lowers to the same `Directive::CodeBlock` the two inline code directives
produce, tagged `CodeBlockSource::LiteralInclude`. Every option it adds is a
parse-time source transform — `:lines:`, `:dedent:`, `:prepend:`,
`:lineno-match:`, `:diff:` — so by the time the AST exists it *is* a code
block: the same content and the same nine presentation options.

The renderer needed no changes whatsoever. Highlighting, line numbers,
emphasis and captions all worked as they stood.

### Selecting nothing is always a diagnostic

`:start-after:` text that appears nowhere, a `:lines:` past the end of the
file, a range that ends before it begins, bounds that cross: each is its own
code in the `include.*` family rather than a silently empty block. A marker
that does not match is a typo nine times out of ten, and a directive that
answered it with silence would leave the author looking at a page with a
section quietly missing.

## Consequences

- One Bazel attribute, `parse_data`, declares everything the parser reads:
  csv data, included reStructuredText, literal-included sources. It replaced
  `csv_data` outright rather than being added beside it — the project is
  pre-1.0 and one honest name beats two overlapping ones.
- An `.rst` reached by `.. include::` must **not** also appear in `srcs`, which
  would publish it as a page of its own as well as splicing it in.
- A `.. toctree::` written *inside* an included fragment still needs its
  documents in `deps`: `validate_toctree` runs on the already-merged AST, so
  this works, but the dependency is real.
- Nested includes resolve relative to the file they are written in, as docutils
  does, which is why `ParseFileLoader::load` takes a `relative_to` and returns
  a resolved `id` alongside the text. That id is also what cycle detection
  compares — `../a/x.rst` and `x.rst` may well be the same file.
- A cycle is reported with the whole chain (`a.rst -> b.rst -> a.rst`), because
  the offending edit may be in any link of it.

## Narrowings

- `:pyobject:` on `.. literalinclude::` is refused with
  `literalinclude.pyobject-unsupported`, pointing at `:start-after:`/
  `:end-before:`. Implementing it means a Python block scanner, which belongs
  in its own change.
- `:parser:` on `.. include::` (pluggable alternate markup parsers) is
  unsupported.
- `:encoding:` accepts UTF-8 and its ASCII subset only, matching the limit
  `.. csv-table::` already had.
- `:lineno-match:` is refused on a `:diff:` or a discontinuous `:lines:`, since
  neither has a single line to number from.
