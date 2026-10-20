# 41. A quick fix adding the missing `deps` entry

## Status

Accepted.

## Context

In a Bazel build a toctree is a dependency. An entry may name a document of
its own library or of a library in its `deps`, and the build fails on any
other (`toctree.undeclared-dependency`, `toctree.glob-matches-no-dependency`).
Since roadmap #19 the language server underlines such an entry exactly as the
build fails on it. The author still had to work out which `rinx_library`
lists the document and edit `BUILD.bazel` by hand. Roadmap #21 offers the edit
as a quick fix.

The fix needs three answers:

1. **Which library to add.** The site's `lsp_manifest` (ADR-038 §2) lists
   every library the site collects, and which documents each one lists. A
   site collects its libraries through `deps`, though. A library nobody in the
   site depends on yet, which is the usual reason for the mistake, is in no
   manifest. Removing `team_b` from `//examples:root_docs` shows this: the site
   then no longer knows `//examples/team_b:docs` at all.
2. **Where to write it.** The build file of the library writing the entry,
   and the place in its `deps` list.
3. **How to apply it.** The server never runs Bazel, and the edit should work
   in every editor and be undoable.

## Decision

### The edit is computed in Rust from the build file's text

The server computes the edit itself and returns it as an ordinary
`WorkspaceEdit` on the build file. It does not run buildozer. A build file is
Starlark, which is lexically Python, so `rinx_pyconf::tokenize` splits it.
`rinx_lsp`'s `project/build_file/` reads two things on top of the tokens: the
calls written at the top level, with their keyword arguments, and list
literals of strings.

- **The call** is the one whose `name = "…"` argument is the library's name,
  whatever function it calls, since `load` may rename `rinx_library`.
- **The edit** depends on how `deps` is written:

  | `deps` is… | Edit |
  |---|---|
  | a multi-line list | one line per label, at the items' indentation |
  | a single-line list | rewritten one item per line once it holds two |
  | `[]` | the label, inline |
  | absent | `deps = […]` after the last argument |

- **Order and spelling.** A label goes where buildifier's order puts it when
  the list is already in that order, and at its end otherwise. It is written
  `:name` in the library's own package.
- **Comments** are no tokens, so every edit inserts text beside them. The one
  rewrite is of a single-line list, which cannot hold a comment.
- **Refused:** whatever cannot be read as written — a computed `name`,
  `deps = COMMON + […]`, a `select`, two calls of the same name. The fix
  would otherwise be a guess.

### The library comes from the manifest, else from its package's build file

For a document entry, the server takes the library of the site listing the
document. When the site lists it nowhere, it reads the build file of the
document's package: the nearest directory holding a `BUILD.bazel` or `BUILD`,
as Bazel defines packages. There it takes the `rinx_library` whose `srcs`
holds the file. A `srcs` is evaluated when it is a list of strings, a
`glob(…)` of them with an `exclude`, or a `+` of those. The fallback is
limited in three ways:

- It checks the function name: a `filegroup` listing the same files must not
  be mistaken for a library.
- It serves document entries only. A glob entry's matches are found among the
  site's documents.
- It skips the cycle check. A library from the site is refused when it already
  depends, through its `deps`, on the library writing the entry. A library
  from the fallback has no `deps` the server knows, so the build reports a
  cycle it would close.

### The edit comes with the action

An action is offered only when its edit can be made. So the edit is computed
in `textDocument/codeAction` itself rather than on `codeAction/resolve`. An
editor asks for actions as the cursor moves, so the build file is read just
before the click. A client that cannot resolve an action gets the edit too.
The edit uses `changes`, which every client supports.

### `--check` lists the fixes

`rinx lsp --check` prints each document's quick fixes beside what it shows
(`quick_fixes`, left out when empty). `tests/test_dependency_quick_fix.sh`
uses this to check the whole loop against a real build. It removes `team_b`
from `examples/BUILD.bazel`, applies the fix `--check` offers, and requires
the file to be the original again and the site to build.

## Consequences

- One click fixes the mistake. The build file is left modified and unsaved,
  and saving it is what makes the extension rebuild the manifests, which
  clears the diagnostic.
- The server never sees a build file's unsaved buffer, since the extension
  syncs only reStructuredText. The edit is computed from the disk.
- `visibility` is not checked, since the manifest does not record it. A
  library its package keeps private fails the next build with Bazel's own
  message.
- A library declared by a macro gets no fix. Its call carries no
  `name = "…"` the reader can trust, and its `srcs` may be anything.
- The same reader serves the build's other declarations. `images`,
  `downloads` and `diagrams = True` could be fixed the same way.

## Alternatives considered

- **Buildozer.** It handles macros and formatting, but the server would have
  to find and run a second external binary. That binary is not installed by
  default, and it writes the file itself, so the change cannot be undone in
  the editor.
- **Asking the extension to run `bazel query`** for the library listing a
  file. That is exact, macros included. But it is VS Code only, takes a
  second or two per lookup, and gives nothing in Neovim, Emacs or `--check`.
- **The manifest alone.** No fix would be offered in the most common case: a
  library the site does not collect yet.
