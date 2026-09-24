# 16. `.. needimport::`: a bridge to sphinx-needs, not this build's import

## Status

Accepted.

## Context

`spec_gaps.md` listed `needimport` among the three sphinx-needs directives
that *add or modify* entities, with the note that each "needs a decision about
where in the pipeline it belongs". ADR-010 deferred it explicitly: importing
external entities "raises questions about ids the project does not own and
about what `entity.unknown-target` should say, and deserves its own decision."

This is that decision.

The construct itself is small. `.. needimport:: needs.json` reads a
sphinx-needs export and turns every need in it into a need of the importing
document — rendered there, linkable from there, and part of that project's
graph. The benchmark corpus uses it: the unresolved `REQ_1_1_ext` and
`SPEC_1_1_ext` targets ADR-010 mentions are needs the useblocks demo imports
from a `needs_external.json` this build did not read.

What makes it worth an ADR is that three of its questions have answers that
are not obvious, and one of them cuts against a rule this repository otherwise
follows without exception.

## Decision

### 1. It is a parse-time transclusion, so an imported need is an ordinary entity

`.. needimport::` joins `.. include::` and `.. if-builder::` in
`try_parse_splicing_directive` (`crates/parser/src/directives/dispatch.rs`),
as the third directive that contributes *any number* of nodes to the enclosing
block rather than exactly one. Each need becomes a
`Node::Directive(Directive::Entity(..))` — the same node a written `.. req::`
produces, carrying the same `EntityBody`.

The consequence is the point: **no later phase was changed at all.** The
analyzer indexes the entity, `document_index.rs` registers it as a `:ref:`
target, `derive_entity_backlinks` derives the incoming side of its relations,
and `.. entity-table::` and `.. entity-flow::` select it — none of them
knowing an import happened. `crates/worker/tests/integration/entity_import.rs`
asserts exactly those four, because they are the claim.

It is also why the import cannot happen later. **Merging an external graph
into `ProjectIndex` at index time was considered and rejected**: an entity no
document holds has no anchor, no `:ref:` target and nowhere to render, so the
merge would have to introduce an "external entity" that every view then has to
know about — a concept the model does not have and does not need, since
`needimport`'s whole semantics is that the importing document *does* hold the
need.

This answers ADR-010's open question about `entity.unknown-target` by
dissolving it. There is nothing new for it to say: an imported entity is in
the project, so a relation pointing at one resolves, and one pointing at
nothing is the same unresolved target a hand-written entity would have.
Likewise a clash with an existing id is the existing tier-2
`entity.duplicate-id`.

### 2. The name stays `needimport`, with no name of this build's beside it

This reverses, deliberately and for the first time, the interoperability rule
every other borrowed construct here follows: *give it this project's own name
and accept the other tool's spelling as an alias*. `entity-table` has
`needtable`, `entity-flow` has `needflow`, `entity-diagram` has `needuml`.

`needimport` gets no `entity-import`, because the rule's purpose does not
apply. That rule exists so a migrating project keeps its documents without the
foreign vocabulary becoming *ours* — it presumes the construct is one this
build wants to own. This one is not. It reads another tool's file format,
carries that format's limitations (no sections, a fixed field vocabulary, ids
someone else assigned), and exists to keep an existing project building while
the rest of its documents move over. A richer import — one that could read
this project's own interchange format, carry named sections, and report
against positions in the file it read — is a different construct, and
`entity-import` should be free for it.

So the diagnostic family is `needimport.*`. That looks like a violation of
"name a diagnostic code after the construct rather than the spelling it was
written under", and is not: here there is only one spelling, and it *is* the
construct.

### 3. The schema is the authority over the file

A need's `type` must name a declared `[[entity_type]]`; a need of a type this
project has no vocabulary for is reported and skipped, since there is nothing
to read its fields against. Its remaining fields must be attributes or
relations that type declares.

That last rule cannot be applied naively, because a real `needs.json` carries a
great deal of sphinx-needs' own bookkeeping beside the project's data —
`docname`, `lineno`, `is_need`, `type_color`, `parent_needs`, `sections` and
some thirty more. `rusty_sphinx_entity::needs_json`'s `INTERNAL_FIELDS`
enumerates them explicitly and `is_internal_field` matches the `type_*` family
by prefix. **Ignoring every unrecognised field instead was rejected**: the
common fault is a project attribute the schema has not declared yet, and
silently dropping it would lose exactly the data the directive exists to
carry. So bookkeeping is ignored by name and everything else is reported.

Values go through the **same** `parse_attribute_value` funnel a written option
does. JSON is typed and an RST option is not, so an imported value could
plausibly have had its own type ladder — and then an `enum` attribute would
validate one way when typed and another when imported.
`needs_json::field_text` renders a JSON scalar or array into the text an
option would have been written with, and `directives/entity_fields.rs` — split
out of `entity.rs` for this — holds the one implementation both callers use.

The one deliberate softening: an empty JSON value is an *unset* field, not a
malformed one. A real export spells "no owner" as `""`, and reporting that as
an invalid value would fire on nearly every need in a real file.

### 4. `content` is parsed; named sections are not importable

An imported entity's body is whatever its `content` field holds, parsed as
reStructuredText into the unnamed content section — which is what upstream
renders. Named sections, the part of this model that goes beyond sphinx-needs,
have no spelling in the format. Inventing one here would be designing the
richer import in the bridge, and belongs to `entity-import` instead.

### 5. Nothing is fetched

sphinx-needs accepts an `http`/`https` URL. A sandboxed build action may only
read files declared before it runs, so fetching would either fail in the
sandbox or make the build unreproducible. A URL argument is therefore refused
*by name* as `needimport.remote-source`, pointing at saving the file locally
and declaring it in `parse_data` — the shape `uml.save-unsupported` already
uses for an option the build system cannot honour.

The file itself needs no new build attribute. It is ordinary **`parse_data`**,
joining the `.csv` behind a `.. csv-table::`'s `:file:` and the sources
`.. include::` splices in: a file the parser opens, not a dependency on
another library. `tests/test_parse_data.sh` gained a fourth case pinning that.

An unreadable file therefore *fails the build*, as every parse-time read does.
That makes the neighbouring case — an argument that was never a path — worth
separating, and the benchmark found it on the first run: sphinx-needs looks the
argument up in `needs_import_keys` **before** treating it as a path, and the
useblocks demo writes `.. needimport:: imported_project`, an alias its
`conf.py` maps to `/needs_import.json`. Decision 8 is how that map is declared
here. An argument matching no key is refused *by name* as
`needimport.unsupported-import-key`, and — the part that matters — is **never
handed to the loader**, since the worker fails the build on any recorded load
failure. It warns and draws the visible error block instead. That is the same
line `directive.unknown` draws for `needpie`: a missing declaration warns, a
declared-but-missing file fails.

### 6. Positions are the directive's line, or none

JSON carries no line numbers a diagnostic could point at. Every `needimport.*`
diagnostic is therefore reported against the `.. needimport::` line itself —
the line the author can act on — with the file and the offending need's id in
the message, since one document may import several files.

Content parsed out of the JSON runs under `ParseCtx::synthetic()`, so a
diagnostic from inside imported prose is **positionless** rather than pointed
at a line of the document that has nothing to do with it. This follows
`.. csv-table::`'s `:file:` exactly, and for the same reason: report no
position rather than a wrong one.

**Interning a `FileId` for the JSON, as `.. include::` does, was rejected.**
An id is only worth having if a `Span` can be measured against the file, and
none can be here — the result would be spans that name a file but carry a line
number from nowhere.

### 7. `:id_prefix:` is scoped to the import

`:id_prefix:` prefixes every imported id, and rewrites the relation targets
that point at another need **in the same import**. A target naming anything
else is left alone, because it refers to an entity the project already holds
under its own id.

That scoping is what lets one file be imported twice under two prefixes
without the two copies linking into each other, which
`examples/entities/imported.rst` does four times over. Both halves of the rule
are pinned by unit tests: a link into the same import follows the prefixed
copy, and one naming anything else keeps the id the project already knows it
by.

### 8. Import keys are entity vocabulary, not build wiring

sphinx-needs lets `conf.py` map a bare name onto a path, and a document then
writes `.. needimport:: imported_project`. This build declares the same map as
a top-level `[import_keys]` table in **`entities.toml`**.

The first framing of this decision claimed configuration must not reach the
parser. That was wrong, and the correction is worth recording: `entities.toml`
*is* a parse-time config file — it is what makes `.. req::` a directive.
`rusty_sphinx.toml` happens to be site-only, but that is a fact about that one
file, not a principle. ADR-001's rule is about **paths in configuration**, and
its rationale is relocation, which does not bite here: a source-root-relative
path survives a sandbox move fine. The real constraint is narrower — **Bazel
must know a file is an action input before the action runs**, which no config
file can express. `parse_data` already does, and a keyed file needs an entry
there exactly as a `.. csv-table::`'s `:file:` does.

So the map goes in the schema, for two reasons:

- **The alias is written in a document.** For `.. needimport:: imported_project`
  to mean anything, the reader needs the map; putting it in `BUILD.bazel` would
  send them to the build system to understand a line of prose, and the map
  would not travel with the documents.
- **It is entity vocabulary.** `.. needimport::` imports entities, so where
  external entity sets come from belongs beside the types they arrive as.

**A value is the path itself**, resolved by the rule every other written path
in this build follows: relative to the file that wrote it, or to the source
root with a leading `/`. Anchoring at the `entities.toml` keeps a schema
relocatable together with the data files beside it, and takes sphinx-needs' own
`/needs_import.json` spelling verbatim, so the benchmark's converter copies the
value across untouched. `rusty_sphinx_entity` keeps the values **as written**
and the worker resolves them once — only it knows where the schema file is.

`[import_keys]` is deliberately **excluded from the schema fingerprint**. That
hash exists so a library and the site agree on the vocabulary the *index* and
the *renderer* read, and an import key is read only while parsing; including it
would report `entity.schema-mismatch` for a difference no later phase can
observe. `crates/entity/src/schema.rs`'s tests pin both halves.

## Consequences

- **Every tier-1 diagnostic, by ADR-010's test.** Each of the sixteen
  `needimport.*` codes is emitted by `rusty_sphinx_parser` and is something
  the parse cannot proceed without — which is the line ADR-010 drew, and it
  lands on the same crate boundary. Nothing about importing moves a check
  into tier 3.
- **The live preview gets it for free.** `process_preview` parses through the
  same path, so an import resolves in the editor with no work: the entities it
  brings are in the fresh local analysis that gets merged into the stale
  global index.
- **There is no cache firewall, and that is correct.** Editing the
  `needs.json` re-parses and re-renders every document that imports it, the
  same way editing an `.. include::` source does. The needs really are part of
  the importing document.
- **`entity.duplicate-id` becomes reachable by accident more easily.** Two
  documents importing the same file both declare the same ids. That is a real
  conflict rather than a false positive, and `:id_prefix:` is the answer
  upstream already supplies.
- **An export's unset fields are silent, and that is measured.** A
  `needs.json` writes *every* registered option for *every* need. Against the
  benchmark corpus's converted schema, 19 field names would have been reported
  as `needimport.unknown-field`: 16 empty on all four needs (registered by
  sphinx-needs' built-in github and jira **services**, declared by the project
  nowhere), one a derived back-link (`links_back`), two internals. So three
  rules, each narrow: an undeclared field with an **empty** value is skipped;
  a field naming a back-link this build **derives** is skipped, asked of the
  schema rather than matched as a `_back` suffix; and `INTERNAL_FIELDS` covers
  the rest. A field carrying a real value is still reported, so a schema
  genuinely missing a declaration is not hidden.
- **`null` means unset, not malformed.** JSON's own spelling for an absent
  value, and what an export writes for every `nullable` field left blank —
  distinct from an object or nested array, which says something this model has
  nowhere to put and is still `needimport.invalid-value`. The `bool` exception
  that reads an empty option as *true* is RST option syntax and deliberately
  does **not** apply to an imported value.
- **`rusty_sphinx_ast` gained `impl From<&AttributeValue> for FieldValue`.**
  `rusty_sphinx_index`'s `EntitySubject` and the import's own filter subject
  both needed it and may not depend on each other; `ast` already depends on
  `rusty_sphinx_filter`, so the mapping lives there and the duplicate that
  existed in `entity_subject.rs` was deleted rather than copied.

## Narrowings

- **`:hide:`, `:collapse:`, `:layout:` and `:style:`** are refused by name.
  They are presentation, which this build takes from the schema's per-type
  `template` and the render-time site config — a per-import switch would put
  presentation back into a parse-time input, which ADR-009 decided against.
- **`:setup:`, `:pre_template:` and `:post_template:`** are refused by name.
  They run Python.
- **A `.. noqa:` cannot silence a diagnostic from imported prose.** A
  suppression matches on a file and a line range, and imported content has
  neither. The `needimport.*` diagnostics themselves are suppressible
  normally, since they are reported against the document's own line.
- **No `needs.json` is written.** ADR-010's export stays `Proposed`; this is
  the reading direction only, and the format is pinned by this reader's tests
  and by `docs/entities.md` rather than by a writer.
- **Named sections cannot be imported**, the format having no spelling for
  one (Decision 4). An argument matching no declared import key and not ending
  in `.json` is refused by name rather than opened, so a forgotten declaration
  warns where a forgotten `parse_data` entry fails.

## Not done

`.. needextend::` and `.. needservice::`, the other two entity-mutating
directives, remained unimplemented as of this decision. `needextend` — which
*modifies* entities other documents declared, a project-wide operation this
ADR left with no obvious phase — is answered by ADR-019 (`entity-update`).
`needservice` calls out to external systems, which a sandboxed build cannot
do at all for the reason §5 gives, and remains undone.
