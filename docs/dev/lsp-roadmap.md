# Language server roadmap

The PR-by-PR path from a first squiggle to the full language server designed in
`docs/decisions/038-language-server.md`. Every step is one pull request, and
every step ends in something an author can *see* in the editor. Stopping after
any milestone still leaves a useful tool.

**End state:** a fast language server for any reStructuredText or Sphinx
project, with no setup. It understands a project's `conf.py`, or its Bazel
build exactly. It provides diagnostics identical to the build's, completion,
navigation, rename, an always-current preview, and a guided migration into
rinx.

**Checkpoints:**

- After #3, we use it ourselves.
- After #12, others can try it, from a local VSIX.
- After #19, rinx projects get exact build parity.
- After #31, it is a full language server.

**Known risks:**

- The directive and option table #25 needs does not exist yet.
- Whether rinx builds on Windows (#39) is unknown.

## Status

A finished step's heading is marked ✅ with its pull request, a partly
finished one 🔶. A heading with no marker means the step has not started.

- **Done:** #1 (PR #226), plus the testing and CI groundwork that came before #2
  (PRs #238–#248, listed at the end of M0), #2 (PR #253), and #3 (PR #255).
- **Done early:** most of #12, which is the packaging, the CI artifact and the
  installed-VSIX test.
- **Next: #4, Workspace scan + per-document index.**

The pull request that finishes a step also updates this section and the
step's marker.

## Trying it without publishing

The extension stays off the Marketplace and Open VSX until #39. Until then it
is used in one of three ways, none of which uploads anything:

1. **Development host.** Open `editors/vscode` in VS Code and press F5. Point
   `rinx.binaryPath` at `target/debug/rinx` (or `target/release/rinx` for a
   large corpus). The loop is `cargo build`, then reload the window.
2. **Local VSIX.** Run `npm run package` in `editors/vscode`, then
   `code --install-extension rinx.vsix`.
3. **Sharing with a few people.** Give them the VSIX as a CI artifact (every
   CI run uploads one, as `rinx-vsix`) or
   GitHub release asset, installed via "Install from VSIX…", together with a
   binary from `cargo install --git …`.

## What "Tests" means in each step

Every step's "Tests" item includes the following:

- **Handler tests** in `rinx_lsp`.
- **A stdio test** (`crates/worker/tests/integration/lsp.rs`) when the step changes the protocol surface.
- **An end-to-end test** through real VS Code (`editors/vscode/src/test/e2e/`) when the step is something an author can see.
- **Parity work**: narrowing the parity test's relaxations (`lsp_parity.rs`) when the step closes a gap it names.

CI runs all of these on every pull request.

---

## M0: First contact (single file, no project knowledge)

### 1. `rinx lsp` skeleton + language client ✅ (#226)

**You experience:** type `.. foo::` and it is underlined at once, with the
message and code.

- New `rinx_lsp` crate (`lsp-server`, `lsp-types`), plus `CARGO_BAZEL_REPIN`
- `lsp` subcommand in the worker's dispatcher
- Main loop: initialize/shutdown, didOpen/didChange/didClose, in-memory document store
- Convert `Diagnostic`s to LSP: 1-based char columns → 0-based UTF-16
- Extension: `vscode-languageclient`, reusing the preview's binary lookup, plus an output channel
- Extension: activate on `onLanguage:restructuredtext`, not only on the preview command
- Tests: position conversion, handlers over an in-memory connection

### 2. Diagnostic polish ✅ (#253)

**You experience:** hovering a squiggle shows `directive.unknown` with a docs
link, and a `.. noqa:` makes it disappear.

The severity table this step first listed was dropped: every diagnostic stays a
*Warning*, as the build prints it.

- Docs page listing all codes, generated from `DiagnosticCode::ALL`, as the `codeDescription` target
- Lift `retain_reportable*` from the worker into a shared crate and call it from both front ends
- Tests, including tightening the parity test (`crates/worker/tests/integration/lsp_parity.rs`) from "the editor reports a superset" to equality for documents holding a `.. noqa:`

### 3. Diagnostics inside included fragments ✅ (#255)

**You experience:** a mistake in an included file is underlined in *that*
file, and editing the fragment updates the document that includes it.

While an open document includes a file, the file shows what its includers
found in it, not its own standalone parse. Only open documents count as
includers until #4 scans the workspace. Each `.. include::` that brings a
problem in also carries an *Information* summary on its own line, linking to
each problem through `relatedInformation`; it is the editor's alone, since the
build's warning already names both files. The step also fixed two positions
in the build as well as the editor: `.. include::` with a selection
(`:start-after:`, `:start-line:`) counted fragment lines from the selection
rather than the file, and the directive's own diagnostics landed on the line
below it.

- Overlay `ParseFileLoader`: open buffers first, disk second
- Map `FileId` → URI via `source_files`, publish per URI, clear stale URIs
- Track which documents include which fragments; re-parse the includer when a fragment changes
- Tests, including comparing the build's warnings about included fragments in the parity test, which skips them today

### Done between #1 and #2: testing and CI groundwork ✅

This was not a numbered step. It built the tests that "What Tests means" above
expects from every step:

- #238: `rinx lsp` run as a process over stdio, exiting 1 unless shut down
- #241: property tests for the server, which found and fixed two bugs
- #242: the parity test (`lsp_parity.rs`), checking the editor reports what the build reports
- #243: coverage floors and fuzzing for the parser and the server
- #245: extension tests with test-cli, oxlint and a VS Code version matrix
- #247: end-to-end tests driving the real binary from VS Code
- #248: VSIX packaging, and the e2e suite run against the installed VSIX (most of #12)

---

## M1: Workspace awareness (no configuration yet)

### 4. Workspace scan + per-document index

**You experience:** the status bar shows `rinx: 512 docs indexed (0.8 s)`.

- Discover sources under the workspace folders
- Parallel parse and analysis (rayon)
- `BTreeMap<DocPath, DocumentAnalysis>` folded through the existing global phases (refactor `build_project_index` to accept it)
- Re-analyse one document per edit
- `$/progress` plus a custom `rinx/status` notification → status bar item
- Test: the folded index equals `build_project_index` on the examples

### 5. `:ref:` and `:doc:` completion

**You experience:** typing `` :ref:` `` lists every label with its title.

- Work out the role context at the cursor from the line text
- Completion items from `targets`/`target_titles` and `documents`/`document_titles`
- `:doc:` names relative to the current document, plus the `/`-absolute form
- `Title <target>` form
- Tests

### 6. Broken-reference diagnostics

**You experience:** a typo in a `:ref:` is underlined, and adding the label in
another file clears it.

- Render each open document against the effective index, without a template, keeping only the problems
- Apply `.. noqa:` filtering
- Debounce, and cancel stale renders
- Re-diagnose open documents when the index changes
- Tests

### 7. Hover on references

**You experience:** hovering a reference shows the resolved title and file.

- `node_at(document, position)`: walk the inline nodes by span
- Reuse the renderer's resolution for the title and target
- Markdown hover content
- Tests

### 8. Go to definition (file level)

**You experience:** F12 opens the target document.

- `node_at` + resolution → document path → URI
- Cover `:doc:`, `:ref:`, `:numref:`, domain roles, `:any:`
- Tests

### 9. File-system watching

**You experience:** renaming a file in the Explorer immediately breaks its
`:doc:` references.

- Register `didChangeWatchedFiles` for sources and include fragments
- Add, remove or re-analyse documents
- Re-diagnose affected open documents
- Tests

---

## M2: Plain Sphinx projects (legacy mode)

### 10. Static `conf.py` reader

**You experience:** the status bar shows `Sphinx project (docs/conf.py)`,
excluded files are not indexed, and the default role works.

- Small Python-literal reader (tokenizer + literal parser), a leaf crate in the manner of `rinx_cdecl`
- Extract settings and report the ones it couldn't read
- `ProjectModel` + `ProjectSource::SphinxConf`
- Route each file to the nearest `conf.py`; multi-root workspaces
- Apply `root_doc`, `source_suffix`, `exclude_patterns`, `default_role`, `primary_domain`
- Tests, including CPython's `conf.py` as a fixture

### 11. Extension profiles + strictness filter

**You experience:** CPython's `Doc/` is quiet, and `automodule` is
*Information*, not an error.

- Extension profile TOML + loader (`include_str!`)
- Profiles for the common extensions: autodoc, autosummary, napoleon, todo, viewcode, graphviz, sphinx-design, copybutton, …
- Filter keyed on code, directive or role name, and domain
- One-time notice naming the extensions it couldn't model
- Regression test: CPython diagnostic count stays under a threshold

### 12. Local packaging 🔶 partly done (#248)

**You experience:** `npm run package` produces a VSIX that installs into a
normal VS Code and works. Nothing is published.

- ~~`package` script running `vsce package`, plus a `.vscodeignore`~~ (done early, with an esbuild bundle, in the CI testing work)
- Optionally bundle the binary for the packaging machine's platform
- Binary lookup order: setting → bundled → Bazel
- ~~CI job uploading the VSIX as a workflow artifact~~ (done: `VS Code / package`, artifact `rinx-vsix`)
- ~~CI smoke test installing the VSIX~~ (done: `VS Code / packaged`, which runs the e2e suite against the installed VSIX on Linux and macOS; Windows joins with #39)

### 13. Intersphinx

**You experience:** `` :py:class:`dict` `` resolves, and hover shows the
external URL.

- Extract `intersphinx_mapping` (tuples, `None` → the default `objects.inv`)
- Consent prompt; the extension does the fetching, so the server stays off the network
- Cache in `globalStorageUri` with a time-to-live
- Feed into `ExternalInventory`
- Tests

### 14. `rst_prolog` / `rst_epilog`

**You experience:** substitutions from `conf.py` no longer show as undefined.

- Parser: prepend/append text with correct line mapping
- Build-side support too (library attribute), plus `compatibility.rst` and an example
- Server reads them from `conf.py`
- Tests

### 15. sphinx-needs schema in Rust

**You experience:** `.. req::` is recognised, and need ids complete and
resolve.

- Port `needs_schema.py`'s `convert` into `rinx_entity`
- Read `ubproject.toml` and literal `needs_*` settings
- CLI subcommand; the script calls it
- Port its Python tests
- Entity-id completion source

### 16. Executing `conf.py` in a trusted workspace (opt-in)

**You experience:** projects with computed settings get exact values.

- Workspace Trust gate
- Interpreter discovery (Python extension API, or a setting)
- Dump script: run `conf.py` with Sphinx-like globals (`tags`), print whitelisted names as JSON
- Merge over the static values; report errors
- Tests

### 17. Other editors

**You experience:** `rinx lsp` works in Neovim, Helix and Emacs.

- `docs/editors.rst` with configuration snippets
- Toctree entry
- Check by hand in Neovim

---

## M3: rinx projects (Bazel)

### 18. `lsp_manifest` output group + Bazel source

**You experience:** in this repository, entity directives and each library's
settings work, and the status bar shows `rinx project //docs:site`.

- Output group in `rules/site.bzl` collecting library attributes via `RinxInfo`
- Versioned JSON format + Rust type
- Extension builds or locates the manifest and watches `BUILD` files
- `ProjectSource::Bazel` with `ParseInputs` per library (schema, default role, domain, Jinja, `parse_data`)
- Shell test for the rule; loader unit tests

### 19. Strict mode

**You experience:** a toctree entry missing from `deps`, or an undeclared
image or download, is underlined exactly as the build fails.

- `Strictness::Rinx`
- `validator.rs`'s allowed-documents check per library
- Refactor the `validate_assets` checks into a pure function, then reuse it
- Tests

### 20. Jinja-templated documents

**You experience:** `jinja = True` documents get diagnostics at the right
source positions.

- Enable Jinja per library from the manifest, with `jinja_context`
- Templates loaded through the overlay loader
- Position tests through `TemplateMap`

### 21. Quick fix: add the missing dep

**You experience:** one click adds the library to `deps` in `BUILD.bazel`.

- Find the target library's label
- Locate the `rinx_library` call and its `deps` (text edit vs. buildozer: decide in the PR)
- `WorkspaceEdit`
- Tests

---

## M4: Preview served by the language server

### 22. A `rinx/preview` request

**You experience:** the preview is faster and never stale; a title changed in
another file shows up immediately.

- Custom request rendering with the model's template and config
- Webview switches from spawning a process to the request
- Decide whether the `preview` subcommand stays for other callers
- Latency measurement

### 23. Preview in legacy projects

**You experience:** the preview works in a Sphinx project with no Bazel.

- Bundled default template and CSS for `SphinxConf` models
- Images resolved against the workspace (`localResourceRoots`)
- Manual check on CPython

### 24. Diagram template errors

**You experience:** mistakes in `.. uml::` and `.. entity-flow::` are
underlined, and no JVM runs.

- Call `rinx_uml::expand`/`build_flow`/`build_sequence` in the server's render path
- Map the errors to diagnostics
- Preview shows the diagram source in place of the picture
- Optional: PlantUML if a jar is configured

---

## M5: Editing comfort and precise navigation

### 25. Directive and option completion

**You experience:** `.. ` lists the directives, and `:` inside one lists its
options.

- **Risk:** options are not declared centrally today, so the parser needs a declarative directive table (names, aliases, options)
- Entity schema directives and options
- Context detection: line start, indented option block
- Snippets with argument placeholders
- Tests

### 26. Role and domain-object completion

**You experience:** `:py:func:` completes the indexed functions.

- Expose the role table behind `find_role_kind`
- List objects by object type from the index
- Rank by the current scope (`py:module`/`py:class`)
- Tests

### 27. Entity hover + id completion

**You experience:** hovering an entity reference shows its attributes and
relations.

- Entity record → markdown (attributes, relations, back-links)
- Completion in relation options and entity roles
- Tests

### 28. Definition spans + outline

**You experience:** the Outline view and breadcrumbs show sections.

- ADR: definition spans vs. the Bazel cache
- `span` on `Heading`, `Target`, domain objects, entities and directives
- Spans kept out of the `ProjectIndex` Bazel writes (server-side position map)
- Test: a pure line shift leaves the index bytes unchanged
- `documentSymbol` handler
- Tests

### 29. Precise go to definition

**You experience:** F12 jumps to the label's line or the object's directive.

- Server-side `DefinitionMap`: target → (URI, range)
- Upgrade the definition handler
- Tests

### 30. Find all references

**You experience:** Shift+F12 lists every use of a label.

- Reverse index (target → reference spans), built during analysis
- Incremental update
- `references` handler
- Tests

### 31. Rename

**You experience:** F2 renames a label or entity id across the workspace.

- `prepareRename` + `rename`
- Rewrite the definition, all reference forms (explicit titles) and entity relation options
- Refuse external or generated targets
- Tests

### 32. Workspace symbols

**You experience:** Ctrl+T searches sections, labels, objects and entities.

- `workspace/symbol` with fuzzy matching
- Tests

### 33. Folding + semantic tokens

**You experience:** sections and directives fold, and roles and references are
coloured by kind.

- Folding ranges from section and directive spans
- Token legend (role, reference, directive, option, substitution)
- Tokens emitted from spans plus light lexing
- Tests

### 34. Preview scroll sync

**You experience:** the preview follows the cursor, and clicking in it jumps
to the source.

- `data-source-line` attributes behind a renderer flag, so build output is unchanged
- Two-way webview ↔ editor messages
- Tests

### 35. Quick fix: add `.. noqa:`

**You experience:** any squiggle can be suppressed with one click.

- Code action inserting the suppression at the right place under the noqa scoping rules
- Tests

---

## M6: Migration and scale

### 36. "rinx: Analyse project"

**You experience:** a report of unsupported constructs, with counts and links
to the compatibility page.

- Port `benchmark.py`'s `AstTally` to Rust
- Command + report view
- Anchors in `compatibility.rst` to link to
- Tests

### 37. `rinx init --from-sphinx`

**You experience:** one click writes `rinx.toml` and `BUILD.bazel`, and the
server switches to rinx mode.

- `init` subcommand: `rinx.toml`, `BUILD.bazel`, `MODULE.bazel` snippet, `entities.toml`
- `benchmark.py` calls it instead of its own generator
- Extension command + reload into Bazel mode
- Tests

### 38. Performance pass

**You experience:** edits stay under ~50 ms on the CPython docs.

- Benchmark harness: scripted edits on the CPython corpus
- Cancellation through the render
- Incremental global phases (e.g. skip numbering when no toctree changed)
- Evaluate salsa
- Timing report in CI

---

## M7: Publishing (only when we decide to)

### 39. Marketplace and Open VSX

**You experience:** the extension installs from the Marketplace, with no
binary to set up.

- CI matrix of release binaries (Linux/macOS × x64/arm64, Windows x64; check first that rinx builds on Windows)
- Platform-specific VSIX (`vsce package --target`)
- Release workflow publishing to the Marketplace and Open VSX
- Publisher account and extension listing (README, icon, changelog)
