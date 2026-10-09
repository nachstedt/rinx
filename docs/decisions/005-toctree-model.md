# ADR-005: The Toctree Model

**Status:** Accepted
**Date:** 2026-08-31

## Context

`.. toctree::` is the directive the whole site structure hangs off, and it was
the least complete directive in the codebase. Only `:maxdepth:` had any effect;
the other eight options were parsed into a `Vec<String>` of raw option lines
that nothing ever read. Entries were raw strings, so `Title <target>`, `self`
and external URLs were all treated as filesystem paths.

Underneath that sat four structural problems, and none of them could be fixed by
adding option handling to the existing shape:

1. **`ProjectIndex.nav_tree` was a pre-flattened document tree.** Every option
   in this feature attaches to *a toctree directive* — its own entries, its own
   depth, its own caption — but a flattened tree has no directive to attach
   them to.
2. **The renderer never looked at the directive.** `render_toctree_directive`
   looked the *document* up in `nav_tree` and rendered its children, so two
   toctrees in one document produced an identical list.
3. **No section hierarchy existed anywhere.** The analyzer indexed only a
   document's first `H1`. But Sphinx's toctree lists the sections *inside* each
   referenced document — which is precisely what `:titlesonly:` turns off — so
   neither option could behave as specified.
4. **Headings rendered no `id`.** Anchors came only from explicit `.. _label:`
   targets, so there was nothing for a section entry to link to.

## Decisions

### 1. Store the toctree graph, expand it at render time

`NavEntry` and `nav_tree` are deleted. `ProjectIndex` instead stores:

```rust
pub toctrees: BTreeMap<String, Vec<DocumentToctree>>,   // per document
pub document_outlines: BTreeMap<String, DocumentOutline>, // per document
pub root_documents: Vec<String>,                          // project-wide
pub page_order: Vec<String>,                              // project-wide
pub section_numbers: BTreeMap<String, DocumentNumbers>,   // project-wide
```

The renderer walks that graph itself, applying the options of the directive it
is actually rendering.

The decisive reason is not elegance but **`ProjectIndex::merge`**. The merge
deliberately skipped `nav_tree`, because a flattened tree is a project-wide
product that cannot be merged per document. That is why the live preview
(`preview` subcommand, `editors/vscode/`) showed stale navigation. The first two
fields above *are* per-document data, so they merge like `document_titles`, and
the preview path becomes correct as a side effect.

Materialising sections into a tree instead would also have multiplied the
serialized index by (documents × sections × referencing toctrees), and would
have had to be recomputed on every preview keystroke.

The cost is real and accepted: the sidebar walk now runs per page rather than
once. It is bounded by `:maxdepth:` and reads two maps already in memory.

### 2. A `:glob:` stays unexpanded all the way into the index

Three phases expand toctree entries, and each has a **different** universe of
documents to expand against:

| phase | universe | purpose |
|---|---|---|
| `validate_toctree` (worker) | the Bazel `--allowed` deps | strict-deps enforcement |
| `build_project_index` (analyzer) | every document in the project | numbering, page order, diagnostics |
| `nav::expand` (renderer) | the documents the index knows | rendering |

Expanding once, early, would leave the strict-deps check either reimplementing
the matcher against its own narrower list or not checking globs at all. Keeping
the pattern and sharing one `expand_toctree` means a glob that reaches an
undeclared document still fails the build — it simply matches nothing in that
narrower universe.

This is why `rinx_toctree` is its own crate. It cannot live in the
analyzer: the renderer would then depend on the whole analyzer crate, which is
exactly what splitting `rinx_index` out was meant to stop.

Matching is `SphinxPattern`, a port of Sphinx's own `_translate_pattern`:
`*` does not cross a `/`, `**` does in any position, and `{a,b}` stays literal
(Sphinx has no alternation). The owning document is excluded from its own
pattern. It began as `globset` with `literal_separator(true)`, which refuses
a `**` inside a component; the port replaced it when the language server
needed the same patterns for a `conf.py`'s `exclude_patterns`, where
`**.ipynb_checkpoints` is common, so the two cannot mean different things.

### 3. Section ids are derived once, in `rinx_ast`

Two phases need the *same* answer: the analyzer records a section's id in the
outline so a toctree entry can link to it, and the renderer emits that id on the
heading the link lands on. If they ever disagreed, every section link in the
site would break silently — and it would break silently, which is the worst
property a bug can have.

So `ast::allocate_section_ids(nodes)` is the single implementation, keyed by
node index, and both phases call it. Keying by index rather than by title is
deliberate: two headings can share a title, and that is exactly the case where a
mismatch would go unnoticed.

The derivation itself is a faithful port of docutils' `make_id`, with its
digraph and stroked-letter tables transcribed rather than re-derived, and its
`create_id` disambiguation (`overview-1`, `section-1`) reproduced. It lives flat
beside `object_naming.rs` for the same reason that does: every later phase calls
it. `rinx_scope` already provides this contract for domain objects; this
is the same idea for sections.

### 4. A toctree's entries go where the directive was written

Sphinx splices a toctree's entries into the document's own section tree at the
position of the directive, so a toctree written under "Advanced" lists its
documents *under* Advanced rather than beside it. Reproducing that needs to know
which section encloses each directive, which is only knowable during the same
walk that builds the outline — so `build_document_outline` returns both, and
`DocumentToctree` carries the enclosing `SectionId`.

The alternative (always appending a document's toctree entries after its
sections) would have been simpler and wrong in a way authors would notice.

### 5. `root_doc` is configuration, and does not violate ADR-001

There was no root-document concept: roots were *inferred* as "every document no
toctree references", which silently made an orphan a second top-level sidebar
entry and left page order ambiguous. `rinx.toml` gains `root_doc`.

ADR-001 forbids **file paths** in the config, because Bazel relocates files. A
logical document name is not a path — nothing about it has to survive a sandbox
move — so this is metadata in the sense ADR-001 intends. It falls back to the
old inference when it names no document that exists, so existing projects keep
building.

### 6. Project-wide diagnostics are found in `index`, reported by `index`

A missing document, a glob matching nothing and an orphan are invisible to a
parser reading one file. They are detected in `collect_nav_diagnostics` while
the index is built, and reported by the `index` subcommand — the first time that
subcommand emits warnings at all.

Suppression is applied by the reporter, never by the detector, per ADR-003. A
document's `.. noqa:` travels on its own AST, so a comment written beside the
offending toctree still silences a warning raised a phase later.

One consequence worth recording: in a Bazel build these warnings are mostly
unreachable for *missing* documents, because `validate_toctree` runs earlier and
fails the build outright. That is the intended ordering — the strict-deps error
is more actionable — and it is why the example site cannot demonstrate
suppressing `toctree.missing-document`.

### 7. `:orphan:` gets the minimum field-list support, not a general one

Field lists are unimplemented (`docs/compatibility.rst`), and `:orphan:` is a file-wide
metadata field. Rather than build half a field-list implementation, only a
contiguous run of `:name: value` lines at the very top of a document is read,
into `Document.metadata`, as raw strings, rendering nothing. That is exactly
where Sphinx requires `:orphan:`.

The one subtlety: docutils' field marker requires whitespace or end-of-line
after the closing colon. Without that rule a document opening with an inline
role — `` :func:`spawn` `` — parses as a field named `func` and the paragraph
disappears. The rule is ported, and tested.

## Consequences

- Every page render now expands the sidebar itself. Watch this on the CPython
  benchmark; a `BTreeMap<String, usize>` sidecar for `page_order` is the first
  thing to reach for if prev/next lookup shows up.
- The default template changed, which invalidates every cached page render.
  ADR-001 already accepts that.
- `NavEntry` is gone from `rinx_index`'s public API.
- `rinx_toctree` is a new workspace crate, depended on by the analyzer,
  the renderer and the worker.
