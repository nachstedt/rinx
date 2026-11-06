# 49. `rinx init --from-sphinx` writes what the documents use

## Status

Accepted.

## Context

ADR-038 §7 makes legacy mode a way in: *Analyse project* (roadmap #36) says
what a Sphinx project writes that rinx lacks, and roadmap #37 writes its
Bazel build in one step. Two hand-written generators existed already: the
benchmarks' `render_corpus_build_file`s. Each hardcoded one corpus. They
globbed `srcs`, `images` and `parse_data` by extension, scanned the
transclusions with regular expressions, and read nothing from `conf.py`.

## Decision

### One generator, in the language server

The plan (`rinx_lsp::migration`) is computed from a project the server has
scanned. Two front ends apply it:

- `rinx init --from-sphinx <dir>` (`crates/worker/src/commands/init.rs`),
  which discovers and scans the directory in-process;
- the `source.migrateToRinx` source action on `conf.py`, which returns a
  workspace edit creating the files.

So every editor writes the same files, and both benchmarks now call `rinx
init` instead of their own generators. The action is offered only to a
client announcing `resourceOperations: ["create"]` with `documentChanges`,
and only once the scan has finished, since the plan depends on every
document's parse.

### Declarations are measured, not globbed

The library declares exactly what the documents need:

- `parse_data`: every file a parse read (`IndexedDocument::reads`).
- `images` and `downloads`: from `rinx_analyzer::required_declarations`.
  This is the inverse of `undeclared_assets` and `disabled_diagrams`, built
  from the same walks, so the build file and the build's own checks cannot
  disagree.
- `diagrams = True`: when a document draws a diagram.

`srcs` stays a glob, `**/*.rst`. Its excludes are not translated from
`exclude_patterns`, because Bazel's glob has no `[...]` and Sphinx's
patterns exclude whole directories. Instead they are measured: every `.rst`
file of the package that is not a document is excluded, and a directory
holding nothing else is excluded as one `dir/**`. A document in a
subpackage, or one whose suffix is not `.rst`, cannot be listed and is
reported.

`rst_prolog` and `rst_epilog` become `rst_prolog.txt` and `rst_epilog.txt`.
The `.txt` suffix keeps both the glob and Sphinx from taking them for
documents.

### The module goes where Sphinx's source root is

- **An enclosing `MODULE.bazel`** gains a `bazel_dep` on rinx, unless it has
  one or is rinx's own. Nothing else of it is touched. Its packages name
  documents from its root, so `rinx.toml`'s `root_doc` is prefixed with the
  package's path.
- **Outside any module**, the `conf.py` directory becomes a module of its
  own. The build then names each document, and resolves each `/`-rooted
  path, from where Sphinx does. A module at the workspace folder's root
  would have prefixed every page's address and broken every `/`-rooted path
  in a nested `docs/`.

Such a module is nested inside a workspace folder that is no Bazel
workspace. The server now asks the client for the manifests of the folder's
Bazel workspace (`bazel_workspace_of`): the folder itself, or else a Sphinx
project's directory holding `MODULE.bazel`. The extension needed no change,
since it runs Bazel wherever it is asked to.

### What cannot be carried over is reported, never guessed

`MigrationFinding` names each case:

- a setting `conf.py` computes;
- an `intersphinx_mapping` entry, which needs a pinned `rinx_inventory`
  that cannot be written without fetching;
- a file outside the source root;
- a missing file;
- a file in a subpackage;
- a document that is no `.rst`;
- an unconverted sphinx-needs construct.

For a file outside the source root, the finding also names where the build
looks for it (the path past the climb above the root). The editor's loader
resolves `..` on the disk, while the build's parser stops at the root.

The settings only a migration needs (`project`, `version`/`release`,
`html_domain_indices`, `pep_base_url`, `rfc_base_url`,
`modindex_common_prefix`) are read quietly in `ConfFacts`. A computed one is
a migration finding, not a squiggle in `conf.py`, because it changes nothing
the editor shows. A `conf.py` run is still asked for them.

`init` never overwrites a file, and `--dry-run` reports without writing. The
entity benchmark uses that to copy its outside sources in before migrating.

## Consequences

- A `source-read` hook is invisible to reading `conf.py`, so `--jinja` says
  it; the editor's action cannot.
- The benchmarks' corpora are now built as a user's migration would be.
  CPython's `includes/*.rst`, which its `exclude_patterns` names, are no
  longer published as pages, and its root document is named rather than
  inferred.
- `entities.toml` written by a migration is the project's own: its header
  says so, unlike the regenerated output of `rinx needs_schema`.
