# 007 — Image assets: declaration, layout, and embedding

## Context

`.. image::` and `.. figure::` are the first constructs whose output depends on
a file the author wrote that is neither a `.rst` nor read by the parser. That
raises four questions the other directives never did: how the build learns the
file exists, where it lands in the site, what happens when the author asks for
its bytes to be inlined, and what to do about the options docutils answers by
opening the file and measuring it.

## Decision 1 — images are declared build inputs, in their own attribute

`rusty_sphinx_library` gains an `images` attribute; the files ride to the site
rule on `RustySphinxInfo.image_files`, and `validate_images` fails the build for
any referenced project file missing from the bundle.

This is a third, separate mechanism from the two that already exist, and the
separation is the point:

- `deps` declares *other libraries* pulled in via `.. toctree::`.
- `csv_data` declares *bytes the parser reads*, at parse time.
- `images` declares *bytes the site serves*, and that no phase reads at all
  unless a document asks to embed one.

Unlike `csv_data`, images are not parse-action inputs: nothing about parsing a
document depends on the picture existing. An undeclared image therefore cannot
fail at parse time, which is why the site's validation step is what catches it.

An external URL (`https://…`, or a `data:` URI written by hand) is never
declared, never bundled and never validated — it is not part of this project.

## Decision 2 — `_images/` keeps the source-root-relative path

Sphinx flattens `team_a/logo.png` to `_images/logo.png` and renames collisions
with a global counter (`logo1.png`). That pass is order-dependent across
libraries and hostile to per-action caching: adding an image to one team's
library can silently renumber another team's.

rusty-sphinx instead copies `examples/data/logo.svg` to
`_images/examples/data/logo.svg`. Collisions become impossible by construction,
no global pass is needed, and the path a page links to depends only on the file
itself. `crates/renderer/src/blocks/asset_href.rs` computes the relative href
for both authored images and compiled PlantUML diagrams — they share one
implementation because they share one directory.

This is a visible deviation from Sphinx's output layout, recorded in
`spec_gaps.md`.

## Decision 3 — `:loading: embed` embeds, via a per-document sidecar

docutils' `:loading:` has three values, and `embed` means what it says: the
file's bytes go into the HTML as a `data:` URI. Three existing constraints meet
here.

- `rusty_sphinx_renderer` performs no I/O, by design — the same rule that makes
  `rusty_sphinx_parser` read a `:file:` through an injected loader.
- A `.ast` must stay small: it is a cache unit, and the live preview ships one
  per keystroke. Base64 image data must not be baked into it at parse time.
- A render action must not take the whole site's images as inputs, or every
  page would re-render whenever any picture changed.

So a per-document `embed_assets` action reads the AST plus every declared image
and writes a `<doc>.embeds.json` mapping resolved project path to `data:` URI;
the render action consumes only that sidecar. This is the same **cache
firewall** as the doctest plans in ADR 002: the embed action re-runs for every
document when any image changes, but its bytes only differ for a document that
actually embeds the changed file, so only that page re-renders.

Keys are the *resolved* project path, from `ImageUri::resolve` — the one
function the embedder, the validator and the renderer all resolve with, so
`logo.svg` and `./logo.svg` cannot become two different assets.

Embedding an external URL is refused (`image.embed-external`, reported at parse
time) and falls back to a link: fetching it would make the build depend on the
network. A missing sidecar entry is reported at render time
(`image.embed-unavailable`) and also falls back to a link, rather than emitting
an image with no source.

## Decision 4 — the options that need the file's own dimensions

docutils answers two options by opening the image and measuring it. This build
never opens an image while rendering, so each gets its own answer:

- **A bare `:scale:`** — with no `:width:` or `:height:` to multiply — is
  dropped and reported as `image.scale-no-dimensions`. Silently ignoring an
  author's explicit instruction is exactly what the diagnostics exist to
  prevent.
- **`:figwidth: image`** is emitted as `width: fit-content`. CSS can express
  "as wide as the content" directly, so this is not a degradation: the browser
  measures the image at display time, which is more correct than a pixel count
  baked in at build time, and it needs no file access at all.

`:scale:` *with* a dimension needs no file and is applied normally.

## Consequences

- A build on a machine with no images still works; only `bazel build` of a site
  that references one needs the declaration.
- `spec_gaps.md` marks both directives 🔶 rather than ✅: the bare-`:scale:` gap
  above, and Sphinx's `.. image:: logo.*` candidate-selection wildcard, which
  presumes a source-tree glob that this explicitly-declared-inputs model
  deliberately does not have.
