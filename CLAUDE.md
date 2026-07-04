# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`rusty-sphinx` is a Rust re-implementation of (a subset of) the Sphinx documentation generator, designed to be fast and to integrate natively with Bazel as a first-class, cache-friendly build step (not a wrapped external tool). It parses reStructuredText (`.rst`) into HTML documentation sites, with cross-file references, toctree-based navigation, and PlantUML diagram rendering.

Read `requirements.md` and `architecture.md` for the full design rationale — the paragraphs below only cover what differs from those aspirational docs, or what's needed to be productive immediately. Note `architecture.md` describes a future multi-crate split (`rusty_sphinx_parser`, `rusty_sphinx_ast`, etc.); the current implementation is a single crate (`src/`) with modules of the same names — treat the multi-crate framing as directional, not current state.

## Commands

Development uses plain Cargo; the Bazel build is the production/integration path and wraps the same binary.

```bash
cargo build
cargo test                       # unit tests (co-located #[cfg(test)] modules) + tests/integration_test.rs
cargo test <substring>           # run a single test by name substring
cargo test --test integration_test
cargo clippy --tests             # must be warning-free before finishing any change (pedantic lints are on, see Cargo.toml)
cargo fmt

bazel build //:rusty_sphinx_worker    # build the CLI binary via Bazel
bazel build //examples:site           # build the example multi-team site end-to-end
bazel build //examples/team_a:docs    # build one team's library in isolation
bash tests/test_strict_deps.sh        # verifies Bazel fails the build when a toctree dep is missing from BUILD.bazel
bazel run //scripts:benchmark         # clone CPython docs and benchmark the pipeline against it (see docs/benchmark.md)
```

There are no `rust_test` Bazel targets — tests are run through Cargo only.

After any code change: run `cargo clippy --tests` (fix all warnings, don't `#[allow(...)]` them — treat them as refactor signals) and `cargo fmt`, then verify `bazel build //examples:site` still succeeds.

## Architecture

### Pipeline: parse → analyze/index → render

The binary (`src/main.rs`) is a multi-subcommand CLI, one subcommand per pipeline phase, because Bazel needs each phase as a separately cacheable action:

- `parse --input file.rst --output file.ast` — RST text to a serialized `ast::Document` (JSON).
- `validate_toctree --input file.ast --output file.ast [--allowed <doc>...]` — checks toctree entries only reference declared docs.
- `extract_diagrams` / PlantUML compile / `validate_images` — pulls `Directive::PlantUml` content out of the AST, compiles it to SVG via the `plantuml` jar, and later checks every referenced diagram has a matching SVG.
- `index --inputs a.ast b.ast ... --output project.index` — merges all documents' local analysis (`analyzer::analyze`) into one global `analyzer::ProjectIndex` (targets, document titles, nav tree, glossary terms).
- `render --input file.ast --index project.index --doc-path rel/path --output file.html --config site.toml --template layout.html` — turns one AST + the global index into a final HTML page.
- `preview` — collapses parse+local-analyze+merge+render into one process reading RST from stdin, for low-latency editor use (see "Live preview" below).
- Legacy: `rusty-sphinx <file.rst>` prints HTML straight to stdout (used by `process_rst()` in `src/lib.rs`, mostly for quick manual checks).

Each subcommand handler in `main.rs` is split into a pure `process_*` function (testable without file I/O) and a thin `cmd_*` wrapper that does the file reads/writes — keep new subcommands following that split.

### Module layout (`src/`)

- `ast.rs` — AST node types. Note `HashedContent` and `TargetName`: both are "parse, don't validate" opaque types (smart constructors + custom `Deserialize` that re-validates the invariant on load, e.g. `HashedContent`'s stored hash must match `sha256(body)`). Follow this pattern for new invariants rather than validating ad hoc at call sites.
- `parser/` — one file per construct (`headings.rs`, `blocks.rs`, `bullet_list.rs`, `directives.rs`, `admonitions.rs`, `glossary.rs`, `inline.rs`); `parser::parse()` is the entry point. The parser is meant to be error-resilient (bad input becomes an error/unknown node, not a panic/abort) to support live preview over incomplete documents.
- `analyzer.rs` — builds the per-document and project-wide index: cross-reference targets, document titles, the toctree-derived nav tree, glossary terms. `ProjectIndex::merge` is what lets the `preview` subcommand and the VS Code extension combine a fresh local analysis with a stale global index.
- `renderer/` — AST + `ProjectIndex` to HTML. `mod.rs` renders body content; `page.rs` wraps it in the MiniJinja-templated page chrome (nav sidebar, CSS link); `nav.rs` builds sidebar markup from the nav tree; `directives.rs`/`inline.rs` handle directive- and inline-markup-specific rendering.
- `config.rs` — `SiteConfig`, deserialized from `rusty_sphinx.toml`. Deliberately holds only metadata (project name/version), never file paths — see "Config vs CLI flags" below.
- `validator.rs` — toctree/allowed-docs validation used by the `validate_toctree` subcommand.

### Bazel rule pair: library vs. site (`rules/library.bzl`, `rules/site.bzl`, `defs.bzl`)

Mirrors `cc_library`/`cc_binary`: `rusty_sphinx_library` runs Phase 1 (parse, toctree-validate, extract/compile PlantUML diagrams) per `.rst` file and exposes a `RustySphinxInfo` provider carrying `.ast` files + SVG dirs. `rusty_sphinx_site` collects everything transitively from `deps`, runs the single Phase 2 index action, then one Phase 3 render action per `.ast` file, bundles images, and copies CSS.

Two independent dependency mechanisms, don't confuse them:
- **`deps` on `rusty_sphinx_library`** — strict, DAG-enforced, required *only* for docs pulled in via `.. toctree::`. This is what `validate_toctree` checks against (`--allowed`).
- **Cross-references / hyperlinks in text** — not declared in `deps` at all; stored as symbolic markers in the Phase-1 AST and resolved later, globally, during the Phase 2 index step. This is why cyclic hyperlinks between docs are fine but cyclic toctrees are not.

See `examples/BUILD.bazel` + `examples/team_a`, `examples/team_b` for a concrete two-team library/site setup, and `tests/test_strict_deps.sh` for how the strict-toctree-dep enforcement is tested (it mutates `examples/BUILD.bazel` temporarily to prove a missing dep fails the build).

### Config vs. CLI flags (see `docs/decisions/001-template-system.md`)

`rusty_sphinx.toml` (`config.rs`) holds only metadata — never file paths. Template/CSS/index paths are always passed as separate CLI flags / Bazel rule attributes (`--template`, `--config`, `template =`, `css =`). This is deliberate: Bazel sandboxes relocate files, so a path baked into the config would break; keeping paths out of the config also means new metadata fields don't require CLI changes. Don't "fix" this by inlining paths into the TOML.

Templates are rendered with MiniJinja (chosen over compile-time Rust templates or reusing Sphinx's Python/Jinja2 templates directly — see the ADR for why). Changing the default template invalidates every cached page render, which is expected.

### Live preview / LSP direction

`docs/vscode.md` describes the intended VS Code extension architecture (not yet implemented under `editors/vscode/`, which currently only has scaffold `package.json`/`tsconfig.json`): parse+locally-index+merge-into-stale-global-index+render on every keystroke via the `preview` subcommand, with a background `bazel build` to reconcile global state (titles, cross-refs) once it drifts. Keep this flow in mind when touching `analyzer::ProjectIndex::merge` or the `preview` subcommand — both exist specifically to support it.

### Spec coverage

`spec_gaps.md` tracks which RST/Sphinx features are implemented vs. missing (bold/italic inline markup and several block types are still unimplemented, for instance). It's a living checklist — update it when you add or fix parser/renderer coverage for a construct, and don't trust it blindly if it looks stale relative to recent commits.

## Working conventions

These come from `guidelines.md`, which is actively maintained (an agent rule auto-appends new learned preferences there) — check it for the current, authoritative list; highlights that shape how code here is structured:

- Enforce invariants at the type level with opaque types + smart constructors ("parse, don't validate") rather than validating at call sites.
- All functions, including private/helper ones, should have unit tests; new functions introduced during refactoring need tests too.
- Tests follow Given-When-Then (see the inline `#[cfg(test)]` modules throughout `src/` for the expected shape/comment style).
- Name functions after what they concretely do (`build_project_index()`, not `analyze_many()`).
- New `.rst` files added under `examples/` must be added to the corresponding `BUILD.bazel` `srcs`, and new features should get an example added to the example project covering all variants.
- Don't commit unless explicitly asked.

### Shared rules from `.agents/rules/`

This repo also defines always-on rules for the Antigravity agent under `.agents/rules/` (each file's `trigger: always_on` frontmatter is Antigravity-specific and can be ignored — the frontmatter is just inlined as text below by Claude Code's import). Importing them here keeps both agents' behavior in sync — in particular, `learning-agent.md` is the rule that keeps `guidelines.md` above up to date, so it must load for Claude Code too:

@.agents/rules/learning-agent.md
@.agents/rules/rust-fundamentals.md
@.agents/rules/rust-testing.md
@.agents/rules/test-drive-development.md
