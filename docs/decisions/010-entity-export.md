# 010 — Exporting the entity graph, and where validation belongs

- **Status**: Proposed
- **Date**: 2026-09-06

## Context

`scripts/benchmark_entities.py` builds useblocks' sphinx-needs demo against a
schema converted from that project's own `ubproject.toml`, and reports what the
conversion could not carry over. The first run found 28 such constructs. Six of
the shapes are *policy asked of the entity graph*, and they are what this ADR is
about:

- **Expressions over an entity's own fields** — `[needs.constraints]` holds
  Python: `"asil is not None and asil in ['A','B','C','D']"`,
  `"len(mitigates) > 0"`, each with an `error_message` and a severity that
  drives both a warning and a CSS bar colour.
- **Rules conditional on where the document lives** — six `schemas.json` rules
  select on `allOf: [type-req, {docname: {pattern: ".*automotive-adas.*"}}]`:
  *requirements under this subtree must carry a status; hazards under that one
  must carry an ASIL rating.*
- **Rules over linked entities** — `spec-links-to-req` requires every target of
  `:reqs:` to itself be of type `req`.
- **Disjunctions** — `test-has-spec-or-impl`: at least one of `:spec:`,
  `:specs:` or `:links:`.
- **Patterns on values** — eleven id-pattern rules, e.g.
  `^(R_|SWREQ_|REQ_|EX_REQ_)[A-Za-z0-9_]+$`, which our `id = { prefix = "R_" }`
  approximates with a single literal prefix applied only to *derived* ids.
- **Cardinality beyond required/multiple** — `minItems`, `minLength`.

Three of the reported constructs are *not* policy, and the tiering below places
them without needing this ADR's answer. `[needs.variants]` selects different
attribute values per build (`:status: customer_a:open, customer_b:closed`) — a
build-configuration feature, not a check, and one that would have to reach the
parser because it changes what a value *is*. `[needs.links.runs]`'s
`predicates` generate edges by calling a Python function, which ADR-009
decision 9 already covers. And sphinx-needs' generic `.. need::`, which names
its type in a `:type:` option, is a parsing question: tier 1 by construction,
since our model resolves a type from the directive name.

The tempting response is to grow the schema language until it covers all six.
ADR-009 decision 9 already declined *Python* for filters, and planned "a small
typed expression language, evaluated against `EntityRecord`" for a later
increment. That plan matters here and is not simply an argument for this ADR:
if such a language arrives for `needtable`-style filters, constraints could
reuse it. See Decision 6, which is where that tension is resolved rather than
avoided.

This ADR answers the prior question: which of these belong inside rinx
at all, and what does the build hand to whatever handles the rest.

## Decision 1 — three tiers, drawn by what the pipeline must know

The line is *not* "validation versus not validation". It is what each phase
cannot proceed without.

**Tier 1 — the parser cannot parse without it.** The schema is what makes
`.. req::` a directive rather than an unknown name, and an attribute's declared
type is what decides whether a value becomes an `AttributeValue::Int` or a
`String` in the `.ast`. These are not checks bolted onto parsing; they are the
failure modes *of* parsing:

`entity.unknown-attribute`, `entity.invalid-attribute-value`,
`entity.missing-required-attribute`, `entity.malformed-argument`,
`entity.invalid-id`, `entity.unknown-section`, `entity.duplicate-section`,
`entity.missing-required-section`, `entity.section-outside-entity`,
`entity.missing-required-relation`, `entity.multiple-relation-targets`.

**Tier 2 — resolution computes it anyway.** An entity reference has to resolve
for the renderer to emit a link at all, and back-links have to be derived for
the index to be complete. The check is a by-product of work already done;
deferring it would mean building the graph twice:

`entity.duplicate-id`, `entity.unknown-target`, `entity.disallowed-relation`,
`entity.role-type-mismatch`, plus `entity.schema-mismatch`, which is a
build-integrity check rather than a document one (a library parsed against a
different schema than the site indexes with).

**Tier 3 — policy over a finished graph.** All six shapes listed in the Context.
None of them changes how a document parses or how a reference resolves. They are
questions asked of the whole graph once it exists.

The tiers were derived from what each phase needs, but they land exactly on the
crate boundary that already exists: every tier-1 code is emitted by
`rinx_parser`, every tier-2 code by `rinx_analyzer` or
`rinx_renderer`. That the two derivations agree is the main evidence the
line is in the right place.

## Decision 2 — the index gains a documented entity export, in its own output group

The site rule's index action already produces `<name>.project.index`, and
`rinx_index::ProjectIndex` already carries the whole graph:
`entities: BTreeMap<EntityId, EntityRecord>` — type, document, title, parsed
attribute values and outgoing edges (`crates/index/src/entity_record.rs`) — and
`entity_backlinks`, the derived incoming side.

So the export is close to free. What is missing is not data but a *contract*:
`project.index` is an internal cache format that may change shape whenever a
phase needs it to.

`rinx_site` therefore gains an `entities.json` in a **non-default output
group** (`entity_export`), the same mechanism `doctest_plans` uses on
`rinx_library` and `domain_warnings` on the site. Building a site never
produces it; a target that wants it asks for the group.

It stays out of the site bundle deliberately. The bundle is what gets published;
the export is build metadata, and a project that publishes its requirement graph
to a web root should do so because it chose to, not because we bundled it.

## Decision 3 — tier-3 validation is a test target, not a build step

This is the shape ADR-002 already established for doctests: the build produces a
`.doctests.json` and never starts an interpreter; a separate `py_test` executes
it; between them sits a cache firewall.

Entity validation is the same move. The build indexes and exports; a validator
consumes the export as a test. Consequences worth stating plainly:

- **It is a whole-project action.** Unlike a tier-1 diagnostic, which is cached
  with the one document it belongs to, a validator over one exported artifact
  re-runs whenever any document changes and cannot be sharded. That is
  acceptable for `bazel test` — doctests have the same property — and is a
  large part of why tiers 1 and 2 are *not* moved out (see Decision 5).
- **Projects bring their own validator.** rinx ships no expression
  language, severity taxonomy or rule format. A project that already has
  `schemas.json` and a JSON-Schema runner keeps using it; one that wants Python
  expressions writes Python.

## Decision 4 — the export follows sphinx-needs' `needs.json` field names where they fit

A migrating project already has tooling pointed at `needs.json` — and needs an
interchange format for reasons that have nothing to do with validation.
`needimport` and external needs are exactly this format read backwards: the
unresolved `REQ_1_1_ext` and `SPEC_1_1_ext` targets in the benchmark's first run
are needs the demo imports from a `needs_external.json` we do not read.

Reusing the field names costs nothing and makes both directions plausible later.
Where our model has no sphinx-needs equivalent — sections, whose bodies are
parsed RST rather than strings — the export carries our own shape, and section
*bodies* stay out for the reason `EntityRecord` already omits them: a project
with thousands of requirements should not pay for its prose twice.

The obligation this creates is the real price of the whole split: an exported
format is a public API, and versioning it is a cost `project.index` never had.
It is worth paying only because interop needs the format regardless.

## Decision 5 — tiers 1 and 2 stay where they are

Stated explicitly, because "export the raw data and validate externally" reads
like it should apply to all of it.

- **Caching granularity.** A tier-1 diagnostic is attributed to one document and
  cached with its `.ast`; edit one file and only that file re-reports. Moving
  those to a whole-project validator would trade per-file caching — the thesis
  of `docs/dev/architecture.md` — for uniformity.
- **Live preview.** `process_preview` merges a fresh local analysis into a stale
  global index on every keystroke, which is what makes tiers 1 and 2 available
  in the editor for free. Tier 3 never will be, and should not be: an
  expression evaluator does not belong in a debounce loop. That the two behave
  differently is a reason to keep the boundary, not to erase it.

## Decision 6 — a future filter language does not collapse the tiers

ADR-009 plans a typed expression language over `EntityRecord`, for filters. Once
it exists, `"len(mitigates) > 0"` becomes expressible, and the obvious question
is whether tier-3 constraints should then move back in.

Partly, and the tiers say where: such a language would evaluate against the
*index*, not during parsing, so a constraint written in it would be a **tier 2**
check — computed where the graph already exists — not a tier 1 one. Nothing
about that changes the parser, and nothing about it changes Decision 5.

What it would not absorb is the rest of tier 3: rules conditional on document
location, rules over linked entities' types, severity taxonomies with
failure-driven styling, and any rule format a project already owns and points
existing tooling at. Nor does it remove the need for the export, whose primary
consumers are interop (Decision 4) and the reporting directives (Consequences).

So the tiers are a statement about *phases*, not a permanent statement about
which features exist. A construct may move from tier 3 to tier 2 when the
machinery to evaluate it cheaply is already running; it may not move into tier 1
unless the parser genuinely cannot proceed without it.

## Options considered

**Grow the schema language until it covers tier 3.** Rejected as a whole,
though not in every part. Expressing all six shapes in `entities.toml` means an
evaluator in the parser — the phase that must stay fast and error-resilient —
for rules that do not affect parsing. Decision 6 records the part that is not
rejected: a construct may become a tier-2 check once the index can evaluate it
cheaply.

**Move all entity validation out, including tiers 1 and 2.** Rejected for the
two reasons in Decision 5. It also cannot be done for tier 1 even in principle:
the parser must consult the schema to know that `.. req::` is a directive, so
the information is present and the check is nearly free at that point.

**Publish `project.index` as the interop format.** Rejected. It is an internal
cache format carrying toctree graphs, section numbering and outlines, none of
which an entity consumer wants, and freezing it would constrain every future
phase that needs a new field.

**Emit per-document entity sidecars instead of one project file.** Rejected for
the *export* — the point of the export is the resolved graph, and back-links are
a project-wide product (ADR-009 decision 5) that no single document can produce.
Per-document sidecars would push the join into every consumer.

**Make validation part of `bazel build` rather than `bazel test`.** Rejected.
A whole-project action in the build path serializes against every render, and
would fail the site build on a policy question. `rinx_site` already
takes the opposite position for broken links — they warn by default and fail
only under an opt-in `strict_links`, which `tests/test_strict_links.sh` pins —
and the same reasoning applies with more force to project-defined policy. The
`bazel test` boundary also lets a project adopt validation incrementally.

## Consequences

- The benchmark's "Sphinx-Needs Constructs Without An Entity-Model Equivalent"
  section changes meaning: it stops being a list of gaps to close in the schema
  language and becomes a specification for what an external validator must
  cover. `scripts/needs_schema.py`'s categories should be re-grouped
  accordingly — they currently conflate "cannot be expressed", "could be
  expressed and is not yet" (`minItems: 1` is our `required`; a `network` items
  rule is our `to = [...]`), and "already converted, with a redundant half
  dropped".
- `needtable`, `needflow`, `needpie` and `needbar` — 35 of the 79 unsupported
  directive uses in the demo corpus, a larger slice than validation — consume
  the same exported graph. Whatever answers those will want this export, so its
  shape should not assume a validator is the only reader.
- Two-way interop becomes possible but is explicitly out of scope here:
  *importing* external entities (sphinx-needs' `needimport`) raises questions
  about ids the project does not own and about what `entity.unknown-target`
  should say, and deserves its own decision.
- The export is one more thing the index action writes, so it must not become a
  reason for the index to hold data no other phase needs.

## Open questions

- Does the export belong to `rinx_site` only, or should
  `rinx_library` expose a per-library one for a project that composes
  sites from several libraries?
- Should `id = { pattern = "..." }` be added after all? It is the one tier-3
  shape that is purely local, cheap, and checkable at parse time — which would
  make it tier 1 rather than an exception to this ADR.
