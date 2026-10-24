# 45. Definitions know their lines, and the written index does not

## Status

Accepted.

## Context

Roadmap #28 gives the language server an outline, and #29–#33 need more of
the same knowledge: go to a label's line, find its references, rename it, fold
a section. Each needs to know where a definition was written. Until now, the
only definitions with a position were an entity's marker, an equation, a code
block and an image. A section title, a `.. _label:`, a domain object, a
glossary term and a table's `:name:` had none, and neither had any entry of
`ProjectIndex`.

Where a position is stored decides what a Bazel build re-runs. A document's
`.ast` is one parse action's output, and it already moves with its lines:
inline references, diagnostics and entities carry spans. The written
`.index`, though, is an input of **every** render action. A position in it
would re-render the whole site whenever a line moved anywhere.

## Decision

### The spans live in the `.ast`, on the definition

`Node::Heading` and `Node::Target` carry a `span`, as do
`Directive::DomainObject`, each glossary term (`GlossaryTerm`, replacing a
bare `String`), `.. table::` and the two data-table directives. A domain
object's span sits once beside the object, `DomainObject { object, span }`,
not in each `DomainObjectBody` variant: where a directive was written does not
depend on which kind of object it documents, and one field gives every kind a
span by construction, set in one place, the parser's dispatcher. Each field is optional and skipped when absent,
as the existing spans are. A section's span is its title's text line. A
directive's span is its marker line, which `DirectiveInvocation::span`
already holds. A definition inside an `.. include::`d fragment carries the
fragment's `FileId`, as any span does.

### The analyzer collects them beside the index, never into it

The indexing walk records each definition's span at the very place it inserts
the name into the index (`definition_spans.rs`'s `DefinitionSpans`). A site
therefore exists under exactly the key the index names the definition by:
scope qualification, an option's several names and `:no-index:` included,
with no second copy of those rules. The key is `rinx_index::DefinitionKey`:
`Label`, `Term`, `Object(type, name)`, `Equation`, `Entity` or
`Section(id)`, the last by the id `allocate_section_ids` gives, as the outline
and the renderer's headings do.

`analyze_with_sites` returns the result, `DefinitionSites`, beside the index
fragment. `DocumentAnalysis` keeps it, which no fold reads. A fragment's
`FileId` means nothing outside its document, so a site names its fragment by
path. The build's `index` action builds the analyses and writes only the
`ProjectIndex`, so no position reaches the `.index`.
`crates/worker/tests/integration/definition_spans.rs` checks this: parsing a
document again with its definitions moved down leaves the written index
byte-identical, while every site moves.

### The language server keeps them per document

The server keeps one `DocumentAnalysis` per document, open or closed, and no
closed document's AST. So the sites ride along for free:
`Project::index_and_definitions` lends the fold beside a `Definitions` view
over every document's sites. An analysis that differs only in its sites
replaces the recorded one without folding again
(`DocumentAnalysis::folds_like`), so pressing Enter above a heading costs no
fold, as before.

## Consequences

- The outline (`textDocument/documentSymbol`) reads the open document's
  latest parse directly. `:ref:` completion names the fragment a label was
  written in. Go to definition (#29) reads the same `Definitions`.
- A `.ast` written before this change no longer loads where it holds a
  glossary term, now an object rather than a string, or a domain object, now
  wrapped with its span. Every other new field defaults. That is acceptable
  for a build intermediate that is always rebuilt by the binary that reads
  it.
- `EntityRecord::relation_spans`, read only by the fold that reports a
  missing relation target, is no longer written either.
- The written index still carries lines in three places: toctree entries,
  entity update records, and the update history, whose `line` entity
  templates show. Issue #372 tracks projecting the first two away, so the
  line-shift test can take them into its fixture.
- `.. contents::` and a toctree's `:name:` have no span, so their labels have
  no site.

## Alternatives considered

- **A position table in the `ProjectIndex`, stripped before writing.** It
  keeps one structure, but every fold and every clone of the index pays for
  data only an editor reads, and "stripped before writing" is a rule each
  later writer must remember. A separate type cannot leak.
- **A second walk collecting the sites.** It would be simpler to thread, but
  it would repeat the index's naming rules, and a site would exist under a key
  the index does not use the moment one of them changed.
- **No spans in the `.ast`, with the server re-deriving positions from the
  text.** A heading or a label could be found again by scanning, but a domain
  object's qualified name, an option's several names, or a definition spliced
  in from a fragment cannot be recovered without the parse.
