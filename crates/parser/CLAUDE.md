# Notes on `rinx_parser`

`parse(path, text, &ctx)` is the one entry point; it returns the `Document` and the `SourceMap` beside it. The crate performs no I/O: files are read through `ParseCtx`'s injected `ParseFileLoader`.

## Positions

- **Any code that dedents or re-slices lines before calling `parse_blocks` must rebase with `ctx.nested(line_offset, column_offset)`**, or every position inside that block is wrong by the amount trimmed. That is why helpers that trim return how much they trimmed.
- Content with no source line (generated rows, imported needs) parses under `ctx.synthetic()` and reports positionless.
- A Jinja-rendered source (`jinja = True`) moves lines, so `ParseCtx::position` resolves every position through the `TemplateMap` last; it is not an offset and does not compose.

## Directives and roles

- Every directive with a fixed name has a `DirectiveSpec` beside its parser, listed in `directives/spec/registry.rs`'s `DIRECTIVES` with its interpreter; `directives/reader.rs` is the one reader splitting a block into arguments, options and content (ported from docutils). Never split a block by hand. Keep specs syntax-independent: a future MyST front end should produce the same invocation.
- Options are declared with `directive_options!`, one row per option, generating the enum the parser matches with no `_` arm, so an option listed but not read (or read but not listed) fails to compile. An option the reference implementation takes but this build does not is a `_ =>` row with advice (`directive.unsupported-option`); a row can name a type its value is parsed into, so interpreters never see a bad value.
- Roles work the same way: a `RoleSpec` beside each handler, one `ROLES` list pairing spec, pattern and dispatch.
- An unknown directive becomes `Directive::Unknown` and its body is not parsed; a recognized one whose content must be refused becomes `Directive::Malformed`. Both render as visible error blocks.
- A directive that splices several nodes into the enclosing block (`.. include::`, `.. if-builder::`, `.. needimport::`) parses under `DirectiveInput::splice_ctx`, so headings, targets and toctrees inside it belong to the document. Don't wrap spliced content in a container node.
- **A new container directive needs an arm in every traversal that walks block content** (`analyzer`'s `index_nodes`, `parser`'s `assign_index_ids`, `renderer`'s `collect_anonymous_targets`, `ast`'s `walk_nodes`), or targets written inside it silently vanish. Parse its body under `ctx.nested(...)`. An unreadable option never costs the container, and a misplaced child is reported and kept.
- Directives from extensions (sphinx-design, sphinx-needs, sphinx-simplepdf) are always available, since there is no `extensions =` config. Where we name a construct ourselves (`.. entity-table::`), the sphinx-needs name is an alias, with one node recording which was written.
