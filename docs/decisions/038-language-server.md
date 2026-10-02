# 38. A language server, for rinx projects and plain Sphinx ones

## Status

Proposed.

## Context

`docs/dev/architecture.md` has planned a `rinx_lsp` crate from the start, and
much of the groundwork has been laid with it in mind:

- **The parser is error-resilient.** Bad input becomes a
  `Directive::Unknown`/`Malformed` node, not an abort.
- **Diagnostics are collected, not thrown.** They carry a `Span` range rather
  than a point (ADR-003), and a `FileId` naming an included fragment
  (ADR-008).
- **The core crates perform no I/O.** Every parse-time read goes through the
  injected `ParseFileLoader`.
- **`process_preview` already runs parse → local analysis → merge → render in
  one process.** It merges into a stale global index, and `ProjectIndex::merge`
  exists for that reason.
- **Every index-resolved inline node carries the `Span` it was parsed
  with.** A cursor position can therefore be mapped to the reference under it.

Four gaps stand between that and a language server:

1. **Definitions have no position.** `Node::Heading` and `Node::Target` carry
   no span, and neither do most block nodes. `TargetLocation::Internal` holds a
   document path only. Go-to-definition could open a file but not find the
   line, and there is nothing to build a document outline from.
2. **The index cannot forget.** `ProjectIndex::merge` only extends. If a label
   is deleted in the editor, the stale index still resolves it. A renamed
   document's old entries stay too. The preview tolerates this; a server
   publishing "unknown target" diagnostics cannot.
3. **How a document parses is decided by Bazel.** Its library's
   `default_role`, default domain, `jinja`, `entity_schema` and `parse_data`
   are rule attributes. The VS Code extension's `BazelScanner` discovers only
   a *site's* config, template and index.
4. **The bar to try it is a Bazel migration.** Today a project must already be
   a rinx project before any editor support applies. The users with the most
   to gain from a fast editor are Sphinx users, who do not have that yet.

The fourth gap matters most for adoption. `scripts/benchmark.py` shows the way
around it: it wraps CPython's unmodified `Doc/` tree in a single
`rinx_library` globbing `**/*.rst`, with no strict dependencies, and rinx builds
it. A Sphinx project is a source directory, a `conf.py` and `.rst` files, which
is all a language server needs to know as well.

## Decision

### 1. One binary, a `lsp` subcommand, `lsp-server` over stdio

The server is `rinx lsp`, a subcommand of the existing binary. Its
implementation is a new `rinx_lsp` library crate, which the worker depends
on. With one artifact, the editor and the build cannot disagree about how a
document parses or what a diagnostic code means. The extension already knows
how to find the binary.

The protocol layer is `lsp-server` + `lsp-types` (rust-analyzer's crates):

- **Synchronous by design.** A blocking main loop dispatches work to a thread
  pool.
- **Request handlers are pure.** Each has the shape
  `fn(&WorldSnapshot, Params) -> Result<Response>`. This continues the
  `process_*`/`cmd_*` split: the protocol loop is the thin I/O wrapper, and a
  handler is testable without it.

On the VS Code side, `vscode-languageclient` starts the server and forwards
file watching. It passes what `BazelScanner` discovers as
`initializationOptions`.

### 2. Two project sources, one project model

What the server needs to know about a project is captured in one type:

```rust
struct ProjectModel {
    source_root: PathBuf,
    libraries: Vec<LibraryModel>, // srcs, ParseInputs, entity schema
    site: SiteConfig,
    inventories: Vec<ExternalInventory>,
    strictness: Strictness,       // see §5
}
```

It is produced by one of two `ProjectSource`s:

- **Bazel** (*rinx mode*): `rinx_site` gains a non-default output group
  writing an `lsp_manifest.json`. It lists every library's sources and parse
  attributes, the schema, config and inventories. This is the same shape as
  `doctest_plans`: Bazel writes what a tool needs, and the tool never queries
  Bazel's graph itself.
- **Sphinx** (*legacy mode*): the nearest `conf.py` above a file (§4), plus an
  optional override (§4).

Nothing downstream of `ProjectModel` knows which source built it, except
through `strictness`. In a monorepo, each `conf.py` or manifest is one model,
and a file belongs to the nearest one above it. The extension already picks a
site the same way.

### 3. The server indexes the workspace itself

Legacy mode has no prebuilt index to start from, so the server always builds
its own index. On startup it parses every source of every library in parallel
(rayon) and analyses each document. On each edit, it re-runs that for the
edited document, followed by the global phases of `build_project_index`. The
Bazel-built index is at most an optional warm start for rinx mode, never
a requirement.

This needs the index to **retract a document's contribution**, which is gap 2.
`ProjectIndex` keeps its serialized shape for Bazel. The server instead holds
a `BTreeMap<DocPath, DocumentAnalysis>` and folds it through the existing
global phases. A retraction method would have to be kept in sync by hand every
time a field is added to the index; a fold cannot forget a field.

`salsa` is deferred until measurement shows the global phases dominate an
edit. Per-document analysis plus the named phases is already a hand-written
query graph with one level of caching.

### 4. Reading `conf.py` statically, executing it only when trusted

`conf.py` is Python. By default the server **does not execute it**. Instead,
it reads the top-level assignments whose right-hand side is a literal:
strings, numbers, booleans, lists, tuples, dicts, string concatenation. The
reader is a small one of our own, in the manner of `rinx_cdecl` and
`rinx_filter`, rather than a full Python parser. Anything else (an import, a
computed value, a conditional, `extensions += [...]`) is **reported** as
unread, naming the setting, and is never guessed at.

As an opt-in, available only in a VS Code trusted workspace, the server runs
`conf.py` in the project's interpreter and dumps a whitelist of names as
JSON. This gives exact values to projects that compute them.

On top of either, a `[tool.rinx]` table (in `rinx.toml` or `pyproject.toml`)
overrides any value. It is the escape hatch for what the static reader cannot
see, and the first file a migration (§7) writes.

The settings read, and what each maps to:

| Sphinx setting | rinx equivalent |
|---|---|
| `root_doc` / `master_doc` | root document |
| `source_suffix`, `exclude_patterns` | the library's `srcs` |
| `default_role` | `--default-role` |
| `primary_domain` | default domain |
| `highlight_language`, `numfig`, `pep_base_url`, … | `SiteConfig` fields |
| `extensions` | the strictness policy (§5) |
| `intersphinx_mapping` | inventories (§6) |
| `needs_*` / `needs_from_toml` | the entity schema |

The sphinx-needs conversion exists today as `scripts/needs_schema.py`. It
moves into Rust (`rinx_entity`), so the server can call it. The script then
calls the same code through the CLI, and the conversion is not kept in two
languages.

### 5. Strictness is a diagnostic post-filter

A legacy project uses constructs rinx does not support, such as autodoc,
napoleon or a project's own `_ext/`. Reported as-is, these drown the real
findings. An unknown directive is one warning. Worse are the cascades: every
`:py:func:` into an autodoc-generated object is "broken", because the
definition exists only in a build rinx is not running.

A table of known extensions, kept as data in the repository so contributors
can extend it without touching Rust, records what each extension contributes:

```rust
struct ExtensionProfile {
    name: String,               // "sphinx.ext.autodoc"
    directives: Vec<String>,    // automodule, autoclass, ...
    roles: Vec<String>,
    produces_targets: TargetScope, // None | Domains(["py"]) | Everything
}
```

Under `Strictness::Legacy`:

- **An unknown directive or role from a declared extension** is reported as
  *Information*: "rinx does not analyse `sphinx.ext.autodoc`; contents not
  checked". One from no declared extension stays a warning, since it is
  probably a typo.
- **An unresolved reference** into a domain that an unsupported extension
  produces targets for is reported as a *Hint*. `:ref:` and `:doc:` stay
  warnings.
- **An extension missing from the table** is assumed to produce
  `Everything`. The server says so once, naming the extensions it could not
  model.
- **The toctree strict-deps check is off.** Every document under the source
  root is allowed, as in Sphinx.
- **Images, downloads and parse-time files** are checked for existence on
  disk rather than for a declaration.
- **Diagrams** need no `diagrams = True`. Their templates are expanded and
  reported on, but nothing is compiled.
- **Jinja is off.** A `source-read` hook cannot be detected statically; the
  override can switch it on.

`Strictness::Rinx` changes nothing: a rinx-mode diagnostic is exactly what the
build reports.

The filter runs in `rinx_lsp`, in one place, keyed on `DiagnosticCode` plus
the project model. The parser and renderer stay unaware of modes. The `.. noqa:`
filtering both modes need (`commands/suppression.rs`'s `retain_reportable*`) is
lifted out of the worker into a crate both front ends call. Otherwise the
editor and CI would disagree about what is suppressed.

### 6. The editor may fetch inventories; the build still may not

ADR-023's rule that nothing is fetched exists because a build must be
hermetic, and an editor need not be. In legacy mode the server fetches each
`intersphinx_mapping` inventory **once, with the user's consent**, and caches
it in the extension's `globalStorageUri` with a time-to-live.
`rinx_inventory::read_inventory` reads it unchanged. Without an inventory,
an unresolved external reference is a Hint. In rinx mode, inventories come
from the manifest, as the build declared them, and nothing is fetched.

### 7. Legacy mode is a way in, not a destination

Two commands turn trying rinx into adopting it:

- **Analyse project**: counts the unsupported directives, roles and
  extensions in the workspace, linking each to `docs/compatibility.rst`.
- **`rinx init --from-sphinx`**: a CLI subcommand the extension also
  invokes. It writes `rinx.toml`, an `entities.toml` for a sphinx-needs
  project, and a `BUILD.bazel` with one globbed library. `scripts/benchmark.py`'s
  `render_corpus_build_file` becomes a call to it, so the generator exists
  once. Once the manifest appears, the server switches the project to rinx mode.

### 8. Positions: converted at the boundary, definitions in the server

Columns stay Unicode scalar values (`span.rs`, ADR-003). The server converts
to UTF-16 at the protocol boundary, or negotiates `utf-32` where the client
offers it.

Definition spans (gap 1) are added to the AST nodes, but **the index written
by Bazel stays position-free**. Today a prose edit that only shifts lines
leaves every later document's index contribution byte-identical, and every
render cached. Positions in the index would end that. The server parses
in-process, so it builds its own position map and never needs positions
from a Bazel-built file.

### 9. Diagnostics in two tiers

Parse diagnostics are published on every change. Render-time ones need a full
render, syntect included: broken links, empty listings, math, highlighting,
diagram templates. These are debounced and computed for open documents only.
PlantUML is never compiled in the editor.

## Consequences

- A Sphinx user gets diagnostics, completion and hover by installing the
  extension. They need no Bazel, no Python and no configuration.
- Every feature is built once, against `ProjectModel`. Rinx mode is legacy
  mode with exact configuration and strictness switched to the build's.
- The workspace scan is the server's own, so titles and references are
  current without a `bazel build`. The preview's staleness problem
  (`docs/vscode.rst`) is solved too, once the preview is served by the
  server (a custom `rinx/preview` request) instead of a process per
  keystroke.
- Legacy mode is approximate, and says so. Its value depends on the extension
  table, which needs maintenance as projects bring extensions rinx has not
  met.
- `ProjectIndex` grows a per-document form in the server. The two shapes must
  produce the same answers, which the server's tests check by building both
  from one corpus.
- `scripts/needs_schema.py` and `render_corpus_build_file` become callers of
  Rust code instead of implementations.

## Rollout

The order is laid out PR by PR in `docs/dev/lsp-roadmap.md`. Every step ends
in something an author can see in the editor. Single-file diagnostics come
first, then workspace awareness, then legacy mode, then rinx mode. The
extension stays unpublished until the very end and is tried as a local VSIX.

Legacy mode is testable against the CPython corpus `scripts/benchmark.py`
already clones. A clean CPython checkout is the bar: no warnings that Sphinx
itself would not give.

## Alternatives considered

- **`tower-lsp` (async).** Rejected: the pipeline is synchronous and
  CPU-bound, and nothing in the workspace uses an async runtime. An async
  layer would add tokio for nothing it runs.
- **A separate `rinx-lsp` binary.** Rejected: two artifacts to version and
  ship, which could then disagree about parsing.
- **Starting from the Bazel-built index only, like the preview.** Rejected as
  the foundation. It cannot serve legacy mode, and stays stale until a build.
- **Querying Bazel from the server (`bazel cquery`) for library attributes.**
  Rejected: slow, a second interpreter of the build graph, and unavailable
  in legacy mode anyway. The manifest output group keeps Bazel the only
  reader of `BUILD` files.
- **Executing `conf.py` by default.** Rejected: it needs the project's Python
  environment and runs workspace code before the user has trusted it.
- **Running real Sphinx, as esbonio does.** That is esbonio's niche: exact,
  but slow, and it needs the project's environment. rinx's case is the
  opposite: instant and dependency-free, and approximate only where it says
  so.
- **`salsa` from the start.** Deferred, not rejected (§3).

## Open questions

- **`rst_prolog`/`rst_epilog`.** Common in legacy projects, mostly for
  substitutions, and not supported by the build either. Implementing them, as
  a parse-time prepend and append, may be cheaper than downgrading every
  "undefined substitution" they cause.
- **A file under two `conf.py`s or two libraries.** The nearest model wins.
  Whether the second should be reported is undecided.
- **Severity names.** Should the legacy downgrades be configurable per
  `DiagnosticCode`, or is the table in §5 enough?
