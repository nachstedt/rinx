# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`rinx` is a Rust re-implementation of (a subset of) the Sphinx documentation generator, designed to be fast and to integrate natively with Bazel as a first-class, cache-friendly build step (not a wrapped external tool). It parses reStructuredText into HTML sites with cross-file references, toctree navigation and PlantUML diagrams, and ships a language server with a VS Code extension.

The code is a Cargo workspace under `crates/`. `docs/dev/requirements.md` and `docs/dev/architecture.md` are the original, aspirational design (they predate most crates). The rationale for each feature is an ADR in `docs/decisions/` — read the relevant one before changing a feature's design. Crates with non-obvious internals have their own `CLAUDE.md` (`crates/{ast,parser,renderer,lsp,uml,entity}/`, `rules/`), loaded when working under them; read it yourself before a change that reaches into that crate from elsewhere.

## Commands

Development uses plain Cargo; the Bazel build is the production/integration path and wraps the same binary.

```bash
cargo build
cargo test --workspace            # unit tests (co-located #[cfg(test)] modules) + crates/worker/tests/integration/
cargo test <substring>
cargo clippy --workspace --tests  # must be warning-free before finishing any change (pedantic lints are on)
cargo fmt
cargo run -p rinx -- diagnostic_codes_rst > docs/diagnostics.rst  # after adding a DiagnosticCode or editing its doc comment; a test compares
cargo llvm-cov --workspace        # CI enforces coverage floors for the workspace and rinx_lsp
fuzz/run.sh parse 60              # fuzz a target (also lsp_session, pyconf); fuzz/ is its own nightly crate

bazel build //:rinx
bazel build //examples:site       # the example multi-team site end-to-end
bazel build //docs:site           # rinx's own documentation, built by rinx (strict_links = True)
bash tests/test_<mechanism>.sh    # one shell test per Bazel mechanism (strict deps, parse_data, images, diagrams, ...); run the one for what you touch
(cd e2e/smoke && bazel test //...)   # rinx consumed as a dependency — what the BCR presubmit runs
bazel run //scripts:benchmark     # CPython docs benchmark (see docs/benchmark.rst); also :benchmark_entities
CARGO_BAZEL_REPIN=1 bazel build //examples:site   # after changing a Cargo dependency

# Python scripts (scripts/, examples/shared/) — see docs/dev/python.md
uv run ruff format && uv run ruff check && uv run ty check && uv run pytest --cov
bazel test //scripts:all //tests/js:all   # the scripts' pytest files, and the theme's browser scripts (Bazel only)

# VS Code extension (editors/vscode) — npm, not Bazel
npm ci && npm run typecheck && npm run lint && npm run format:check
npm test                          # drives a real VS Code against target/debug/rinx (cargo build first)
editors/run-tests.sh neovim       # also emacs, helix: run the snippets docs/editors.rst shows
```

There are no `rust_test` Bazel targets — Rust tests run through Cargo only.

After any code change: run `cargo clippy --workspace --tests` (fix warnings, don't `#[allow(...)]` them — treat them as refactor signals) and `cargo fmt`, then verify `bazel build //examples:site`. After a Python change, run ruff, ty and pytest likewise (fix findings, don't `# noqa` them). `doctest_runner.py`, `publish_crates.py` and `publish_pages.py` must stay stdlib-only, and the former Python 3.9-compatible, since it runs in users' `py_test`s.

`Cargo.bazel.lock` (crate_universe's lockfile, separate from `Cargo.lock`) is checked in and required: without it rinx cannot be consumed as a non-root bzlmod module. Bazel asks for a repin rather than using stale crates.

## Architecture

### Pipeline: parse → index → render

The CLI (`crates/worker/src/main.rs`) has one subcommand per pipeline phase, because Bazel caches each phase as a separate action: `parse` (RST → serialized `.ast`), `validate_toctree`, `index` (all documents' analyses merged into one `ProjectIndex`), `render` (one `.ast` + the index → HTML, also writing diagram sources), PlantUML compile and `validate_assets` at the site level, plus `lsp` (the language server) and `init --from-sphinx` (migration). Each subcommand pairs a pure `process_*` function with a thin file-I/O `cmd_*` wrapper in one file under `commands/`; keep new subcommands to that split.

Dependencies between crates flow one way: from `ast` (and the leaf crates beneath it) through `entity`, `parser`, `scope`, `index` and `toctree` to `analyzer`, `uml` and `renderer`, then `lsp`, then `worker` (the `rinx` binary) — never add an edge backwards. Small leaf crates (`cdecl`, `filter`, `inventory`, `pyconf`, `template`, `toctree`) exist so that phases which may not depend on each other can share one implementation — when two phases need the same logic, lift it into such a crate rather than duplicating it.

Invariants that span crates:

- **Parse, don't validate**: invariants live in opaque types with smart constructors, and their `Deserialize` re-validates on load.
- **No I/O below the worker.** The parser reads files only through the injected `ParseFileLoader`; the renderer returns what should be written (e.g. diagram sources) and `cmd_*` writes it.
- **The `.ast` and `.index` are cache keys.** Nothing positional that changes when lines move may enter them: where text was written lives in the parser's `SourceMap`, kept beside the `Document`, never serialized. Anything written must be deterministic (iterate `BTreeMap`s), or an unchanged input misses the cache.
- **The parser is error-resilient**: bad input becomes a diagnostic plus an `Unknown`/`Malformed` node rendered as a visible error block, never a panic or silently dropped content.
- **One implementation per question.** The build and the language server must answer alike (`lsp_parity.rs` checks the server reports what `rinx parse` warns): `.. noqa:` filtering goes through `rinx_ast::retain_reportable` only, the preview renders through the build's `render_document_page`, and so on.
- **A diagnostic code names the construct, not its spelling** — `.. needtable::` and `.. entity-table::` share `entity-table.*`.

### File organization

A file is split once it grows past ~800 lines, along construct boundaries, into a same-named subdirectory (`inline.rs` + `inline/reference.rs`), not prefixed siblings. The owner then becomes a **pure forwarder**: a module doc comment, `mod` declarations and re-exports — no logic, no tests (so re-exported items need `pub(crate)`, not `pub(super)`). When a file is one cohesive unit whose bulk is tests, split only the tests into topic-named siblings and keep no inline `mod tests`. Place a module under the dispatcher that actually calls it (grep the callers), and split general-purpose helpers out under a name saying what they do rather than leaving them in a construct's file.

### Bazel rules: library vs. site (`rules/`, `defs.bzl`)

Mirrors `cc_library`/`cc_binary`: `rinx_library` parses and validates its documents and exposes a `RinxInfo` provider; `rinx_site` collects everything transitively, runs the one index action, then one render action per document. Its dependency attributes are independent mechanisms — don't confuse them: `deps` (strict, only for documents a toctree pulls in), cross-references (never declared, resolved at index time — so link cycles are fine, toctree cycles are not), `parse_data` (files the parser reads), `rst_prolog`/`rst_epilog`, `images`, `downloads`, `diagrams` and `jinja` (both opt-in), `entity_schema` (on both rules), `parse_config`, and `inventories` (on the site). `rules/CLAUDE.md` has their traps.

The recurring design is the **cache firewall**: a cheap per-document action extracts only what a costly step depends on (doctest plans, embedded images, `.puml` diagram sources), so an unrelated edit leaves its output bytes identical and the costly step stays cached. Keep new expensive steps behind one.

### Config vs. CLI flags (`docs/decisions/001-template-system.md`)

`rinx.toml` holds only metadata, never file paths: Bazel sandboxes relocate files, so paths are always CLI flags / rule attributes (`--template`, `--config`, `template =`, `css =`). Don't "fix" this by putting paths into the TOML. A value naming a language or a URL the browser fetches is fine.

### Documentation, compatibility, changelog

- `docs/` is built by rinx itself; only the guides are pages, while `docs/decisions/` and `docs/dev/` stay Markdown. A page uses only constructs rinx supports (no footnotes). A new user-facing feature belongs on `docs/syntax.rst` or the guide it extends, and in `examples/`.
- `docs/compatibility.rst` lists every construct as ✅, ✅ ℹ️ (deliberate deviations), 🔶 partial or ❌; update it when coverage changes, and don't trust it blindly if it looks stale.
- `CHANGELOG.md` follows Keep a Changelog: every user-noticeable pull request adds a short bullet to `[Unreleased]` (Added/Changed/Removed/Fixed), extending an existing bullet where it fits, with no PR numbers; refactors, CI, tests and dependency bumps add none. Breaking the Bazel rules or the CLI goes under Changed or Removed.

## Working conventions

These come from `guidelines.md`, which is actively maintained — check it for the authoritative list; highlights:

- Enforce invariants at the type level with opaque types + smart constructors rather than validating at call sites.
- All functions, including private helpers, have unit tests, written Given-When-Then like the existing `#[cfg(test)]` modules.
- Name functions after what they concretely do (`build_project_index()`, not `analyze_many()`).
- New `.rst` files under `examples/` must be added to their `BUILD.bazel` `srcs`, and new features get an example covering all variants.
- Don't commit unless explicitly asked.

### Shared rules from `.agents/rules/`

These rules are shared with the Antigravity agent (their `trigger: always_on` frontmatter is Antigravity-specific). `learning-agent.md` is what keeps `guidelines.md` up to date, so it must load here too:

@.agents/rules/learning-agent.md
@.agents/rules/test-drive-development.md
