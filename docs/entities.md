# Entities

An **entity** is a typed, identified, attributed thing that can be referenced
from prose and can point at other such things.

That shape is not new. A sphinx-needs requirement is one. CPython's
`.. audit-event::` is one. A Python class, as far as the documentation build is
concerned, is one. What differs between them is vocabulary, not structure — so
rusty-sphinx lets a project declare its own vocabulary in a schema file and
treats the result as a first-class construct: parsed, validated, indexed
project-wide, cross-referenceable, and rendered.

Sphinx-needs, in this model, is *a schema*, not a feature.

There is a worked schema and a set of documents using it under
`examples/entities/`; every example in this guide is taken from there, so the
two cannot drift apart.

---

## The four kinds of declaration

An entity type declares four different kinds of thing, and knowing which to
reach for is most of learning the model.

| kind | what it is | parsed as RST? | in the index? | filterable? |
|---|---|---|---|---|
| **attribute** | a value — a status, a priority, a list of tags | no | yes | yes (a later increment) |
| **section** | a document — prose with headings, directives, links | **yes** | no | no |
| **relation** | an edge to another entity | no | yes | yes |
| **role** | how prose *points at* an entity | — | — | — |

The rule in one line:

> **Attributes are values; sections are documents.**

If the thing you want to record is a string, a number, a choice from a fixed
set, or a list of those, it is an attribute. If it is prose that deserves
paragraphs, a code sample, a nested `.. note::` or a cross-reference, it is a
section. A requirement's `status` is an attribute; its verification criteria
are a section.

This is why the two are separate concepts rather than one field kind with a
`parsed = true` flag: they differ in every later phase, not just at parse time.

Sections are the part of this model that goes beyond what sphinx-needs offers.

---

## Declaring a type

```toml
[[entity_type]]
name     = "req"                      # the directive name: `.. req::`
label    = "Requirement"              # shown by the built-in rendering
argument = { fields = ["title"] }     # what `.. req:: <text>` means
id       = { prefix = "REQ_" }

  [[entity_type.attribute]]
  name    = "status"
  label   = "Status"
  type    = "enum"
  values  = ["open", "in_progress", "closed"]
  default = "open"

  [[entity_type.section]]
  name     = "verification-criteria"
  label    = "Verification criteria"
  required = true

  [[entity_type.relation]]
  name           = "links"
  label          = "Links to"
  to             = ["spec", "impl"]
  multiple       = true
  incoming       = "linked_by"
  incoming_label = "Linked by"
```

and in a document:

```rst
.. req:: The system shall boot within two seconds
   :id: REQ_001
   :owner: platform
   :status: in_progress
   :tags: boot, kernel
   :links: SPEC_001, IMPL_001

   The unnamed leading prose is the *content section*.

   .. verification-criteria::

      Measured with the boot harness on the reference board.

      .. note::

         A section's body is fully parsed, which is what lets it carry this.
```

### Attribute types

`string`, `text`, `int`, `bool`, `enum` (with `values`), `list<string>`,
`list<enum>` (with `values`).

A `bool` written bare — `:deprecated:` with no value — is `true`, which is the
ordinary RST spelling of a flag. A list is comma-separated. Giving `values` for
a non-enum type, or omitting it for an enum one, is refused when the schema
loads: both spellings look like they constrain the value and only one does.

### The `title` convention

Nothing in the schema privileges any attribute name. The *rendering* convention
is that an attribute called `title` becomes the entity's heading instead of a
row in its field table. Declare it like any other attribute; a type that has no
title — `audit-event`, say — simply does not declare one, and its links fall
back to showing the id.

### The argument

`argument.fields` says what the text after `::` means.

- `{ fields = ["title"] }` — the whole argument is one value. The sphinx-needs
  shape.
- `{ fields = ["name", "args", "version"], split = "comma" }` — a
  comma-separated signature. CPython's `.. audit-event::` shape.
- omitted — the type takes no argument, and one given is diagnosed.

Fewer parts than declared fields is fine; the trailing fields are simply unset,
and a `required` one among them is then reported by name rather than by comma
count. *More* parts than declared is an error: there is nowhere to put them,
and dropping them silently is exactly the degradation these diagnostics exist
to catch.

---

## Identity

An entity's id is determined three ways, in this order:

1. **An explicit `:id:`.** Used exactly as written. An illegal one is reported
   rather than repaired — silently rewriting it would leave the author's
   `:id:` and their `:links:` naming different things.
2. **Derived from attributes**, via `id = { from = ["name"] }`. The named
   values are joined with `_`, with any character an id forbids replaced by
   `_`. This is how `.. audit-event:: os.system, ...` gets the id `os.system`.
3. **Generated**, from a hash of the document path, the type and the source
   line.

An `id.prefix` is prepended to the derived and generated forms, never to an
explicit one. `id = { required = true }` refuses to generate at all.

Ids are **case-sensitive** and may hold letters, digits, `_`, `-`, `.` and `:`.
`REQ_001` and `req_001` are different entities. That matches sphinx-needs.

Generated ids are deterministic, which is not negotiable: a parsed document is
a build-system output cached on its inputs. The trade-off is that inserting a
line above an entity changes its generated id. That only affects entities whose
ids were never shown to anyone — give an entity an explicit `:id:` if you mean
to link to it.

---

## Cardinality

`required` and `multiple` are declarable on all three kinds.

| kind | `required` means | `multiple` means |
|---|---|---|
| attribute | the option must be given, unless it has a `default` | n/a — use a `list<…>` type |
| section | the sub-directive must appear at least once | it may appear more than once, kept in document order |
| relation | at least one target must be named | more than one may be named; otherwise the option takes exactly one |

Each violation has its own diagnostic, so you are told which rule you broke:
`entity.missing-required-attribute`, `entity.missing-required-section`,
`entity.duplicate-section`, `entity.missing-required-relation`,
`entity.multiple-relation-targets`.

An attribute that is both `required` and `default`-ed is refused at load: it
could never be missing, so one of the two is a mistake.

---

## Sections

A section is a sub-directive written inside an entity's body. Its content is
fully-parsed RST.

- The unnamed prose before the first section directive is the **content
  section**. It is one of the sections, not a field beside them, so document
  order is preserved and a section named `content` cannot be confused with it.
- Sections **do not nest**: a section directive inside a section is diagnosed
  rather than silently accepted.
- Entities *may* nest inside a section, and everything inside one — targets,
  glossary terms, further entities — is indexed normally.
- A section name may not shadow a directive the build already has, so you
  cannot declare a section called `note`. This is checked when the schema
  loads.
- A section is recognised only inside a type that declares it. Written anywhere
  else it is reported (`entity.unknown-section`,
  `entity.section-outside-entity`) rather than degrading into an unknown
  directive that renders as nothing.

Section content is deliberately **not** stored in the project index: a listing
directive needs fields, not paragraphs, and keeping prose out is what bounds
the index on a project with thousands of requirements.

---

## Relations and back-links

A relation is declared on the type that **carries** it, beside that type's
attributes. There is no `from` list, because the source is wherever the option
is written — which removes a whole class of error where a declared source
drifts from actual use.

```toml
  [[entity_type.relation]]
  name           = "verifies"
  to             = ["req"]        # allowed target types; omit for any
  required       = true
  multiple       = true
  incoming       = "verified_by"  # the back-link derived on the target
  incoming_label = "Verified by"
```

**Back-links are derived, not declared.** A `req` never says it can be
`verified_by` something; that falls out of `test.verifies` pointing at it. So
reading the `req` block alone does not tell you it will show a "Verified by"
list — the accepted cost of declaring relations where they are used.

Derivation happens project-wide while the index is built, because an entity's
incoming edges come from documents it has never heard of.

### What must agree

A shared **outgoing** name constrains nothing. `req.links` and `spec.links` are
independent declarations that may differ in `to`, in `label`, and even in
`incoming` — they render on different types and feed different back-links.

The constraints are on the derived side:

1. Two relations whose `incoming` names meet **on a shared target type** must
   agree on `incoming_label`. That target has one back-link fed by both, so two
   labels leave it with no coherent heading. If their `to` sets are disjoint,
   no entity ever sees both and they may differ freely.
2. A derived back-link name must not collide with an attribute, section or
   relation the target type declares itself.

There is no reserved-word rule: the rendering keeps attributes, sections,
outgoing links and incoming links in separate namespaces, so a back-link cannot
shadow `id` or `title` however it is named.

Omitting `incoming` makes a relation one-directional.

---

## Roles

**A role is not required for an entity to be linkable.** Every entity registers
an ordinary target, so ``:ref:`REQ_001` `` reaches one with no role machinery
at all, and the built-in ``:entity:`REQ_001` `` role accepts every type without
being declared.

What a declared role adds is exactly two things:

1. **A type check on the link.** ``:req:`SPEC_003` `` is reported as pointing
   at the wrong kind of thing. It still links — the target exists, and a dead
   link would help nobody — but you are told.
2. **The spelling an existing project already writes**, which is what lets a
   sphinx-needs project migrate by choosing a schema rather than by editing
   every document.

Roles are declared at the schema's top level, not inside a type, because the
relation is many-to-many: `:need:` refers to four types at once.

```toml
[[role]]
name  = "req"
types = ["req"]        # a typed reference

[[role]]
name  = "need"         # the sphinx-needs spelling
types = ["req", "spec", "impl", "test"]

[[role]]
name  = "anything"     # omit `types` to accept any entity
```

A reference with no explicit title shows the entity's title, falling back to
its id. ``:req:`the boot budget <REQ_001>` `` overrides that.

---

## Presentation

With no configuration, an entity renders as a titled block carrying:

- its id as an anchor (`id="entity-REQ_001"`), which is what every link to it
  targets;
- a header with the type label, the title and the id;
- a table of its attribute values, under their declared labels;
- its sections, its prose first and then its named ones **in the order the
  schema declares them**, each under its declared label;
- its outgoing relations and its derived incoming back-links, as labelled
  lists of links.

The CSS classes are `entity`, `entity-<type>`, `entity-header`,
`entity-attributes`, `entity-content`, `entity-section`,
`entity-section-<name>`, `entity-links`, `entity-link-label` and
`entity-link-list`. Note that they describe the *generic* shape, not any one
type: a project that declares a new `.. req::` needs no CSS of its own.

Both stylesheets in the repository carry these rules — `assets/default.css`,
the theme `templates/default.html` links, and the inline `<style>` in
`examples/custom_template.html`, which the example site uses instead. Styling a
new construct means updating both, or the example site silently keeps rendering
it unstyled.

### Per-type templates

A type may name a MiniJinja template instead, which is what lets
`.. audit-event::` read as prose while a `.. req::` renders as a box:

```toml
[[entity_type]]
name     = "audit-event"
template = "entity_audit_event.html"
```

The schema names the file; the site supplies it:

```python
rusty_sphinx_site(
    entity_templates = ["//examples/entities:entity_audit_event.html"],
)
```

The split is ADR-001's rule — a sandboxed build relocates files, so a path in
config would break. A template a type names but the site does not supply fails
the build rather than silently falling back.

The template receives:

| name | what it is |
|---|---|
| `id`, `type`, `label`, `title` | the entity's identity; `title` may be absent |
| `anchor` | the `id` attribute a link to this entity targets |
| `content` | the content section, **as rendered HTML** |
| `sections` | `{name: [html, …]}` — a list per name, since a section may be `multiple` |
| `section_list` | `[{name, label, body}, …]` — the same bodies **in render order** |
| `attributes` | `{name: value}`, as strings |
| `outgoing`, `incoming` | `{relation: [{id, href}, …]}` |
| `labels` | `{attributes, sections, relations}` — the headings the schema declares |

Four separate namespaces, so a back-link named `title` cannot shadow the
entity's own — which is why no reserved words are needed. Hyphens in section
and relation names become underscores (`sections.verification_criteria`),
because a hyphen is a subtraction in a Jinja expression.

Section bodies arrive as **already-rendered HTML**: MiniJinja cannot render
RST, so the node tree never reaches a template. They are marked safe and are
not escaped again; every other value is ordinary text and is escaped as usual.

`labels` exists so a template renders a heading under the name the schema
declares instead of holding a second copy that can drift from it. Its
`relations` map covers both directions, and the incoming half is derived the
way the built-in rendering derives it — a back-link is declared on the type at
the *other* end, so `req` never mentions the `implemented_by` that lands on it.

`section_list` exists because `sections` is keyed by name and a map carries no
order at all. It holds what the built-in rendering shows, in the order it shows
it, so a template can match the built-in box; use `sections` to reach one
section by name.



---

## Build wiring

The schema is a **parse-time** input: it is what makes `.. req::` a directive
rather than an unknown name. So it is declared on every library whose documents
use it, *and* on the site that assembles them.

```python
rusty_sphinx_library(
    name = "docs",
    srcs = glob(["*.rst"]),
    entity_schema = "entities.toml",
)

rusty_sphinx_site(
    name = "site",
    entity_schema = "//examples/entities:entities.toml",
    deps = [":docs"],
)
```

They must be the same file. A document parsed against a *different* schema than
the site indexes with is reported as `entity.schema-mismatch` rather than
producing quietly wrong output — including the likelier mistake of declaring it
on the library and forgetting it on the site.

Only libraries that actually use entities need to declare it. A library parsed
against no schema at all is not reported: most libraries in a multi-library site
use no entities and have no reason to name one. A library that *does* use them
and forgets to declare it is not silently lost either — its directives are never
recognised, so it surfaces as unknown directives and dangling references, which
point at the offending line rather than at the whole document.

The cost of reaching parse time is real and accepted: **editing the schema
re-parses every document in every library**, not just re-index and re-render.
That is the price of parse-time diagnostics.

A project that declares no schema behaves exactly as it did before this feature
existed, byte for byte.

---

## Editor support for the schema file

TOML has no schema language of its own, so `schemas/entities.schema.json` is a
JSON Schema over TOML's data model. Editors with TOML support — `taplo`, and
through it VS Code's *Even Better TOML* — pick it up two ways: `.taplo.toml`
associates every `**/entities.toml` in this repository, and a file outside it
can point at the schema itself:

```toml
#:schema ../../schemas/entities.schema.json
[[entity_type]]
name = "req"
```

That gives completions for every key and every attribute type spelling, and
flags a mistyped key while you write it.

**It validates the grammar, not the semantics.** The schema knows about table
structure, field names and types, required versus optional, the seven `type`
spellings, the two `split` values, and unknown keys. It cannot know that a
relation's `to` names a declared entity type, that `argument.fields` names a
declared attribute, that two types share a name, or any of the other rules in
the *Cardinality* and *Relations* sections above — JSON Schema has no way to
say "this string must name something declared elsewhere in this document".

So a clean editor means the shape is right, not that the schema loads. The
loader remains the authority, and it is what the build runs.

The schema is **generated** from the types the loader deserializes into, and
checked in. After changing them:

```bash
cargo run -p rusty_sphinx_worker -- entity_json_schema > schemas/entities.schema.json
```

A test regenerates and compares, so a stale file fails the build; another
validates `examples/entities/entities.toml` against it, so the schema, the
example and the loader cannot drift apart.

---

## Migrating from sphinx-needs

`examples/entities/entities.toml` reproduces the built-in vocabulary — `req`,
`spec`, `impl`, `test`, their statuses and tags, `links` and its back-link, and
the `:need:` role — closely enough that documents using those constructs work
unchanged.

Deliberately **not** supported:

- **Filter strings.** Sphinx-needs evaluates them as Python expressions.
  rusty-sphinx has no interpreter, and will grow its own small typed filter
  language instead, with unsupported syntax diagnosed rather than silently
  matching nothing.
- **Listing directives** (`needtable`, `needlist`, `needflow`, `needpie`) —
  a later increment.
- **Dynamic functions** (`[[copy('id')]]`) and `needextend`.

---

## Diagnostics

Every one of these carries a source position and a stable code a `.. noqa:` can
name.

**While parsing**, with the position of the offending line:

| code | when |
|---|---|
| `entity.unknown-attribute` | an option the type declares as neither attribute nor relation |
| `entity.invalid-attribute-value` | a value that does not fit its declared type |
| `entity.missing-required-attribute` | a `required` attribute left unset |
| `entity.malformed-argument` | an argument a type takes none of, or too many comma parts |
| `entity.invalid-id` | an illegal `:id:`, or one that could not be derived |
| `entity.unknown-section` | a sub-directive this type does not declare |
| `entity.section-outside-entity` | a section directive written outside any entity |
| `entity.duplicate-section` | a section repeated without `multiple` |
| `entity.missing-required-section` | a `required` section not written |
| `entity.missing-required-relation` | a `required` relation with no target |
| `entity.multiple-relation-targets` | several targets on a single-target relation |

**While building the index**, attributed to the document that wrote the source
entity:

| code | when |
|---|---|
| `entity.duplicate-id` | two entities claiming one id |
| `entity.unknown-target` | a relation naming an entity no document declares |
| `entity.disallowed-relation` | a target whose type is outside the relation's `to` |
| `entity.schema-mismatch` | a document parsed against a different schema |

**While rendering:**

| code | when |
|---|---|
| `entity.role-type-mismatch` | a role resolving to a type it does not accept |

A faulty schema is not a diagnostic but a hard error: it is the vocabulary the
parser works from, so continuing would report a cascade of unknown-directive
messages instead of the one real problem. Every fault in the file is listed at
once.
