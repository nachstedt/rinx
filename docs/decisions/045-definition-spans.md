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
- `.. contents::` and a toctree have no span, so their labels had no site at
  first. Since name spans (below), such a label is sited at its name.

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

## Amendment: name spans

Renaming (roadmap #31) needs a definition's *name*, not its line: the
`install` in `.. _install:`, the value of a `:name:`, `:label:` or `:id:`.
The parser records it where it reads the name, as `rinx_ast::Written<T>`, a
value with the span of exactly its characters, and a `DefinitionSite` carries
it as `name`.

- **Never serialized.** `Written<T>` writes and reads the value alone, as
  `IncludeSite` is skipped, so the `.ast` stays byte for byte what it was and
  a name read back from one has no span. Only the editor, which analyses its
  own parse, ever sees one. A test pushes every `:name:` and `:label:` value
  right and compares the written `.ast`.
- **Equality ignores it.** Two values a round trip through the `.ast` cannot
  tell apart are equal, so a definition moved along its line still folds like
  it did, as one moved down does.
- **An entity's id is `Written<EntityId>`**, with a span only when it was
  written as the `:id:` value. A composed, generated or imported id has none,
  which is how an editor tells an id the author can rename from one it cannot.
- **A directive with no span of its own** (`.. contents::`, `.. toctree::`)
  is sited at its `:name:` value, which gives those labels a definition and a
  declaration at last.

The alternative, the server finding the name again on the site's line, would
be a second reader of every name's grammar: a label after `.. _`, an option
value after blanks, an id the `:id:` line failed to give.

## Amendment: names by handle

`Written<T>` kept a span in a serialized type and skipped it on the way out,
so a name read back from a `.ast` was not the name the editor parsed, and
equality had to ignore the span to hide it. It is replaced by
`rinx_ast::Named<T>`: the value and the `Option<NameId>` under which the
parser recorded the name's characters in the `SourceMap` beside the document
(ADR-047's amendment). Both are serialized and equality is derived.

- **The `.ast` still does not move with a line.** An id is a counter of the
  names one parse met, so a line moving changes no id — the test pushing every
  `:name:` and `:label:` right still compares the written `.ast` — and only a
  name added before another shifts it, which changes the document anyway.
- **Only the parse knows where a name stands.** `SourceMap::record_name` is
  the one way an id is made, and `DocumentAnalysis::of` takes an
  `Option<&SourceMap>`: the language server passes its parse's, the build has
  none to pass, so a site's `name` cannot be filled in the build — which reads
  no site.
- **What a name has no characters for has no id**, as before: a composed,
  generated or imported entity id is `Named::unplaced`.

## Amendment: lines by handle

What this ADR stored in the `.ast` — a section title's line, a target's, a
domain object's and a table directive's marker, a glossary term's — no build
phase reads: only `DefinitionSites` does, and the renderer's `written_uses`,
which only the language server calls. Each is now a `rinx_ast::BlockId`
(`block` on `Node::Heading`, `Node::Target`, `Directive::DomainObject`,
`Directive::Table`/`DataTable` and `GlossaryTerm`), under which the parser
records the lines in the `SourceMap` beside the document, as a `NameId` names
a name's characters.

- **The `.ast` no longer moves when lines move above definitions.** The
  premise above — that it moves anyway — held because of these spans as much
  as any; a test now writes a document of definitions with blank lines added
  and compares the `.ast` byte for byte. A document holding a reference, an
  entity, an equation or a code block still moves, since a build phase reports
  at those.
- **A site is the parse's alone.** `DocumentAnalysis::of` fills sites only
  from the map it is given; a build, analysing documents read back from
  `.ast` files, records none — and reads none.
- **A map belongs to its parse.** An id read against another parse's map
  finds another block's lines: building a test document by adding a node to a
  parsed one must record into that parse's map, never a fresh one.
