# 11. Listing entities: a directive of our own, and a filter language of our own

## Status

Accepted.

## Context

The entity model (ADR-009) could declare, parse, index, cross-reference and
render entities, but a project could not *ask the graph a question*. That is
half of what sphinx-needs is for, and the benchmark measured the cost: of the
73 unsupported directive uses in useblocks' demo corpus, `needtable` was 17 —
the largest single construct after `uml`.

Reading those 17 settled what the feature actually is. **Every one of them
carries a `:filter:`**, and the filters are Python expressions:

```
type == "fsr" and docname is not None and "safety_example" in docname
  and ("Process" in title or "Component" in title)
```

The rest of the option surface is small — `:columns:` in 15, `:style: table` in
15, `:colwidths:` in 2, `:sort:` in 1, and nothing else at all. A listing
directive without filters would therefore have rendered an unfiltered table for
100% of real uses and called it support.

ADR-009 decision 9 had already declined Python and planned "a small typed
expression language, evaluated against `EntityRecord`" for a later increment.
This is that increment.

## Decision

### 1. The filter language is ours, hand-rolled, in its own crate

`rusty_sphinx_filter` is a leaf crate depending on nothing of ours, because two
phases that may not depend on each other both need it: the **parser** parses a
filter and the **renderer** evaluates one. What it knows about the thing being
filtered arrives through an injected `FilterSubject` trait — the same seam
`ParseFileLoader` uses for the filesystem — so the crate has never heard of an
entity.

Three families of library were considered and declined:

- **Ready-made expression languages** (`evalexpr`, `cel-interpreter`) bring
  their own syntax: `&&`/`||`, no `is not None`, and CEL's `in` is list
  membership only, so `"Power" in title` over a string is a type error there.
  Every corpus filter would fail to parse, which defeats the whole
  compatibility argument below. `rhai` is a full interpreter, which ADR-009
  decision 9 declined on its own terms.
- **`pest`** is cheap in dependencies (five crates) but not in fit: it yields
  `Pairs<Rule>` rather than our `Expr`, so the left-associative fold is still
  hand-written; its idiomatic walker is `unwrap`-shaped where this parser must
  never panic; the rejection messages below would still be ours to write; and a
  `#[grammar = "…"]` file needs `compile_data` in Bazel, whose failure mode is a
  green `cargo build` and a red `bazel build`.
- **`rustpython-parser`** was the strongest runner-up — sphinx-needs filters
  *are* Python, so parsing the real thing and lowering a subset would give the
  best rejection messages for free. Declined on weight: roughly fifteen crates
  including a bignum library and LALRPOP tables, in every sandboxed build, for a
  twelve-rule language. (`ruff`'s parser is not an option; `ruff_python_parser`
  on crates.io is a placeholder.)

So the front end is a hand-written tokenizer and recursive descent, as
`rusty_sphinx_cdecl` already is for a *harder* grammar, with no dependency at
all beyond `serde`.

### 2. Unsupported Python is refused by name, not as "syntax error"

This is what makes the hand-rolled choice cost little, because these messages
have to be written by hand under any front end — and it is the concrete form of
ADR-009 decision 9's promise that unsupported syntax would be "diagnosed rather
than silently matching nothing".

| written | reported |
|---|---|
| `len(mitigates) > 0` | function calls are not supported, pointing at `len` |
| `[[copy('id')]]` | dynamic functions are not supported |
| `[n for n in needs]` | comprehensions are not supported |
| `status != None` | comparing to `None`; use `is not None` |
| `a && b` | use `and` |
| `priority > 2` | ordering comparisons are not supported |

Ordering is absent deliberately rather than accidentally: no corpus filter
orders, and admitting `<` would mean deciding how text compares to a number,
which this language has no reason to answer.

### 3. The filter is parsed at parse time and stored in the `.ast`

ADR-009 decision 2's argument applies unchanged: the parser is the only phase
still holding the source position of the option line, so `type == "fsr` is
reported at the column it breaks at. The renderer then only evaluates, and
evaluation is total — every operator has an answer for a missing value — so it
cannot fail.

This is a deliberate departure from the `.. math::` precedent, which stores
LaTeX verbatim and converts while rendering. The difference is ownership: LaTeX
is a *backend's* notation and must not leak into a cache artefact, whereas this
expression tree is our own type, and parsing it early is what buys the
diagnostic.

A filter that fails to parse leaves the table listing **everything**. The
diagnostic already says what is wrong; an empty table on top of it would hide
what the author was reaching for.

### 4. The directive's own name is `entity-table`; `needtable` is an alias

The entity model's claim is that *sphinx-needs is a schema, not a feature*. A
core directive called `needtable` would contradict it, so the canonical name is
`.. entity-table::` and `.. needtable::` is accepted as a second spelling of the
same directive — which is what lets a migrating project choose a schema rather
than edit every document.

Consequences, all deliberate:

- **Both names are reserved** against entity schemas, since this build has no
  `extensions =` config and a supported extension directive is simply always
  available.
- **They parse to one node**, recording which was written — the shape
  `TableSource` already uses for `list-table`/`csv-table`. The spelling is kept
  only so a diagnostic can quote what the author wrote; it changes nothing about
  the rendering, because two documents using different names for one directive
  should not look different.
- **One diagnostic family covers both.** A `.. needtable::` author suppresses
  with `entity-table.invalid-filter`, because a code names the construct, not
  the spelling it was written under.

### 5. Which fields exist is schema knowledge; what they are worth is index knowledge

The vocabulary is split across two crates on purpose, because two phases must
agree about it exactly. `rusty_sphinx_entity::field` owns *which* names resolve
— the five built-ins plus every attribute, relation and derived back-link any
type declares — because the parser must check a column without an index in
hand. `renderer`'s `entity_table/subject.rs` owns what each is *worth*, because
that needs a record. A second copy of either list would drift, and the drift
would surface as a column that validated and then rendered empty.

The `type`/`type_name` pair reads backwards — `type` is the directive name,
`type_name` the human label — and is kept because it is sphinx-needs' own, so a
migrating project's filters mean here what they meant there.

### 6. Rows are ordered by id when nothing says otherwise

`ProjectIndex::entities` is a `BTreeMap`, so this is free. It matters because a
rendered page is a build artefact cached on its inputs: the default order has to
be deterministic, not merely stable within one run.

### 7. An empty result is a diagnostic, and only the renderer can raise it

Whether a filter selects anything depends on every document in the project, so
the parser — which sees one — cannot know. `entity-table.empty-result` follows
`.. literalinclude::`'s rule that every way of selecting nothing is its own
diagnostic: an empty table is far more often a filter that no longer matches
than a deliberate statement that there is nothing to show.

Measured against the benchmark corpus before being kept unnarrowed, per the
guideline: all 17 corpus tables match, and the run reports zero
`entity-table.*` warnings of any kind.

The live preview stays quiet about it when no project index is available. There
the graph is unknown, so every table would be empty through no fault of the
author — the one case this diagnostic must not fire in.

## Consequences

- The benchmark's unsupported-directive count drops from 73 uses to 56,
  exactly the 17 `needtable` uses, with no new warnings.
- **A page's content now depends on the whole project.** No new caching problem
  arises, because the index is already an input to every render action; but it
  is now true that an entity edited anywhere changes what another page *shows*,
  not just what it links to.
- `needlist`, `needflow`, `needpie` and `needbar` are the same question with a
  different presentation. Each reuses this filter language unchanged, which is
  why the language and the directive are separate crates.
- ADR-010's tiering is unaffected but slightly clarified: this language
  evaluates against the index, so a constraint expressed in it would be a
  **tier-2** check, exactly as decision 6 of that ADR predicted. Nothing here
  moves a check into the parser that the parser does not need.

## Narrowings

- `:style:` accepts only `table`. `datatables` is a JavaScript grid, and is
  reported rather than silently downgraded: the author asked for browser-side
  sorting and would otherwise get a static table with no sign that they did not.
- `:show_filters:`, `:show_parts:`, `:layout:` and `:filter-func:` are
  unimplemented. The last is Python by definition.
- Arithmetic, ordering, attribute access and every call into Python are refused
  by name, per decision 2.
- A list-valued column becomes links only when every item names an entity the
  index knows, so a `tags` column stays text while a `verifies` column links.
  The two are indistinguishable by name — only the index can tell them apart —
  so the decision is made per cell. An id naming no entity stays text rather
  than becoming a dead link, since the index phase already reported it as
  `entity.unknown-target`; reporting it again from every table that lists it
  would multiply one fault by its readers.
