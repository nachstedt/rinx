# 27. The `:download:` role: a declared file, copied beside the pages

## Status

Accepted.

## Context

`` :download:`file` `` links a file a reader can save — a script, a data file,
an archive — and Sphinx copies that file into the output as a side effect of
reading the role. It was the last common Sphinx role this build parsed as
nothing.

Unlike every other reference role it names a *file*, not something a document
defines: nothing about it is a question for the project index. The closest
precedent is therefore not `:doc:` (ADR-026) but `.. image::` (ADR-007), which
also names a file the site must serve. Four questions followed: how the build
learns which files to copy, where they land, what happens to one nobody
declared, and how the declaration is checked against documents whose inline
text the typed walker does not reach. The markup was checked against
`sphinx-build` 9.1.

## Decision

### 1. A `downloads` attribute, read by nothing but the copy

`rinx_library` gains `downloads`, riding to the site on
`RinxInfo.download_files` — a ninth dependency mechanism, next to `images`.
It is neither a parse input nor a render input: the page's `href` follows from
the page's path and the path as written, so no render needs the bytes. Editing a
downloadable file therefore re-runs only the site's `RinxBundleDownloads` copy
and re-renders no page. That is the cache firewall `images` has to build with a
sidecar for `:loading: embed`; downloads never embed, so they get it for free.

A file may be declared in several attributes at once — `examples/BUILD.bazel`
declares `logo.svg` in `images` and `downloads`, and `greeter.py` in
`parse_data` and `downloads` — because each attribute answers its own question.

### 2. `_downloads/` keeps the source-root-relative path

Sphinx copies `team_a/tool.py` to `_downloads/<md5>/tool.py`. rinx copies it to
`_downloads/team_a/tool.py`, for ADR-007 §2's reasons: no collisions by
construction, no global pass, and an `href` that depends on the file alone.
`renderer/src/asset_href.rs` — moved to the crate root, since `blocks/` and
`inline/` both reach it — computes the `href` for both directories through one
`AssetDir` enum.

### 3. One URI split and one resolver for images and downloads

`ImageUri` became `rinx_ast::AssetUri`, unchanged in behaviour and in its
serialized form: the external-URL/project-file split and `resolve` are the same
question for a picture and a download, and two copies could disagree about
where `../shared/x` points. An external URL is linked as written, marked
`external` as Sphinx marks it, and never copied or validated.

### 4. An undeclared file fails the build

Sphinx warns and shows the text unlinked. rinx fails the site's validation
action, as it does for an undeclared image, reporting
`error: path:line:column: download.undeclared: …` in the shape every other
diagnostic has — the role carries its span. The action was renamed
`validate_images` → `validate_assets`, and now runs for **every** site, not
only one that bundled images: a document naming a file in a site that declared
none must fail too, which previously slipped through for images as well.

### 5. The roles are found in the AST's JSON

A `:download:` can sit in any inline content — a paragraph, a title, a table
cell, an entity section — and `rinx_ast::walk_nodes` deliberately does not
descend into inlines. The validator therefore finds `DownloadReference`
objects in the `.ast`'s JSON, as `index.rs`'s `written_reference_targets`
already finds reference targets, and reads each hit back as an `InlineNode`
so its shape is checked rather than assumed. A typed inline walker would need
to know every place inline content lives and was not worth building for one
caller.

### 6. The markup is Sphinx's

`<a class="reference download internal" download="" href="…">` around a
`<code class="xref download docutils literal notranslate">` whose words are
each a `<span class="pre">`. The link text is the explicit title, else the
target *as written*. `!` shows everything after it as the literal alone,
linking and copying nothing, as `XRefRole` does. `:std:download:` is the same
role. There is no `:external:` form: no inventory lists a download.

## Consequences

- `:download:` needs a `downloads` entry in a Bazel build; `tests/test_download_data.sh`
  proves the declaration is load-bearing and that the file lands where the
  link points.
- The live preview renders the link without checking it, as it renders an
  image: it has no bundle to check against.
- `docs/compatibility.rst` records the layout and fail-vs-warn deviations.
