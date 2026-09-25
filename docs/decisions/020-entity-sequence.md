# 20. Sequence diagrams of the entity graph

## Status

Accepted.

## Context

After `needflow` (ADR-014), `needpie` (ADR-017) and the refusal of
`needservice`, the entity benchmark's unsupported-directive summary was down to
`needbar` and `needsequence`, each used in useblocks' demo corpus.
`needsequence` appears twice there, in `coffee-machine/index.rst`, drawing the
startup and safety-shutdown sequences of a coffee machine: components send
`seq_msg` needs to each other along a `startup_calls` or `shutdown_calls` link.

Its semantics were taken from sphinx-needs 8.5.0's
`sphinx_needs/directives/needsequence.py` rather than from its documentation,
because the observable behaviour is in the walk:

- From each `:start:` need — split on `,` or `;` — every link type in
  `:link_types:` leads to *message* needs, and the same link types lead on from
  each message to its *receivers*.
- Every hop emits `SENDER -> RECEIVER: <message title>`, and every receiver not
  yet visited is walked in turn, **depth first**, so the arrows read in walk
  order, not in id or document order.
- A sender is declared as a `participant` just before its first message.
- `:filter:` applies to receivers only; a rejected receiver gets no arrow and
  is not walked.
- `:max_items:` caps the arrows, keeps counting so the page can say "the first
  N of M", and restores the declaration of a participant the last drawn
  message points at.
- The argument is the caption.

ADR-014 closed by calling the remaining views "a presentation problem alone".
For `needsequence` that is only half true: the presentation is new, and so is
the question — a flowchart *filters* the graph, a sequence diagram *walks* it.

## Decision

### 1. `.. entity-sequence::` is a question, generated like a flowchart

The directive parses to `Directive::EntitySequence`, a sibling of
`Directive::EntityFlow`, and `rusty_sphinx_uml::build_sequence` generates its
PlantUML while rendering. Everything after the text exists — `assemble.rs`'s
wrapping, `:config:` preamble and hash, the `.puml` file, the compile action,
`validate_images`, `diagram_figure.rs`'s markup — is shared, for ADR-014 §3's
reason: one population of diagram hashes. A library holding one therefore needs
`diagrams = True`, reported as `entity-sequence.diagrams-disabled`.

`.. needsequence::` is the reserved alias, following every `entity-*`/`need*`
pair before it, and the diagnostic family is `entity-sequence.*`: a code names
the construct.

### 2. `:relations:` is mandatory

sphinx-needs defaults `:link_types:` to `links`. ADR-014 §4 already rejected
that default for a flowchart, since a schema here names its own relations, and
replaced it with "every relation the schema declares". That replacement does
not carry over: a walk along every relation follows edges that are no message
at all, and draws a plausible-looking but wrong diagram. Neither default says
which edges carry messages, so the option is **required**, and its absence is
`entity-sequence.missing-relations`, whose message lists the relations the
schema declares.

`:start:` is required upstream too. Both are `NonEmptyVector`s on the node, so a
diagram with nowhere to begin or nothing to follow cannot reach the renderer:
it degrades to a `Directive::Malformed` error block while parsing. When the
option *was* written but every entry was reported — an unknown relation, a
malformed id — the block is built without a second diagnostic.

### 3. Three departures from the walk, each fixing an upstream accident

- **One visited set across every start.** sphinx-needs gives each `:start:`
  entry a fresh visited list, so a second start re-walks what the first already
  drew — declaring participants twice and drawing messages twice. Here the
  second start continues where the first stopped, and a start already reached
  is not walked again.
- **Every lifeline carries its title.** sphinx-needs declares only senders, so
  a receiver that never sends is created by PlantUML under its raw id. Here
  every entity a drawn arrow touches is declared. Undeclared lifelines are
  placed after every declared one anyway, so declaring them after the walk
  moves nothing — and it subsumes upstream's special case of restoring a
  declaration `:max_items:` suppressed.
- **Nothing aborts the build.** An unknown start raises an exception upstream.
  Here it is `entity-sequence.unknown-start`, and the other starts are still
  drawn. A link to a need no document declares is left out, since it is
  already an `entity.unknown-target` where it was written.

Otherwise the walk is upstream's: the relation order as written, the target
order as written, depth first, and `:filter:` on receivers only.

### 4. Findings are not all failures

A flowchart either draws or does not, so `build_flow` returns a `Result`. A
walk can draw *and* have something to report — an unknown start among known
ones, or a truncation — so `build_sequence` returns a `SequenceDrawing` holding
the content and a list of `SequenceError`s. The renderer reports every one as a
`DiagramFailure::Sequence`, which is why that type's documentation no longer
promises a missing picture.

A truncation is reported twice, on purpose, as sphinx-needs does: a notice
beneath the picture for the reader, and `entity-sequence.truncated` for whoever
runs the build and never opens the page. A deliberate cap is silenced with a
`.. noqa:`.

### 5. What is refused by name

`:show_filters:`, `:show_legend:`, `:show_link_names:` (a message is always
labelled with its own title), `:highlight:`, `:filter-func:`, `:sort_by:` (the
walk decides the order), `:export_id:`, `:filter_warning:`, `:height:` (which
upstream itself accepts and ignores), `:engine:` and the legacy
`:tags:`/`:status:`/`:types:` filters are each `entity-sequence.unsupported-option`,
with what to write instead. `:width:`, which upstream also ignores, is honoured
as on every diagram here.

## Consequences

- `needsequence` joins `needtable`, `needflow`, `needpie`, `needuml` and
  `needarch` as supported; `needbar` is the last unimplemented view. (It has
  since been answered by ADR-021, `entity-bar`.)
- The benchmark corpus's two sequence diagrams draw without change to their
  documents: both use only `:start:` and `:link_types:`, and both walk a single
  start.
- Like a flowchart, each directive builds a `Snapshot` of the whole project,
  and ADR-012's note about hoisting it per document applies here too.
- The walk recurses once per participant, so its depth is bounded by the number
  of entities a single walk reaches, not by the size of the project.
- The directive dispatcher's four views over the entity graph moved into their
  own `try_parse_entity_view`, which the addition pushed past clippy's length
  limit for `try_parse_extension_directive`.
