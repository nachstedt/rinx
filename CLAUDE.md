# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`rusty-sphinx` is a Rust re-implementation of (a subset of) the Sphinx documentation generator, designed to be fast and to integrate natively with Bazel as a first-class, cache-friendly build step (not a wrapped external tool). It parses reStructuredText (`.rst`) into HTML documentation sites, with cross-file references, toctree-based navigation, and PlantUML diagram rendering.

Read `requirements.md` and `architecture.md` for the full design rationale — the paragraphs below only cover what differs from those aspirational docs, or what's needed to be productive immediately. The codebase is a Cargo workspace under `crates/` (`rusty_sphinx_ast`, `rusty_sphinx_scope`, `rusty_sphinx_index`, `rusty_sphinx_parser`, `rusty_sphinx_analyzer`, `rusty_sphinx_renderer`, `rusty_sphinx_worker`); `rusty_sphinx_scope` and `rusty_sphinx_index` aren't part of architecture.md's crate split (that doc is aspirational and predates both), the other five match it. The `rusty_sphinx_lsp` crate architecture.md also describes is not yet built.

## Commands

Development uses plain Cargo; the Bazel build is the production/integration path and wraps the same binary.

```bash
cargo build
cargo test --workspace            # unit tests (co-located #[cfg(test)] modules) + crates/worker/tests/integration/
cargo test <substring>           # run a single test by name substring
cargo test --test integration
cargo clippy --workspace --tests  # must be warning-free before finishing any change (pedantic lints are on, see workspace Cargo.toml)
cargo fmt

bazel build //:rusty_sphinx_worker    # build the CLI binary via Bazel
bazel build //examples:site           # build the example multi-team site end-to-end
bazel build //examples/team_a:docs    # build one team's library in isolation
bash tests/test_strict_deps.sh        # verifies Bazel fails the build when a toctree dep is missing from BUILD.bazel
bash tests/test_strict_links.sh       # verifies broken links only warn by default, and fail the build when strict_links = True
bazel run //scripts:benchmark         # clone CPython docs and benchmark the pipeline against it (see docs/benchmark.md)
```

There are no `rust_test` Bazel targets — tests are run through Cargo only.

After any code change: run `cargo clippy --workspace --tests` (fix all warnings, don't `#[allow(...)]` them — treat them as refactor signals) and `cargo fmt`, then verify `bazel build //examples:site` still succeeds.

## Architecture

### Pipeline: parse → analyze/index → render

The binary (`crates/worker/src/main.rs`) is a multi-subcommand CLI, one subcommand per pipeline phase, because Bazel needs each phase as a separately cacheable action:

- `parse --input file.rst --output file.ast` — RST text to a serialized `ast::Document` (JSON).
- `validate_toctree --input file.ast --output file.ast [--allowed <doc>...]` — checks toctree entries only reference declared docs.
- `extract_diagrams` / PlantUML compile / `validate_images` — pulls `Directive::PlantUml` content out of the AST, compiles it to SVG via the `plantuml` jar, and later checks every referenced diagram has a matching SVG.
- `index --inputs a.ast b.ast ... --output project.index` — merges all documents' local analysis (`analyzer::analyze`) into one global `rusty_sphinx_index::ProjectIndex` (targets, document titles, nav tree, glossary terms).
- `render --input file.ast --index project.index --doc-path rel/path --output file.html --config site.toml --template layout.html` — turns one AST + the global index into a final HTML page.
- `preview` — collapses parse+local-analyze+merge+render into one process reading RST from stdin, for low-latency editor use (see "Live preview" below).
- Legacy: `rusty-sphinx <file.rst>` prints HTML straight to stdout (used by `process_rst()` in `crates/worker/src/lib.rs`, mostly for quick manual checks).

Each subcommand handler is split into a pure `process_*` function (testable without file I/O) and a thin `cmd_*` wrapper that does the file reads/writes, the two living together in their own `cmd_*.rs` file (e.g. `cmd_render.rs` holds both `process_render` and `cmd_render`), with `main.rs` itself reduced to `mod` declarations and the `run()` dispatcher — keep new subcommands following that split.

### Crate layout (`crates/`)

Each crate is a workspace member with its own `Cargo.toml` and `BUILD.bazel`; dependencies between them flow strictly `ast → scope`/`index` → `analyzer`/`renderer` → `worker`, with `parser` depending only on `ast`, parallel to `scope` and `index`.

Within a crate, a file is split once it grows past ~800 lines, along construct/responsibility boundaries. Prefer expressing the relationship via a same-named subdirectory (`inline.rs` splitting into `inline/reference.rs`, `inline/hyperlink.rs`, ...) over flat filename-prefixed siblings (`inline_reference.rs`) — `mod x;` in the parent file resolves to `parent_dir/x.rs` by default, so this needs no `#[path]` attribute; the one place this isn't possible is a crate root (`lib.rs`/`main.rs`), whose direct children are necessarily flat siblings of `lib.rs`/`main.rs` itself since that file's own directory *is* `src/`. When an implementation is genuinely one cohesive unit and its bulk is in `#[cfg(test)]` content (e.g. `ast/domain_object_body.rs`'s single enum+impl, `cdecl/parser.rs`'s one recursive-descent `Parser`), split only the tests into topic-based sibling files instead of fragmenting the implementation.

- `rusty_sphinx_ast` (`crates/ast/src/lib.rs`) — AST node types. Note `HashedContent` and `TargetName`: both are "parse, don't validate" opaque types (smart constructors + custom `Deserialize` that re-validates the invariant on load, e.g. `HashedContent`'s stored hash must match `sha256(body)`). Follow this pattern for new invariants rather than validating ad hoc at call sites.
- `rusty_sphinx_scope` (`crates/scope/src/python_scope.rs`) — `PythonScope`, the enclosing `py:class`/`py:exception`/`py:module` scope tracked while indexing and rendering domain objects, shared by `analyzer` and `renderer` so a definition's index key and a reference's resolution always agree. Kept as its own crate (rather than folded into `ast`, where it briefly lived) because it's traversal state, not parsed-document data — nothing in it is ever serialized to a `.ast` file.
- `rusty_sphinx_index` (`crates/index/src/`) — `ProjectIndex` and the data types it's built from (`NavEntry`, `TargetLocation`, `GenIndexEntry`), plus `ProjectIndex::merge`/`insert_domain_object`. Pure data + `Serialize`/`Deserialize`, no traversal logic — kept separate from `rusty_sphinx_analyzer` (which builds a `ProjectIndex`) because `rusty_sphinx_renderer` only ever reads one; before this split, renderer depended on the whole analyzer crate just to get these types.
- `rusty_sphinx_parser` (`crates/parser/src/`) — one file (or, once split, one directory) per construct (`headings.rs`, `blocks.rs`, `bullet_list.rs`, `directives/`, `admonitions.rs`, `glossary.rs`, `inline/`, `domains/`, ...); `parser::parse()` is the entry point. The parser is meant to be error-resilient (bad input becomes an error/unknown node, not a panic/abort) to support live preview over incomplete documents.
- `rusty_sphinx_analyzer` (`crates/analyzer/src/lib.rs`) — builds the per-document and project-wide index: cross-reference targets, document titles, the toctree-derived nav tree, glossary terms, split across `document_index.rs`/`domain_object_index.rs`/`nav_tree.rs`/`project_index.rs`. `rusty_sphinx_index::ProjectIndex::merge` is what lets the `preview` subcommand and the VS Code extension combine a fresh local analysis with a stale global index.
- `rusty_sphinx_renderer` (`crates/renderer/src/`) — AST + `rusty_sphinx_index::ProjectIndex` to HTML. `lib.rs` holds the public types and the `render()` entry point; `blocks.rs`/`tables.rs`/`scope_directives.rs` render body content; `page.rs` wraps it in the MiniJinja-templated page chrome (nav sidebar, CSS link); `nav.rs` builds sidebar markup from the nav tree; `domain_object/` and `inline/` handle directive- and inline-markup-specific rendering, one file per domain-object kind or inline role; `config.rs` holds `SiteConfig`, deserialized from `rusty_sphinx.toml` — deliberately only metadata (project name/version), never file paths, see "Config vs CLI flags" below.
- `rusty_sphinx_worker` (`crates/worker/src/`) — `main.rs` is the CLI entry point, dispatching to one `cmd_*.rs` file per subcommand, each pairing a pure `process_*` with its file-I/O `cmd_*` wrapper per the split described above; `cli_args.rs` holds the shared flag-parsing helpers and `diagnostics.rs` the broken-link/mismatch warning formatting shared by `render` and `preview`. `lib.rs` holds `process_rst()`, `doctest_plan.rs` (see "Doctests" below), `domain_warnings.rs`, and `validator.rs` (toctree/allowed-docs validation used by the `validate_toctree` subcommand).

### Bazel rule pair: library vs. site (`rules/library.bzl`, `rules/site.bzl`, `defs.bzl`)

Mirrors `cc_library`/`cc_binary`: `rusty_sphinx_library` runs Phase 1 (parse, toctree-validate, extract/compile PlantUML diagrams) per `.rst` file and exposes a `RustySphinxInfo` provider carrying `.ast` files + SVG dirs. `rusty_sphinx_site` collects everything transitively from `deps`, runs the single Phase 2 index action, then one Phase 3 render action per `.ast` file, bundles images, and copies CSS.

Two independent dependency mechanisms, don't confuse them:
- **`deps` on `rusty_sphinx_library`** — strict, DAG-enforced, required *only* for docs pulled in via `.. toctree::`. This is what `validate_toctree` checks against (`--allowed`).
- **Cross-references / hyperlinks in text** — not declared in `deps` at all; stored as symbolic markers in the Phase-1 AST and resolved later, globally, during the Phase 2 index step. This is why cyclic hyperlinks between docs are fine but cyclic toctrees are not.

(A third attribute, `py_deps` on `rusty_sphinx_doctest_tests`, is *not* a third mechanism of rusty-sphinx's own — it is rules_python's ordinary `py_test.deps`, surfaced so documented code is importable while doctests run. It has nothing to do with either of the above.)

See `examples/BUILD.bazel` + `examples/team_a`, `examples/team_b` for a concrete two-team library/site setup, and `tests/test_strict_deps.sh` for how the strict-toctree-dep enforcement is tested (it mutates `examples/BUILD.bazel` temporarily to prove a missing dep fails the build). Note `examples/BUILD.bazel` declares *two* libraries in one package: `root_docs` and the `doctest_docs` it depends on — see the doctest section below for why that split exists.

### Doctests: rendered by the build, executed by tests (`rules/doctest.bzl`, see `docs/decisions/002-doctest-execution.md`)

The `sphinx.ext.doctest` family (`doctest`, `testcode`, `testoutput`, `testsetup`, `testcleanup`) is split across Bazel's build/test line, and the split is the design:

- **Rendering** happens in the normal pipeline. `bazel build //examples:site` produces HTML for these blocks and never starts a Python interpreter.
- **Execution** is `rusty_sphinx_doctest_tests`, a macro emitting one stock `py_test` per `rusty_sphinx_library`. It is opt-in and enforced by `tests/test_doctest_isolation.sh`.

Between them sits the **cache firewall**. `rusty_sphinx_library` declares an `extract_doctests` action per document (`crates/worker/src/doctest_plan.rs`) producing a `.doctests.json` that keeps only what changes how the code runs — never `:hide:`, the trim tri-state, or line numbers. A prose edit re-runs that cheap AST walk but leaves its bytes identical, so `bazel test` reports `(cached)` and no interpreter starts. `tests/test_doctest_cache_firewall.sh` asserts both directions.

Two details worth knowing before touching this:

- The plans live in a **non-default output group** (`doctest_plans`), so building a site never produces them. The macro consumes that group through a `filegroup` — this is the whole interface between the two files, and the reason neither needs the document list.
- Granularity is the library, not the document: a code edit in one document re-runs its library's other documents, and they all share one interpreter. **Split the library** to narrow that — `examples/BUILD.bazel` keeps `doctests.rst` in its own `doctest_docs` library for exactly this reason.

`scripts/doctest_runner.py` drives CPython's stdlib `doctest` rather than reimplementing its comparison semantics. Note the one non-obvious mechanism: `doctest` hard-codes `compile(..., "single", ...)`, which rejects the multi-statement code a `testcode` block normally holds, so the runner replaces the `compile` that `doctest`'s module globals resolve — the same workaround Sphinx uses.

### Config vs. CLI flags (see `docs/decisions/001-template-system.md`)

`rusty_sphinx.toml` (`rusty_sphinx_renderer::config`) holds only metadata — never file paths. Template/CSS/index paths are always passed as separate CLI flags / Bazel rule attributes (`--template`, `--config`, `template =`, `css =`). This is deliberate: Bazel sandboxes relocate files, so a path baked into the config would break; keeping paths out of the config also means new metadata fields don't require CLI changes. Don't "fix" this by inlining paths into the TOML.

Templates are rendered with MiniJinja (chosen over compile-time Rust templates or reusing Sphinx's Python/Jinja2 templates directly — see the ADR for why). Changing the default template invalidates every cached page render, which is expected.

### Live preview / LSP direction

`editors/vscode/` (`src/extension.ts`, `src/bazel.ts`) is a working VS Code extension implementing the architecture described in `docs/vscode.md`: a webview panel spawns the `preview` subcommand (parse+locally-analyze+merge-into-stale-global-index+render, all in-process in `crates/worker/src/main.rs::process_preview`) on every edit (debounced) or on save, per the `rusty-sphinx.previewMode` setting. `bazel.ts`'s `BazelScanner` auto-discovers the site's config/template/index paths by `bazel query`-ing for a `rusty_sphinx_site` target and resolving its `config`/`template` labels to filesystem paths (falling back to the `rusty-sphinx.*` settings if discovery is disabled or fails), and `BazelConfigCache` invalidates that discovery when `BUILD`/`WORKSPACE`/`MODULE.bazel` files change. Not yet implemented: the background-`bazel build`-to-reconcile-global-state step from the design doc (titles/cross-refs go stale until a manual rebuild), the LSP server itself (`rusty_sphinx_lsp` crate), and PlantUML rendering in the preview. Keep this flow in mind when touching `rusty_sphinx_index::ProjectIndex::merge` or the `preview` subcommand — both exist specifically to support it.

### Spec coverage

`spec_gaps.md` tracks which RST/Sphinx features are implemented vs. missing (field lists and several block types are still unimplemented, for instance). It's a living checklist — update it when you add or fix parser/renderer coverage for a construct, and don't trust it blindly if it looks stale relative to recent commits.

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
