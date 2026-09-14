# 17. Charting the entity graph, and a picture that costs nothing

## Status

Accepted.

## Context

ADR-011 gave the entity model a way to *list* the graph, ADR-012 a way to
*draw* it from a template, and ADR-014 a way to draw it from a question. Each
closed by naming the same remainder. After `needflow` was covered, the entity
benchmark measured `needpie: 8` as the **largest single unsupported construct**
in useblocks' demo corpus — ahead of `button-link: 3` and `needextend: 3`.

Reading all eight settled what the feature is, as reading the seventeen
`needtable`s settled ADR-011. They have one shape:

```rst
.. needpie:: Safety Artifacts by Type
   :labels: Hazards, Safety Goals, FSRs, SYSREQs

   type == "hazard" and docname is not None and "safety_example" in docname
   type == "safety_goal" and docname is not None and "safety_example" in docname
   type == "fsr" and docname is not None and "safety_example" in docname
   type == "sysreq" and docname is not None and "safety_example" in docname
```

The title is the **argument**, each **content line** is one wedge's filter, and
`:labels:` pairs with them by position. `:labels:` is the *only* option any of
the eight carries — no `:legend:`, `:colors:`, `:explode:`, `:shadow:`,
`:style:` or `:filter-func:` anywhere in the corpus.

## Decision

### 1. `.. entity-pie::` is a third presentation, not a generic chart directive

The starting proposal was to make `needpie` an alias of a *generic* diagram
directive. It does not survive contact with the code, twice over.

**`entity-diagram` is already taken** — it is this build's name for
`.. needuml::`, a templated `PlantUML` diagram (ADR-012 §1). There is no
generic directive there to alias.

**And a generic chart directive is the wrong seam.** The only other candidate
for unification is `needbar`, whose single corpus use has a **two-dimensional**
body — a grid of filters with a header row — and an option surface
(`:stacked:`, `:show_sum:`, `:show_top_sum:`, `:xlabels: FROM_DATA`,
`:transpose:`) sharing nothing with a pie's. One `.. entity-chart::` with a
`:type:` switch would leave most of its options valid in exactly one mode, each
needing a "not valid for this `:type:`" diagnostic — which is a worse version
of refusing them by name.

This build already has both precedents, and they decide it. `Directive::DataTable`
unifies `list-table` and `csv-table` because the two are *structurally
identical after parsing*. `EntityTable` and `EntityFlow` stay separate nodes
despite asking one question, because their presentation differs (ADR-014 §1). A
pie is the second case, so `Directive::EntityPie` is a sibling of both: rows, a
graph, or proportions.

`.. needpie::` is the reserved alias, both names join `BUILTIN_DIRECTIVE_NAMES`,
and one `entity-pie.*` family covers both — the ADR-011 §4 arrangement,
unchanged.

**The genericity went one layer down, where it is real.**
`renderer/blocks/entity_pie/series.rs` turns wedges into counts and knows
nothing about pie geometry; a later `.. entity-bar::` asks the same question
with a second dimension and reuses it whole. That is where `needpie` and
`needbar` actually overlap — in *counting*, not in presentation — which is the
shape `filter_option.rs`, `uml/assemble.rs` and `diagram_figure.rs` already
have.

### 2. Nothing is compiled, so a chart costs a project nothing

This is the decision with consequences beyond the directive. The chart is
**SVG emitted inline by the render action**, not a file a build action
produces. So a pie has:

- no `.puml` file, no hash, no per-document compile action, no `validate_images`
  entry;
- **no `diagrams = True`** on its library, and therefore no
  `entity-pie.diagrams-disabled` code at all.

That is a deliberate departure from ADR-012 §4, and it is available precisely
because that ADR's cost argument does not apply here. `diagrams = True` exists
because Bazel must create actions before reading any document, so a project
that draws nothing would otherwise pay for a pipeline it never uses. A chart
creates no actions to begin with: it is drawn inside a render that was already
going to run. `examples/entities/charts.rst` therefore lives in the *plain*
library beside the tables, not in `diagram_docs`, and `bazel aquery` confirms
the site still declares exactly the three `PlantUMLCompile` actions it did
before.

The properties the renderer has to keep in exchange are the ones a build
artefact always demands:

- **Determinism.** The same counts must produce the same bytes on every
  machine, or an unchanged page changes and every cached render downstream of
  it misses. Iteration is over `ProjectIndex::entities`, a `BTreeMap`; the
  palette is a fixed table; and the font is vendored rather than found.
- **Totality.** Drawing must not panic on anything a document can hold, so
  every failure is a `None` the caller reports.

### 3. `plotters`, with the font vendored, and what that cost

The backend is `plotters` with `default-features = false` and the `ab_glyph`
feature — seven crates, and verified to pull **no `font-kit`, `fontconfig` or
`dirs`**. That matters more than the crate count: the default `ttf` path
resolves *system* fonts, which would make a rendered page differ between build
hosts. It is contained in `renderer/src/pie_chart.rs`, the one place it is
named, exactly as `syntect` is contained in `highlight.rs`, `math-core` in
`math.rs` and `octicons-pack` in `octicon.rs` — and for the same reason: a
backend's vocabulary must never reach a `.ast` file.

Two costs surfaced while measuring, both accepted with the alternative in hand:

- **`ab_glyph` embeds no font.** Without `register_font` every draw fails with
  `FontError(FontUnavailable)`, so `crates/renderer/fonts/DejaVuSans.ttf`
  (~760 KB, Bitstream Vera licence, `compile_data` on the `rust_library`) is
  checked in.
- **Slices are polygon approximations**, so a four-wedge pie is ~16 KB of
  inline SVG where four `<path>` arcs would be ~1.7 KB. Both were prototyped
  and both are byte-deterministic; the library was chosen over hand-rolled arc
  math so that `.. entity-bar::` inherits axes, ticks and bar layout instead of
  hand-rolling those too.

`:explode:` is refused by name partly *because* of this choice — `plotters`'
`Pie` has no slice offset — which is the honest form of the trade.

### 4. The filter language grew exactly two string methods

Eight of the corpus' wedges select by id prefix:

```
type == "fsr" and id.startswith("FSR_STEER")
```

ADR-011 §2 refuses every Python call by name, so each of these was reported as
"attribute access are not supported here" and — under that ADR's own rule that
an unparseable filter selects *everything* — left the wedge counting the whole
project. A chart of eight identical wedges is not partial support; it is a
wrong answer drawn convincingly.

So `rusty_sphinx_filter` admits `field.startswith("…")` and
`field.endswith("…")`, as one `Expr::TextAffix` variant, and **nothing else
past a `.`**. A bare attribute access keeps the message it always had; any
other method is refused by name as `` `.lower()` is not supported here; only
`startswith` and `endswith` are ``; Python's tuple argument is refused by the
tokenizer's existing tuple message, whose advice ("combine conditions with
`and` or `or`") is already right here.

Two details worth keeping:

- The test is measured through `FieldValue::as_text`, the same view `in`
  already takes, so `"F" in id` and `id.startswith("F")` cannot disagree about
  what a non-text field is worth.
- There is no `negated` field, unlike `Contains`: `not in` is a distinct token
  sequence that must be recognised where it is written, whereas
  `not x.startswith(…)` already parses as a `Not` around this.

This is the one place this increment *relaxes* an earlier refusal, and the
benchmark is what justifies it — see Consequences.

### 5. The empty-result plumbing was generalised rather than duplicated

A chart whose every wedge counts zero cannot be drawn: a pie of nothing has no
geometry. That is `entity-table.empty-result`'s situation exactly — only the
renderer can know it, since it depends on every document in the project
(ADR-011 §7).

Rather than a seventh parallel error vector, `EntityTableError` became
`EmptyListingError`, carrying its own `DiagnosticCode`. A code still names the
construct — a table reports `entity-table.empty-result` and a chart
`entity-pie.empty-result` — but everything downstream is shared: the
`RenderOutput` field, the `.. noqa:` filtering in `suppression.rs`, the warning
loop in `render.rs`, and, most importantly, the live preview's
`if index_json.is_some()` gate. A second vector would have had to remember that
gate, and a preview that reported every chart empty for want of an index is
precisely the failure ADR-011 §7 warned about.

### 6. What is refused by name

`:explode:`, `:shadow:`, `:style:` and `:filter-func:`, each with what to write
instead. ADR-014 §5's rule applies unchanged: silence is the one behaviour that
cannot be right, because an author who asked for exploded wedges and received a
flat chart has no way to find out why.

Two further diagnostics exist because a pie's body is a shape neither sibling
has. `entity-pie.no-slices` reports a chart with no content line at all —
while *parsing*, unlike the empty result, since the body is this document's own
text and no index is needed to see it is empty. And
`entity-pie.label-count-mismatch` reports `:labels:` disagreeing with the wedge
count: they pair by position, so a surplus or shortfall means at least one
wedge is named wrongly, and the author cannot see it from the labels alone. The
wedges are drawn either way — losing the data over a naming mistake would be
the larger failure.

## Consequences

- **The benchmark's unsupported-directive count drops from 23 occurrences to
  15, exactly the 8 `needpie` uses, with no new warnings of any kind.**
  `needpie` was the largest remaining construct; `button-link: 3` now is.
- **Decision 4 fixed two existing warnings on the way.**
  `entity-flow.invalid-filter` drops from 2 to 1, and the survivor is a
  *different* refusal — `type in ["swreq", …]`, a list literal — that the
  attribute-access error had been masking. That is the benefit of refusing by
  name compounding: fixing one refusal reveals the next honestly.
- **A chart is linear in the size of the project, once**, not once per wedge:
  the index is walked a single time and each entity offered to every filter.
  The same note ADR-012 and ADR-014 carry about hoisting per document applies,
  but less sharply — a chart builds no `Snapshot`.
- **A chart of written numbers alone never reads the index at all**, which is
  the one case a picture here is free.
- `:name:` registers as an ordinary `:ref:` target, so a chart is linkable like
  a table or a figure.
- `needbar` and `needsequence` remain unimplemented. `needbar` is now a
  presentation problem alone: it reuses this increment's counting module, and
  what it adds is a second dimension and an axis.
