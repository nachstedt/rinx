# 19. `.. entity-update::`: a project-wide, non-destructive mutation

## Status

Accepted.

## Context

ADR-016's own "Not done" section named this directly: `needextend` "*modifies*
entities other documents declared, which is a project-wide operation with no
obvious phase." Every later ADR that mentions it (017, 018) repeats that
verdict without revisiting it. This is that decision.

sphinx-needs' `.. needextend::` lets an author mutate the fields of one or
many already-declared needs — from any document — after the fact: set,
append to, or remove from an attribute or a relation, targeted either by a
single id or by a filter over the whole project. The construct is small; what
makes it worth an ADR is the same thing that stalled it for three prior
decisions — where in the pipeline a project-wide mutation belongs — plus one
deliberate addition beyond upstream: **applying an update is never
destructive**. It never overwrites an entity's as-authored data; it builds a
separate, derived audit trail beside it, which every reader (built-in
rendering, a custom template, a filter, a diagram) reads through instead of
the raw record.

## Decision

### 1. Own name: `entity-update`, with `needextend` as the alias

Unlike `.. needimport::` (ADR-016 §2), this reverses back to the ordinary
interoperability rule: this project's own name, with sphinx-needs' spelling
accepted alongside it — the same pairing `entity-table`/`needtable`,
`entity-flow`/`needflow` and `entity-pie`/`needpie` already use. ADR-016's
test for the *other* rule ("no name of this build's beside it") is whether
the construct "reads another tool's file format, carries that format's
limitations... and exists to keep an existing project building." Nothing
about `needextend` does: its argument is this project's own filter grammar
(`rinx_filter`), and its field vocabulary is this project's own typed
attribute/relation model. There is no foreign format to bridge to and no
richer future construct whose name needs protecting. The diagnostic family is
`entity-update.*` regardless of which spelling was written, both are reserved
against entity schemas, and both parse to one `EntityUpdateSource`-tagged
node — `crates/ast/src/entity_update/source.rs`, the exact shape
`EntityTableSource` already has.

### 2. The phase: a derived overlay, applied between merge and back-links — never a mutation of `entities`

`rinx_analyzer::apply_entity_updates` runs inside
`build_project_index_reporting`, **between** `merge_document_analyses` and
`derive_entity_backlinks`. It needs the whole merged graph, exactly as
back-link derivation does, but must run *before* it: an update can change an
entity's effective outgoing edges, and back-links must be derived from those,
not from the as-authored ones.

The choice that resolves ADR-016's "no obvious phase" complaint is narrower
than just picking a phase: **the phase never mutates `ProjectIndex::entities`
at all.** It builds a new, separate structure —
`ProjectIndex::entity_update_history`, a `BTreeMap<EntityId,
EntityFieldHistory>` — recording, per touched field, the entity's original
value, every applied change (with its source document, span, mode and
resulting value), and the current value. This was not the first design
considered; an earlier draft had the phase write straight into
`EntityRecord.attributes`/`.outgoing`, which would have been this project's
first case of a project-wide phase rewriting already-merged entity data,
breaking the rule every other entity mechanism follows (`derive_entity_backlinks`,
`collect_entity_diagnostics`: read the merged graph, write only *beside* it).
The overlay design instead **preserves** that rule: `entities` stays exactly
what documents wrote, and a derived, additive structure sits next to it —
the same shape `entity_backlinks` already has, just carrying more than one
kind of fact.

The corollary is `ProjectIndex::effective_attribute`/
`effective_relation_targets`: two new accessors that fold the history over
the original, and the **one choke point** every reader must go through
instead of `entities` directly:

```rust
pub fn effective_attribute(&self, id: &EntityId, field: &str) -> Option<&AttributeValue> {
    if let Some(history) = self.entity_update_history.get(id).and_then(|h| h.attributes.get(field)) {
        return history.current.as_ref();
    }
    self.entities.get(id)?.attributes.get(field)
}
```

Four call sites needed the swap: `EntitySubject::field` (which is what makes
`entity-table`, `entity-flow`, `entity-pie` and every diagram's `filter()` see
an update's effect automatically — all four already resolved exclusively
through `EntitySubject`), `derive_entity_backlinks` and
`collect_entity_diagnostics` (so a back-link and a target-validity check both
reflect an update-appended/removed relation, not just the as-authored one),
and — the one that would otherwise have made this whole feature invisible
where an author would look for it first — the built-in entity rendering and
its template escape hatch (§5).

### 3. The target: an id wins over a filter reading of the same text, resolved once the project is merged

The argument is either a filter expression or a single entity's id, and which
reading applies can only be decided once the whole project's entities are
known — a document's own parser never sees them. `read_update_argument`
(`crates/parser/src/directives/filter_option.rs`) computes both readings
independently at parse time: `candidate_id` when the trimmed text is a legal
`EntityId` spelling, `filter` when it also parses as a filter expression (a
bare id like `REQ_001` does — it is `Truthy(Field("REQ_001"))` — while one
containing `:`/`-`/`.` usually does not). It reports
`entity-update.invalid-argument` only when *neither* reading survives —
deliberately not an unknown-field diagnostic against the filter reading even
when one is available, since which reading apply time ends up using depends
on the whole project, and flagging a filter reading that turns out unused
(because the argument was actually a good id) would be a false positive on
the overwhelmingly common case.

`apply_entity_updates::resolve_targets` makes the actual call, once the
project is merged: `candidate_id` wins whenever `index.entities` actually
contains it, exactly how upstream sphinx-needs disambiguates. Only then,
knowing the filter reading is the one actually driving selection, does it
check the filter's field names against the schema and defer the
`entity-update.unknown-field` diagnostic to this point.

### 4. Field mutations: four operations, value conversion deferred to the matched entity's own type

Every option line other than `:strict:` is a field mutation:

- `field: value` — **Set**, overwriting the effective value.
- `+field: value` — **Append**, to a list-valued attribute or a relation's
  target list.
- `-field: value` — **Remove**, one value from a list-valued field/relation.
- `-field:`, written with no value — **Clear**, the field entirely.

`FieldMutationMode` folds the value into the mode as one four-variant enum
(`Set(String) | Append(String) | Remove(String) | Clear`) rather than a
3-mode enum plus a separate `Option<String>`, so `-field:` and `-field: value`
are distinct variants an exhaustive match must handle, instead of one
operation whose optional argument silently means two different things.

The field name is checked against the schema's *full* vocabulary — every
type's fields, unioned, via `EntitySchema::declares_field` — at parse time,
since a filter target may match several types at once and the parser cannot
yet know which. What cannot be checked that early is whether the value fits:
the `AttributeType` a `Set`'s text must parse against belongs to the matched
entity's own declared type, known only once a specific entity is matched, at
apply time. So a `FieldMutation` stores its value as raw, unconverted text,
and `apply_attribute_mutation`/`apply_relation_mutation` do the actual
conversion — through the **same** `rinx_entity::parse_attribute_value`/
`split_list` funnel a written `.. req::` and `.. needimport::` already share,
so none of the three can validate a value differently.

Two identity fields are refused outright, at parse time, as
`entity-update.protected-field`: the entire
`rinx_entity::field::BUILTIN_FIELDS` set (`id`, `type`, `type_name`,
`docname`) plus `title`. `title` is the one addition beyond the built-in set,
and the reason is a real desync risk, not a style preference:
`EntityRecord` keeps a **denormalized** `title: Option<String>` separate from
`attributes["title"]`, read directly by nav/listing code and by
`EntitySubject`'s own `"title"` arm. Allowing `Set` on the attribute alone
would leave `record.title` stale with no dual-write code to keep the two in
sync — no other field has this duplication, so no other field needs the
protection. This is a deliberate divergence from upstream, which does allow
editing a need's title.

### 5. Sections are explicitly out of scope

Unlike sphinx-needs' needs, this project's entities can carry named
**sections** — fully-parsed, arbitrary nested RST content declared via
`SectionSpec`, not scalar/list values (ADR-009). "Override a section from
another document" is new territory this feature could reach into, and
deliberately does not.

The deciding fact is architectural, not a matter of taste: **sections are
excluded from `ProjectIndex` by design** (ADR-009 — "sections are documents...
not indexed," specifically to bound index size), while everything §2's
overlay is built on works *because* attributes and relations already are
index data. Extending to sections would mean either breaking that size bound
(indexing section content after all, unpredictably — only once some other
document happens to override it) or inventing a second, parallel
cross-document content channel this feature doesn't otherwise need, plus a
much harder conflict model — diffing two RST trees is not like comparing two
`AttributeValue`s — and a genuinely open question about what re-render
invalidation even means once any document might override any entity's
section. None of that reuses a single piece of the machinery attributes and
relations already get for free. Weighed against a use case with no
corpus-backed demand (sphinx-needs itself has no analog at all — this would
be a from-scratch design, not a bridge), the cost is not justified now. If a
real need for cross-document prose contribution appears later, it deserves
its own purpose-built construct — the same call already made once for
`entity-import` (ADR-016 §2) — rather than stretching a single option line's
text value, a poor fit for multi-line nested RST regardless.

An option naming a declared section is rejected at parse time with its own
code, `entity-update.section-not-supported`, distinct from
`entity-update.unknown-field` so the message can say plainly why rather than
reading like the name was simply not recognized. The directive's own
justification body (§6) already gives an author a place to attach
explanatory prose to a *specific change*, covering a good part of the
practical motivation for this at a fraction of the cost.

### 6. A body, rendered by default — the one deliberate divergence in visibility

`.. entity-update::` accepts a body: ordinary RST prose, parsed exactly as
`.. dropdown::`'s is (`parse_blocks` under `ctx.nested(...)`, after the
option block). It is the justification for the change, and — because
`EntityUpdate` therefore carries real block content — it needed the same
traversal arms a container gets (`walk_nodes`, `resolve_nodes`,
`assign_index_ids`, `collect_anonymous_targets`, `document_index::index_nodes`),
not the leaf treatment a content-free directive like `Highlight` gets.

The directive **renders its own visible box by default**: its target, its
field mutations, and its justification prose — controlled by
`SiteConfig::show_entity_updates`. This is the one deliberate divergence from
sphinx-needs, whose `needextend` produces no output at all. The audit trail
is the whole point of §2's non-destructive design; an audit trail nobody can
see on the page is not much of one. A site that wants the mutation applied
silently sets `show_entity_updates = false`.

The history built in §2 is also exposed to a **custom entity template**
(`crates/renderer/src/blocks/entity/template.rs`) as a `history` namespace —
`history.attributes.<field>.original`/`.current`/`.applied[]` (each entry's
`doc_path`, `mode`, resulting value, and a `justification` string, the
directive's body pre-rendered to HTML, looked up by the entry's
`update_index` into `ProjectIndex::entity_updates`) — so a template author can
build their own changelog/audit UI without new renderer code. Empty
namespaces stand in for an entity nothing ever touched, rather than an absent
`history` variable, so `history.attributes.status.current` reads
unconditionally.

### 7. Conflicting updates: detected, but never blamed on one side

A **conflict** is a `Set`/`Clear` from one directive overwriting a field
whose current value was already established by a `Set`/`Clear` from a
*different* directive, with a *different* resulting value.

`Append`/`Remove` are deliberately excluded: they are explicitly incremental
— building a tag list a piece at a time across several directives is the
normal, intended use, not a disagreement — and two `Set`s that happen to
agree are not a conflict either, since nothing is actually in dispute. The
check (`conflicting_attribute_update`/`conflicting_relation_update`) walks a
field's `applied` entries backward for the most recent `Set`/`Clear`; if one
exists from a different `update_index` and its resulting value differs, the
new entry records `conflicts_with: Some(that entry's update_index)` — pure
bookkeeping, a side effect of the deterministic application order (below),
never surfaced to an author as "later wins."

**Reporting is symmetric.** `.. noqa:` matches a diagnostic only against its
own file's span (`Suppression::suppresses` refuses a positionless diagnostic
outright, and never crosses files), so attributing one shared
`entity-update.conflicting-update` diagnostic to whichever directive happens
to sort later would leave the *other* author with no way to suppress it, and
no way to even know a side was picked — "later" here is only an artifact of
the tie-break order below, not a real precedence either author could act on.
`apply_entity_updates` therefore pushes **two** diagnostics per detected
conflict, one per involved directive, each carrying that directive's own span
in its own file and naming the other's location. Either author can
independently silence their own copy; if only one does, the other's still
shows — the right default, since suppressing a disagreement between two
documents should not be something either side can do unilaterally on the
other's behalf.

The conflict is also made **visible in the rendered output**, not just
logged: the entity's own attribute/relation row (`crates/renderer/src/blocks/entity.rs`)
carries a distinct `entity-conflict` class and a title naming the reason
whenever its current value's last-applied entry is flagged. The directive's
own box (§6) does *not* additionally re-mark the specific mutation — a
filter-targeted update can match many entities at once, and whether *this*
mutation conflicted is then a fact per matched entity, not one the directive
itself can summarize in one marker without either picking an arbitrary entity
or listing all of them; the entity-side marker is the well-defined place to
show it. The `history` template namespace carries `conflicts_with` per
applied entry regardless, so a template-driven audit UI can still build its
own per-entity view with no conflict-detection logic of its own.

### 8. Deterministic application order, and why a phase at all

`index.entity_updates` is sorted **in place**, once, by `(doc_path, span
start line)` ascending (stable, so ties keep merge order) — the directive's
canonical order, and what every `AppliedFieldUpdate`/`AppliedRelationUpdate`'s
`update_index` is simply a position in.

Why a dedicated phase rather than computing effective values on demand,
lazily, from every reader: not primarily performance, though avoiding a full
re-walk of every update on every single field lookup during rendering is a
real secondary win. It is structurally required, for reasons a lazy design
has no answer to:

1. The id-vs-filter disambiguation (§3) needs the whole merged project, which
   exists only once `merge_document_analyses` has run — nothing shy of a
   phase running after that point can answer it, and every caller would
   otherwise have to re-answer the same question independently.
2. Diagnostics must be computed exactly once, at a point already wired into
   the existing warning pipeline (`build_project_index_reporting`'s
   `Vec<DocumentDiagnostics>`, which `--warnings-output` and `.. noqa:`
   suppression already consume) — there is no principled place for a
   render call over one page to discover "did this directive's target match
   zero entities."
3. The traceable history itself (§2) is the artifact a phase produces — there
   is no such thing as lazily exposing a fold nobody has computed yet to a
   template.
4. One canonical, ordered pass avoids the built-in renderer, a custom
   template, `EntitySubject`'s filter evaluation, and this phase's own later
   iterations each re-implementing the same fold over the same update list,
   which would risk them silently disagreeing about which update "wins" a
   conflict.

### 9. Live preview: stale, not blank — and needs no new code to be so

`crates/worker/src/commands/preview.rs::process_preview` calls only
per-document `analyzer::analyze` + `ProjectIndex::merge`, never
`build_project_index_reporting`'s phases. So `entity_update_history` in
preview is whatever the last full `index` build computed — **stale, not
blank**, exactly like `entity_backlinks`/`page_order`/section numbers already
are, per this project's own documented live-preview limitation ("titles/
cross-refs go stale until a manual rebuild").

Verified directly against `ProjectIndex::merge`: its entity handling always
does `self.entities.insert(id, record)` with `record` coming from the fresh
local analysis, so a live-edited document's own entity attributes still show
correctly in preview — ordinary edits are unaffected. The one real, narrower
caveat: `effective_attribute`/`effective_relation_targets` prefer
`entity_update_history` (from the stale index) over `entities` (which now
holds the fresh edit) *whenever a history entry exists for that field* — so
editing a field that also has needextend history elsewhere can leave the
stale historical `current` value masking the fresh edit in preview until the
next rebuild. This is a sharper version of the same staleness every other
cross-document fact already has, not a new category of bug, and needs no new
gating code: rebuilding is already the prescribed remedy for a stale fact in
this preview design.

## Consequences

- **`ProjectIndex` gained two fields and two accessor methods** —
  `entity_updates: Vec<EntityUpdateRecord>` (accumulated across documents like
  `genindex_entries`, never merged-with-dedup) and
  `entity_update_history: BTreeMap<EntityId, EntityFieldHistory>` (recomputed
  globally, never merged, like `entity_backlinks`), plus
  `effective_attribute`/`effective_relation_targets`. Every reader of an
  entity's attribute or outgoing-relation *value* now goes through the latter
  two rather than `entities` directly.
- **`rinx_analyzer` gained a dependency on `rinx_filter`**,
  needed for `Expr`/`FieldName` in target resolution;
  `rinx_index::EntitySubject` was already reachable.
- **No Bazel changes at all.** `ProjectIndex` was already the sole input every
  render action's cache key is built from; an edit to an
  `.. entity-update::` directive, or to an attribute it targets, already
  changes the serialized index the same way an ordinary attribute edit does.
- **`EntityMultipleRelationTargets` is reused**, not duplicated, for a
  cardinality violation an `Append` produces onto a non-`multiple` relation —
  a code names the construct's fault, not the phase that found it.
- **Relation-target existence/type-acceptance is not re-validated inside
  `apply_entity_updates`.** `collect_entity_diagnostics`, switched (§2) to
  read effective outgoing edges, already catches a bad id an `Append` just
  wrote, for free.

## Not done

`.. needservice::`, the remaining sphinx-needs entity-mutating directive,
stays unimplemented: it calls out to external systems, which a sandboxed
build cannot do at all, for the reason ADR-016 §5 gives for refusing a
`needimport` URL. It is refused by name as `needservice.unsupported`, pointing
at a `needs.json` snapshot read by `.. needimport::`.
