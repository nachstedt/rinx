# 42. The preview draws diagrams with PlantUML's JavaScript build

## Status

Accepted.

## Context

The build compiles every diagram with the PlantUML jar, in a per-document
action after rendering (ADR-012 §4), and the page links the SVG as
`_images/<hash>.svg`. The language server compiles nothing: ADR-038 §9 keeps
the JVM out of the editor, for its start-up time and for the Java an editor
user may not have. Up to roadmap #24 the preview therefore showed a broken
image for every diagram, and nothing at all for a diagram that could not be
expanded.

Roadmap #24 put a diagram's *template* errors in the editor: an unknown entity,
a `filter()` the build cannot evaluate, an unknown `:config:`, an empty
result, and a Jinja mistake, now placed on its own line of the template. Only
`rinx_uml` can find those, and it needs no JVM. A PlantUML *syntax* error is a
different thing: only PlantUML can find it, and the build finds it when its
compile action exits non-zero.

Two questions followed. Can the preview show the picture without a JVM? And
can the editor report a syntax error?

## Decision

### The webview draws the picture

PlantUML publishes `@plantuml/core`: its own engine compiled to JavaScript
with TeaVM, with Graphviz (Viz.js) as WebAssembly, MIT-licensed from 1.2026.6
on. The VS Code extension depends on it, pinned to the version the build's
jar is (`MODULE.bazel`'s `plantuml_jar`, 1.2026.8), and
`scripts/copy-plantuml.mjs` copies what drawing needs into `dist/plantuml/`
when the extension is bundled: the engine, Graphviz, and the themes and icons
it reads as globals, with its licence. The 1.8 MB of emoji and the demo pages
are left out. The VSIX grows by 1.7 MB.

The server puts each diagram's expanded PlantUML text in the picture's place,
as `<pre class="plantuml-source" data-rinx-plantuml>`. It does so under
`rinx_renderer::DiagramPictures::Source`, which `PageAssets` carries next to
#23's `ImageLocator`. The preview passes it exactly when the request names a
`resourceRoot`, so a request without one still gets the build's own page,
which the parity test compares. The renderer still runs nothing and knows no
address. A diagram that could not be expanded shows the build's warning in
its place, quoting the template, where the build's page shows nothing.

The page loads the engine from the extension (`diagrams.ts`), and a drawer
replaces each `pre` with the picture. It draws one text at a time, because
drawing is asynchronous. It keeps each result by its text, so a page swapped
in after every pause in typing redraws only what changed. Any other reader of
the page, such as a client without the script, sees the text.

### A syntax error shows in the picture, not in the problems list

PlantUML answers a syntax error as the jar does, with a picture of the error
naming its line (`Syntax Error?`). That picture is what the preview shows, so
the author sees PlantUML's own words, from PlantUML's own code. The editor
does not underline it: the server, which publishes diagnostics, never runs the
engine. How to change that is left open (see Consequences).

## Consequences

- No JVM runs, in the server or the extension, and the server is unchanged
  but for the markup. ADR-038 §9 holds.
- The engine lays a diagram out with Graphviz. The build uses ELK
  (`-Playout=elk`, ADR-012 §8), so a picture in the preview may be arranged
  differently from the published one. Its content does not differ.
- Only VS Code draws diagrams. The other editors have no preview at all.
- A new PlantUML version needs the extension's pin raised with the build's.
  If the two drift apart, the preview may accept text the build refuses, or
  the other way round.
- PlantUML's standard library (`!include <C4/C4_Context>`, the sprite sets)
  is not in the package; the engine fetches it from an address the page would
  have to name, so a diagram including it does not draw in the preview.
- A syntax error reaches the problems list only through more work. The
  options are a jar the client grants, as `conf.py`'s interpreter is granted
  (ADR-038 §4, which would amend §9), a Rust reimplementation once one is
  mature, or the extension running the engine and reporting back. Each would
  map PlantUML's line back to the document through the template's position
  (`Uml::template_span`).

## Alternatives considered

- **Show the text alone.** Correct, and no larger VSIX, but an author
  writing a diagram wants to see it. This was the first plan for #24, and the
  text is still what a client without the script shows.
- **Run the jar in the server, given one.** Exact, ELK included. But it
  needs Java and a JVM per compile, or a long-running one. It contradicts
  ADR-038 §9, and it would serve only the users who configure it.
- **A Rust reimplementation in the server.** `plantuml-little` is the most
  complete one, but it matches upstream only as of 1.2026.2, has few users and
  two maintainers, and links a native Graphviz that the hermetic Bazel build
  and the release binaries (#39) would both have to carry. `puml` and
  `plantuml.rs` are partial. A reimplementation would also accept and refuse
  text differently from the jar the build runs, which the parity promise
  (ADR-038) forbids for anything the editor reports.
