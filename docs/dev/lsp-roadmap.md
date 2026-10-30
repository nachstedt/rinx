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

Steps are numbered in the order they were planned; #40, added later, belongs
in M4.

**Known risks:**

- Whether rinx builds on Windows (#39) is unknown.

## Status

A finished step's heading is marked ✅ with its pull request, a partly
finished one 🔶. A heading with no marker means the step has not started.

- **Done:** #1 (PR #226), plus the testing and CI groundwork that came before #2
  (PRs #238–#248, listed at the end of M0), #2 (PR #253), #3 (PR #255),
  #4 (PR #267, prepared by #259, #261–#263 and #266), #5 (PR #269), #6 (PR #270),
  #7 (PR #273, prepared by #272), #8 (PR #275), #9 (PR #276), #10 (PR #281),
  #11 (PR #286), #12 (PR #292, most of it early in PR #248), #13 (PR #297),
  #14 (PR #300), #15 (PR #305), #16 (PR #311), #17 (PR #315), #18 (PR #317),
  #19 (PR #319), #20 (PR #324), #21 (PR #328), #22 (PR #330), #23 (PR #333),
  #40 (PR #335), #24 (PR #340) and #25 (PR #355, prepared by #343, #345,
  #348, #351 and #353), #26 (PRs #358 and #362, prepared by #357) and
  #27 (PR #365, prepared by #363), #28 (PR #374), #29 (PR #378), #30 (PR #386),
  #31 (PR #393, prepared by #387 and #390) and #32 (PR #395).
- **Next: #33, folding and semantic tokens.**

The pull request that finishes a step also updates this section and the
step's marker.

## Trying it without publishing

The extension stays off the Marketplace and Open VSX until #39. Until then it
is used in one of three ways, none of which uploads anything:

1. **Development host.** Open `editors/vscode` in VS Code and press F5. Point
   `rinx.binaryPath` at `target/debug/rinx` (or `target/release/rinx` for a
   large corpus). The loop is `cargo build`, then reload the window.
2. **Local VSIX.** Run `npm run package` in `editors/vscode`, then
   `code --install-extension rinx.vsix`. `npm run package:bundled` packages
   a release build of `rinx` into it as well, as `rinx-<platform>.vsix`,
   which then needs no binary of its own.
3. **Sharing with a few people.** Give them the VSIX as a CI artifact (every
   CI run uploads one, as `rinx-vsix`) or
   GitHub release asset, installed via "Install from VSIX…", together with a
   binary from `cargo install --git …`.

## What "Tests" means in each step

Every step's "Tests" item includes the following:

- **Handler tests** in `rinx_lsp`.
- **A stdio test** (`crates/worker/tests/integration/lsp.rs`) when the step changes the protocol surface.
- **An end-to-end test** through real VS Code (`editors/vscode/src/test/e2e/`) when the step is something an author can see.
- **An editor test** in Neovim, Emacs and Helix (`editors/run-tests.sh`) when the step changes what every client sees, rather than the VS Code extension alone.
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
found in it, not its own standalone parse. Since #4, documents that are not
open count as includers too. Each `.. include::` that brings a
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

### 4. Workspace scan + per-document index ✅ (#267)

**You experience:** the status bar shows `rinx: 512 docs indexed (0.8 s)`, and
a fragment opened on its own is checked as the documents including it read
it, open or not.

Three preparatory pull requests came first, because a fold must not depend on
the order documents arrive in: `:no-index:` on every domain object (#259),
named references resolved within their own document (#261, #262), and a
definition claimed by two documents defining nothing (#263, ADR-039).

- Discover sources under the workspace folders: every `*.rst`, skipping hidden and symlinked directories, never consulting `.gitignore`
- Parallel parse and analysis (rayon), through the same parse an open document gets
- `BTreeMap<DocPath, DocumentAnalysis>` folded through the existing global phases (`build_project_index_from_analyses`)
- Re-analyse the changed document and every document including it, open or not, on each change
- Documents that are not open count as includers of an open fragment (the remainder of #3)
- `$/progress` plus a custom `rinx/status` notification → status bar item
- Test: the folded index equals `build_project_index` on the examples and the docs, and after edits equals a fresh scan

Measured on CPython's 528 documents: the scan takes 0.45 s, a fold 27 ms.

### 5. `:ref:` and `:doc:` completion ✅ (#269)

**You experience:** typing `` :ref:` `` lists every label with its title.

- Work out the role context at the cursor from the line text
- Completion items from `targets`/`target_titles` and `documents`/`document_titles`
- `:doc:` names relative to the current document, plus the `/`-absolute form
- `Title <target>` form
- Tests

The list is marked incomplete while the scan runs, so the editor asks again.
A document outside every workspace folder gets no completion, and a role whose
text wraps onto the next line is not recognised.

### 6. Broken-reference diagnostics ✅ (#270)

**You experience:** a typo in a `:ref:` is underlined, and adding the label in
another file clears it.

- Render each open document against the effective index, without a template, keeping only the problems
- Apply `.. noqa:` filtering
- Debounce, and cancel stale renders
- Re-diagnose open documents when the index changes
- Tests

The renderer now words each finding through `Reported::message`, which the
build's warning line and the editor share. Closed documents including an open
file are rendered too, for that file. The loop stays synchronous: it renders
one document per turn after 300 ms of quiet, so there is no stale render to
cancel; cancelling *inside* a render is #38's. Until #10/#13/#18 the render
uses the default site configuration and no inventories, so intersphinx
references and `numfig` projects report what a bare site would.

Measured on CPython's 528 documents: the render tier arrives 140–230 ms after
the pause (`library/os.rst`, `library/stdtypes.rst`), fold included.

Later (issue #265): the render tier also carries what the fold found about
the document — the toctree, duplicate-definition and entity-graph findings
the build's `index` action prints — so `:implements: REQ_GHOST` is
underlined on its own line, as the build now reports it: an entity keeps the line each relation was written on, as a toctree entry does. A second parity test compares both over
a small sphinx-needs project (`crates/worker/tests/parity_project/`).

### 7. Hover on references ✅ (#273)

**You experience:** hovering a reference shows the resolved title and file.

A preparatory pull request comes first, because a domain role, `:option:`
and `:any:` resolve against the scope where they are written, and that scope
existed only inside the analyzer's and the renderer's walks, as two copies of
the same rules. `rinx_scope`'s `DocumentScopes` now computes it once per
document, in memory, and every phase asks it by node: the analyzer and the
renderer already, and hover, go-to-definition (#8) and find-references (#30)
next.

- `node_at(document, position)`: walk the inline nodes by span
- Reuse the renderer's resolution for the title and target
- Markdown hover content
- Tests

`node_at` is `rinx_lsp`'s `reference_at`. The renderer's answer is
`rinx_renderer::ReferenceResolver`: each role's writer was split into a
resolution it shares with a `*_target` function, so the hover names exactly
what the page links, and a reference drawn broken has no hover. Every
index-resolved role is covered — `:ref:`, `:doc:`, `:term:`, `:numref:`,
`:eq:`, `:any:`, `:option:`, the domain and the entity roles — and #8 reads
the same `ReferenceTarget`. The hover shows the target's title and a link to
its file; the line comes with #29. It reads the latest parse, so it does not
wait for the render tier, and resolves under the default site configuration
with no inventories until #10/#13/#18. A reference inside an `.. include::`d
fragment has no hover in the fragment yet.

### 8. Go to definition (file level) ✅ (#275)

**You experience:** F12 opens the target document.

- `node_at` + resolution → document path → URI
- Cover `:doc:`, `:ref:`, `:numref:`, domain roles, `:any:`
- Tests

The definition reads the `ReferenceTarget` hover shows, through one lookup
the two share, so every role hover covers has a definition too, and a
reference drawn broken has neither. It is the start of the target
document's file; a client announcing `linkSupport` gets a `LocationLink`
whose origin is the whole reference, so Ctrl+hover underlines the role
rather than one word of it. A reference into another site's inventory, or
to a page the build writes (`genindex`), has no definition: there is no
source to open. The limits are hover's: the default site configuration, no
inventories, and nothing inside an `.. include::`d fragment.

A `.. toctree::` entry has a definition and a hover too, asked of
`ReferenceResolver::toc_entry_targets`, which expands the entry as the
page's navigation does (`nav/expand.rs`'s `entry_documents`): the start of
the document it names, of its own document for `self`, and every match for
a `:glob:` pattern — an array, or one link per match from the entry's line.

### 9. File-system watching ✅ (#276)

**You experience:** renaming a file in the Explorer immediately breaks its
`:doc:` references.

- Register `didChangeWatchedFiles` for sources and include fragments
- Add, remove or re-analyse documents
- Re-diagnose affected open documents
- Tests

The server registers one watcher, `**/*`, and drops what touches no document
itself: a fragment may have any extension, and a directory renamed or deleted
arrives as one event naming the directory, which a `**/*.rst` watcher never
sees. An event is an edit of every document it touches — a source under a
folder (by the scan's rules), a file a document reads, or every document under
a directory created or deleted — while an open file's event is ignored, its
buffer being the truth. A document whose file is gone leaves the index, which
also fixed one that was renamed while open staying indexed after `didClose`;
one deleted while the scan ran is not brought back by its result. A batch is
parsed in parallel: touching all 528 CPython documents takes 0.5 s, the
scan's time. A client without dynamic registration for
`didChangeWatchedFiles` is not watched.

---

## M2: Plain Sphinx projects (legacy mode)

### 10. Static `conf.py` reader ✅ (#281)

**You experience:** the status bar shows `Sphinx project (docs/conf.py)`,
excluded files are not indexed, and the default role works.

- Small Python-literal reader (tokenizer + literal parser), a leaf crate in the manner of `rinx_cdecl`
- Extract settings and report the ones it couldn't read
- `ProjectModel` + `ProjectSource::SphinxConf`
- Route each file to the nearest `conf.py`; multi-root workspaces
- Apply `root_doc`, `source_suffix`, `exclude_patterns`, `default_role`, `primary_domain`
- Tests, including CPython's `conf.py` as a fixture

The reader is `rinx_pyconf`, our own, after Ruff's parser, `rustpython-parser`
and an embedded interpreter were considered (ADR-038 lists why each was
rejected). What it cannot take as written is reported in `conf.py` itself,
as `conf.*` diagnostics. A literal changed afterwards keeps the literal, so
CPython's conditional `exclude_patterns.append(...)` costs one warning rather
than its exclusions. Projects are found in the scan's own walk, which never
enters an excluded directory. Each `conf.py` is a project, with the folder
itself for what lies under none, and a file belongs to the nearest one.
Every document is named `.rst` whatever its suffix. Changing, creating or
deleting a `conf.py` finds the folder's projects again, re-scanning only the
documents that changed project. `numfig`, `numfig_secnum_depth` and
`highlight_language` are read too, since #6 left them to this step.
`exclude_patterns` match through toctree's glob translation, now a port of
Sphinx's own, because `globset` refused `**.ipynb_checkpoints`. `conf.py` is
read from the disk, so its diagnostics update on save.

Measured on CPython: the scan finds 524 documents in 0.40 s, the three
`includes/` fragments and `README.rst` excluded, and `conf.py` shows one
warning, the conditional `exclude_patterns.append` on line 119.

### 11. Extension profiles + strictness filter ✅ (#286)

**You experience:** CPython's `Doc/` is quiet, and `automodule` is
*Information*, not an error.

- Extension profile TOML + loader (`include_str!`)
- Profiles for the common extensions: autodoc, autosummary, napoleon, todo, viewcode, graphviz, sphinx-design, copybutton, …
- Filter keyed on code, directive or role name, and domain
- One-time notice naming the extensions it couldn't model
- Regression test: CPython diagnostic count stays under a threshold

The table is `crates/lsp/src/project/extension_profiles.toml`, and the filter
is `rinx_lsp`'s `Strictness`, which `conf.py`'s `extensions` decide. An
unknown directive that a declared extension provides is *Information*. A
domain reference resolving nowhere is a *Hint* when a declared extension may
define it. Everything else stays a warning. What a finding is about now
travels on the diagnostic as a `DiagnosticSubject`, so the filter never
reads a message.

An extension the table does not model is assumed to define every domain's
objects, as ADR-038 says, but it explains no directive. CPython's own
directives (`availability`, `audit-event`, …) therefore stay warnings, as do
Sphinx's that rinx lacks (`sectionauthor`, `rubric`, …). So "quiet" is a
bar rather than zero. Roles are listed but not filtered on, since the parser
reports no unknown role yet. The notice naming the unmodelled extensions is a
`window/showMessage`, sent once per `conf.py` and set of names.

The bar is measured by `rinx lsp --check <folder>`. It is the server driven
in-process: scanned, every document opened and rendered, and what it shows
printed as JSON. Over stdio, only a timeout could tell when the render tier is
done, since a render that changes nothing publishes nothing. The CPython
benchmark runs it and fails above `MAX_LSP_WARNINGS`.

Measured on CPython's 524 documents, `--check` takes 9 s. It shows 2564
warnings (1857 of them unknown directives), 2882 hints and 33 include
summaries.

### 12. Local packaging ✅ (#292, most of it early in #248)

**You experience:** `npm run package` produces a VSIX that installs into a
normal VS Code and works. Nothing is published.

- `package` script running `vsce package`, plus a `.vscodeignore` (done early, with an esbuild bundle, in the CI testing work)
- Optionally bundle the binary for the packaging machine's platform
- Binary lookup order: setting → Bazel → bundled → PATH
- CI job uploading the VSIX as a workflow artifact (done early: `VS Code / package`, artifact `rinx-vsix`)
- CI smoke test installing the VSIX (done early: `VS Code / packaged`, which runs the e2e suite against the installed VSIX on Linux and macOS; Windows joins with #39)

The bundled package is `npm run package:bundled`, a platform-specific
`rinx-<platform>.vsix` (`vsce --target`) carrying `bin/rinx`; `npm run
package` stays the platform-neutral package without one. The lookup puts
Bazel ahead of the bundled binary, against this step's first plan: a Bazel
workspace's `@rinx//:rinx` is the version its build runs, which M3's parity
needs. `VS Code / packaged` now also runs the e2e suite against a bundled
VSIX with no `rinx.binaryPath` set, on Linux and macOS. It bundles CI's debug
binary and uploads nothing, since release binaries are #39's.

### 13. Intersphinx ✅ (#297)

**You experience:** `` :py:class:`dict` `` resolves, and hover shows the
external URL.

- Extract `intersphinx_mapping` (tuples, `None` → the default `objects.inv`)
- Consent prompt; the extension does the fetching, so the server stays off the network
- Cache in `globalStorageUri` with a time-to-live
- Feed into `ExternalInventory`
- Tests

The mapping is read only when `extensions` declares `sphinx.ext.intersphinx`,
as Sphinx reads it; the address-keyed form Sphinx 8 removed is refused by
name. Each entry's sources are tried in order: a file is read by the server,
relative to `conf.py`, and read again when it changes; an address is asked of
the extension with a `rinx/fetchInventory` request, once per address and
session, and only of a client passing the `fetchInventories` initialization
option. The extension asks once per workspace, naming the hosts
(`rinx.inventories.download` decides without asking), and caches each
download for the project's `intersphinx_cache_limit` days, falling back to an
expired copy when the network fails. ADR-038 §6 said the server fetches; it
now says the extension does.

While an entry is not loaded, a broken domain reference and an
`:external+name:` naming it are Hints saying why (`DiagnosticSubject::Inventory`
is new for the latter). An entry none of whose sources could be read is
`conf.inventory-unreadable` on its line in `conf.py`; a declined download is
not. Loaded inventories join the fold whole, not pruned as the build's index
is, since the next edit may name anything. They are moved into each fold and
back out rather than copied: Python's 20 176 entries cost ~4 ms a copy against
a ~27 ms fold. Reading that inventory takes ~40 ms, once, when the download
arrives.

Not done here: a cache that expires during a long session is refreshed only
on the next start or `conf.py` change, `:ref:` and `:doc:` into an inventory
not loaded stay warnings (ADR-038 §5), and an inventory's targets do not
complete.

### 14. `rst_prolog` / `rst_epilog` ✅ (#300)

**You experience:** substitutions from `conf.py` no longer show as undefined.

- Parser: prepend/append text with correct line mapping
- Build-side support too (library attribute), plus `compatibility.rst` and an example
- Server reads them from `conf.py`
- Tests

The parser reads the prolog and the epilog as segments of their own around the
document's lines, sharing its adornment order, diagnostics and role table, so
the document's positions never move (ADR-040). A segment's spans carry a
`FileId` under its name, as an included fragment's do, so a mistake in it is
reported at its own line. The build takes them as files,
`rinx_library(rst_prolog = …, rst_epilog = …)`, so that name is a real
path. The server reads `conf.py`'s literal strings and names them
`<rst_prolog>`/`<rst_epilog>` as Sphinx does, placing a mistake in them at the
start of the document.

Not done here: CPython's `rst_epilog` is an f-string over an imported
`version`, extended by `+=` inside an `if`, so it stays `conf.unread-setting`.
23 `substitution.undefined` remain in `--check`, which now shows 2565 warnings
(issue #299).

### 15. sphinx-needs schema in Rust ✅ (#305)

**You experience:** `.. req::` is recognised, and need ids complete and
resolve.

- Port `needs_schema.py`'s `convert` into `rinx_entity`
- Read `ubproject.toml` and literal `needs_*` settings
- CLI subcommand; the script calls it
- Port its Python tests
- Entity-id completion source

The conversion is `rinx_entity::needs_conf`, and it yields the loader's own
`RawSchema`, so `load_raw_schema` checks a converted schema exactly as it
checks a written one; `needs_schema.py` and its tests are gone, ported. Its
input is one `NeedsConfig`, which a `ubproject.toml`'s `[needs]` table and a
`conf.py`'s literal `needs_*` settings both deserialize into (the latter
through a serde deserializer over `rinx_pyconf`'s values), with sphinx-needs
6's `fields`/`links` tables and the older `extra_options`/`extra_links` lists
both read. `rinx needs_schema --ubproject … --output …` writes the
`entities.toml` and prints the report as JSON; the entity benchmark runs it
with the binary its warm-up compiled, so it converts between the warm-up and
the timed build. On the demo corpus the Rust conversion matches the Python
one entry for entry.

The server converts only when `extensions` declares `sphinx_needs`, as
Sphinx reads the settings. The TOML overrides `conf.py` a setting at a time,
and a path in it is relative to it, both as sphinx-needs documents;
`needs_from_toml_table` names the table above `needs`. What could not be read
is `conf.needs-invalid` and each construct with no equivalent
`conf.needs-unconverted`, on the `needs_from_toml` line. Editing the TOML or
the `schemas.json` reloads the project. The schema reaches every parse, the
fold, the render tier, hover and definition, and a role the schema declares
(`:need:`, a type's own, `:entity:`) completes the ids it accepts. A
`/`-rooted path now resolves against the project's source root, as in the
build, which resolved import keys need.

Measured on the sphinx-needs demo: `--check` shows its need types as
directives, and `conf.py` shows the twelve constructs the benchmark's report
lists. Not done here: the demo's documents are Jinja templates, rendered by a
`source-read` handler in `conf.py`'s `setup()`, which no reading or run of
`conf.py` sees; #20 renders only a Bazel library's, and a switch for a
`conf.py` project waits on ADR-038's override (issue #320). Completion inside a
relation option is #27's.

### 16. Executing `conf.py` in a trusted workspace (opt-in) ✅ (#311)

**You experience:** projects with computed settings get exact values.

- Workspace Trust gate
- Interpreter discovery (Python extension API, or a setting)
- Dump script: run `conf.py` with Sphinx-like globals (`tags`), print whitelisted names as JSON
- Merge over the static values; report errors
- Tests

The editor grants a run and the server runs it (ADR-038 §4). For each
`conf.py` whose reading fell short — a computed or modified setting, a
wildcard import, a syntax error — the server asks a client passing the
`evaluateConf` initialization option with a `rinx/confInterpreter` request.
The extension answers only in a trusted workspace with `rinx.conf.execute` on,
naming `rinx.conf.pythonPath` or else the Python extension's interpreter, and
sends `rinx/confInterpreterChanged` when trust, a setting or the interpreter
changes. The script, `conf_dump.py`, is embedded in the binary and started as
`python -c` on a thread, with a 20 s timeout, so `rinx lsp --check <folder>
--python <exe>` runs it too. It runs `conf.py` as Sphinx's `eval_config_file`
does and prints each setting as its `repr`, which the server reads back with
`rinx_pyconf`'s literal reader. The reader's per-setting checks therefore
apply unchanged, and a value no literal spells is `conf.unrepresentable-value`.
A successful run is authoritative for every setting, so none of the reading's
`conf.*` findings is made; a failed one is `conf.execution-failed` on its line,
beside them. A `conf.py` saved since, or a project module it imported, runs
again. The status bar says `Sphinx project (docs/conf.py, run)`.

Measured on CPython's 524 documents with Sphinx 9.0.4 installed: `--check`
still takes 10 s, `conf.py` shows nothing, and the warnings fall from 2566 to
2548, `substitution.undefined` from 23 to 8. Without Sphinx and docutils the
run fails at `from pyspecific import …` on line 19 and the reading stands.

Not done here: the benchmark still checks without running `conf.py`, since its
interpreter has no Sphinx. Saving `conf.py` shows the reading's findings until
the new run arrives.

### 17. Other editors ✅ (#315)

**You experience:** `rinx lsp` works in Neovim, Helix and Emacs.

- `docs/editors.rst` with configuration snippets
- Toctree entry
- Check by hand in Neovim

Each snippet is a file under `editors/` (`neovim/rinx.lua`,
`helix/languages.toml`, `emacs/rinx-eglot.el`): the page shows it with
`.. literalinclude::`, and CI runs that same file in its editor, so the two
cannot drift. `editors/run-tests.sh <editor>` copies
`editors/test-fixtures/workspace` out of the repository and drives the editor
against a `rinx` put first on the `PATH`. The `Editors` jobs run it against
CI's binary on Neovim 0.11 and 0.12 and on Emacs 29 and 31. These check
diagnostics, hover, definition and completion through each editor's own
client. Helix has no scripting interface, so its job runs `hx --health rst`
and then a real session in a detached tmux, reading the diagnostics from
Helix's log; its hover, definition and completion stay checks by hand.

The server needed one change: a client sending only `rootUri`, with no
`workspaceFolders`, now gets its folder scanned rather than no workspace at
all. A stdio test speaks as such a minimal client: no progress, no watcher
registration, no initialization options, and `utf-8` offered first. It checks
that the server agrees on `utf-32` and asks the client for nothing it did not
announce. On Linux, Neovim announces no file watching, so the page says how to
turn it on. Downloading inventories, running `conf.py`, the preview and the
status bar remain VS Code's. The page lists the opt-in protocol extensions
another client would implement for the first two.

---

## M3: rinx projects (Bazel)

### 18. `lsp_manifest` output group + Bazel source ✅ (#317)

**You experience:** in this repository, entity directives and each library's
settings work, and the status bar shows `rinx project //docs:site`.

- Output group in `rules/site.bzl` collecting library attributes via `RinxInfo`
- Versioned JSON format + Rust type
- Extension builds or locates the manifest and watches `BUILD` files
- `ProjectSource::Bazel` with `ParseInputs` per library (schema, default role, domain, Jinja, `parse_data`)
- Shell test for the rule; loader unit tests

Each `rinx_library` encodes its attributes as JSON into `RinxInfo`, and the
site writes them, sorted by label, as `<site>.lsp_manifest.json` in Starlark
alone, with no action; the `lsp_manifest` group also holds every generated file
the manifest names, such as another site's `objects.inv`. A file is named by
its workspace path when it is a source and by its execution-root path when it
is generated. The server never runs Bazel: for a folder holding `MODULE.bazel`
or a `WORKSPACE`, it asks a client passing the `bazelManifests` initialization
option with a `rinx/bazelManifests` request, and the extension finds every
`rinx_site`, builds the group and answers; editing a `BUILD`, `.bzl` or
`MODULE.bazel` file, `rinx.site`, or the preview's choice of site sends
`rinx/bazelManifestsChanged`. `rinx lsp --check <folder> --manifest <file>`
grants manifests with no client. Each site is a project whose documents are
exactly its libraries' `srcs`, each parsed with its library's
`ParseSettings`: domain, default role, schema and import keys, prolog and
epilog read from their files (and named by them, so a mistake shows in the
prolog), and `parse_data`, outside which the loader refuses to read, open
buffers included. The site's `rinx.toml`, `domain_indices` and inventories
render its pages. A listed file belongs to its site wherever it lies; a
document two sites list, to `rinx.site`'s, else the preview's, else the first
by label. What reading a manifest finds is a `bazel.*` finding shown once in a
notification, since the manifest is no file anyone opens. The schema loader
the build and the server share moved into `rinx_parser` as
`load_entity_schema`; the worker's `ParseInputs` itself stays where it is,
since `ParseSettings` is the server's counterpart.

Measured on this repository, granting the manifests of `//docs:site`,
`//examples:site` and `//examples/intersphinx:sibling_site`: `--check` shows
200 diagnostics under `examples/` and `docs/` without them and 20 with them —
the warnings the examples show on purpose, plus two `image.embed-unavailable`
the build does not report, since the server embeds no images yet.

Not done here: `deps`, `images` and `downloads` are recorded for #19 and
#21, and `jinja` and `jinja_context` for #20.

### 19. Strict mode ✅ (#319)

**You experience:** a toctree entry missing from `deps`, or an undeclared
image or download, is underlined exactly as the build fails.

- `Strictness::Rinx`
- `validator.rs`'s allowed-documents check per library
- Refactor the `validate_assets` checks into a pure function, then reuse it
- Tests

The build's checks became typed diagnostics with positions and codes, and
the server calls the same functions. `rinx_toctree::undeclared_toctree_entries`
replaced `validator.rs` and reports `toctree.undeclared-dependency` or
`toctree.glob-matches-no-dependency` at the entry. `rinx_analyzer::undeclared_assets`
reports `image.undeclared` (new) and `download.undeclared` at the construct,
finding the roles with the typed inline walk rather than the AST's JSON. The
diagram opt-in, `disabled_diagrams`, moved there from the worker too, since
the server parsed every diagram as if `diagrams = True` was set. The build
now prints each failure as an `error: path:line:column: code: message` line.

Each library's `ParseSettings` carries its `BuildDeclarations` from the
manifest: the documents its toctrees may name, its `diagrams` flag, and the
site's bundled images and downloads, the union over every library as the
build's bundle directories are. A parse checks them and shows what fails as an
*Error* that no `.. noqa:` silences, as the build does not. A site setting
`strict_links` makes its broken references *Errors* too, which `.. noqa:`
does silence. Which findings fail the build is known by where they come from,
never by their code. `Strictness::Rinx` lowers nothing. A `:loading: embed`
image the site bundles is embeddable, which removes #18's two
`image.embed-unavailable` findings. A toctree entry naming a document no
library lists shows both the build's `toctree.undeclared-dependency` and the
fold's `toctree.missing-document`. A third parity test runs the build's
failing actions over a small Bazel fixture (`crates/worker/tests/parity_bazel/`)
and compares their errors with the editor's.

Measured on this repository with the manifests of `//docs:site`,
`//examples:site` and `//examples/intersphinx:sibling_site`: `--check` shows 18
diagnostics under `examples/` and `docs/`, all the warnings the examples show
on purpose, and no error.

Not done here (issue #318): an undeclared image with `:loading: embed` still fails the
build first in `embed_assets`, with that action's untyped message, where the
editor shows `image.undeclared`.

### 20. Jinja-templated documents ✅ (#324)

**You experience:** `jinja = True` documents get diagnostics at the right
source positions.

- Enable Jinja per library from the manifest, with `jinja_context`
- Templates loaded through the overlay loader
- Position tests through `TemplateMap`

Each library's `ParseSettings` carries its `jinja_context` when it sets
`jinja = True`, and `parse_source` renders with it, so the scan, the fold, the
render tier, hover and definition all read the rendered text. The rest was
already there. The parser's `TemplateMap` puts each finding on its written
line. A template is read through the overlay loader under the source root,
inside `parse_data` and from an open buffer first, as an `.. include::`d
fragment is, so editing it, open or on disk, re-diagnoses the documents
rendering it. A mistake inside one is shown in the template. The Bazel parity
test now also compares a templated library's warnings with `rinx parse
--jinja`'s.

Measured on this repository with the manifests of `//docs:site`,
`//examples:site` and `//examples/intersphinx:sibling_site`: `--check` still
shows the 18 warnings the examples show on purpose, now with
`examples/jinja/` rendered. Dropping `release` from its `jinja_context`
shows `jinja.undefined-value` in `_templates/page_header.rst`, where it is
written.

Not done here: a Sphinx project's Jinja, rendered by a `source-read` handler
no reading of `conf.py` sees (issue #320). `TemplateMap` maps lines, not
columns, so a position to the right of a `{{ … }}` on its line is off by the
substitution's length, in the build as in the editor; hover and definition
there can miss (issue #321). An `{% include %}` carries no summary of the
problems it brought in, as an `.. include::` does (issue #322), and a template
error underlines nothing but the start of its line (issue #323).

### 21. Quick fix: add the missing dep ✅ (#328)

**You experience:** one click adds the library to `deps` in `BUILD.bazel`.

- Find the target library's label
- Locate the `rinx_library` call and its `deps` (text edit vs. buildozer: decide in the PR)
- `WorkspaceEdit`
- Tests

A text edit, decided in ADR-041. The server reads the build file through
`rinx_pyconf`'s tokenizer and returns the change as an ordinary, undoable
`WorkspaceEdit`, so no editor needs buildozer. The undeclared toctree finding
now carries what its entry resolved to (`DiagnosticSubject::UndeclaredToctreeEntry`).

**Which library.** The site's library listing the document, or, for a glob,
each library listing a match. Since a site collects its libraries through
`deps`, a library no one depends on yet is in no manifest. That is the usual
case, so for a document entry the server then reads the build file of the
document's package and takes the `rinx_library` whose `srcs` holds the file:
literal lists, `glob(…, exclude = …)` and `+` of those. A library already
depending on the writer is refused as a cycle.

**The edit.** The writer's `name = "…"` call is found and its `deps` list
edited, in buildifier's order when the list is sorted and appended
otherwise, comments kept. A computed `deps`, a `select` or a macro gets no
fix, and an action is offered only with its edit, computed with it rather
than on resolve.

`rinx lsp --check` lists each document's fixes (`quick_fixes`), which is how
`tests/test_dependency_quick_fix.sh` checks the loop against the real build:
remove `team_b` from `examples/BUILD.bazel`, apply the fix offered, and the
file is the original and the site builds again.

Not done here:
- The edit is computed from the disk, so an unsaved buffer of the build file
  is not seen.
- The diagnostic clears once the build file is saved and the manifests
  rebuilt.
- `visibility` is not checked.
- Neovim's and Emacs' tests do not grant manifests, so the fix is tested
  there by no editor test.
- No fix yet for a toctree inside an `.. include::`d fragment (issue #326),
  for a glob whose libraries the site does not collect (issue #327), or for
  the build's other declarations: `images`, `downloads`, `diagrams` (issue
  #325).

---

## M4: Preview served by the language server

### 22. A `rinx/preview` request ✅ (#330)

**You experience:** the preview is faster and never stale; a title changed in
another file shows up immediately.

- Custom request rendering with the model's template and config
- Webview switches from spawning a process to the request
- Decide whether the `preview` subcommand stays for other callers
- Latency measurement

The page is rendered from the document's latest parse and its project's
current fold, by `rinx_renderer::render_document_page`, now the one call the
build's render action makes too; a parity test checks that the preview of a
document is the page `rinx render` writes, but for the stylesheet's address.
A `rinx_site`'s manifest already named its `template`, `css` and
`entity_templates`; the server now reads them, mapping a file of another
repository (`../rinx+/…`, as the default template is in a consuming module)
under the execution root's `external/`, and reads each file when a page is
rendered, so editing one shows on the next. Every other project, and a file
outside every workspace folder, gets the build's default template and
stylesheet, which #23 had planned to bundle. The stylesheet is embedded as a
`data:` address, since a webview reads no file it was not given; the request
takes a `resourceRoot` for #23's images, which no client sends yet.

The subcommand is gone: the extension was its only caller. With it went the
extension's own Bazel discovery (`rinx.configPath`, `rinx.templatePath`,
`rinx.indexPath` and the site picker), and the preview no longer decides
which site a shared document belongs to: `rinx.site` does, else the first by
label. The panel shows the reStructuredText document last active in an
editor, so it keeps its page while it has the focus, and any edit or save of
a reStructuredText document asks again, since any of them may change it.
Only the first page is loaded into the webview; each later one is posted to
it and swapped into the open document, which keeps the scroll position — a
reload had VS Code scroll back down, visibly under the theme's smooth
scrolling. As in VS Code's own Markdown preview, a page is loaded again only
when the panel was hidden, never because it gained or lost the focus; the
panel opens beside the editor, which keeps the focus.
Without a running server it says so.

Measured on CPython's 522 documents with release builds, from an edit to the
page (`rinx lsp --check <folder> --preview <document>`): `library/os.rst`
100 ms and `library/stdtypes.rst` 155 ms, against 140 ms and 240 ms for the
old `rinx preview` process with the built index, which was stale besides.

Not done here: images and every other file a page links beside it (#23),
links that lead somewhere (#40), and the version switcher's script, which a
webview cannot load (issue #329).

### 23. Preview in legacy projects ✅ (#333)

**You experience:** the preview works in a Sphinx project with no Bazel.

- Bundled default template and CSS for `SphinxConf` models (done early, in #22)
- Images resolved against the workspace (`localResourceRoots`, and the
  request's `resourceRoot`)
- Manual check on CPython

The extension now sends the `resourceRoot` #22 added, which is
`asWebviewUri(file:///)`, and sets the panel's `localResourceRoots` to the
workspace folders. The server links an authored image under it where the image
lies in the source tree, through a `rinx_renderer::ImageLocator` the preview
injects (`render_with_page_assets`). The renderer still reads no file and
knows no address. Only a file inside a workspace folder is linked, since the
webview reads no other. A stylesheet outside them, such as a consuming
module's default template under the execution root, stays embedded; sending
the root would otherwise have broken it. A `:loading: embed` image is linked
too, since the server reads no image's bytes. Without a `resourceRoot` the
page is the build's own, `_images/` included, which keeps the parity test
unchanged.

Checked by hand on CPython's `Doc/` over stdio: every image of
`library/pathlib.rst` and `using/windows.rst` links a file in the source tree.

Not done here: an image outside every workspace folder, or one Bazel
generates, is not shown (issue #331); an image edited on the disk may show the
webview's cached copy until the page loads again (issue #332); `:download:`
links and compiled diagrams keep their site addresses (#40, #24).

### 40. Preview navigation ✅ (#335)

Numbered last so the steps after it keep their numbers; it belongs here, after
#23.

**You experience:** clicking a link in the preview opens the document it
leads to, and the preview follows.

- A click handler in the webview posts the link's `href` to the extension
- A `rinx/previewLink` request: the server resolves the `href` against the
  current document's name to a source URI and fragment (`Project::path_of`)
- An `http(s):` link opens in the browser (`env.openExternal`), a `#fragment`
  scrolls the page; `genindex` and the domain indices have no source and are
  not followed
- Tests

The server reads an `href` as the inverse of what wrote it:
`rinx_index::relative_doc_href` for a page, and the renderer's
`relative_asset_href` for a `:download:` file, which the editor then opens.
Neither encodes a path, so `rinx_lsp`'s `preview_link.rs` decodes none. A
page leads to its document's file, `.txt` suffix and all, only when the
project's fold holds the document, which is what leaves `genindex` and the
domain indices unfollowed. A document in no project leads only to itself.
The webview scrolls to a `#fragment` itself, and the extension opens an
`http(s):` or `mailto:` link in the browser without asking the server. A
document opens in the column its source was last shown in, keeping the focus
on the panel, and the panel asks for its page at once, scrolled to the
link's anchor. A page loaded again after the panel was hidden keeps the
reader's place rather than the anchor. A test follows every link a rendered
preview writes back to a source.

Not done here: a link opens its document at the first line, not the label's,
until #28/#29 give definitions their spans. The panel keeps no history to go
back through (issue #334).

### 24. Diagram template errors ✅ (#340)

**You experience:** mistakes in `.. uml::` and `.. entity-flow::` are
underlined, the preview draws the diagrams, and no JVM runs.

- Call `rinx_uml::expand`/`build_flow`/`build_sequence` in the server's render path (there since #6)
- Map the errors to diagnostics
- Preview shows the diagram source in place of the picture (it draws the picture instead)
- Optional: PlantUML if a jar is configured (not done; see below)

The render tier had expanded every diagram since #6, so the step's first two
items needed only tests, and the parity test now runs the build's `render`
action too: `parity_project/diagrams.rst` holds one broken diagram of each
kind. What changed is where a Jinja error lands. `Uml::template_span` records
where the template starts, and a template error MiniJinja places on a line is
reported on that line of the document, in the build and the editor alike. An
error inside an imported diagram (`uml()`) names the import instead, since its
line is another template's.

The preview draws each diagram with PlantUML's own engine compiled to
JavaScript, `@plantuml/core`, pinned to the build's jar (1.2026.8) and bundled
into the extension (ADR-042; 1.7 MB in the VSIX). The server puts the
expanded text where the picture would be, under
`rinx_renderer::DiagramPictures::Source`, exactly when the request names a
`resourceRoot`, so the parity test's page is unchanged. A diagram that could
not be expanded shows the build's warning in its place. The webview draws one
text at a time and keeps each picture by its text, so typing redraws only
what changed. The Rust reimplementations were rejected: the most complete,
`plantuml-little`, matches upstream only as of 1.2026.2 and links a native
Graphviz.

A PlantUML syntax error is PlantUML's to find. The preview shows PlantUML's
error picture, naming the line, as the build's jar draws it; the problems list
does not show it (issue #336).

Not done here:
- Diagrams including PlantUML's standard library do not draw in the preview
  (issue #337).
- The preview lays diagrams out with Graphviz, where the build uses ELK
  (issue #338).
- In a `jinja = True` document, a template error is misplaced when the
  document's own Jinja splices lines into the diagram's body (issue #339).

---

## M5: Editing comfort and precise navigation

### 25. Directive and option completion ✅ (#355)

**You experience:** `.. ` lists the directives, and `:` inside one lists its
options.

- ~~**Risk:** options are not declared centrally today, so the parser needs a declarative directive table (names, aliases, options)~~ — done in four preparatory pull requests (ADR-043): every directive has a spec beside its parser, read by one reStructuredText reader, and its options are an enum generated from one table, each with its value kind and doc
- Entity schema directives and options
- Context detection: line start, indented option block
- Snippets with argument placeholders
- Tests

The server keeps no list of its own. A directive's name, its options and an
option's choice of values are read from the parser's specs:
`rinx_parser::directive_specs` and `domain_directive_specs`, and an
`EntitySpec` per type of the project's schema. So the editor offers exactly
what the parser takes. `completion/directive_context.rs` reads the cursor's
place from the lines around it, by the reader's rule: a name after a line's
`..`, an option name after the `:` opening a line of the option block (the
lines indented below the marker, up to the first blank one), a value after
`:name: `. A bare domain-object name is resolved against the project's
default domain through `rinx_parser::directive_spec`, the parser's own
lookup. An option already written in the block is not offered again, under
any of its spellings. An option this build does not implement is offered
last, struck through, with its advice. A client taking snippets gets a tab
stop for the argument and the content's first line.

`.. entity-update::` takes, besides `:strict:`, the fields of the entities
its argument matches. The server resolves the argument against the project
index with `rinx_analyzer::matched_entities`, which the build's update phase
calls too: an existing id wins, else the filter is evaluated. It then offers
the fields every matched type declares first, every type's when nothing
matches, and after `:+`/`:-` only the list fields.

Not done here:
- A figure's and a block image's `:align:` offer the three vertical values
  only an image in a substitution takes (issue #354).
- Unknown and unsupported options are still reported by `classify_options`
  per directive, rather than by the dispatcher from the spec (issue #352).

### 26. Role and domain-object completion ✅ (#358 and #362, prepared by #357)

**You experience:** `:py:func:` completes the indexed functions.

- Expose the role table behind `find_role_kind` (done in #357, ADR-044)
- List objects by object type from the index
- Rank by the current scope (`py:module`/`py:class`)
- Tests

Every role with a fixed name now has a `RoleSpec`, and its pattern is built
from it (#357), so `rinx_parser::role_specs()` lists exactly the names the
parser matches. Role names complete from it (#358): a `:` where a role may
start lists the built-in roles spelled for the default domain, the schema's
entity roles, and the roles a `.. role::` defined above the cursor. Those
come from a *probe*, a parse of the text up to the cursor's line
(`parse_prefix_with_ctx`).

The step went beyond domain objects: every role the index resolves completes
its target (#362). That covers `:ref:`, `:doc:`, `:term:`, `:numref:`,
`:eq:`, the entity roles, `:option:`, the domain roles, `:any:`, and a bare
backtick read as the default role. The targets of loaded inventories are
included, closing #13's gap. What a role is comes from the parser
(`rinx_parser::read_role`). Where it is written comes from the same probe,
with the target closed, for a role resolved in its scope.

A candidate is offered only if the page would link it, checked by the new
`ReferenceResolver::resolve_as`. It is written in its shortest spelling that
leads to the same place, so the ranking by scope falls out of the resolver
rather than a copy of its rules: a method of the current class is `open`,
and comes first. Lists are cut at 200, and a scoped role's is always
incomplete, so the editor asks again as you type.

Measured at the end of CPython's `library/stdtypes.rst`, probe included:
`:ref:` 5 ms, `:py:func:` 51 ms, `:any:` with nothing typed 61 ms.

Not done here: `:download:` targets, which are files (issue #360), and a
role wrapping onto the next line (issue #361).

### 27. Entity hover + id completion ✅ (#365, prepared by #363)

**You experience:** hovering an entity reference shows its attributes and
relations.

- Entity record → markdown (attributes, relations, back-links)
- Completion in relation options and entity roles
- Tests

What an entity shows besides its prose became one list first (#363):
`rinx_index::EntityFields`, its attributes under their labels in declared
order, its relations and the back-links derived onto it, each with every
`.. entity-update::` applied and marked when conflicting updates dispute it.
The renderer's built-in box draws from it, byte for byte as before, and the
hover shows it as Markdown lines, so the two list the same fields. An entity
whose id two documents declare is in no index, and both are drawn from
what they wrote; a comment claiming that fallback never ran was wrong.

A `ReferenceTarget` now names the entity it is (`entity`): an entity role's,
and a `:ref:` or `:any:` whose label is an entity's, found by its anchor since
a label's name is lowercased. Hovering one shows the entity's fields under its
title and file. The same shows on an id written in a relation option,
`.. entity-update::` fields included, and on an entity's own marker or `:id:`
line, read from the parse, so a generated id and an unsaved entity have a
hover too, without the file, where the reader already is. Lists of ids are
cut after 20.

A relation's value completes the indexed entities its `to` accepts:
`OptionValue::EntityIds` carries it, as `Declared` carries its words. An
`.. entity-update::` changing a relation offers only what every matched
type's relation accepts, since the build checks each updated entity by its
own type. The ids already listed and the entity's own are left out, prefix
matches come first, and the list is cut at 200 and marked incomplete. Each
item, and each entity-role target, is documented with the hover's text:
5000 entities take under 1 ms per completion.

Not done here: a sphinx-needs link has no `to`, so every need is offered,
and a relation with `multiple = false` still completes as a list.

### 28. Definition spans + outline ✅ (#374)

**You experience:** the Outline view and breadcrumbs show sections.

- ADR: definition spans vs. the Bazel cache
- `span` on `Heading`, `Target`, domain objects, entities and directives
- Spans kept out of the `ProjectIndex` Bazel writes (server-side position map)
- Test: a pure line shift leaves the index bytes unchanged
- `:ref:` completion (#5) names the file a label was written in — the
  `.. include::`d fragment, not the document including it, which is all
  `targets` knows until then
- `documentSymbol` handler
- Tests

The spans live in the `.ast`, on the definitions themselves: a section's
title line, a target's line, a domain object's or a table's marker line, and
each glossary term's line (`GlossaryTerm`). The analyzer records each one where
it inserts the name into the index, under the key the index names it by
(`rinx_index::DefinitionKey`), into the `DefinitionSites` a `DocumentAnalysis`
keeps beside its index fragment. No fold reads them, and the build's `index`
action writes only the `ProjectIndex` (ADR-045). A test parses a document with
every definition moved down: the written index stays byte-identical while
every site moves. The server keeps one analysis per document, open or closed,
so a closed document's definitions have their lines too. An analysis
differing only in its sites replaces the recorded one without folding again,
so pressing Enter above a heading costs no fold. `EntityRecord::relation_spans`,
which only the fold reads, is no longer written either.

The outline reads the open document's latest parse. It shows the sections
nested by level, each spanning up to the next, and under them each domain
object, named as the index names it, a class's methods under the class, and
each entity by its id. A section or object an `.. include::` brought in is
placed on the `.. include::` line. A client that does not announce nested
symbols gets them flat, each naming its holder. `:ref:` completion names the
fragment a label was written in.

Not done here:
- The written index still carries the lines of toctree entries and entity
  updates (issue #372).
- `.. contents::` and a toctree's `:name:` have no span, so their labels have
  no site. Fixed since, by #31's name spans: such a label is sited at its name.
- Labels and glossary terms are not outline entries.

### 29. Precise go to definition ✅ (#378)

**You experience:** F12 jumps to the label's line or the object's directive.

- Server-side `DefinitionMap`: target → (URI, range) (#28's `Definitions`)
- Upgrade the definition handler
- Tests

No map was needed: #28's `Definitions` already answers where a definition
was written, by the key the index names it under. What was missing was that
key on a resolved reference, since an anchor cannot be read back into one. A
`ReferenceTarget` now names its `definition` (`rinx_index::DefinitionKey`),
set by each role's own resolution, which already held the name it looked up:
a label for `:ref:`, `:numref:` and a label `:any:` finds, a term, a domain
object of its resolved type (`:option:` as `std:cmdoption`), an equation and
an entity. A `:doc:` names none and still opens its document's first line.
An entity id written in a relation option, an `.. entity-update::`'s
included, is read as a reference to that entity, the id alone underlined,
so it has a definition too, and its hover the same line; an entity's own
directive has none, since the reader is there already.

The definition is the site's range: a label's `.. _name:` line, a term's line,
an object's or entity's directive marker, an equation's own line, in the
`.. include::`d fragment's file when it was written there. Its columns are
counted on the file's open buffer, else the disk, the text the analysis read,
so an unsaved edit above a label moves the jump with it. A client taking
links gets the site as both target ranges. Hover's file link carries the same
line, as a `#L<line>` fragment, as #7 promised.

Not done here:
- A label written by `.. contents::` or a toctree's `:name:` has no site
  (#28), so it opens its document's first line (issue #376, fixed since by
  #31's name spans).
- A link followed in the preview still opens its document at the first line
  (#40; issue #375).
- Hover's link opens an included fragment but names the including document
  (issue #377).

### 30. Find all references ✅ (#386)

**You experience:** Shift+F12 lists every use of a label.

- Reverse index (target → reference spans), built during analysis
- Incremental update
- `references` handler
- Tests

A use's target depends on the whole project: a label two documents define
leads nowhere, and a domain role searches every scope. So the analysis
records only what each document *wrote*, and the server resolves it against
the current fold the way the page resolves it. That covers every reference
hover covers, every `.. toctree::` entry (`rinx_renderer::written_uses`, each
scoped reference with the scope it was written in), and each entity id written
in a relation option, an `.. entity-update::`'s included. The renderer's new
`UseResolver` is a `ReferenceResolver` that holds no page. It resolves a use
with the scope the use carries, so a closed document needs no AST. The
analysis keeps these uses in `IndexedDocument`, beside the definition sites,
and the `.index` never sees them.

The reverse index is `Project::uses_of`. It keeps what each document's uses
resolved to until the fold changes, and drops one document's entry when only
that document's analysis changes. Adding a reference to prose therefore
re-resolves one document, and only an edit to a definition re-resolves every
document. It fills on the first request, in parallel. A use counts when its
resolution names the same definition key in the same document (`UseTarget`),
so a `:py:func:` in another class naming a method of the same name is not
listed, and a reference drawn broken is nobody's use. A `:doc:` or a toctree
entry is a use of its document.

The request answers on a reference, a relation id, a toctree entry, or a
definition's own site: a label's line, a term, an object's or entity's
marker. It also answers on the option line naming a definition, wherever the
cursor is on it: a directive's `:name:`, a `.. math::`'s `:label:`, an
entity's `:id:`. The option is read with completion's `directive_at` and
kept only when the index has this document defining the name, so a
`.. contents::` or a toctree `:name:`, which has no site, finds its uses too. On an entity's marker the entity wins over its label, since every
reference names the entity. `includeDeclaration` adds the definition's site.
Uses come back ordered by file and position, each listed once, in the
fragment an `.. include::` brought them from. A relation id is underlined
alone. `rinx lsp --check <folder> --references <document>:<line>:<column>`
times a request cold, warm, and after an edit to the document it is asked
from.

Later, preparing #31: a named reference (`install_`, `` `text <install_>`_ ``,
an alias target's `.. _docs: install_`) is a use of the label its page
defines, followed through the aliases the page writes, and an
`.. entity-update::` whose argument names an entity is a use of it, its id
alone underlined.

Measured on CPython's 524 documents with a release build: the 132 uses of
`dict` (`library/stdtypes.rst:5021`) take 49 ms cold, 11 ms warm and 12 ms
after an edit; a label with six uses takes 2 ms warm, 3 ms after an edit.

Not done here:
- A label written by `.. contents::` or a toctree's `:name:` has no site
  (#28), so its declaration is not listed (issue #376, fixed since by #31's
  name spans).
- A code block's `:name:` label is sited on its first content line, so that
  is the declaration listed, and its marker line finds nothing (issue #385).
- A use in a `conf.py` project's `rst_prolog` or `rst_epilog` is in no file,
  so it is not listed (issue #384).
- An edit to a definition re-resolves every document's uses on the next
  request (#38).

### 31. Rename ✅ (#393, prepared by #387 and #390)

**You experience:** F2 renames a label or entity id across the workspace.

- `prepareRename` + `rename`
- Rewrite the definition, all reference forms (explicit titles) and entity relation options
- Refuse external or generated targets
- Tests

Two preparatory pull requests gave the server what a rewrite needs. In #387
the parser records where each name was written, as `rinx_ast::Written<T>`: a
label after `.. _`, every `:name:`, a `.. math::` `:label:`, an entity's
`:id:`. The span is never serialized, so the `.ast` and `.index` are unchanged,
and an entity id has one only when written as `:id:`. A toctree's or a
`.. contents::`' `:name:` is sited at its name, closing #376. In #390 a named
reference (`install_`, `` `text <install_>`_ ``, an alias target) became a use
of the label its page defines, and an `.. entity-update::` naming an entity a
use of it; Shift+F12 lists both.

Rename asks what the cursor names as find-references does, and rewrites the
definition's name and every use `Project::uses_of` lists. Labels, entities and
equations are renamed (ADR-046). Each place is narrowed to the name's
characters by how it was written (`UseForm`): a role's target inside its
backquotes after any explicit title (`rinx_parser::role_target_range`, the
parser's own split), an id on a relation line, a named reference's name, an
exact name. Every place is then read, from the open buffer or the disk, and
must still name the definition, or the whole rename is refused, naming the file
and line. So a stale disk, a Jinja line or an escape never yields a partial
rename. A named reference's text is its name, so it changes on the page too; a
bare `install_` gains backquotes when the new name needs them, and a reference
through an alias keeps the alias. A `:ref:` to an entity's label now names the
entity, for find-references too, so both are rewritten.

The refusals are errors the editor shows: a document (rename its file), a
section, a term or a domain object; a name two documents define; an id not
written as `:id:`; a new name already defined, not an id, or holding what a
label cannot; a new id the type's `pattern` refuses; a use in `conf.py`'s
`rst_prolog` or `rst_epilog`; a place outside every workspace folder. A client
announcing `prepareSupport` gets the refusal before typing a name, and one
taking `documentChanges` gets each open document's edit versioned.
`rinx lsp --check <folder> --rename <document>:<line>:<column>=<name>` prints
the edit and times it.

Measured on CPython's 556 documents with a release build: renaming
`types-genericalias` (`builtins/stdtypes.rst:6170`) writes 55 edits in 12
files in 59 ms, 15 ms again with every document's uses resolved; renaming
`debug-build`, 52 edits in 25 files, 6 ms.

Not done here:
- Ids inside a `:filter:`, a diagram template (`ref()`, `flow()`), a
  `needs.json` or a `jinja_context` are not renamed (issue #391), and nor is
  another site linking in through its `objects.inv`.
- A reference inside a directive the build does not know is not read, so not
  renamed: the 56th occurrence of `types-genericalias` is in a
  `.. describe::` body.
- The cursor on a named reference itself names nothing yet (#388), and an
  image's `:target: name_` is no use (#389).
- Terms, domain objects and documents are refused (issue #392 for terms,
  and for documents through their files).

### 32. Workspace symbols ✅ (#395)

**You experience:** Ctrl+T searches sections, labels, objects and entities.

- `workspace/symbol` with fuzzy matching
- Tests

The list is #28's definition sites, named. Each document's analysis now
lends its own index fragment (`DocumentAnalysis::index_fragment`), and
`IndexedDocument` builds its `DocumentSymbols` beside its uses, once per
analysis and in parallel during the scan: a section by its title, a label,
a glossary term as written, an equation, a domain object as its definition spelled it
(`pkg.Box.open`), an entity by its id. An entity, which is a label too, is
listed once. Reading the document's own fragment rather than the fold keeps
a label two documents define (ADR-039), listed under both, and needs no fold
to answer. A definition an `.. include::` brought in is placed in its
fragment; one a `conf.py`'s `rst_prolog` wrote, in no file, is left out.

A request matches the names of every project with frizbee, case-insensitively
and without typos, since VS Code filters what it shows with its own matcher
and would hide a name the query does not spell. nucleo-matcher was the first
choice and was rejected for its MPL-2.0 licence. Ties go to the shorter name,
then by document and place, so the answer never depends on the order projects
are held in. The best 200 are listed, a definition two Bazel sites index once,
each naming its document as `containerName`, in the flat
`SymbolInformation` shape every client takes. Only those are placed, reading
their files' text then. `rinx lsp --check <folder> --symbols <query>` times a
search. `handle_request`'s arms now share one parse-or-refuse helper.

Measured on CPython's 524 documents with a release build: `os.path.join`
takes 1.3 ms, `dict` 14 ms and an empty query 28 ms, most of it placing the
200 it lists.

Not done here:
- No `workspaceSymbol/resolve`: each symbol's range is computed with the
  answer.
- Labels and terms are no outline entries yet (#28).

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
