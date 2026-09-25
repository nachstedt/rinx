# 12. Diagrams over the entity graph, and moving compilation behind the index

## Status

Accepted.

## Context

ADR-011 gave the entity model a way to *list* the graph. It could not draw it.
The entity benchmark measured what that cost: after `needtable` was covered,
the largest remaining unsupported construct in useblocks' demo corpus was
`uml: 21`, followed by `needflow: 12`.

sphinx-needs' answer is `.. needuml::` — PlantUML source run through Jinja with
the need graph in scope — and `.. needarch::`, its form written inside a need.
Supporting them is not another directive. It is the first construct whose
**content depends on the global entity index**, and PlantUML was compiled in
Phase 1 (`rules/library.bzl`), per library, before any index exists.

So the pipeline had to move, and that move is the bulk of this change.

## Decision

### 1. One node for all six spellings, and one hash population

`Directive::PlantUml(HashedContent)` became `Directive::Uml(Box<Uml>)`, carrying
the diagram's *template* rather than its finished text. Six spellings parse to
it:

| ours | sphinx-needs / sphinxcontrib-plantuml |
|---|---|
| `.. plantuml::` | `.. uml::` |
| `.. entity-diagram::` | `.. needuml::` |
| `.. entity-arch::` | `.. needarch::` |

Our name is primary and the other is a reserved alias, following ADR-011's
`entity-table`/`needtable` precedent: a migrating project keeps its documents,
a new one need not adopt another tool's vocabulary.

Keeping the plain PlantUML pair on a *second*, earlier path was considered and
declined. It would have avoided all the Bazel churn below, and it would have
meant two hash populations with nothing keeping the compiled set and the
validated set in step — which is the one failure `commands/diagrams.rs` exists
to prevent, and which had already shipped once as a nested diagram rendering a
broken `<img>` that validation did not catch.

A plain `.. plantuml::` is simply a template with nothing to expand, so it
flows through the identical path and keeps the hash it has always had. That is
what made the move provably behaviour-preserving: the example site's compiled
SVGs kept their filenames.

### 2. The diagnostic family is `uml.*`, not `entity-diagram.*`

A code names the construct, per ADR-011. But once one node covers plain
PlantUML too, `entity-diagram.invalid-align` on a `.. plantuml::` names an
entity that is not there. `uml.*` names what all six spellings actually are.

### 3. The render action expands, and writes what it expanded

Two things need a diagram's finished text: the page's `<img>`, which names the
SVG by the text's hash, and the compile action, which needs the text itself.

The obvious shape is the one the embedded-image assets use (ADR-007): one
action computes and writes a sidecar, the other reads it. The first version of
this change did something close to it — a per-document `expand_diagrams`
action wrote the `.puml` files, and the renderer independently re-expanded
every diagram to learn its hash. That was duplicate work, and it had to be
kept byte-identical across two processes or a page would point at an SVG
nothing compiled.

So the render action does both. It already loads the index and walks the
document, so `rusty_sphinx_uml::expand` is called once per diagram, the
`<img>` is built from the result's hash, and the text goes back in
`RenderOutput::diagram_sources` for `cmd_render` to write as `<hash>.puml`. The
renderer crate still performs no I/O. The page and the file it names come from
one call in one process, so they agree by construction.

The expander must still be **deterministic** — every iteration is over a
`BTreeMap` — because an unchanged diagram must hash the same on the next build,
or its compile action misses the cache.

The enclosing entity travels on the node, not in `UmlContext`: it is recorded
by the parser, the only phase that still knows it, so no caller has to
rediscover it by walking outwards from the node.

Diagnostics come from the renderer, which holds the directive's span — so
template errors get positions, `.. noqa:` suppression and the standard
`warning: path:line:col: code: message` shape for free. A side benefit: live
preview reports template *syntax* errors even though it cannot draw a picture.
It stays quiet about failures that depend on the graph, since a preview merges
a stale index — the same distinction an empty `.. entity-table::` already makes.

### 4. Diagrams are opt-in per library, and compiled by the site

`rusty_sphinx_library` gained `diagrams`, **off by default**. Only documents of a
library that sets it get diagram actions; `RustySphinxInfo.diagram_ast_files`
tells the site which ones. For each, the render declares a `<doc>` puml
directory and a per-document `PlantUMLCompile` consumes it.

**Why opt-in.** Bazel creates actions before it reads any document, so it
cannot know which documents hold a diagram. The first version of this change
therefore gave *every* document an expansion action and a compile action. On
CPython's docs — roughly five hundred documents and not one diagram — that
measured as a cold build of **15.6s before, 20.4s after**: a third slower for a
project that draws nothing. Two ways to claw it back were tried and rejected:

- **Batching the expansion into one action** with an output directory per
  document measured *slower*, at 24s — Bazel runs five hundred small actions in
  parallel better than one action declaring five hundred tree artifacts.
- **Batching the compilation** would remove the cost and also mean one
  diagram's edit recompiles every diagram in the site, defeating the firewall
  below.

The principle that settled it is "pay only for what you use": if the build
cannot discover usage, the library declares it — the same shape `images` and
`parse_data` already have. With the opt-in and expansion folded into the
render, CPython builds in **15.5–15.7s** again, and the example site declares
two compile actions (one per document that draws something) where it declared
about seventy.

**Why the default is off**, even though it made existing `.. plantuml::` users
add a line: a default-on attribute would leave every project that never heard
of it paying the cost, which is the problem being solved. The forgotten case
fails loudly: `cmd_parse` rejects a diagram in a library without the attribute
as `uml.diagrams-disabled`, on the directive's own line, naming the attribute.
It is the parse *subcommand* that is strict — the parser crate stays
resilient and unaware of build configuration, so live preview parses a diagram
like any other construct. `tests/test_diagram_opt_in.sh` covers both halves.

**Keep diagram documents in a library of their own** to keep the cost narrow —
`examples/entities/BUILD.bazel`'s `diagram_docs` does, exactly as
`doctest_docs` isolates doctests (ADR-002).

**The cache firewall is the puml directory.** The index is an input to every
render, so any edit anywhere re-runs them all — but for a document whose
diagrams did not change the `.puml` bytes are identical, so the compile
action's key is unchanged and no JVM starts. Compilation stays per document
within an opted-in library, so a diagram edit recompiles only its own
document's diagrams. `tests/test_diagram_cache_firewall.sh` asserts both
directions.

`validate_images` no longer expands anything: it checks each `<hash>.puml` a
render wrote has its `<hash>.svg` in the bundle — the one thing that can still
go wrong is the compile in between.

**What is genuinely lost** by compiling at the site rather than the library:
two sites sharing a library each compile the same diagram, and building a
library alone produces no SVGs. The compile action's output path is
site-namespaced, so even a remote cache will not dedupe them. Accepted, as the
price of one hash population.

### 5. The template surface, and what it reuses

| written | what it does |
|---|---|
| `needs` | every entity, by id |
| `need` | the entity an `.. entity-arch::` sits inside |
| `need(id)` | one entity's fields |
| `filter(expr)` | the entities matching a filter, in id order |
| `flow(id)` | a clickable PlantUML node |
| `ref(id, text)` | a PlantUML link |
| `uml(id, key)` | another entity's diagram, expanded here |
| `imports(id, rel…)` | the diagrams of everything `id` points at |

Nothing here is a second implementation of something the build already had:

- `filter()` is `rusty_sphinx_filter`, the same crate and the same refusals a
  listing directive's `:filter:` goes through. It is parsed at *render* time
  rather than parse time only because the text does not exist until the
  template runs.
- `flow()` and `ref()` build hrefs from `relative_doc_href` and
  `entity_anchor`, so a clickable node lands exactly where a `:ref:` to the
  same entity would. Both moved from the renderer into `rusty_sphinx_index` to
  make that possible.
- `EntitySubject` — what a field *name* is worth — likewise moved into
  `rusty_sphinx_index`. A filter meaning one thing in a table and another in a
  diagram would be a bug neither crate's tests could see.

`imports()` takes an explicit id where sphinx-needs' takes only relation names
and assumes the current need. The explicit form works outside an architecture
diagram too, which is strictly more useful.

### 6. `:key:` templates live on the entity record

`EntityRecord` gained `uml: BTreeMap<String, String>`. This knowingly relaxes
that type's "no prose" rule, and the trade is deliberate: `uml('REQ_001')`
imports a template from an entity declared in a document the importing one has
never seen, and only the project index spans documents. The alternative was a
second, diagram-shaped index. Growth is bounded by what an author actually
writes inside an entity.

Recursion is guarded by a shared import chain rather than a depth counter, so a
genuine cycle is reported exactly — with the route it took — while a diamond
(importing one picture twice, side by side) is correctly *not* a cycle. A depth
cap remains as a backstop against a long non-repeating chain.

### 7. `@startuml` is added when it is missing; `:save:` is refused

Both sphinxcontrib-plantuml and sphinx-needs wrap a bare fragment, and here it
earns its place twice: PlantUML refuses a file without the markers outright, so
without it a fragment fails the build with a message about the *compiler*
rather than the document — and a fragment is exactly what a `:key:` diagram is.
A diagram that already opens with the marker is untouched, so no existing hash
moves.

An expansion that draws *nothing* is refused the same way, as
`uml.empty-result`, and no `.puml` is written. It has to be: `PlantUML` rejects
an empty diagram, so compiling one fails the whole build with a syntax error
naming a generated file the author never wrote — while the real cause is
usually a `filter()` that matched nothing, which is a content problem and
belongs in a warning beside the directive, exactly as
`entity-table.empty-result` already is. This surfaced from CI, where dropping a
library's `entity_schema` left every entity unknown and one diagram's filter
matching nothing.

`:save:` cannot work as sphinx-needs spells it: a sandboxed action may only
write files declared at analysis time, and the path is written inside the
document. It is reported as `uml.save-unsupported` rather than silently
ignored, and the site's `diagram_sources` output group carries every diagram's
expanded source instead — sphinx-needs' `needs_build_needumls`, as an output
group rather than a config flag.

## Consequences

- A `rusty_sphinx_site` now needs the PlantUML tool; a `rusty_sphinx_library`
  no longer does.
- A library with diagrams must set `diagrams = True`. This is the one change an
  existing `.. plantuml::` user has to make, and the parse error says so.
- `validate_images` takes the puml directories (`--diagram-dirs`) rather than
  re-deriving diagrams from the index.
- `uml_configs` is a new `rusty_sphinx.toml` table. It holds preamble *text*,
  not a path, so it respects ADR-001.
- The snapshot a template is expanded against is built per templated diagram
  and is linear in the size of the project. A project with thousands of
  entities and hundreds of diagrams would want it hoisted per document; the
  untemplated case never builds one at all.
- `needflow`, `needpie`, `needbar` and `needsequence` remain unimplemented.
  (All but `needbar` have since been answered, by ADR-014, ADR-017 and
  ADR-020.)
  Each is the same question with a different presentation, and each can now
  reuse both the filter language and this expander.
