# 21. Bar charts of the entity graph

## Status

Accepted.

## Context

ADR-020 closed with `needbar` as the last unimplemented sphinx-needs view over
the entity graph, and the entity benchmark's unsupported-directive summary
still counted it once. Its one corpus use, in
`automotive-adas/analysis.rst`, is the shape sphinx-needs' own examples use:

```rst
.. needbar:: Object authors
   :legend:
   :xlabels: FROM_DATA
   :ylabels: FROM_DATA
   :show_sum:
   :show_top_sum:
   :stacked:

   , Peter, Steven, Sarah, Thomas
   SW Reqs, <filter>, <filter>, <filter>, <filter>
   SW Arch, ...
   Tests, ...
```

ADR-017 had already decided the shape of the answer: a sibling node, not a
`:type:` switch on a generic chart, reusing the pie's *counting*. What it left
open was the presentation, and that is where the decisions below are. The
semantics were taken from sphinx-needs' `needbar.py` rather than from its
documentation:

- Each content line is a row, split on `:separator:` (default `,`); each cell
  is a number when it is all digits, and otherwise a filter whose match count
  is the value.
- `:xlabels: FROM_DATA` takes the first row as category labels (dropping its
  first cell when `:ylabels: FROM_DATA` too); `:ylabels: FROM_DATA` takes each
  row's first cell as that series' label. Explicit labels are comma lists.
- Rows are **series** — a legend entry, one colour — and columns are
  **categories** along the axis. `:transpose:` swaps them.
- Side by side, series `s` of category `c` sits at `s + c × (n + 1)`; stacked,
  every series of `c` sits at `c`. `:horizontal:` swaps the axes and reads the
  categories top to bottom.
- `:show_sum:` writes each segment's value in its middle; `:show_top_sum:`
  writes, at the bar's end, the *position* of that end — so a stack is topped
  with its total, since matplotlib's edge `bar_label` is what draws it.
- `:colors:` is **extended** by matplotlib's default cycle, not repeated —
  unlike `needpie`, which passes the written list alone.

## Decision

### 1. `.. entity-bar::` is the fourth presentation, with a grid for a body

`Directive::EntityBar` is a sibling of `EntityTable`, `EntityFlow` and
`EntityPie`: rows, a graph, proportions, or bars. `.. needbar::` is the
reserved alias, and one `entity-bar.*` family covers both names — ADR-011 §4's
arrangement, unchanged.

The body is settled **while parsing**. `FROM_DATA`, the explicit label lists
and `:transpose:` are all this document's own text, so the node carries a
`BarGrid` that is already labelled and already in drawing orientation, and the
renderer never sees how it was written. `BarGrid` is a smart-constructed type:
always rectangular, always one label slot per row and per column, and
re-validated on load.

Each cell is a `ChartValue` — the pie's former `SliceSource`, moved flat beside
`ChartColor` and renamed, since two node families now hold it. Each cell's
filter is read through the same `read_filter_text` a pie's content line goes
through, at the cell's own column, so a broken expression in the third cell of
a line is reported at that cell.

The layout options sphinx-needs spells as flags became types: `BarOrientation`
and `BarArrangement` name both sides of `:horizontal:` and `:stacked:`, and
`BarValueLabels` holds the two sums. Five bools on one node was clippy's
`struct_excessive_bools`, and the enums make the renderer's branches say which
layout they draw.

### 2. What sphinx-needs raises on is reported, and the chart still drawn

sphinx-needs raises an exception — failing the whole build — for rows of
different lengths and for label lists that disagree with the grid. Here:

- A short row is padded to the **widest** row with zero cells, and each row
  that was not the widest is reported as `entity-bar.ragged-row` on its own
  line. Widest rather than first, so a long row loses nothing either.
- A label list of the wrong length is `entity-bar.label-count-mismatch` on the
  option line; surplus labels are dropped and missing ones fall back to the
  ordinals sphinx-needs uses (`1`, `2`, …).

Losing the whole chart — or the whole build — over one mistyped row would be
the larger failure, which is the rule `entity-pie.label-count-mismatch` already
follows. A rotation that is not a whole number of degrees is
`entity-bar.invalid-rotation`: sphinx-needs silently ignores one, which leaves
the author no way to learn why their `-45` did nothing.

Cells are split naïvely, exactly as sphinx-needs splits them, so a filter
holding a comma needs `:separator:`. The label options are always
comma-separated, again as upstream.

### 3. Counting is shared; the walk is still one per chart

`renderer/blocks/entity_pie/series.rs`'s walk moved to
`renderer/blocks/chart_counts.rs` as `count_values`, over a slice of
`&ChartValue`. The pie's wedges and the bar chart's flattened grid both go
through it, so the index is walked once per chart, however many cells it has,
and `:filter:` scopes every counted cell identically in both.

### 4. `plotters` draws the shapes; the text is ours

`plotters`' SVG backend turns text only by quarter turns — `FontTransform` has
no arbitrary angle — while `:xlabels_rotation:`, `:ylabels_rotation:` and
`:sum_rotation:` take any whole number of degrees, and 45 is the value people
write. Snapping it to 90 would be a wrong picture drawn convincingly, so:

- `plotters` draws bars, the frame, tick marks and legend swatches;
- every piece of text is a `PlacedText`, collected while drawing and spliced
  into the finished SVG as a `<text>` with a `rotate()` transform
  (`renderer/src/chart/placed_text.rs`);
- `placement` decides, per angle and per side, which `text-anchor` and
  `dominant-baseline` make the turned text grow *away* from what it names —
  exact integer comparisons, so the cases where text runs across its side
  (0°, 180° below an axis) cannot be missed by rounding;
- margins are measured with the vendored font's own metrics, projected by the
  angle, so a turned label gets the room its turned box needs.

The element copies what `plotters` writes — the same `font-family`, and the
same `size / 1.24` — so a bar chart's 12 reads like a pie's 12.

This also means ADR-017 §3's expectation that `.. entity-bar::` would "inherit
axes, ticks and bar layout" from `plotters` did not survive: axes whose labels
must turn by any angle are not `plotters`' axes. What the bar chart inherits is
the backend, the vendored font and the palette. The value axis is counted in
whole steps of 1, 2 or 5 × 10ⁿ, since the values are entity counts and a tick
at 2.5 would name a number no chart can hold.

`pie_chart.rs` became the `chart/` tree — `pie.rs`, `bar/`, `style.rs`,
`placed_text.rs` — still the one place `plotters` is named. The move left the
pie's output byte-identical.

### 5. Nothing is compiled, as for the pie

ADR-017 §2 applies unchanged: the SVG is drawn inline by the render action, so
a bar chart writes no `.puml`, has no hash, creates no action, and needs no
`diagrams = True`. `examples/entities/charts.rst` holds the examples in the
plain library, and `bazel aquery` shows the same `PlantUMLCompile` actions as
before.

### 6. What is refused by name

`:style:` (a matplotlib stylesheet), and the legacy selection options
`:status:`, `:tags:`, `:types:` and ubCode's `:cypher:` — each
`entity-bar.unsupported-option`, with the `:filter:` to write instead. Refusing
the three legacy filters rather than translating them keeps `.. entity-bar::`
to the one selection language its three siblings accept.

## Consequences

- `needbar` joins every other sphinx-needs view as supported; the benchmark's
  unsupported-directive summary no longer names it.
- A chart of written numbers alone still never reads the index.
- Placement options, colours and the empty-result plumbing
  (`EmptyListingError::bar`) are shared with the pie rather than copied:
  `parser/directives/chart_options.rs` reads the shared options under each
  chart's own codes, and `renderer/blocks/chart_figure.rs` places either chart
  on the page.
- The one visible difference between the two charts' option handling is
  deliberate and inherited: a short `:colors:` list repeats on a pie and is
  continued by the palette on a bar chart, as in sphinx-needs.
