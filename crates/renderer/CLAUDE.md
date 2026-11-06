# Notes on `rinx_renderer`

- The renderer performs no I/O: it returns what must be written (e.g. `RenderOutput::diagram_sources`) and the worker writes it.
- Each third-party backend is contained in one module (`highlight.rs` for syntect, `math.rs`, `octicon.rs`, `chart/` for plotters) and must never reach a `.ast` file.
- Every index-resolved role splits into a resolution and its HTML writer, and `ReferenceResolver` (`reference_target.rs`) reuses the resolution. The language server's hover, go-to-definition and completion therefore never describe a link the page would not write. Keep new roles to that split.
- External (intersphinx) lookup: a local definition always wins, then Sphinx's order. Inventories are pruned to the targets documents write, which is exact only because an external lookup never scope-qualifies a name — keep it that way, or widen the pruning with it.
- The index stores the toctree *graph*, not a flattened navigation tree; the renderer expands it per directive (`docs/decisions/005-toctree-model.md`).
- `highlight.rs` returns one HTML string per line and rebalances `<span>`s per line, because `:linenos:` and `:emphasize-lines:` address single lines.
