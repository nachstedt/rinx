# ADR-004: Math Rendering

**Status:** Accepted
**Date:** 2026-08-30

## Context

`:math:`, `:eq:` and `.. math::` were the last widely-used Sphinx constructs
with no implementation at all: a `.. math::` fell through to
`Directive::Unknown` and rendered as nothing, and `` :math:`x` `` rendered as
its own literal source text.

The question is not *whether* to reuse an established LaTeX renderer — writing
one is out of the question — but **where the conversion runs**: in the reader's
browser, as Sphinx does by default, or in the build, as `.. csv-table::`'s CSV
parsing already does.

## Decision

Convert LaTeX to [MathML Core](https://www.w3.org/TR/mathml-core/) **at render
time**, using the [`math-core`](https://crates.io/crates/math-core) crate.

`MathError`, carrying a span and the `math.invalid-latex` code, joins
`BrokenLink` and `ObjectTypeMismatch` as a third kind of render-time
diagnostic, plumbed through `RenderOutput` and the `.. noqa:` filter exactly
like the other two.

### Considered alternatives

**MathJax or KaTeX from a CDN** — what `sphinx.ext.mathjax` does, and what
gives Sphinx its complete LaTeX coverage. Rejected on three counts, each of
which contradicts something the project already decided:

- The built site would need the network *at view time*, and its HTML would no
  longer be self-contained. ADR-001 and ADR-002 both defend hermeticity, and
  ADR-002 in particular established that anything needing a foreign runtime
  moves off the build path rather than onto the page.
- `templates/default.html` has no `<head>` injection hook, and adding one to
  ship a hardcoded CDN `<script>` would put a file path — the thing ADR-001
  deliberately keeps out of the config — into the template instead.
- rusty-sphinx would never see the LaTeX fail. A malformed equation would be a
  silent rendering failure in one reader's browser rather than a build warning,
  which is precisely the class of problem the diagnostics work in ADR-003 exists
  to eliminate.

**`pulldown-latex`** — the other maintained pure-Rust LaTeX→MathML crate, with
comparable environment coverage. Rejected narrowly: it ships its own
`styles.css` and webfont directory that would have to be vendored and taught to
`rules/site.bzl` (which can copy exactly one stylesheet today), where
`math-core` targets MathML Core directly and needs neither.

## Consequences

### The toolchain moved

`math-core` 0.8 requires rustc 1.96, above both `rust-toolchain.toml`'s 1.94.1
and the 1.93.1 that `rules_rust` 0.69 resolved to. Rather than pin the older
`math-core` 0.7, `rules_rust` was upgraded to 0.74 — whose default Rust version
is 1.98.0 — and `rust-toolchain.toml` moved to 1.98.0 to match. Cargo and Bazel
must keep agreeing here; a future `math-core` bump may move both again.

0.8 is worth that: it provides `LatexError::to_html`, which renders a failed
equation's source with the message as a tooltip, so a page with broken LaTeX
still shows what its author wrote instead of dropping the content.

### rusty-sphinx owns equation numbering, not `math-core`

`math-core` has its own equation counter, reachable via
`convert_with_global_state`. It is deliberately not used: it counts LaTeX
`equation`/`align` environments per converter, while Sphinx numbers *labeled
directives* per document, and letting both count would put two disagreeing sets
of numbers on one page. `convert_with_local_state` is called instead, and
`rusty_sphinx_analyzer`'s `number_equations` assigns the numbers, storing them
in `ProjectIndex::equations` so a cross-document `:eq:` can read them.

One visible edge follows from converting each block independently: under
`:nowrap:`, a self-numbering environment such as `align` restarts its own count
in every directive. Sphinx's MathJax would continue it across the page.

### A `Directive` carries a span

`Directive::Math` is the first `Directive` variant with a `span`. Every
render-time diagnostic until now came from an `InlineNode`, which is why only
those carried positions. A math block's content can fail *after* parsing
succeeded, so without a span its diagnostic would have nowhere to point. It is
the body's first line, not the offending character — see `spec_gaps.md`.

### `split` becomes `aligned`

Sphinx wraps a `\\`-containing equation in `\begin{split}`, which `math-core`
does not implement. `\begin{aligned}` is emitted instead; it produces the same
alignment. This is the only divergence in the LaTeX-assembly step, which
otherwise follows `sphinx.ext.mathjax`'s `html_visit_displaymath` exactly.

### No new asset — but rendering leans on a locally installed math font

MathML needs no script and downloads no webfont, so equations are styled by
rules added to `assets/default.css` (and restated in
`examples/custom_template.html`) and nothing was added to `rules/site.bzl` —
which can still ship only the one hardcoded `default.css`.

That is not the same as being self-sufficient, and the distinction is easy to
miss. The stretchy brackets that grow to fit a matrix or a `cases` block are
drawn from glyph variants in a font's OpenType MATH table, which no text font
has. The stylesheets therefore set a `font-family` on `math` naming the common
math fonts and ending in the CSS Fonts 4 `math` generic, which defers to the
browser's own default. A reader whose system has none of them still gets
correct, readable equations — but the brackets will not stretch.

Shipping a patched webfont (upstream provides one at
[math-core-fonts](https://github.com/tmke8/math-core-fonts)) would remove that
dependency, at the cost of ~500 KB per site and a second asset attribute on
`rusty_sphinx_site`, which today can copy exactly one stylesheet. Deferred, not
rejected.
