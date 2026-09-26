# ADR-003: Diagnostic Positions and Per-Block Suppression

**Status:** Accepted  
**Date:** 2026-08-30  

## Context

rinx reported every problem it found without saying where. A warning read:

```
warning: broken ref 'missing-section' in guide/intro.rst
```

For CPython's `library/stdtypes.rst` — 5,600 lines — that is close to unusable. Worse, the two families of warning had drifted into unrelated shapes:

- **Parse-time** diagnostics were bare `String`s on `Document.diagnostics`, printed by `parse_with_ctx` from inside the parser crate and then never read again.
- **Render-time** diagnostics were the typed `BrokenLink` / `ObjectTypeMismatch`, printed by the worker.

Neither carried a position, and there was nothing they shared to hang one on. Three things stood in the way:

1. **The parser knew the line and discarded it.** Every block parser has the shape `try_parse_X(lines: &[&str], i: usize, …)`, so the index was in scope at every reporting site. Only the two table parsers used it, by formatting `start_i + offset + 1` into the message text by hand.
2. **Nesting destroyed the offset.** Roughly two dozen sites call `parse_blocks` on a *freshly built* `Vec<&str>` of body lines. Inside that recursion the index restarts at zero, and most of those sites also dedent, so the column moves too.
3. **The AST had no positions at all**, so render-time diagnostics — the common ones — had nothing to recover a position from.

Separately, a document's author had no way to say "yes, I know". The only filter that existed was `scripts/domain_warnings_whitelist.json`: developer-owned, out-of-band, and keyed by `(doc_path, kind, target)` rather than by anything in the document.

`architecture.md` already promised better ("functions will return `(Output, Vec<Diagnostic>)` … so the LSP can underline multiple errors"), and `requirements.md` §4 lists LSP syntax diagnostics as a goal. Underlining needs a range, so this was an unmet requirement rather than a nicety.

## Decisions

### 1. A span is a range, not a point

`Span` holds a `start` and an `end` `Position`, both 1-based.

A point would have been cheaper to *use* but not cheaper to *produce*: the inline scan already computes both ends of every match it builds, and a block-level parser has the offending line's text in hand, so `Span::whole_line` costs the same call a point would have. Deferring the end would have meant revisiting the identical ~60 reporting sites and ~180 node-construction sites a second time, once the language server needed it.

The terminal still prints only the start (`guide/intro.rst:42:18`); a `42:18-42:35` form is noise to a human. The end exists for the LSP and for the `--warnings-output` sidecar.

### 2. Columns count characters

`Position::column` counts Unicode scalar values. Bytes would make a column meaningless on any line containing `—`, `’` or `π`, which real documentation constantly does.

LSP's default `PositionEncodingKind` is UTF-16, so the future `rinx_lsp` crate converts at its own boundary. Do not "fix" one side to match the other in the middle.

### 3. Positions survive nesting by riding on `ParseCtx`

`ParseCtx` gained a private `origin`: where `lines[0]`, column 0, of the slice currently being parsed sits in the original document. `ctx.nested(line_offset, column_offset)` returns a rebased copy, and offsets compose, so a role inside a list item inside a directive body still resolves correctly.

`ParseCtx` was chosen over a new parameter because it was already threaded through *every* block-level parser and is already immutable and `&`-shared. No signature outside `context.rs` changed.

The consequence worth knowing: **any code that dedents or re-slices lines before parsing them must rebase the context by the same amount.** Every such site does — the directive dispatcher, all three list parsers, both table cell extractors, and the glossary. `normalize_cell_lines` and `collect_directive_body` were changed to *return* how much they trimmed rather than silently discarding it, precisely so their callers can.

Content that corresponds to no source position at all — the rows a `.. csv-table::` builds out of CSV data, which may not even come from this file — parses under `ctx.synthetic()` and reports positionless diagnostics. Pointing at a line that does not contain the offending text would be worse than pointing nowhere.

### 4. Inline spans land in one place

`parse_inline_text_mapped` attaches a span as each node is pushed, using the `start`/`end` the scan already has. **No role parser under `inline/roles/` changed.** The escape rewrite is length-preserving by design, so those offsets still address the original text.

The reflow that block parsing performs (trim each line, join with `\n`) is undone by a `SourceMap` built *while* the joining happens — the one moment both forms exist.

Only the six reference-bearing `InlineNode` variants carry a span, because only they can fail to resolve. Giving every variant one would roughly double a parsed document to record something nothing reads.

This was deliberately *not* done as a wrapper variant (`Located { span, node }`). A wrapper is invisible to the `if let` and `matches!` checks scattered through the analyzer and renderer, which would silently stop matching. A field on the variant makes every construction site a compile error, which is the point.

### 5. Diagnostic codes are an enum, generated from one table

`DiagnosticCode` is a flat enum with a dotted id (`link.broken-ref`, `table.grid.no-columns`). The enum, its `as_str`, its `FromStr` and its `ALL` listing are generated by one macro from a single table, because the two directions must be exact inverses: an author writes an id in a `.. noqa:` and the parser maps it back to the variant the diagnostic was raised with. Three hand-written tables would let a half-finished edit compile and silently break suppression for one code.

A string would have been simpler and worse: a typo in *our* code would be silent, where now it does not compile.

Ids become documented surface the moment an author writes one into a document. Treat a rename as a breaking change.

### 6. Suppression is a comment, scoped to the next block

```rst
.. noqa: link.broken-ref, link.broken-term

See :ref:`work-in-progress` for details.
```

**Why a comment and not a directive.** A directive (`.. rinx:allow::`) would give us option parsing for free, but real Sphinx errors on an unknown directive. A comment it ignores silently, so a document using suppression stays buildable by both tools. That dual-buildability is worth more than the parsing convenience.

**Why the next block.** It is the usual lint-pragma scope, it is local, and it expires by itself. Because `parse_blocks` recurses, a `.. noqa:` inside a directive body or list item resolves against *that* block, and one before a container covers everything nested in it — both of which read the way an author expects.

A comment between the suppression and its block — including a second `.. noqa:` — does not consume it, so two written in a row both apply.

**Why a mistyped id is reported.** `noqa.unknown-code` fires for any id that names no diagnostic. A suppression that silently matches nothing is exactly the "silently degrades valid-looking input" case `guidelines.md` says to diagnose. A bare `.. noqa` is supported but discouraged: it also hides the next problem to appear in that block, which nobody chose.

### 7. Suppression is applied at the reporting boundary

The parser records *everything* and filters nothing. The worker filters, in `commands/suppression.rs`, immediately before printing.

This is not a stylistic choice. A broken link is found while rendering, in a different process from the parse that read the comment excusing it — the two only meet through the `.ast`, which is why `Document.suppressions` is serialized. It also keeps the existing separation the project already relies on: a phase finds problems, the build step decides which a human sees.

Filtering happens *inside* `process_render` and `process_preview` rather than in their callers, so a suppressed link is invisible to every consumer: the warning it would print, the `--strict-links` failure it would cause, and the `--warnings-output` sidecar it would appear in. Silencing the message while keeping the consequence would make the mechanism useless.

A diagnostic with no span is never suppressed. There is nothing to match against, and silencing it on the strength of its code alone would reach across the whole document.

## Consequences

- Every warning now reads `path:line:column: code: message`, and the path is the `.rst` the document was parsed from — `render` previously named the site-relative `doc_path` (`examples/domains`), which does not open in an editor.
- `Document.diagnostics` changed from `Vec<String>` to `Vec<Diagnostic>`. `.ast` files are Bazel intermediates and never checked in, so there is no on-disk compatibility burden; `scripts/benchmark.py` now aggregates on `diag["code"]`, which is strictly better bucketing than the message-prefix heuristic it replaced.
- `examples/noqa.rst` demonstrates the mechanism and doubles as a test: the example site is built with `strict_links = True` and that page contains seven deliberately broken references, every one suppressed.

## Not Decided Here

- **Ranges on AST nodes.** `Span` covers diagnostics, which is what `textDocument/publishDiagnostics` needs. Folding, document symbols and go-to-definition would need a range on every `Node` — a much larger change, and only once `rinx_lsp` exists.
- **A `noqa.unused` warning.** It cannot be decided in one phase: a document's parse-time and render-time diagnostics are evaluated in different processes, so neither alone can tell that a suppression matched nothing.
- **Retiring `scripts/domain_warnings_whitelist.json`.** The benchmark corpus is third-party CPython source we do not edit, so an in-document mechanism cannot replace it.
