# 009 — A user-definable entity meta-model

- **Status**: Accepted
- **Date**: 2026-09-06

## Context

rinx had no extension mechanism at all. Every construct it understood
was a hard-coded Rust enum variant: `ObjectType`, `DomainObjectBody`'s fourteen
variants with eight exhaustive accessors each, a static role-regex table, and a
linear `if name == ...` directive chain. Adding one domain object type touched
around twenty files.

That is fine for a fixed vocabulary (`py`, `c`, `std`). It cannot express what
real projects define for themselves: sphinx-needs' requirements, specs, tests
and the typed links between them; CPython's `.. audit-event::`. All of these are
the same shape — a typed, identified, attributed thing that can be referenced
and can point at other such things — differing only in vocabulary.

See `docs/entities.rst` for what the feature *is*. This records why it is built
the way it is.

## Decisions

### 1. Types are declared in config, not in Rust

The whole point is that a project names its own types, so a type cannot be an
enum variant. `EntityBody` therefore carries a `type_name: String` and a
validated attribute *map*, which is a deliberate deviation from the guideline
that a family of related kinds should each get their own variant carrying only
its own fields.

The schema is what buys that back. Validation happens once, at the parse
boundary, against the declared type; everything downstream reads data that has
already been checked and never re-checks the schema. The alternative — a
dynamically-typed map validated at each use — is the thing the guideline exists
to prevent, and is not what this does.

### 2. The schema reaches the parser

Rejected alternative: parse generically (any unknown directive becomes a
structured name/options/body node) and bind meaning later, at index time. That
would have kept parse actions schema-independent, so editing the schema would
cost one index plus N renders rather than N parses.

We chose parse-time binding anyway, for the diagnostics. The parser is the only
phase that still holds the source position of each individual option line, so
only there can `:status: pending` be reported at the line the author wrote
rather than against the document as a whole. Late binding would also have left
the parser unable to distinguish a mistyped entity directive from a directive
this build simply does not implement.

The cost is stated plainly and accepted: **editing the schema re-parses every
document in every library.** On a large corpus that is the whole build.

Two consequences follow:

- `entity_schema` is an attribute on `rinx_library` as well as on
  `rinx_site`. It is the first thing to cross that boundary —
  `default_domain` deliberately does not.
- A library can therefore be parsed against a different schema than the site
  indexes with. `Document` carries an `entity_schema_hash` and the index phase
  reports `entity.schema-mismatch` rather than producing quietly wrong output.

  The rule is deliberately *"a document parsed against some schema must have
  been parsed against this one"*, not *"every document carries this hash"*. Only
  libraries that use entities declare the schema, so in a multi-library site
  most documents legitimately carry none; demanding the hash everywhere reports
  every one of them, which is noise. Two drafts got this wrong in opposite
  directions before it settled — first missing the site-forgot-it case, then
  reporting every entity-free library — which is why
  `tests/test_entity_schema.sh` covers the real misconfiguration and a unit test
  covers the mixed set.

  The case the check does *not* cover — a library that uses entities and forgot
  to declare the schema — is not lost: its directives were never recognised, so
  it surfaces as unknown directives and dangling references, which point at the
  offending line rather than at the whole document.

### 3. Attributes and sections are two concepts, not one field with a flag

A `parsed = true` flag on attributes would have been smaller. It was rejected
because the two differ in *every* later phase, not just at parse time:

|  | attribute | section |
|---|---|---|
| representation | a typed value | a `Vec<Node>` tree |
| in the project index | yes | no |
| filterable | yes (later increment) | no |
| rendered as | a table row | body content through `render_nodes` |

A shared type would have carried the union of both meanings everywhere, with
most of it meaningless on most instances. Keeping them apart also gives authors
a rule they can apply without reading any code: attributes are values, sections
are documents.

Sections are the part of this model that goes beyond sphinx-needs, and they are
what makes a requirement's verification criteria real prose — able to hold a
nested directive, a cross-reference, a code sample — rather than a string.

### 4. The section body is parsed by the ordinary block parser

An entity's body goes through `parse_blocks` under a context carrying the
enclosing type, and the resulting nodes are *then* partitioned into sections.
The dispatcher recognises a section name only when that context field is set
and the type declares it.

This buys indentation handling, nesting, span rebasing and error resilience for
free, instead of a bespoke body splitter that would have had to reimplement all
four. It is also why sections do not nest: a section's own body is parsed with
the enclosing type cleared, which makes a nested section a diagnosable mistake
rather than a structure with no meaningful rendering.

### 5. The entity graph is stored; back-links are derived

`ProjectIndex` stores `entities` — each record carrying only its own outgoing
edges — and recomputes `entity_backlinks` from the merged whole.

This is the same split ADR-005 makes for toctrees, for the same decisive
reason: per-document data *merges*, which is what keeps the live-preview path
(`ProjectIndex::merge`) correct. A pre-flattened view of who points at whom is a
project-wide product and cannot be merged one document at a time.

Section prose is deliberately absent from the index. A listing directive needs
fields, not paragraphs, and keeping prose out is what bounds the index's size on
a project with thousands of requirements.

### 6. Relations are declared on the source type

Rejected alternative: a top-level `[[relation_type]]` with a `from` list. That
invites the declared source types to drift from where the option is actually
written, and forces a reader to look in two places to learn a type's option
vocabulary.

The accepted cost is that a target type's own block says nothing about the
back-links it will display. The schema loader therefore derives a per-type
back-link table at load, which is where the two collision rules live — see
`docs/entities.rst`. Notably, a shared *outgoing* name constrains nothing; only
names that meet on a shared target do.

There is deliberately no reserved-word rule for back-link names. The rendering
keeps attributes, sections, outgoing links and incoming links in separate
namespaces, so a name collision is a readability problem rather than a
correctness one — and only the two genuine ambiguities are refused.

### 7. Roles are optional sugar

Every entity registers an ordinary `TargetName`, so `:ref:` reaches one with no
role machinery, and a built-in `:entity:` role accepts any type. A declared role
adds a type check on the link and the spelling an existing project already
writes — nothing else.

Roles are declared at the schema's top level rather than inside a type, because
the relation is many-to-many: sphinx-needs' `:need:` refers to four types, and
listing it inside each would force a reader to union four lists.

Matching them needs no per-project regex compilation: one static pattern matches
the *shape* of any role, tried last so every built-in role wins on a tie, and
the captured name is checked against the schema. A name the schema does not
declare degrades to plain text rather than to a diagnostic — that pattern also
matches ordinary prose and the many role spellings this build does not
implement, and reporting those would be reporting on text the author never meant
as markup.

### 8. Presentation is built-in by default, templated by exception

Every type renders without configuring anything: a titled block with the id
anchor, a field table, the sections in document order, and the link lists. A
type that wants something else names a MiniJinja template, which the site
supplies as a file.

The schema holds the *name* and the build holds the *path*, per ADR-001: a
sandboxed build relocates files, so a path in config would break.

Section bodies are rendered to HTML before the template sees them, because
MiniJinja cannot render RST — the same containment `math.rs` and `highlight.rs`
give their backends, and for the same reason: the node tree must not reach a
foreign renderer. The context is four separate namespaces (`attributes`,
`sections`, `outgoing`, `incoming`) rather than one flat map, which is what
makes a back-link named `title` harmless and why no reserved-word list is
needed anywhere in this design.

A template a type names but the site does not supply fails the build. That is a
misconfiguration of the *site*, not a fault in any document, so it is reported
separately from the broken links and no document's `.. noqa:` can silence it.

### 9. Our own filter language, eventually — not Python

Sphinx-needs evaluates filter strings as Python expressions. rinx has no
interpreter, and embedding one would import a foreign toolchain into a sandboxed
multi-platform build for one feature. A small typed expression language,
evaluated against `EntityRecord`, is planned for a later increment; unsupported
syntax will be diagnosed rather than silently matching nothing.

This is why `docs/entities.rst` documents the sphinx-needs compatibility target
as "the vocabulary, not the filters".

### 10. The schema file has a generated JSON Schema, covering shape only

TOML has no schema language, so editor validation means JSON Schema over TOML's
data model, which `taplo` applies. It is **generated** from the `Raw*` types the
loader deserializes into — the guideline about deriving an artefact from the
same source the code uses, with a test that regenerates and compares. A
hand-written copy is precisely the parallel definition that guideline forbids,
and it would drift exactly when it mattered.

Wanting a good schema improved the loader: `RawAttribute::value_type` became a
typed enum instead of a free `String`, which lets serde refuse an unknown
spelling (with a message naming the seven valid ones) and lets the schema offer
them as completions. An unknown `type =` is now a shape error rather than a
values/type mismatch, which is the more accurate diagnostic.

The boundary is documented rather than papered over: JSON Schema cannot express
a reference from one part of a document to another, so every cross-reference and
uniqueness rule — a relation's `to`, `argument.fields`, duplicate names,
back-link clashes, `values` present-iff-enum — stays in the loader. A test
asserts that boundary directly, checking that a schema with an undeclared `to`
*validates* and *fails to load*, so nobody later mistakes the JSON Schema for
the authority.

Rejected: restructuring `RawAttribute` as an internally-tagged enum on `type`,
which would have made `values` present-iff-enum expressible in the derived
schema and enforced by serde. Internally-tagged enums need `#[serde(flatten)]`
for the common fields, and serde disables `deny_unknown_fields` under `flatten`
— trading the mistyped-key check for the values check. A typo'd key is the
likelier mistake, so `deny_unknown_fields` was kept.

### 11. The built-in directive list lives in the parser

The schema loader must refuse a section name that would shadow `.. note::`. It
learns which names those are through an injected `ReservedDirectiveNames` trait,
answered by the worker from `rinx_parser::is_builtin_directive_name`.

`rinx_entity` deliberately keeps no copy of that list: a second copy
would drift the moment a directive is added, which is precisely the silent
degradation the check exists to prevent. The list sits beside the dispatch chain
it mirrors, with tests asserting that every enum-derived family — admonitions,
version changes, doctest directives, domain objects — is covered, since those
are where a one-line addition could otherwise leave it stale.

## Consequences

- A new crate, `rinx_entity`, parallel to `rinx_scope` and
  `rinx_toctree`: the parser, analyzer, renderer and worker all need it
  and are forbidden to depend on each other. It owns the *meta-model*;
  `rinx_ast` owns the *instance* data (`EntityId`, `AttributeValue`,
  `EntityBody`, `EntitySection`), because that is what survives into a parsed
  document. The meta-model crate depends on the AST, so the split could not run
  the other way.
- `ProjectIndex::merge` now returns a typed `MergeConflicts` rather than a list
  of strings, so a duplicate entity id gets a real diagnostic code. Duplicate
  *glossary* terms are still only a message and still dropped by the caller —
  giving them a code would start emitting a warning on projects that never asked
  for this feature, and is left for whoever adds that code deliberately.
- Entity directives route their options through the existing option scanner, so
  an unknown option is **diagnosed**. The hand-rolled option extraction the
  py/c/std domain objects use still swallows unknown options silently; this
  feature does not fix that, but it does show what fixing it would look like.

## Presentation stays out of the meta-model

Collapsing an entity's detail behind a disclosure was first built as a
`collapsible = true` flag on the entity type. That was wrong, and the reason
generalises: **the schema is a parse-time input.** It is what makes `.. req::` a
directive, so editing it re-parses every document in the library and changes the
hash `entity.schema-mismatch` compares. A switch that changes only how a box
looks has no business invalidating parse caches.

The meta-model declares four things, all semantic — attributes, sections,
relations, roles. Presentation belongs to render-time inputs, so the switch is
`collapse_entities` in `rinx.toml`, defaulting to on. A type that wants
a different presentation entirely still names a `template`.

An intermediate version routed collapsing through a per-type template
(`entity_req.html`), on the reasoning that the built-in rendering "can never"
collapse. That was an overstatement: the built-in needs a *switch*, and only the
switch's home was ever in question. The template was deleted once the config
setting existed — it had become a hand-written copy of the built-in box, which
is the drift-prone duplication this ADR warns about elsewhere.

`template` itself sits in the schema, which is the one presentation-shaped thing
there. It earns its place by naming *which* renderer a type uses rather than
configuring one, and the site still supplies the file (ADR-001).

Two context additions came out of making the template able to match the built-in
rendering faithfully, and both close real gaps:

- **`labels`** — the headings the schema declares. Without it a template holds a
  second copy of every label, which is the parallel-copy trap. Its incoming half
  must be derived through `backlinks_for`, not from the type's own relations: a
  back-link is declared on the type at the far end.
- **`section_list`** — the sections in document order. `sections` is keyed by
  name, and a map cannot carry the order the model promises to keep.

### Why the disclosure is shaped the way it is

- A closed `<details>` hides every child but its `<summary>`.
  `display: block !important` on a child does not bring it back, and
  `::details-content { content-visibility: visible }` unhides all of it at once.
  Both were checked in a browser rather than reasoned about.
- So whatever stays visible is written outside the element, and the header
  cannot be the click target: making it the `<summary>` would pull the prose
  inside. Block content in a `<summary>` is non-conforming, and a link in that
  prose would fight the toggle for the click.
- Every unnamed section is hoisted, not just a leading run. An intermediate
  version used a prefix rule to protect document order across section names;
  that protection went away with the ordering decision below, and the prefix
  went with it.

## Order is the schema's, not the document's

Attributes, sections and relations render in the order the type declares them.

This was not the first answer. Sections originally rendered in *document* order,
described in `docs/entities.rst` as a promise — but nothing upstream required it.
Sphinx has no entity concept and sphinx-needs has no parsed sections, so the
promise was this model's own, and it was inconsistent with its neighbours: the
built-in box was rendering relations in declared order, sections in document
order, and attributes **alphabetically by name** — the last of those nobody had
chosen at all, it was `BTreeMap` key order leaking into the page. `req` declares
`title, status, tags, owner` and rendered *Owner, Status, Tags*.

Declared order is the right one because it is what makes a *vocabulary* worth
declaring: two requirements are comparable when the same field is in the same
place, whatever order their authors wrote them in. An author's freedom to write
sub-directives in any order is preserved — it just stops leaking into how every
reader sees the type.

Two things are kept:

- **Document order within one name.** Two `.. safety-comment::` blocks render in
  the order written; it is the only order they have.
- **Nothing is dropped.** A section or attribute the schema does not declare is
  appended rather than lost to a lookup miss.

The decision also deleted a wart. Collapsing had needed a *prefix* rule — only
the leading run of prose hoisted, trailing prose folded — purely to avoid
reordering a document. With document order gone, the rule is simply "prose
visible, sections folded", which is also what was originally asked for.
