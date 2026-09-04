# ADR-006: Syntax Highlighting

**Status:** Accepted
**Date:** 2026-09-04

## Context

`.. code-block::` was rusty-sphinx's most conspicuous half-implementation. The
parser read the language argument and nothing else: it never called
`scan_option_lines`, so an author writing `:linenos:` or `:caption:` got those
lines **rendered as code text**, with no diagnostic. The renderer emitted
`<pre><code class="language-python">` and left highlighting to a client-side
library that no shipped template ever loaded, so every code block on a built
site was monochrome.

That second half contradicted what ADR-004 had just settled for math: convert
during the build, so a page is self-contained and malformed input becomes a
build diagnostic rather than a silent failure in a reader's browser. A code
block is the same shape of problem.

## Decision

Highlight **at render time**, with [`syntect`](https://crates.io/crates/syntect)
over [`two-face`](https://crates.io/crates/two-face)'s grammar set, emitting
**classed** HTML whose stylesheet is generated from a syntect theme.

### Why this backend

The deciding constraint was not highlighting quality but the build. The
dependency graph was 76 crates and 100% pure Rust — no `cc`, no `bindgen`, no
`*-sys` crate anywhere — and `MODULE.bazel` pins three platform triples that
`crate_universe` must resolve for.

- **`tree-sitter`** means one crate per language, each with a `build.rs`
  compiling a generated `parser.c` (often megabytes, sometimes a C++ scanner)
  through the `cc` crate. Fifty languages is fifty C-compiling build scripts,
  across three triples, under `crate_universe`. It would have introduced the
  first C toolchain dependency in the project, hurt remote-cache hit rates, and
  turned a seconds-long build into a minutes-long one. `inkjet` is the same
  cost with ~70 grammars vendored and a maintainer who last shipped in 2024.
- **`synoptic`** is pure Rust but emits no HTML and has no context stacks, so
  nested strings, heredocs and embedded languages are simply wrong.
- **`syntect`** with `default-features = false` and the `regex-fancy` backend
  avoids oniguruma entirely. Its grammars are compiled into the rlib as
  compressed dumps, so nothing is read from disk at run time and a render
  action stays hermetic and deterministic. `two-face` extends the set to bat's
  213 syntaxes on the same embedded-asset model.

The 21 crates this added are all pure Rust.

### Why render time, not parse time

Two reasons, both structural:

- `--config` is an input to the `render` action, not the `parse` action. Baking
  a config-derived default language into the `.ast` would leave every parse
  action stale-but-valid after a config change.
- `.. highlight::` is document-order state, and the renderer is the phase that
  walks nodes in order.

So `.ast` files stay backend-free, exactly as ADR-004 requires of LaTeX, and
`crates/renderer/src/highlight.rs` is the only module that names `syntect` —
the same containment `math.rs` gives `math-core`.

### Why scope classes rather than Pygments token names

The alternative was a hand-maintained table mapping syntect's Sublime scope
names (`keyword.control`) onto Pygments' short classes (`k`), which would have
made any existing Sphinx `pygments.css` drop in unchanged.

Rejected because the mapping is a second source of truth that drifts. Emitting
syntect's own scope names under an `hl-` prefix lets the stylesheet be
*generated* by `css_for_theme_with_class_style` **using the same `ClassStyle`
the renderer emits with**, so the two cannot disagree about what a token is
called. The cost is real and accepted: no third-party Sphinx theme's stylesheet
applies to a rusty-sphinx site's code blocks.

## Consequences

### The language became a type, not a string

`Node::LiteralBlock`'s `language: Option<String>` could not distinguish
*unspecified* (inherit from `.. highlight::`) from *explicitly unhighlighted*
(`.. code-block:: none`), and Sphinx treats those differently. It is now
`CodeLanguage` (`Inherit` | `None` | `Default` | `Named(LanguageName)`), with
`ResolvedLanguage` — the same minus `Inherit` — for what survives inheritance.
A renderer that forgot to resolve one is a compile error rather than a bug.

`LanguageName` is opaque, trimmed and lowercased by its smart constructor, so
`Python` and `python ` cannot become two keys. The concrete name stays an open
string deliberately: enumerating all 213 languages would put backend knowledge
in `rusty_sphinx_ast`, inverting the crate dependency direction, and write the
backend's vocabulary into every serialized `.ast`.

`none`, `text` and `plain` all parse to `None`. The latter two are Pygments'
aliases for its do-nothing lexer and Sphinx documents are full of them; treating
them as grammars to look up would have made every such block report an unknown
language.

### Lines are highlighted one at a time, and rebalanced

`:linenos:` needs a number cell before each line and `:emphasize-lines:` needs a
wrapper around whole lines, so the caller must be able to address a line alone.
syntect's `ClassedHTMLGenerator` emits one flat stream with no line structure,
so `ParseState` + `line_tokens_to_classed_spans` are driven directly.

That surfaces a subtlety worth knowing before touching `highlight.rs`: a `<span>`
syntect opens on one line may not close until several lines later (a docstring, a
block comment), and interleaving per-line markup would nest those tags illegally.
Every line is therefore **balanced on its own** — scopes still open at a line's
end are closed there and reopened at the start of the next, with the same class
list. The rendered colours are identical; only the tag nesting differs, in the
direction that keeps the HTML well-formed.

Reopening needs a scope's class list, and syntect's `scope_to_classes` is
private, so `push_open_tag` restates it. A test pins the two together, because a
reopened tag whose classes differed would style the continuation of a string
differently from its first line.

### Failure degrades, and says so

An unknown language or a grammar the `regex-fancy` backend cannot compile leaves
the block as escaped plain text and reports `code-block.unknown-language` or
`code-block.highlight-failed` against the block's span — which is why
`Directive::CodeBlock` carries one, joining `Directive::Math` as the second
variant to do so. `:force:` suppresses the report, which is what the option means
in Sphinx. The reader never loses the code either way.

`ResolvedLanguage::Default` (Sphinx's `default`) tolerates failure silently by
design: it means "Python if it fits".

### The stylesheet is generated but checked in

`rules/site.bzl` can copy exactly one stylesheet, so the token colours live in
`assets/default.css` between marker comments, with a test that regenerates them
and fails if they have gone stale. Changing the palette means changing `THEME`
in `highlight.rs` and copying the test's expected output back.

Base16 Ocean Dark was chosen over the other bundled dark themes for size: it
emits roughly a fifth of the CSS Nord does, and a stylesheet every page loads is
not the place to spend 16 KB on colours a reader cannot tell apart.

The `<div class="highlight hl-code">` wrapper carries `hl-code` because that is
the class the generated stylesheet puts the theme's own foreground and background
on. Without it, tokens the theme colours would sit on its palette while tokens it
leaves alone kept the page's, and the two would visibly disagree.

One accessibility note, left as the theme's own choice rather than overridden:
body text reaches 7.6:1 against the code background and every syntax colour
clears WCAG AA except **comments, at 2.71:1**. Base16 de-emphasises comments
deliberately, as most editor themes do. Overriding it is a one-line rule after
the generated block if that trade is not wanted.

### `pycon` is unavailable

Sphinx highlights `.. doctest::` blocks and bare `>>>` blocks with Pygments'
`pycon` console-session lexer. The bundled Sublime grammars have no equivalent,
so those blocks render unhighlighted. The renderer asks for `pycon` anyway —
stating the intent, and starting to work by itself should the grammar set ever
gain one — and suppresses the diagnostic, because the language was this crate's
choice rather than an author's and no document could act on the warning.

`.. testcode::` blocks are plain Python and highlight normally.

### The corpus set the diagnostic's threshold

`guidelines.md` requires measuring an invented diagnostic against the benchmark
corpus before keeping it unnarrowed, and the first measurement was damning: 245
`code-block.unknown-language` warnings across CPython's documentation. They fell
into three groups, and only one was a real report.

- **Aliases Pygments knows and Sublime does not** — `python3`, `shell` and the
  batch spellings, 35 blocks. `resolve_alias` maps them onto the grammar the
  bundled set actually carries.
- **Session and traceback lexers** — `shell-session`, `console`, `pycon`,
  `doscon`, `ps1con`, `pytb`, ~200 blocks. Every one is a correctly spelled
  language describing a transcript rather than a syntax, and the grammar set has
  no equivalent for any of them. `is_unhighlightable` renders them plain and
  says nothing: reporting them would be blaming an author for our own gap, and
  200 unactionable warnings is how a diagnostic teaches people to ignore it.
- **A genuine gap** — `powershell`, 5 blocks. No grammar, not a session lexer.
  Still reported, deliberately.

245 warnings became 5, and the residual is kept rather than tuned away.

The reason is that catching a **typo** is the only thing this diagnostic uniquely
does, and from inside the renderer `powershell` and `pythn` are the same event:
a name with no grammar behind it. Silencing every unmatched name would buy zero
warnings at the price of never reporting a misspelled language again. The
session lexers could be silenced safely precisely because they are not
susceptible to that argument — a transcript lexer is a category the Sublime
grammar model has no answer to, so no entry on that list can ever be a typo.

Five warnings across ~2,000 code blocks is also well inside the threshold the
200 crossed; they are printed to stderr, feed neither `--strict-links` nor the
warnings sidecar, and fail no build. An author who does not care writes
`:force:`. So `is_unhighlightable` must stay a list of *categorically*
undrawable lexers, never a dumping ground for languages this bundle happens to
lack — that distinction is what keeps the diagnostic meaning anything.
