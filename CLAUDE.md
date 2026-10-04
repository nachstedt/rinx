# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`rinx` is a Rust re-implementation of (a subset of) the Sphinx documentation generator, designed to be fast and to integrate natively with Bazel as a first-class, cache-friendly build step (not a wrapped external tool). It parses reStructuredText (`.rst`) into HTML documentation sites, with cross-file references, toctree-based navigation, and PlantUML diagram rendering.

Read `docs/dev/requirements.md` and `docs/dev/architecture.md` for the full design rationale — the paragraphs below only cover what differs from those aspirational docs, or what's needed to be productive immediately. The codebase is a Cargo workspace under `crates/` (`rinx_ast`, `rinx_scope`, `rinx_index`, `rinx_cdecl`, `rinx_toctree`, `rinx_entity`, `rinx_filter`, `rinx_inventory`, `rinx_uml`, `rinx_template`, `rinx_parser`, `rinx_analyzer`, `rinx_renderer`, `rinx_lsp`, `rinx`); `rinx_scope`, `rinx_index`, `rinx_cdecl`, `rinx_toctree`, `rinx_entity`, `rinx_filter`, `rinx_inventory`, `rinx_uml` and `rinx_template` aren't part of the architecture doc's crate split (that doc is aspirational and predates all nine), the other six match it.

## Commands

Development uses plain Cargo; the Bazel build is the production/integration path and wraps the same binary.

```bash
cargo build
cargo test --workspace            # unit tests (co-located #[cfg(test)] modules) + crates/worker/tests/integration/
cargo test <substring>           # run a single test by name substring
cargo test --test integration
cargo clippy --workspace --tests  # must be warning-free before finishing any change (pedantic lints are on, see workspace Cargo.toml)
cargo fmt
cargo run -p rinx -- diagnostic_codes_rst > docs/diagnostics.rst  # after adding a DiagnosticCode or editing its doc comment; a test compares
cargo llvm-cov --workspace        # line coverage; CI's `Rust / coverage` enforces floors for the workspace and rinx_lsp
fuzz/run.sh parse 60              # fuzz a target for N seconds (also `lsp_session`); fuzz/ is its own nightly crate outside the workspace, see fuzz/Cargo.toml

bazel build //:rinx    # build the CLI binary via Bazel
bazel build //examples:site           # build the example multi-team site end-to-end
bazel build //examples/team_a:docs    # build one team's library in isolation
bazel build //docs:site               # rinx's own documentation, built by rinx and deployed to GitHub Pages (.github/workflows/pages.yml)
bash tests/test_strict_deps.sh        # verifies Bazel fails the build when a toctree dep is missing from BUILD.bazel
bash tests/test_strict_links.sh       # verifies broken links only warn by default, and fail the build when strict_links = True
bash tests/test_diagram_cache_firewall.sh  # verifies an edit elsewhere does not restart the PlantUML JVM, and a diagram edit does
bash tests/test_diagram_opt_in.sh     # verifies a diagram in a library without `diagrams = True` fails the build
bash tests/test_download_data.sh      # verifies a `:download:` file must be declared in `downloads`, and lands under `_downloads/`
bash tests/test_intersphinx.sh        # verifies a site links into another through its objects.inv, and that `inventories` is load-bearing
bash tests/test_domain_indices.sh     # verifies `domain_indices` decides whether py-modindex.html is written and linked, and refuses unknown names
bazel build //examples/intersphinx:sibling_site  # a second site linking into //examples:site through its objects.inv
(cd e2e/smoke && bazel test //...)       # rinx consumed as a dependency by another module — what the BCR presubmit (.bcr/) runs
bazel run //scripts:benchmark         # clone CPython docs and benchmark the pipeline against it (see docs/benchmark.rst); CI adds --site-out/--summary-markdown
bazel run //scripts:benchmark_entities  # benchmark the entity model against useblocks' sphinx-needs demo (see docs/benchmark.rst)
bazel run //scripts:benchmark_warmup    # compile rinx into a disk cache both benchmarks share — CI's first benchmark step

# Python scripts (scripts/, examples/shared/) — see docs/dev/python.md
uv sync                    # install the pinned ruff, ty and pytest
uv run ruff format && uv run ruff check && uv run ty check
uv run pytest --cov        # the script tests, with the coverage floor
bazel test //scripts:all   # the same tests, one target per file, under the same pyproject.toml
bazel test //tests/js:all  # the default theme's browser scripts (assets/*.js), under rules_js' Node.js

# VS Code extension (editors/vscode) — npm, not Bazel: its tests download and drive a real VS Code
npm ci && npm run typecheck && npm run lint && npm run format:check   # oxlint, since TypeScript 7 has no API for typescript-eslint
npm test                   # @vscode/test-cli (.vscode-test.mjs); VSCODE_VERSION=min|stable|insiders, `-- --coverage`, `-- --label unit|e2e|e2e-no-server|e2e-vsix`
                           # e2e drives the real `rinx lsp`: RINX_BINARY, else the repository's target/debug/rinx (`cargo build` first)
npm run package && npm run package:check   # esbuild bundle → rinx.vsix, files compared with vsix-files.txt; RINX_VSIX=rinx.vsix npm test -- --label e2e-vsix tests it installed
uv export --only-group test --no-emit-project --format requirements-txt -o scripts/requirements.txt  # after changing the test group

CARGO_BAZEL_REPIN=1 bazel build //examples:site   # after changing a Cargo dependency
```

There are no `rust_test` Bazel targets — Rust tests are run through Cargo only. The theme's JavaScript is the reverse: its tests (`tests/js/`) run through Bazel only. The Python scripts' pytest files run both through `uv run pytest` and as one Bazel `py_test` per file (`//scripts:all`), each running pytest under the same `pyproject.toml`.

After any code change: run `cargo clippy --workspace --tests` (fix all warnings, don't `#[allow(...)]` them — treat them as refactor signals) and `cargo fmt`, then verify `bazel build //examples:site` still succeeds. After a Python change, likewise run `uv run ruff format`, `uv run ruff check`, `uv run ty check` and `uv run pytest --cov` (the same rule: fix findings, don't `# noqa` them). `doctest_runner.py`, `publish_crates.py` and `publish_pages.py` must stay stdlib-only, and the former Python 3.9-compatible, since it runs in users' `py_test`s.

Adding or removing a Cargo dependency additionally needs `CARGO_BAZEL_REPIN=1` on the next Bazel build, to refresh `Cargo.bazel.lock` — the crate_universe resolver lockfile, which is separate from `Cargo.lock` and is checked in. Bazel fails with a repin request rather than silently using stale crates. That lockfile is not optional: without it rinx cannot be consumed as a non-root bzlmod module at all, which is exactly the shape `scripts/benchmark.py` builds the CPython corpus in.

## Architecture

### Pipeline: parse → analyze/index → render

The binary (`crates/worker/src/main.rs`) is a multi-subcommand CLI, one subcommand per pipeline phase, because Bazel needs each phase as a separately cacheable action:

- `parse --input file.rst --output file.ast` — RST text to a serialized `ast::Document` (JSON).
- `validate_toctree --input file.ast --output file.ast [--allowed <doc>...]` — checks toctree entries only reference declared docs.
- PlantUML compile / `validate_assets` — for a document whose library set `diagrams = True`, the `render` action (below) also writes each diagram's expanded text as `<hash>.puml` into `--diagram-outdir`; a per-document compile action turns those into SVGs via the `plantuml` jar, and `validate_assets` checks every `.puml` got its `.svg` (as well as every authored image and `:download:` file reaching its bundle). All of it is **after** indexing and at the *site* level, because a templated diagram's text is a question asked of the entity graph — see "Diagrams" below.
- `index --inputs a.ast b.ast ... --output project.index` — merges all documents' local analysis (`analyzer::analyze`) into one global `rinx_index::ProjectIndex` (targets, document titles, nav tree, glossary terms).
- `render --input file.ast --index project.index --doc-path rel/path --output file.html --config site.toml --template layout.html` — turns one AST + the global index into a final HTML page.
- `preview` — collapses parse+local-analyze+merge+render into one process reading RST from stdin, for low-latency editor use (see "Live preview" below).
- `lsp` — the language server over stdio, implemented by the `rinx_lsp` crate (see "Live preview / LSP direction" below).
- Legacy: `rinx <file.rst>` prints HTML straight to stdout (used by `process_rst()` in `crates/worker/src/lib.rs`, mostly for quick manual checks).

Each subcommand handler is split into a pure `process_*` function (testable without file I/O) and a thin `cmd_*` wrapper that does the file reads/writes, the two living together in their own file under `commands/` (e.g. `commands/render.rs` holds both `process_render` and `cmd_render`), with `main.rs` itself reduced to `mod commands;` and the `run()` dispatcher — keep new subcommands following that split.

### Crate layout (`crates/`)

Each crate is a workspace member with its own `Cargo.toml` and `BUILD.bazel`; dependencies between them flow strictly `ast → scope`/`index` → `analyzer`/`renderer` → `worker`, with `parser` depending only on `ast`, parallel to `scope` and `index`. `cdecl`, `filter` and `inventory` are leaves depending on nothing of ours, with `ast → filter` because a parsed filter expression is stored in the `.ast`, and `ast → inventory` because an `:external+name:` role's inventory name is.

Within a crate, a file is split once it grows past ~800 lines, along construct/responsibility boundaries. Prefer expressing the relationship via a same-named subdirectory (`inline.rs` splitting into `inline/reference.rs`, `inline/hyperlink.rs`, ...) over flat filename-prefixed siblings (`inline_reference.rs`) — `mod x;` in the parent file resolves to `parent_dir/x.rs` by default, so this needs no `#[path]` attribute; the one place this isn't possible is a crate root (`lib.rs`/`main.rs`), whose direct children are necessarily flat siblings of `lib.rs`/`main.rs` itself since that file's own directory *is* `src/`. When an implementation is genuinely one cohesive unit and its bulk is in `#[cfg(test)]` content (e.g. `ast/domain_object_body.rs`'s single enum+impl, `cdecl/parser.rs`'s one recursive-descent `Parser`), split only the tests into topic-based sibling files instead of fragmenting the implementation.

Once a file owns a same-named directory it becomes a **pure forwarder**: a module doc comment explaining how the submodules fit together, the `mod` declarations, and the re-exports its parent needs — no logic, no tests. Every `X.rs`/`X/` pair in the workspace follows this — see `parser/blocks.rs`, `parser/directives.rs`, `parser/inline.rs`, `renderer/blocks.rs`, `renderer/page.rs`, `renderer/resolution.rs`, `ast/object_type.rs`, `worker/commands.rs`. The exception is the tests-only directory described above, whose owner keeps its implementation but must then hold *no* inline `#[cfg(test)] mod tests` either: all of its tests live in the topic-named siblings. Two consequences worth knowing before doing one: an item re-exported by the forwarder needs a visibility at least as wide as the re-export, so `pub(crate)` rather than `pub(super)`; and a helper used only by a directory's own children can stay a *private* `fn` in the parent, since Rust makes a module's private items visible to its descendants (e.g. `renderer/blocks/entity.rs`'s `effective_attribute`, reached from `blocks/entity/template.rs`).

Two rules keep these moves honest. Place a module under whichever dispatcher actually calls it — grep the callers rather than trusting the name; that is what moved `domains` under `directives`, `bullet_list` under `blocks`, and `renderer`'s `nav.rs` under its `blocks/` (only the node dispatcher calls it — `page/` computes its hrefs separately). And when a file named for a construct also holds general-purpose helpers, split those out under a name saying what they do rather than dragging them along: `parser/indent.rs` exists because `directive_body.rs` and `bullet_list.rs` were each hosting indentation utilities that twenty-odd files called, and `ast/object_naming.rs` because `domain_object_body.rs` was hosting the signature/option name extraction that three other crates call.

- `rinx_ast` (`crates/ast/src/lib.rs`) — AST node types, one per module, with seven families grouped into their own trees: `entity_flow/` (the flowchart's node, its two spellings and the two layout directions `PlantUML` has), `entity_pie/` (the pie chart's node, its two spellings and the wedge), `entity_bar/` (the bar chart's node, its two spellings, the always-rectangular labelled `BarGrid` and the `BarOrientation`/`BarArrangement` enums `:horizontal:`/`:stacked:` parse into), `object_type/` (`combined.rs`'s umbrella `ObjectType` over `py.rs`/`c.rs`/`std_.rs`), `doctest/`, `enumerator/` and `table/` (whose `TableSource` records whether a `Directive::DataTable` came from `list-table` or `csv-table` — the two are structurally identical after parsing, so they share one variant). `object_naming.rs` sits flat beside them because every later phase (parser, analyzer, renderer) calls its signature/option naming helpers. Note `HashedContent`, `TargetName`, `ChartColor` and `LabelRotation` (both flat, being values a chart is drawn with rather than nodes — as is `ChartValue`, the filter-or-number a pie's wedge and a bar chart's cell both hold): all are "parse, don't validate" opaque types (smart constructors + custom `Deserialize` that re-validates the invariant on load, e.g. `HashedContent`'s stored hash must match `sha256(body)`). Follow this pattern for new invariants rather than validating ad hoc at call sites. `asset_uri.rs`'s `AssetUri` is the external-URL/project-file split and the one `resolve` both an image's argument and a `:download:` target go through, so the bundler, validator and renderer agree on a path. `code_language.rs` holds the `CodeLanguage`/`ResolvedLanguage` pair a code block's language is parsed into: the second is the first minus its `Inherit` case, so a renderer that failed to apply `.. highlight::` inheritance cannot compile — see `docs/decisions/006-syntax-highlighting.md`. `directive.rs`'s `Unknown`/`Malformed` pair is the one place a directive that could not become content lands — see the extension-directives section below for the split. `span.rs`, `diagnostic.rs`, `diagnostic_code.rs` and `suppression.rs` are the *reporting* vocabulary rather than document content — they live here because `Document` carries them and every later phase both produces and forwards them; `DiagnosticCode`'s enum, `as_str`, `FromStr`, `ALL` and `description` (each variant's doc comment, which a test requires) are all generated by one macro from a single table, because a `.. noqa:` needs the two directions to be exact inverses (see `docs/decisions/003-diagnostics.md`). `suppression.rs` also holds the one `.. noqa:` filter, `retain_reportable`, over anything implementing its `Reported` trait (`Diagnostic` here, the renderer's typed findings in `renderer/src/reported.rs`): both front ends — the worker and the language server — call it, so the editor and CI cannot disagree about what a comment silences.
- `rinx_scope` (`crates/scope/src/python.rs`) — `PythonScope`, the enclosing `py:class`/`py:exception`/`py:module` scope tracked while indexing and rendering domain objects, shared by `analyzer` and `renderer` so a definition's index key and a reference's resolution always agree. Kept as its own crate (rather than folded into `ast`, where it briefly lived) because it's traversal state, not parsed-document data — nothing in it is ever serialized to a `.ast` file.
- `rinx_index` (`crates/index/src/`) — `ProjectIndex` and the data types it's built from (`GenIndexEntry`, `DocumentOutline`/`OutlineSection`, `DocumentToctree`, `DocumentNumbers`), plus `ProjectIndex::merge`/`insert_domain_object`. Pure data + `Serialize`/`Deserialize`, no traversal logic — kept separate from `rinx_analyzer` (which builds a `ProjectIndex`) because `rinx_renderer` only ever reads one; before this split, renderer depended on the whole analyzer crate just to get these types. Note there is **no** pre-flattened navigation tree: the index stores the toctree *graph* (`toctrees`, per document and mergeable) plus each document's `document_outlines`, and the renderer expands them per directive — see `docs/decisions/005-toctree-model.md` for why, and for why that is what makes the live-preview path correct.
- `rinx_toctree` (`crates/toctree/src/`) — resolving a `.. toctree::`'s written entries (`expand_toctree`, `resolve_docname`, the `:glob:` matcher). Its own crate because three phases that may not depend on each other all need it: the analyzer, the renderer, and the worker's Bazel strict-deps validator. Each expands against a *different* universe of documents, which is why a glob survives unexpanded into the index rather than being resolved once — expanding early would leave the strict-deps check unable to check globs.
- `rinx_inventory` (`crates/inventory/src/`) — Sphinx's `objects.inv` format, read (`read_inventory`) and written (`write_inventory`), and nothing else: no index, no resolution. A leaf crate like `rinx_cdecl`, because the worker writes one, the index holds one and the AST names one (`InventoryName`, re-exported by `rinx_ast` for `InventorySelector`). `testdata/sphinx-9.1.0.inv` is a real `sphinx-build` output and the compatibility bar for both directions.
- `rinx_cdecl` (`crates/cdecl/src/`) — a small C declaration parser (`tokenize` → recursive-descent `Parser` → `Declaration`/`Declarator`), used by `parser`'s `c`-domain directives to find the declared name in a `c:function`/`c:type` signature. `parser.rs` is one cohesive recursive-descent unit, so only its tests are split into siblings (`parser/declaration_tests.rs`, `parser/function_tests.rs`).
- `rinx_template` (`crates/template/src/`) — rendering a document's source as a Jinja template before it is parsed (`render_source`), the transform a Sphinx project gets from a `source-read` hook in its `conf.py`. A leaf crate like `rinx_cdecl` and `rinx_filter`, learning what it cannot do itself through the injected `TemplateLoader` the parser implements over its own file loader. The part that is not "call MiniJinja" is `marker.rs`: MiniJinja has no source map and an `{% include %}` shifts every line below it, so a `(template, line)` marker is injected into each line before rendering and read off afterwards, yielding one `SourceLine` per rendered line — without it every position, `.. noqa:` and stored `Span` below the first include would be wrong. `scan.rs` is the delimiter scanner deciding where a marker may go and collecting the template names to load; it also refuses whitespace-control modifiers and computed names by name. See `docs/decisions/013-source-templating.md`.
- `rinx_parser` (`crates/parser/src/`) — three trees, one per parsing phase, each a forwarder over one module per construct: `blocks/` (block-level constructs — bullet/enumerated/definition lists, grid and simple tables, comments, transitions, literal and doctest blocks), `directives/` (everything behind a `.. name::` marker, with `directives/domains/{py,c,std_}/` for the domain objects and `directives/data_table/` for the two table directives that share an AST node; `directives/dropdown.rs`, `directives/grid.rs`, `directives/entity_table.rs`, `directives/entity_flow.rs`, `directives/entity_sequence.rs`, `directives/entity_pie.rs`, `directives/entity_bar.rs` and `directives/if_builder.rs` are the directives here that are neither docutils' nor Sphinx's — see "Extension directives" below; five of them share `directives/filter_option.rs`, the one reader of a filter: `read_filter_option` for a `:filter:` line and `read_filter_text` for the body lines a pie chart's wedges and a bar chart's cells are written on, so an option and a content cell report at the same column; the two charts also share `directives/chart_options.rs`, which reads the colour and placement options under each chart's own codes), and `inline/` (inline markup, with `inline/roles/{py,c,std_}/` for the cross-reference roles, mirroring `domains/`'s shape). `directives/code_block/` is the forwarder over the four directives that produce a code block (`block.rs` for `.. code-block::`/`.. code::`, `literal_include.rs`, `highlight.rs`) plus the vocabulary they share (`options.rs`, `dedent.rs`, `emphasize.rs`, `selection.rs` — which `directives/include.rs` also uses — and `diff.rs` for `:diff:`). Only `headings.rs`, `indent.rs`, `context.rs` and `diagnostics.rs` sit flat beside them, being reached from all three. `templating.rs` sits flat too, but is reached from neither: it runs *before* them, turning `rinx_template`'s per-line origins into the `TemplateMap` that `ParseCtx::position` resolves every position through. `parser::parse()` is the entry point, `parse_with_domain()` and `parse_with_ctx()` its configurable forms — the latter taking the `ParseCtx` (`context.rs`) that every block-level parser threads, carrying the default domain, the injected `ParseFileLoader` that `.. csv-table::`'s `:file:`, `.. include::` and `.. literalinclude::` read through (the crate itself performs no I/O, so the worker supplies the filesystem implementation), and the **`origin`** that turns a slice-relative line/column back into a document position. That last one is why `ParseCtx` is threaded at all rather than just consulted: **any code that dedents or re-slices lines before calling `parse_blocks` must rebase it with `ctx.nested(line_offset, column_offset)`**, or every position inside that block is wrong by the amount trimmed — which is why `collect_directive_body` and `normalize_cell_lines` return how much they trimmed instead of discarding it, and why the list, glossary, table-cell and directive-body parsers each rebase. Content with no source line at all (a `.. csv-table::`'s generated rows) parses under `ctx.synthetic()` and reports positionless diagnostics. `ParseCtx` also borrows the document's `DocumentRoles` table (`document_roles.rs`), its one piece of mutable state: `.. role:: name(code)` and `.. default-role::` fill it in and the inline scan of every later paragraph reads it, which is what makes a role, and a default role, apply from where it is written onwards without a whole-document pass — see `docs/decisions/029-code-role.md` and `docs/decisions/037-default-role.md`. `default_role.rs` sits flat beside it: `DefaultRole`, the role a bare `` `text` `` is read as, validated by asking the inline role table (`inline/interpreted.rs`'s `find_role_kind`) rather than a list of its own, and configured per library by `rinx_library`'s `default_role` attribute (`--default-role`). `diagnostics.rs`'s `Diagnostics` is the collector threaded alongside it, holding the diagnostics found, the `.. noqa:` suppressions resolved to line ranges, and the table of files an `.. include::` spliced text in from — the three travel together because a span's `FileId` indexes the third and a suppression matches against it. The parser is meant to be error-resilient (bad input becomes an error/unknown node, not a panic/abort) to support live preview over incomplete documents.
- `rinx_analyzer` (`crates/analyzer/src/lib.rs`) — builds the per-document and project-wide index: cross-reference targets, document titles, section outlines, the toctree graph, glossary terms, split across `document_index.rs`/`domain_object_index.rs`/`outline.rs`/`project_index.rs`. `build_project_index` is a sequence of *named phases* (`merge_document_analyses` → `find_root_documents` → `assign_section_numbering` → `collect_page_order` → `collect_nav_diagnostics`) because they have a real order dependency: roots cannot be chosen until every toctree is known, and numbering and page order cannot start until the roots are. The first two are cohesive units whose bulk is tests, so each keeps its implementation whole and splits only its `#[cfg(test)]` content into topic-named siblings (`document_index/targets_and_titles_tests.rs`, …). `rinx_index::ProjectIndex::merge` is what lets the `preview` subcommand and the VS Code extension combine a fresh local analysis with a stale global index. Its rule is a union of claimants per definition, so the result never depends on merge order: a name two documents define is moved to `ambiguous_definitions` and defines nothing, and the `duplicate_definitions` phase warns every claimant — see `docs/decisions/039-duplicate-definitions.md`.
- `rinx_uml` (`crates/uml/src/`) — turning a diagram into the `PlantUML` text that gets compiled: `expand` for a written template, `build_flow` for a generated `.. entity-flow::` flowchart and `build_sequence` for a walked `.. entity-sequence::`, all finishing through `assemble.rs` so there is one population of diagram hashes. The renderer is its only caller; it is a crate of its own because it evaluates templates against an owned snapshot of the entity graph with none of the HTML vocabulary around it — an earlier second caller, a separate expansion action, was removed as duplicate work. `snapshot.rs` owns the awkward part: MiniJinja functions must be `Send + Sync + 'static`, so the entity graph is materialized into an owned snapshot before any closure sees it, with every field's value coming from `rinx_index::EntitySubject` so a diagram's `filter()` and a table's `:filter:` cannot disagree. `template.rs` builds the environment; its `uml()`/`imports()` recurse *through* MiniJinja, which is why the import chain and the first-failure slot are shared `Arc`s rather than stack locals.
- `rinx_renderer` (`crates/renderer/src/`) — AST + `rinx_index::ProjectIndex` to HTML, in four trees mirroring `parser`'s shape. Its diagnostics carry the `Span` the offending `InlineNode` was parsed with; `inline/ref_text.rs`'s `RefText` bundles the display/target/span triple every index-resolved role passes to its renderer. `lib.rs` holds `RenderCtx`, `RenderOutput` and the `render()`/`render_with_config()` entry points (with the diagnostics it reports in `broken_link.rs`); `highlight.rs` sits flat at the root as the one place `syntect` is called — the same containment `math.rs` gives `math-core`, `octicon.rs` gives `octicons-pack` and the `chart/` tree gives `plotters` (`pie.rs`, `bar/`, the shared `style.rs`, and `placed_text.rs`, which writes chart text itself because `plotters` can only turn text by quarter turns), and for the same reason: the backend must never reach a `.ast` file. It returns **one HTML string per line**, because `:linenos:` and `:emphasize-lines:` need to address a line on its own, and it rebalances each line's `<span>` tags so a scope spanning several lines cannot nest illegally around that per-line markup; `blocks/` renders body content, its `dispatch.rs` walking the node tree and delegating to one module per construct (`tables.rs`, `data_table.rs` for `list-table`/`csv-table`, `admonitions.rs`, `doctest.rs`, `glossary.rs`, `scope_directives.rs`, `nav.rs` for local toctrees, `entity_flow.rs` for the generated flowchart, `entity_sequence.rs` for the walked sequence diagram, `entity_pie/` and `entity_bar/` for the two charts — both counting through the flat `chart_counts.rs` and placed by the flat `chart_figure.rs` — and `domain_object/` for the definition directives; `diagram_figure.rs` is the picture markup every *compiled* diagram shares, which a chart deliberately does not use since it has no `<img>`); `inline/` mirrors that for inline markup, one file per role behind its own `dispatch.rs`; `page/` wraps a rendered body in the MiniJinja-templated chrome (`layout.rs`, `nav_hrefs.rs`) and renders the general index (`genindex.rs`). `resolution/` (domain-object, `:option:`, document and `:any:` cross-reference lookup — the last asking the other three rather than repeating their searches, see `docs/decisions/025-any-role.md`; the document one is shared with `:doc:`, see `docs/decisions/026-doc-role.md`) and `nav/` (toctree expansion, entry resolution, section numbers, prev/next, and `collapse.rs`, which trims the sidebar to the current page's branch per `rinx.toml`'s `collapse_navigation` — a post-pass, so the walk stays shared with in-page toctrees; see `docs/decisions/031-collapsed-navigation.md`) stay flat at the root because both `blocks/` and `page/` reach them; `asset_href.rs` likewise, the one `_images/`/`_downloads/` href computation shared by `blocks/`'s pictures and `inline/`'s `:download:`. `config.rs` holds `SiteConfig`, deserialized from `rinx.toml` — deliberately only metadata (project name/version), never file paths, see "Config vs CLI flags" below.
- `rinx_lsp` (`crates/lsp/src/`) — the language server `rinx lsp` serves, on rust-analyzer's synchronous `lsp-server`/`lsp-types` (ADR-038). `server.rs` follows the `process_*`/`cmd_*` split: pure handlers (`handle_notification`, `handle_request`, `initialize_result`) from state plus one message to the messages to send, and the thin `run` loop over a `Connection` — so its tests drive the handlers directly, plus whole sessions over `Connection::memory()`; the real `rinx lsp` process — stdio, the command line and the exit code the protocol asks for (1 unless shut down) — is tested by the worker's `tests/integration/lsp.rs`, and `tests/integration/lsp_parity.rs` checks over every document under `examples/` and `docs/` (plus its own fixtures in `crates/worker/tests/parity/`, which hold the mistakes a published page must not) that the server reports what `rinx parse` warns about, file by file (ADR-038's promise), relaxed by name only for the gaps a roadmap step still has to close. `position.rs` is the one place rinx's 1-based character columns become the protocol's 0-based UTF-16 (or negotiated UTF-32) ones; `diagnostics.rs` parses an open document and converts what the parser reported — dropping what a `.. noqa:` silences through `rinx_ast::retain_reportable`, and linking each code to its section of the generated `docs/diagnostics.rst` (`codeDescription`, anchored by `rinx_ast::section_slug`); `files.rs` is its overlay `ParseFileLoader` — an open buffer first, the disk second — which also records every file a parse read; `uri.rs` converts between `file:` URIs and paths. One parse can report on several files, since a diagnostic inside an `.. include::`d fragment carries its `FileId`, so `diagnostics.rs` returns them by URI and `includes.rs`'s `IncludeGraph` decides what each file shows: what the open documents including it found there, or else its own parse. An edit re-diagnoses the open documents that read the edited file. `include_summary.rs` adds, on each `.. include::` that brought problems in, an editor-only *Information* diagnostic linking to them — found through `rinx_ast::IncludeSite`, which the parser records per include (by file, not by diagnostic index, because substitutions and refused roles are reported by post-passes) and which is `#[serde(skip)]` so the `.ast` is unchanged. The worker depends on it, never the reverse — the worker's `commands/` internals (`ParseInputs`, ...) are therefore out of its reach until lifted into a crate both share, as the roadmap's later steps do.
- `rinx` (`crates/worker/src/`) — `main.rs` is the CLI entry point, dispatching into `commands/`, which holds one file per subcommand, each pairing a pure `process_*` with its file-I/O `cmd_*` wrapper per the split described above; `commands/cli_args.rs` holds the shared flag-parsing helpers, `commands/parse_files.rs` the filesystem loader every parse-time file read goes through, `commands/diagnostics.rs` the warning formatting shared by `parse`, `render` and `preview` (one shape for all of them: `warning: path:line:column: code: message`), with `rinx_ast::retain_reportable` applying `.. noqa:` immediately before it — the filtering runs *inside* `process_render`/`process_preview` rather than in their callers, so a suppressed link is invisible to the warning, the `--strict-links` failure and the `--warnings-output` sidecar alike. `lib.rs` holds `process_rst()`, `doctest_plan.rs` (see "Doctests" below), `domain_warnings.rs`, and `validator.rs` (toctree/allowed-docs validation used by the `validate_toctree` subcommand).

### Bazel rule pair: library vs. site (`rules/library.bzl`, `rules/site.bzl`, `defs.bzl`)

Mirrors `cc_library`/`cc_binary`: `rinx_library` runs Phase 1 (parse, toctree-validate, extract doctests, embed assets) per `.rst` file and exposes a `RinxInfo` provider carrying `.ast` files — plus `diagram_ast_files`, the subset whose library set `diagrams = True`. `rinx_site` collects everything transitively from `deps`, runs the single Phase 2 index action, then one render action per `.ast` file (which, for a `diagram_ast_files` document, also writes its `.puml` sources, followed by a compile action), bundles images, and copies CSS. Diagrams are the site's job rather than the library's — see "Diagrams" below.

Nine independent dependency mechanisms, don't confuse them:
- **`deps` on `rinx_library`** — strict, DAG-enforced, required *only* for docs pulled in via `.. toctree::`. This is what `validate_toctree` checks against (`--allowed`).
- **Cross-references / hyperlinks in text** — not declared in `deps` at all; stored as symbolic markers in the Phase-1 AST and resolved later, globally, during the Phase 2 index step. This is why cyclic hyperlinks between docs are fine but cyclic toctrees are not.
- **`parse_data` on `rinx_library`** — files, not documents: everything the *parser* reads *at parse time*. That is the `.csv` behind a `.. csv-table:: :file:`, the templates a Jinja `{% include %}` reads (see `jinja` below), and the sources `.. include::` and `.. literalinclude::` splice into a document. They join the parse action's inputs, so a path resolves inside the sandbox; an undeclared file is simply absent and the parse fails. Nothing about this is a dependency on another library. Two traps worth knowing: an `.rst` reached by `.. include::` must **not** also appear in `srcs`, or it is published as a page of its own as well as being spliced in; and a `.. toctree::` written *inside* an included fragment still needs its documents in `deps`, because `validate_toctree` runs on the already-merged AST. Unlike `images` below there is no cache firewall here — an included file's text really is part of the including document, so editing it re-parses and re-renders every page that includes it.
- **`images` on `rinx_library`** — the pictures a `.. image::`/`.. figure::` shows. Unlike `parse_data` these are *not* parse-action inputs: parsing succeeds whether or not the file exists, because nothing reads it then. They ride the provider to the site rule, which copies them into `_images/` keeping their source-root-relative path (not Sphinx's flattened basename — see `docs/decisions/007-image-assets.md`), and an undeclared one fails the site's `validate_assets` action. A `:loading: embed` image is additionally read by the per-document `embed_assets` action described below.
- **`downloads` on `rinx_library`** — the files a `:download:` role links. Like `images` minus the embedding: read by *no* action but the site's `RinxBundleDownloads` copy into `_downloads/` (source-root-relative path again), so editing one re-renders no page — the page's `href` follows from the path as written. An undeclared one fails `validate_assets` as `download.undeclared`; that action runs for every site, since a document can name a file in a site that bundled none. It finds the roles in the `.ast` JSON, because `walk_nodes` does not descend into inline content. See `docs/decisions/027-download-role.md`.

- **`diagrams` on `rinx_library`** — whether its documents may hold
  PlantUML diagrams. **Off by default**, and that is the point: Bazel cannot
  know before reading a document whether it draws anything, so without an
  opt-in every document of every project would pay for the diagram pipeline.
  A library that leaves it off gets no diagram actions at all; a diagram
  written in one fails its parse as `uml.diagrams-disabled`, naming the
  attribute. Keep diagram documents in a library of their own to keep the
  cost on them — `examples/entities/BUILD.bazel`'s `diagram_docs` does, the
  same way `doctest_docs` isolates doctests.

- **`jinja` / `jinja_context` on `rinx_library`** — whether its `.rst`
  sources are rendered as Jinja templates before they are parsed, which is what
  a Sphinx project gets by connecting the `source-read` event in its
  `conf.py` — see `docs/decisions/013-source-templating.md`. **Off by default**,
  unlike the extension directives: a directive name is a construct nobody
  writes by accident, but `{{` and `{%` are ordinary characters and a page
  *about* templating would otherwise become a syntax error. The templates an
  `{% include %}` names are ordinary `parse_data`, with the same `srcs` trap
  `.. include::` has — but note a template name resolves against the **source
  root**, as Jinja's own loader does, not against the document that includes
  it. `jinja_context` is the `html_context` such a project passes to the
  render; a name bound by neither it nor a `{% set %}` is reported rather than
  emptied.

- **`entity_schema` on both rules** — the project's construct vocabulary, not
  content. Read at *parse* time (it is what makes `.. req::` a directive), so it
  is declared on `rinx_library`, and again on `rinx_site` whose
  index action derives back-links from it and whose render actions read its
  labels. Both must name the same file; a mismatch is `entity.schema-mismatch`.
  Unlike `parse_data` there is no cache firewall and no per-directive scope:
  editing it re-parses every document in the library. Note this is a *config
  file the parser reads* — there is nothing odd about that, and its
  `[import_keys]` table is a second thing the parser takes from it. What a
  config file cannot do is declare a build input, which is why a keyed
  `needs.json` still needs its `parse_data` entry.

- **`inventories` on `rinx_site`** — other documentation sites this
  one links into (intersphinx), each a `rinx_inventory` pairing a
  pinned `objects.inv` with its `base_url`. Not documents and not parse
  input: only the site's index action reads them, and a reference no document
  defines resolves into them while rendering. Nothing is fetched — `src` is a
  vendored file, an `http_file` with `sha256`, or another site's `inventory`
  output group. See "Inventories" below.

(One more attribute, `py_deps` on `rinx_doctest_tests`, is *not* a mechanism of rinx's own — it is rules_python's ordinary `py_test.deps`, surfaced so documented code is importable while doctests run. It has nothing to do with any of the above.)

See `examples/BUILD.bazel` + `examples/team_a`, `examples/team_b` for a concrete two-team library/site setup, and `tests/test_strict_deps.sh` for how the strict-toctree-dep enforcement is tested (it mutates `examples/BUILD.bazel` temporarily to prove a missing dep fails the build). `tests/test_parse_data.sh` does the same for `parse_data` (checking all three of its readers: `csv-table :file:`, `include` and `literalinclude`), and `tests/test_image_data.sh` for `images` (`tests/test_download_data.sh` for `downloads`); the first two have to edit the referencing `.rst` as well, since Bazel does not invalidate an action when an input is merely *removed*, so the affected action must be forced to re-run before the missing file is observed. Note `examples/BUILD.bazel` declares *two* libraries in one package: `root_docs` and the `doctest_docs` it depends on — see the doctest section below for why that split exists.

### Doctests: rendered by the build, executed by tests (`rules/doctest.bzl`, see `docs/decisions/002-doctest-execution.md`)

The `sphinx.ext.doctest` family (`doctest`, `testcode`, `testoutput`, `testsetup`, `testcleanup`) is split across Bazel's build/test line, and the split is the design:

- **Rendering** happens in the normal pipeline. `bazel build //examples:site` produces HTML for these blocks and never starts a Python interpreter.
- **Execution** is `rinx_doctest_tests`, a macro emitting one stock `py_test` per `rinx_library`. It is opt-in and enforced by `tests/test_doctest_isolation.sh`.

Between them sits the **cache firewall**. `rinx_library` declares an `extract_doctests` action per document (`crates/worker/src/doctest_plan.rs`) producing a `.doctests.json` that keeps only what changes how the code runs — never `:hide:`, the trim tri-state, or line numbers. A prose edit re-runs that cheap AST walk but leaves its bytes identical, so `bazel test` reports `(cached)` and no interpreter starts. `tests/test_doctest_cache_firewall.sh` asserts both directions.

Two details worth knowing before touching this:

- The plans live in a **non-default output group** (`doctest_plans`), so building a site never produces them. The macro consumes that group through a `filegroup` — this is the whole interface between the two files, and the reason neither needs the document list.
- Granularity is the library, not the document: a code edit in one document re-runs its library's other documents, and they all share one interpreter. **Split the library** to narrow that — `examples/BUILD.bazel` keeps `doctests.rst` in its own `doctest_docs` library for exactly this reason.

`scripts/doctest_runner.py` drives CPython's stdlib `doctest` rather than reimplementing its comparison semantics. Note the one non-obvious mechanism: `doctest` hard-codes `compile(..., "single", ...)`, which rejects the multi-statement code a `testcode` block normally holds, so the runner replaces the `compile` that `doctest`'s module globals resolve — the same workaround Sphinx uses.

### Embedded image assets (`docs/decisions/007-image-assets.md`)

`:loading: embed` inlines a picture's bytes into the page as a `data:` URI, and where that reading happens is the design. It cannot be the renderer (which performs no I/O), and it must not be the parser (whose `.ast` is a cache unit and a live-preview payload, so base64 must never enter it). So `rinx_library` runs a per-document `embed_assets` action — `crates/worker/src/commands/embed_assets.rs` — producing a `<doc>.embeds.json` of `resolved path -> data: URI`, which the render action takes via `--embeds`.

That is the same **cache firewall** shape as the doctest plans: every declared image is an input to the embed action (Bazel cannot know at analysis time which one a document embeds), so an image edit re-runs that cheap AST walk everywhere, but its bytes only differ for a document that actually embeds the changed file — and only that page re-renders. `tests/test_image_data.sh` and the ADR cover the rest.

Keys are the path from `ast::AssetUri::resolve` (shared with `:download:`), which the embedder, the `validate_assets` check and the renderer all call, so no two phases can disagree about where `../shared/logo.png` points.

### Importing entities (`docs/decisions/016-needimport.md`)

`.. needimport::` reads a sphinx-needs `needs.json` and splices the needs in it
into the document as ordinary `Directive::Entity` nodes — the third splicing
directive, and the only one whose nodes are not reStructuredText the author
wrote. Four things to know:

- **It is a bridge, so it takes sphinx-needs' name *alone*.** Every other
  borrowed construct here gets a local name plus the foreign spelling as an
  alias; this one deliberately does not, so `entity-import` stays free for the
  richer construct that would read this project's own format and carry named
  sections. The diagnostic family is `needimport.*` for the same reason: here
  the spelling is the construct.
- **Importing at parse time is what makes every later phase free.** The
  analyzer, index and renderer were not touched: an imported need is indexed,
  is a `:ref:` target, contributes derived back-links and is listed by
  `.. entity-table::`/`.. entity-flow::` because it is an ordinary entity of
  the importing document. Merging an external graph at index time was rejected
  — an entity no document holds has no anchor and nowhere to render.
- **The schema is the authority over the file.** `needs_json.rs`'s
  `INTERNAL_FIELDS` lists sphinx-needs' ~34 bookkeeping keys, ignored by
  design; any *other* undeclared field is reported rather than dropped. Values
  go through the same `parse_attribute_value` funnel a written option does, via
  `directives/entity_fields.rs` — which exists so a written `.. req::` and an
  imported one cannot validate differently.
- **The file is ordinary `parse_data`, and nothing is fetched.** A URL argument
  is refused by name. JSON has no line numbers, so every diagnostic lands on
  the `.. needimport::` line and imported prose parses under `ctx.synthetic()`,
  reporting positionless — as a `.. csv-table::`'s `:file:` rows already do.
- **An argument may be a *name*, resolved through the schema's
  `[import_keys]`** — sphinx-needs' `needs_import_keys`. It lives in
  `entities.toml` rather than in the build file because the alias is written in
  a document and must travel with it; a value resolves relative to the schema
  file, or to the source root with a leading `/`. The worker resolves it once
  (`commands/entity_schema.rs`) and `ParseCtx::import_keys` carries the result.
  It is deliberately outside the schema fingerprint: parse-only, so no
  `entity.schema-mismatch`.
- **An export's unset fields are silent.** A `needs.json` writes every
  registered option for every need, so an undeclared field with an empty value,
  an explicit `null`, and a back-link this build *derives* are all skipped; a
  field carrying a real value is still reported. Measured against the benchmark
  corpus, where the alternative was 19 spurious names per need.

### Inventories (`docs/decisions/023-inventories.md`, `docs/intersphinx.rst`)

Every site writes an `objects.inv` (`commands/inventory.rs`, a projection of
`ProjectIndex` using the renderer's own anchor functions), and links into
other sites' through `inventories`. Three things to know before touching it:

- **The index action reads inventories and the index holds them**, as
  `ProjectIndex::external_inventories`, in declaration order and never merged
  — so the preview finds them in its stale index. They are **pruned** to the
  targets some document writes (`ExternalInventory::retain_referenced`, fed
  from each AST's JSON by `written_reference_targets`): stored whole, Python's
  inventory cost every render ~18 ms. Pruning is exact only because an
  external lookup never scope-qualifies a name — keep it that way, or widen
  the pruning with it.
- **Local always wins, then Sphinx's order** (`renderer/src/resolution/
  external.rs`): the target as written in every inventory, then a `name:`
  prefix. `:external:`/`:external+name:` are markup, stripped by the parser's
  one `EXTERNAL_PREFIX` pattern into an `InventorySelector` on the four
  index-resolved inline nodes.
- **A label's link text and anchor come from the index**: `target_titles`
  (what a bare `:ref:` shows — the heading or caption under the label) and
  `target_anchors` (for a target whose `id` is not its name, e.g. an entity's
  `entity-<id>`), read through `ProjectIndex::target_anchor`.

### Numbering: `numfig` and `:numref:` (`docs/decisions/028-numref-role.md`)

`numfig = true` in `rinx.toml` numbers every *captioned* figure, table and code
block (labelled or not, as Sphinx does), and `:numref:` shows a number. Three
things to know before touching it:

- **One function decides which elements are numbered**:
  `rinx_ast::enumerable_elements`. The analyzer stores numbers by an element's
  position in its result (`ProjectIndex::element_numbers`), and the renderer's
  `numbering.rs` calls it again on the same document to find each caption's
  number by the directive's address. Never count elements separately on either
  side.
- **Input merges, numbers are recomputed.** `numbering_steps` and
  `numref_targets` are per document; `element_numbers` is produced by the
  `assign_element_numbers` phase (a port of Sphinx's `assign_figure_numbers`),
  which runs after section numbering and again in `process_preview`.
- **A refused `:numref:` title is reported while parsing** through an
  intermediate `InlineNode::RefusedRole`, which
  `parser/blocks/refused_roles.rs` reports and lowers after substitutions — the
  one node and pass every refusable role shares (`:pep:` too), its
  `RoleRefusal` saying which role and how to lower it. It walks inline content
  with `rinx_ast`'s `for_each_inline_list_mut`, the traversal substitution
  resolution uses too.

### Registry roles: `:pep:`, `:rfc:`, `:cve:`, `:cwe:` (`docs/decisions/030-pep-role.md`, `docs/decisions/034-registry-roles.md`)

`InlineNode::RegistryReference` is one node standing for Sphinx's three — the
general-index entry, its anchor and the link — for all four roles, which differ
only in their `RegistryTarget` (`ast/registry_target/`: one enum over a target
type per registry, plus the `Registry` tag whose `role_name`/`label`/
`index_group` are Sphinx's). Things to know:

- **Per-registry behaviour lives on the target, nowhere else.**
  `display_text()` is both the link text and the index subentry (an RFC spells
  out `section-`/`appendix-`/`page-` anchors), `page_path()` is relative to the
  registry's base. A new registry is a variant there and a regex alternative,
  not a node.
- **The anchor is minted after parsing**, by `parser/blocks/index_ids.rs`'s
  `assign_registry_index_ids`, continuing the `.. index::` counter and running
  after substitutions so each use of a definition gets its own id. The analyzer
  finds the roles with `rinx_ast::for_each_inline_list` — generated from the
  same macro body as the parser's `for_each_inline_list_mut`
  (`ast/inline_lists.rs`), so an entry exists for exactly the roles that have
  an anchor.
- **The `.ast` holds the page, not its URL.** The renderer prefixes
  `rinx.toml`'s `pep_base_url`/`rfc_base_url` (one `RegistryBaseUrl<S>` type,
  `renderer/registry_base_url.rs`), so changing them re-parses nothing; CVE's
  and CWE's addresses are constants, as Sphinx hard-codes them
  (`renderer/inline/registry_reference.rs`).
- **A refusal names its registry, a code names the construct**:
  `RoleRefusal::RegistryTarget` maps to `pep.invalid-number`,
  `rfc.invalid-number`, `cwe.invalid-number` or `cve.invalid-id`. `:cve:` is
  validated where Sphinx accepts anything (year and sequence number; a `CVE-`
  prefix is refused by name) — a deliberate deviation, see ADR-034.
- **Role names stay case-sensitive** (`:PEP:` is not recognized), deliberately:
  docutils' case-insensitivity belongs to every role and is left to a change
  that introduces it for all of them.
- **`:index:` shares the anchor pass, not the node** (`InlineNode::IndexReference`;
  `docs/decisions/035-index-role.md`): `assign_inline_index_ids` numbers it
  among the registry roles, and its target is parsed by the `.. index::`
  directive's own `parse_index_line`/`parse_typed_entry`, which return
  `InvalidIndexEntry` rather than reporting — the directive reports it as
  `index.*`, the role as `index-role.*`. A refused one lowers to its title
  with no entries, so the refusal pass runs before the anchor pass.
  `see:`/`seealso:` entries become `ProjectIndex::genindex_redirects`, kept
  apart from the linked `genindex_entries`.
- **docutils' `:pep-reference:` and `:rfc-reference:` are sibling nodes, not
  flags** (`InlineNode::DocutilsPepReference`/`DocutilsRfcReference`; see
  `docs/decisions/033-pep-reference-role.md`): no anchor, no index entry, no
  title. They never meet the anchor or index passes, and share only the
  refusal pass, the digit reader and their base URL.

### Source transclusion (`docs/decisions/008-source-transclusion.md`)

`.. include::` and `.. literalinclude::` read a file and splice it into the
document **while parsing**, through the injected `ParseFileLoader` seam on
`ParseCtx` (`crates/parser/src/context.rs`) that `.. csv-table::`'s `:file:`
already used — the parser still performs no I/O of its own, and
`crates/worker/src/commands/parse_files.rs` supplies the filesystem.

Two consequences shape the code around them:

- **An include is a transclusion, not a container.** It is the one directive
  yielding *several* nodes, which is why `try_parse_directive` returns a
  `Vec<Node>`. Wrapping the result in a node of its own would break section
  nesting, targets and toctrees inside the fragment.
- **A span can name a file other than the document.** `Span` carries an
  interned `Option<FileId>` indexing `Document::source_files`, so a diagnostic
  from inside a fragment names the fragment — at parse time *and* at render
  time, where a broken `:ref:` is found in a different process entirely. An id
  rather than a path keeps `Span` `Copy`. `Suppression` carries the matching
  field, so a `.. noqa:` only silences diagnostics from the file it was written
  in. The worker's `WarningOrigin` (`commands/diagnostics.rs`) is the one place
  that turns an id back into a path.

`.. literalinclude::` adds no AST node: it lowers to the same
`Directive::CodeBlock` the two inline code directives produce, tagged
`CodeBlockSource::LiteralInclude`, because every option it adds is a parse-time
source transform. The renderer needed no changes at all.

### Templated sources (`docs/decisions/013-source-templating.md`)

`jinja = True` on a `rinx_library` renders each of its `.rst` sources
as a Jinja template before parsing it — what a Sphinx project gets by
connecting the `source-read` event in `conf.py`, which this build cannot run.
Three things to know before touching it:

- **It is opt-in, and within a library a document holding no Jinja delimiter
  is not rendered at all.** Both guards exist because `{{` and `{%` are
  ordinary characters in prose.
- **A template name is source-root-relative**, as Jinja's own loader is —
  the one place `{% include %}` and `.. include::` deliberately disagree.
- **The rendered text is never materialized.** It lives inside the parse
  action; `parse --dump-rendered <path>` is a debugging flag no Bazel rule
  passes (and `preview` has no equivalent — its stdout is the page).

The mechanism worth understanding is the line map. A Jinja pass moves lines and
MiniJinja has no source map, so `rinx_template` injects a
`(template, line)` marker into every markable line and reads it back off the
output. `ParseCtx` holds the result as a `TemplateMap` — and unlike `Origin`
it is **not** an offset and does not compose, so it is consulted once at the
end of `ParseCtx::position`, which is why that returns a `SourcePoint` (a
position *and* its file) and why building a `Span` now lives on
`SourcePoint::to`.

### Entities: a project's own construct vocabulary (`docs/entities.rst`, `docs/decisions/009-entities.md`)

A project declares its own entity types — `.. req::`, `.. audit-event::`,
whatever it names — in a schema file, and the build treats them as first-class
constructs. `docs/entities.rst` is the guide; the two things to know before
touching the code:

- **Four kinds of declaration, and they are not interchangeable.**
  *Attributes* are values (typed, validated, indexed, never parsed as RST);
  *sections* are documents (fully-parsed RST, not indexed) and are what this
  model adds over sphinx-needs; *relations* are edges declared on the type that
  carries them, with back-links **derived** project-wide, never declared;
  *roles* are optional sugar, since every entity is already a `:ref:` target.
- **The schema is a parse-time input.** It is what makes `.. req::` a directive
  rather than an unknown name, so `entity_schema` is an attribute on
  `rinx_library` *and* on `rinx_site`, and editing it re-parses
  everything. A library parsed against a different schema than the site indexes
  with is reported as `entity.schema-mismatch`, in both directions.

A schema may also constrain values with a `pattern` (`docs/decisions/022-value-patterns.md`):
on `id`, checked after the id is determined and reported as
`entity.id-pattern-mismatch` with the id **kept**; and on a text attribute,
where it lives *inside* the `AttributeType` variant so `parse_attribute_value`
enforces it on every route a value arrives by (written option, `needimport`,
`entity-update`). Both are `rinx_entity::ValuePattern`, compiled once
when the schema loads.

`schemas/entities.schema.json` is generated from the loader's `Raw*` types
(`cargo run -p rinx -- entity_json_schema`) and checked in, with a
test that regenerates and compares; it gives editors completion over an
`entities.toml` but validates only its *grammar* — every cross-reference rule
stays in `crates/entity/src/load.rs`, which is the authority.

`rinx_entity` owns the meta-model; `rinx_ast`'s `entity/` owns
the instance data (`EntityId`, `AttributeValue`, `EntityBody`, `EntitySection`),
because that is what survives into a `.ast` file. The dependency runs
`ast → entity`, so the split cannot go the other way.

- `rinx_filter` (`crates/filter/src/`) — the typed expression language a
  listing directive's `:filter:` is written in (`parse_filter` → `Expr` →
  `Expr::matches`). A leaf crate like `rinx_cdecl`, and for the same
  reason its parser is hand-rolled: two phases that may not depend on each other
  both need it — the **parser** parses a filter, so a syntax error lands on the
  option line the author wrote, and the **renderer** evaluates one. It therefore
  knows nothing about entities at all; what it learns about the thing being
  filtered arrives through the injected `FilterSubject` trait, which
  `renderer`'s `blocks/entity_table/subject.rs` implements over an
  `EntityRecord`. Python's spelling over the subset real sphinx-needs filters
  use, with everything outside it — calls, comprehensions, `!= None`, `&&`,
  ordering — refused *by name* with a character offset rather than as a generic
  syntax error. Which field *names* exist is not here but in
  `rinx_entity`'s `field.rs`, because the parser must check them without
  an index in hand; this crate only evaluates.

### Diagrams: opt-in per library, compiled behind the index (`docs/decisions/012-entity-diagrams.md`, `docs/decisions/014-entity-flow.md`, `docs/decisions/020-entity-sequence.md`)

Six spellings — `.. plantuml::`/`.. uml::`, `.. entity-diagram::`/`.. needuml::`
and `.. entity-arch::`/`.. needarch::` — parse to one `Directive::Uml`, which
carries the diagram's **template** rather than its finished text. Two more
constructs, `.. entity-flow::`/`.. needflow::` and
`.. entity-sequence::`/`.. needsequence::`, have no template at all (see
below) but share everything from the finished text onwards. Five things
follow, and none of them is optional:

- **You pay only for what you use.** `diagrams = True` on a library is what
  creates any diagram action at all (see the dependency mechanisms above).
  Measured on CPython's docs — ~500 documents, no diagrams — the site builds in
  the same 15.6s it did before diagrams existed; an always-on pipeline cost it
  20.4s.
- **Compilation is a `rinx_site` action, never a library one.** A
  templated diagram asks the entity graph questions only `ProjectIndex` can
  answer. The plain PlantUML pair goes the same way rather than keeping its old
  Phase-1 path, because two paths would mean two hash populations. A plain
  `.. plantuml::` is a template with nothing to expand, so its hash is
  unchanged.
- **The render action writes the `.puml` files.** It already expands each
  diagram to get the hash its `<img>` names, so `rinx_uml::expand` is
  called once per diagram, in one process that emits both the page and the
  file — no separate expansion action, no sidecar. The renderer crate still
  does no I/O: `RenderOutput::diagram_sources` hands the text back and
  `cmd_render` writes it. The expander must be **deterministic** (every
  iteration is over a `BTreeMap`), or an unchanged diagram hashes differently
  and its compile misses the cache.
- **The cache firewall is the `.puml` directory.** The index is an input, so any
  edit re-runs every render; identical bytes leave the per-document compile
  action's key unchanged and no JVM starts.
- **Layout is ELK, never a host `dot`** (`-Playout=elk` in `rules/site.bzl`;
  Smetana crashes on cyclic flowcharts). Without Graphviz, PlantUML draws a "Cannot find Graphviz"
  picture and exits 0, and the host's `dot` is not in the action key — so a
  build looked fine locally while CI published error pictures. `validate_assets`
  fails on such exit-0 error pictures as a backstop, and `rules/plantuml.bzl`
  pins PlantUML to JDK 21, which the jar's ELK needs, whatever the consuming
  build's `--tool_java_runtime_version` says (ADR-012 §8).

`filter()` is `rinx_filter` and `flow()`/`ref()` build hrefs from
`rinx_index`'s `relative_doc_href`/`entity_anchor` — which is why those
two, and `EntitySubject`, live in `index` rather than in the renderer where
they began. A filter or a link meaning one thing in a table and another in a
diagram would be a bug neither crate's tests could see.

**`.. entity-flow::` / `.. needflow::`** (ADR-014) is the flowchart: a
`Directive::EntityFlow` carrying a *question* — a filter and which relations
become edges — with the `PlantUML` **generated** by `rinx_uml`'s
`build_flow` while rendering. It is a sibling of `Directive::EntityTable`, not
of `Directive::Uml`: same question, different presentation. Lowering it to a
`Uml` node holding generated Jinja was rejected because every diagnostic would
then be reported against text the author never wrote. Three modules exist
purely so the two paths cannot drift: `uml/assemble.rs` (the `@startuml`
wrapping, `:config:` preamble, empty-diagram refusal and hash), `uml/node.rs`
(the `rectangle` one entity is drawn as, shared with the `flow(id)` template
function) and `renderer/blocks/diagram_figure.rs` (the `<img>`, caption and
`:debug:` block). Its diagnostics are `entity-flow.*`, not `uml.*`, for the
reason the diagram family is `uml.*`: a code names the construct.

**`.. entity-sequence::` / `.. needsequence::`** (ADR-020) is the flowchart's
sibling: a `Directive::EntitySequence` carrying a *walk* — start entities and
the relations that carry messages — generated by `rinx_uml`'s
`build_sequence`. Its `start` and `relations` are `NonEmptyVector`s: both are
mandatory (`:relations:` deliberately, where sphinx-needs defaults to `links`),
so a directive missing either degrades to `Directive::Malformed` while parsing.
Unlike a flowchart's, not every finding is fatal — an unknown start or a
`:max-items:` truncation still draws — so `build_sequence` returns a
`SequenceDrawing` (content *and* problems) rather than a `Result`, and the
renderer's `DiagramFailure::Sequence` carries findings that sit beside a
picture as well as ones that replace it.

### Extension directives (`.. dropdown::`, `.. grid::`, `.. entity-table::`, `.. entity-flow::`, `.. entity-sequence::`, `.. entity-pie::`, `.. entity-bar::`, `.. if-builder::`; see `docs/decisions/011-entity-listing.md`)

Everything else this build parses is docutils' or Sphinx's own. `.. dropdown::`
and `.. grid::`/`.. grid-item::` come from **sphinx-design**,
`.. entity-table::`/`.. needtable::`, `.. entity-flow::`/`.. needflow::`,
`.. entity-sequence::`/`.. needsequence::`, `.. entity-pie::`/`.. needpie::`
and `.. entity-bar::`/`.. needbar::` from **sphinx-needs**, and `.. if-builder::`
from **sphinx-simplepdf**;
`directives/dispatch.rs`'s `try_parse_extension_directive`
groups them (the five views over the entity graph in a nested
`try_parse_entity_view`), because they share a rule the built-ins do not:

- **There is no `extensions =` config**, so a supported extension directive is
  simply always available, and its name joins `BUILTIN_DIRECTIVE_NAMES` — which
  means an entity schema can no longer declare a section by that name.
- **An unknown directive never parses its body** — it becomes a
  `Directive::Unknown`, reports `directive.unknown`, and is drawn as a visible
  error block quoting its source, so the content it swallows is never lost
  silently. A directive whose *name* is recognized but whose content this build
  must refuse becomes a `Directive::Malformed` instead, carrying the message its
  own diagnostic reports; both are built by `parser/directives/error_node.rs`
  and rendered by `renderer/blocks/directive_error.rs`. That an unknown one
  parses no body is also why these earned
  their place ahead of prettier candidates: the entity benchmark's corpus nests
  `.. seq_msg::` entities inside dropdowns, and every one of them was invisible
  to the index until the container parsed — and it draws a `.. uml::` inside a
  `.. grid-item::`, which reached no later phase at all until the grid parsed
  its body. The corollary shapes both parsers: an unreadable *option* never
  costs the container, and a misplaced *child* is reported and **kept**, never
  dropped. Two consequences are easy to miss when adding a container here — a
  body must be parsed under `ctx.nested(...)` or every span inside it is wrong,
  and the container needs an arm in every traversal that walks block content
  (`analyzer`'s `index_nodes`, `parser`'s `assign_index_ids`, `renderer`'s
  `collect_anonymous_targets`, `ast`'s `walk_nodes`) or targets written inside
  it silently vanish.

`.. entity-table::` is the odd member of that group: the name is *ours* and
`.. needtable::` is the compatibility spelling, since the entity model's whole
claim is that sphinx-needs is a schema rather than a feature. Both names are
reserved, they parse to one `Directive::EntityTable` recording which was
written, and they share one `entity-table.*` diagnostic family — a code names
the construct, not the spelling. It is also the only directive whose *content*
comes from other documents: the node carries a question and
`renderer/blocks/entity_table/` resolves the rows against `ProjectIndex`, which
is why an empty result can only be reported while rendering. `.. entity-flow::`
is its twin in all of that — our name, a `needflow` alias, one node, one
`entity-flow.*` family, content from other documents — and differs only in
drawing the answer instead of tabulating it; its filter goes through the one
shared reader in `parser/directives/filter_option.rs`, so the two cannot select
differently.

`.. entity-pie::` (`.. needpie::`; ADR-017) is the third presentation of that
same question — rows, a graph, or proportions — and the one that breaks the
other two's shape: a table takes **no argument**, a flowchart takes **no
content**, and a chart takes **both**. The argument is the title and each
content line is one wedge's filter, read through `filter_option.rs`'s
`read_filter_text` so a body line reports at the same column an option line
would. Two things about it are worth knowing before touching the diagram
pipeline:

- **It is never compiled.** The SVG is drawn by the render action itself
  (`renderer/src/chart/`, the one place `plotters` is named), so there is
  no `.puml`, no hash, no compile action, and — deliberately, departing from
  ADR-012 §4 — **no `diagrams = True`** and no `entity-pie.diagrams-disabled`
  code. `diagrams = True` exists because Bazel must declare actions before
  reading any document; a chart declares none. Keep charts in an ordinary
  library, as `examples/entities/charts.rst` is.
- **The generality is in the counting, not in the directive.** Making
  `needpie` and `needbar` one `.. entity-chart::` with a `:type:` switch was
  rejected — their bodies and option surfaces barely overlap — so what they
  share is `renderer/blocks/chart_counts.rs`, which walks the index once per
  chart for a pie's wedges and a bar chart's flattened grid alike.

`.. entity-bar::` (`.. needbar::`; ADR-021) is the pie's sibling with a grid
for a body: each line a series, each `:separator:`-split cell a category.
`FROM_DATA` labels and `:transpose:` are settled while parsing, so the node's
`BarGrid` is already labelled and oriented. Where sphinx-needs raises — ragged
rows, label lists of the wrong length — it reports and draws. Every piece of
its text is a `chart/placed_text.rs` `PlacedText` spliced into the SVG, since
its rotation options take any angle.

Its empty result goes through `renderer`'s `EmptyListingError`, shared with
`.. entity-table::` and carrying its own `DiagnosticCode`: a code still names
the construct, but the `.. noqa:` filtering and — crucially — the live
preview's "stay quiet without an index" gate are written once.

`.. if-builder::` is the odd one out of that group in a different way: it is a
**splicing** directive, not a container at all. It, `.. include::` and
`.. needimport::` are the only three that contribute *several* nodes to the
enclosing block rather than exactly one, so none can live in
`try_parse_extension_directive` (which returns a single `Directive`) — they are
dispatched together by `try_parse_splicing_directive`, ahead of the one-node
chain. That is what keeps a heading, target or `.. toctree::` written inside a
selected block belonging to the document, and it is the directive's whole
point: a container node would put the body where `build_document_outline`
stops. Its body is parsed only when the argument names this build's builder
(`html`), so an excluded block reports nothing and needs no `deps` entry. See
`docs/decisions/015-builder-conditionals.md`, which also records why the
`<div class="docutils container">` upstream emits is deliberately not
reproduced.

Icons come from `octicons-pack` (a redistribution of `@primer/octicons`), and
the split of responsibility is deliberate: the AST carries only the *name*
(`OcticonName`, which validates its shape and nothing else, so an old `.ast`
cannot fail to load after an icon-set upgrade), the **parser** checks that name
against the set — where the `:icon:` line can be pointed at — and the
**renderer** is the only place that draws one.

### Config vs. CLI flags (see `docs/decisions/001-template-system.md`)

`rinx.toml` (`rinx_renderer::config`) holds only metadata — never file paths. Template/CSS/index paths are always passed as separate CLI flags / Bazel rule attributes (`--template`, `--config`, `template =`, `css =`). This is deliberate: Bazel sandboxes relocate files, so a path baked into the config would break; keeping paths out of the config also means new metadata fields don't require CLI changes. Don't "fix" this by inlining paths into the TOML.

`highlight_language` is the one exception-shaped addition: it names a language, not a path, so it respects the rule above, and it is deserialized through the same smart constructor the parser uses so a typo fails on load.

`[version_switcher] json_url` is a URL the *browser* fetches, not a path the build reads, so it respects the rule too. What it deliberately lacks is pydata-sphinx-theme's `version_match`: `assets/version_switcher.js` finds the page's version by the page's own address, so the build never learns which version it is and one commit's `main/` and preview builds share their cached renders — see `docs/decisions/024-versioned-docs.md`. That script's pure functions are tested by `bazel test //tests/js:all`, a rules_js `js_test` under a hermetic Node.js — no local Node needed. `aspect_rules_js` is a `dev_dependency` and must stay one, which is why the test lives in its own package: the root package is loaded by every module depending on rinx, and `assets/` cannot become a package without breaking the `@rinx//:assets/default.css` label. rules_js brings Aspect's usage telemetry, which `.bazelrc` opts out of with `DO_NOT_TRACK`.

Templates are rendered with MiniJinja (chosen over compile-time Rust templates or reusing Sphinx's Python/Jinja2 templates directly — see the ADR for why). Changing the default template invalidates every cached page render, which is expected.

### Live preview / LSP direction

`editors/vscode/` (`src/extension.ts`, `src/bazel.ts`, `src/client.ts`, `src/binary.ts`) is a working VS Code extension implementing the architecture described in `docs/vscode.rst`: a webview panel spawns the `preview` subcommand (parse+locally-analyze+merge-into-stale-global-index+render, all in-process in `crates/worker/src/commands/preview.rs::process_preview`) on every edit (debounced) or on save, per the `rinx.previewMode` setting. `bazel.ts`'s `BazelScanner` auto-discovers the site's config/template/index paths by `bazel query`-ing for a `rinx_site` target and resolving its `config`/`template` labels to filesystem paths (falling back to the `rinx.*` settings if discovery is disabled or fails), and `BazelConfigCache` invalidates that discovery when `BUILD`/`WORKSPACE`/`MODULE.bazel` files change. On activation (any reStructuredText document) `client.ts` also starts `rinx lsp` through `vscode-languageclient`, which publishes each open document's parse diagnostics; the preview and the client find the binary through `binary.ts`'s one precedence (explicit `rinx.binaryPath` → Bazel-discovered → `rinx` on the PATH), so they never run two different builds. Not yet implemented: the background-`bazel build`-to-reconcile-global-state step from the design doc (titles/cross-refs go stale until a manual rebuild), everything in the language server past single-document parse diagnostics, and PlantUML rendering in the preview. Keep this flow in mind when touching `rinx_index::ProjectIndex::merge` or the `preview` subcommand — both exist specifically to support it. The language server's design — including a legacy mode for plain Sphinx projects — is `docs/decisions/038-language-server.md`, and the PR-by-PR order it is built in is `docs/dev/lsp-roadmap.md`.

### Documentation site (`docs/`)

The user guides in `docs/` are reStructuredText, and `//docs:site` builds them with rinx itself — the project's own dogfooding check, with `strict_links = True` so a broken reference fails CI. `.github/workflows/pages.yml` publishes it to <https://nachstedt.github.io/rinx/> together with `//examples:site`, one directory per version on the `gh-pages` branch: `main/` after CI succeeds on main, `vX.Y.Z/` from the release workflow, `pr/<N>/` for every open pull request (`pages-preview.yml`, from an artifact CI uploads; removed on close by `pages-preview-cleanup.yml`), with `//examples:site` under each one's `example-site/` and the two benchmark corpora's rendered sites under `benchmarks/<name>/`. Those come from `.github/workflows/benchmarks.yml`, a reusable workflow `ci.yml` runs on every pull request, push and release (failing only when a corpus stops building; `pages.yml` reuses CI's artifact rather than rerunning it), and each benchmark script exports its site via `--site-out` with its front page named in an `entry.txt`, so neither the publisher (`--benchmarks-dir`) nor any workflow names an individual benchmark. Only `main/`, the previews and the newest release carry them (~70 MB each, against Pages' 1 GB limit): `publish_pages.py` deletes a superseded release's `benchmarks/` whenever it regenerates the derived files. None of the writers share a `concurrency` group — GitHub would cancel all but one pending run — so the publisher's retry is what serializes them, and the preview workflow trusts nothing from CI's artifact but its files: the pull-request number is cross-checked against the pull request's head commit. `scripts/publish_pages.py` is the only writer of that branch, and regenerates `latest/`, `versions.json` and the redirects for the pre-version URLs on every run — see `docs/decisions/024-versioned-docs.md`. Only the guides are pages: `docs/decisions/` (ADRs) and `docs/dev/` (developer notes) stay Markdown and are linked on GitHub. A page is written only with constructs rinx supports — notably no footnotes. A new user-facing feature belongs on `docs/syntax.rst` or in the guide it extends, as well as in `examples/`.

### Compatibility status

`docs/compatibility.rst` is a page of the documentation site listing every RST/Sphinx/extension construct as ✅ supported, ✅ ℹ️ supported with deliberate deviations, 🔶 partial (something we still intend to implement is missing) or ❌ unsupported. Notes are brief bullets naming only deviations or missing pieces. Update it when you add or fix parser/renderer coverage for a construct, and don't trust it blindly if it looks stale relative to recent commits.

## Working conventions

These come from `guidelines.md`, which is actively maintained (an agent rule auto-appends new learned preferences there) — check it for the current, authoritative list; highlights that shape how code here is structured:

- Enforce invariants at the type level with opaque types + smart constructors ("parse, don't validate") rather than validating at call sites.
- All functions, including private/helper ones, should have unit tests; new functions introduced during refactoring need tests too.
- Tests follow Given-When-Then (see the inline `#[cfg(test)]` modules throughout `crates/*/src/` for the expected shape/comment style).
- Name functions after what they concretely do (`build_project_index()`, not `analyze_many()`).
- New `.rst` files added under `examples/` must be added to the corresponding `BUILD.bazel` `srcs`, and new features should get an example added to the example project covering all variants.
- Don't commit unless explicitly asked.

### Shared rules from `.agents/rules/`

This repo also defines always-on rules for the Antigravity agent under `.agents/rules/` (each file's `trigger: always_on` frontmatter is Antigravity-specific and can be ignored — the frontmatter is just inlined as text below by Claude Code's import). Importing them here keeps both agents' behavior in sync — in particular, `learning-agent.md` is the rule that keeps `guidelines.md` above up to date, so it must load for Claude Code too:

@.agents/rules/learning-agent.md
@.agents/rules/rust-fundamentals.md
@.agents/rules/rust-testing.md
@.agents/rules/test-drive-development.md
