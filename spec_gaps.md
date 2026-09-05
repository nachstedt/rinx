# RST / Sphinx Specification Gaps

This file tracks known deviations and missing features relative to the full
[reStructuredText specification](https://docutils.sourceforge.io/rst.html) and
[Sphinx documentation system](https://www.sphinx-doc.org/).

It serves as a living checklist: entries should be updated or removed when
the corresponding feature is implemented. Notes are only added when a
feature is partially implemented or behaves differently from real Sphinx —
not to describe how something is implemented.

Legend: ✅ implemented · 🔶 partial · ❌ not implemented

## Heading Adornments

| Feature | Status | Notes |
|---------|--------|-------|
| Underline-only headings | ✅ | — |
| Overlined headings (`===` above **and** below the text) | ✅ | Treated as a distinct adornment style from a same-character underline, so it can be assigned its own heading level |
| Per-document level reset vs. project-wide level tracking | 🔶 | Heading *levels* still reset per file. Section *numbering* now does accumulate across files linked via `toctree`, so `:numbered:` matches Sphinx even though the underlying level assignment is per-document |
| Inline markup/roles inside heading text (e.g. `` :mod:`x` ``, `**bold**`) | ✅ | — |

## Inline Markup

| Feature | Status | Notes |
|---------|--------|-------|
| Bold (`**text**`) | ✅ | — |
| Italic (`*text*`) | ✅ | — |
| Inline code (`` `text` ``) | ✅ | — |
| `` :ref:`label` `` cross-reference role | ✅ | — |
| `` :term:`glossary term` `` cross-reference role | ✅ | — |
| `` :program:`name` `` role | ✅ | Renders as highlighted text (`<strong class="program">`); not a cross-reference/lookup role |
| Domain-object role target modifiers: `!target`, `~target`, `.target`, trailing `target()` | ✅ | — |
| Hyperlink references (`link text <URL>`_) | ✅ | — |
| Anonymous hyperlinks (`link text <URL>`__) | ✅ | — |
| Smart typography (`---` → em dash, `--` → en dash, `...` → ellipsis) | ✅ | Not applied inside inline literals, `:program:`, code blocks, or glossary terms |
| Inline-markup recognition rules (start/end-string context) | 🔶 | A start-string must open the text or follow whitespace/an opener/a delimiter, and an end-string must end the text or precede whitespace/a closer/a delimiter, using docutils' own Unicode character classes (`crates/parser/src/punctuation.rs`). docutils' rule 5 — rejecting a marker wrapped in a *matching* quote pair, e.g. `"*"` — is not implemented |
| `` :any:`target` `` cross-reference role (finds any cross-reference target) | ❌ | — |
| `` :doc:`page` `` cross-reference role (links to another document) | ❌ | — |
| `` :download:`file` `` role (links to a downloadable file) | ❌ | — |
| `` :numref:`target` `` cross-reference role (numbered reference to a figure/table/section) | ❌ | — |
| `` :code:`text` `` role (inline code with an optional highlighted language) | ❌ | — |
| `` :math:`...` `` role (inline math) | ✅ | LaTeX is converted to `MathML` at build time, so no JavaScript runs and no webfont is downloaded, and invalid LaTeX is a build warning (`math.invalid-latex`) rather than a silent failure in the reader's browser. Brackets that stretch to fit a matrix or `cases` need a locally installed math font (the stylesheets name the common ones); without one the equation is still correct and readable, but its brackets stay at character size |
| `` :eq:`label` `` role (equation-number reference) | ✅ | Resolves across documents; renders the target equation's number as the link text |
| `` :pep:`num` `` / `` :rfc:`num` `` roles (generate a URL to the PEP/RFC) | ❌ | — |
| `` :cve:`num` `` / `` :cwe:`num` `` roles (generate a URL to the CVE/CWE entry) | ❌ | — |
| `` :index:`term` `` role (inline index entry, distinct from the `.. index::` directive) | ❌ | — |
| `` :sub:`text` ``/`` :subscript:`text` ``, `` :sup:`text` ``/`` :superscript:`text` `` roles | ❌ | — |
| Default role for unadorned `` `text` `` (single backtick, no role prefix — resolves to title-reference in real docutils/Sphinx) | ❌ | — |
| Backslash escapes (`\.`, `\*`, …) outside inline-markup context | ✅ | Ported from docutils' two-phase model (`crates/parser/src/escapes.rs`): escapes become markers before markup recognition and are removed when text is emitted. Escaped whitespace disappears entirely (`foo\ *bar*`), inline literals keep their backslashes verbatim, and smart typography sees the escaped form so `\-\-` stays two hyphens |
| Semantic markup roles: `:abbr:`, `:command:`, `:dfn:`, `:file:`, `:guilabel:`, `:kbd:`, `:mailheader:`, `:makevar:`, `:manpage:`, `:menuselection:`, `:mimetype:`, `:newsgroup:`, `:regexp:`, `:samp:` | ❌ | Real-Sphinx text-formatting-only roles (no cross-reference target); none render any HTML wrapper here |

Domain-object cross-reference roles (`:func:`, `:mod:`, `:meth:`, `:class:`, `:attr:`, `:exc:`, `:macro:`, `:struct:`, `:union:`, `:type:`, `:data:`/`:const:`/`:var:`/`:member:`) and `:option:` are tracked per-role under the domain they belong to in the `## Domains` section below, not in this table.

## Block-Level Elements

| Feature | Status | Notes |
|---------|--------|-------|
| Bullet lists (`-`, `*`, `+`) | ✅ | — |
| Enumerated lists (`1.`, `a.`, `i.`, …) | ✅ | All five sequences (arabic, lower/upper alpha, lower/upper roman), all three formats (`1.`, `1)`, `(1)`), the `#` auto-enumerator, and non-1 start values. Deliberate divergence from Sphinx: the prefix/suffix are *rendered* (via a format class plus CSS counters) rather than dropped, so `(a)` and `a.` are distinguishable in the HTML |
| Definition lists | 🔶 | Term classifiers (`term : classifier`) not supported; glossary's "multiple terms share one definition" quirk uses a separate implementation |
| Field lists (`:field: value`) | 🔶 | Only the document-leading field list is read, as file-wide metadata, and only `:orphan:` is interpreted (it silences `toctree.orphan-document`). Values are kept as raw strings and nothing is rendered; field lists elsewhere in a document are unsupported |
| Option lists | ✅ | Grammar matches docutils' `option_marker` transition exactly: short (`-x`)/long (`--xxx`)/old-GNU (`+x`)/DOS-VMS (`/x`) markers, comma-separated synonyms, and all three argument delimiters (space, `=`, and the adjacent short form `-xVALUE`), including a bracketed `<...>` placeholder that may itself contain spaces. Rendered as modern docutils/Sphinx's `<dl class="option-list">` HTML5 markup, not the legacy `html4css1` writer's `<table class="option-list">` |
| Literal blocks (``::`` paragraph ending or standalone ``::``) | ✅ | Highlighted with whatever language `.. highlight::` last set, as in Sphinx; the site-wide default comes from `highlight_language` in `rusty_sphinx.toml` |
| Block quotes (indented paragraphs without a directive) | ✅ | Indentation is checked ahead of every marker-based construct (matching docutils' own precedence), so an indented list, table or directive nests inside a block quote rather than being matched in place. Attributions (`-- Author`, `--- Author`, or a true em-dash, flush left, optionally spanning several consistently-indented lines) are recognized and parsed as inline markup. Deliberate narrowing: an indented comment (`.. `) is still recognized in place rather than nesting inside a block quote, since comment parsing happens structurally before construct dispatch (for the `.. noqa:` suppression-scoping rule). `.. epigraph::` / `.. highlights::` / `.. pull-quote::`, tracked separately below, remain unimplemented |
| Line blocks | ✅ | Nesting-by-indentation and multi-physical-line wrapped continuations both follow docutils' own algorithm (`nest_line_block_segment`) |

## Tables

| Feature | Status | Notes |
|---------|--------|-------|
| Grid tables (ASCII art) | ✅ | Column/row spans and hierarchical/nested headers supported |
| Simple tables (whitespace-column `===` style) | ✅ | Header rules, `-` column-span underlines, multi-line cells, the unbounded rightmost column, and the escaped space `\ ` for a deliberately empty first-column cell |
| `.. list-table::` | ✅ | `:name:` resolves to the document, not a fragment precisely at the table (same limitation as every other internal `:ref:` target); shares one AST node and renderer with `csv-table` |
| `.. csv-table::` directive | 🔶 | Inline data, `:file:`, `:header:`, `:delim:`, `:quote:`, `:escape:`, `:keepspace:` and every `list-table` option supported. `:url:` unsupported (a network fetch would make the build non-hermetic); `:encoding:` limited to UTF-8 and its ASCII subset; `:delim:`/`:quote:`/`:escape:` limited to ASCII characters; a `:file:` must be declared in the Bazel library's `parse_data` attribute, and an unreadable one fails the build |
| `.. table::` (table title/caption/options) | ✅ | Wraps an existing grid or simple table; `:name:` has the same document-not-fragment limitation as `list-table`'s |

## Directives

| Feature | Status | Notes |
|---------|--------|-------|
| `.. toctree::` | ✅ | All nine options (`:maxdepth:`, `:numbered:`, `:caption:`, `:name:`, `:titlesonly:`, `:glob:`, `:reversed:`, `:hidden:`, `:includehidden:`) and all four entry forms (plain, `Title <target>`, `self`, external URL) supported, including in-document section entries, project-wide section numbering rendered into page headings, and prev/next page relations. Deliberate narrowing: a `:glob:` pattern treats `{a,b}` as literal, matching Sphinx rather than the underlying glob library |
| `.. code-block::` | ✅ | All options supported (`:linenos:`, `:lineno-start:`, `:emphasize-lines:`, `:caption:`, `:name:`, `:dedent:`, `:class:`, `:force:`), with syntax highlighting performed during the build into classed HTML (see `docs/decisions/006-syntax-highlighting.md`). Three deliberate narrowings: a `:caption:` is plain text rather than parsed inline markup, matching what the table directives already do with theirs; `pycon` (Pygments' console-session lexer, used for `.. doctest::` blocks) has no equivalent among the bundled Sublime grammars, so interactive blocks render unhighlighted; and a handful of grammars cannot be compiled by the pure-Rust regex backend, which reports `code-block.highlight-failed` and falls back to plain text. Pygments' session and traceback lexers (`pycon`, `console`, `shell-session`, `doscon`, `ps1con`, `pytb`) have no Sublime equivalent either, and render as plain text *without* a diagnostic — they are correctly spelled languages this backend cannot draw, so the gap is ours rather than the author's. Measured over the CPython corpus this leaves 5 warnings in ~2,000 code blocks, all for `powershell` |
| `.. code::` (docutils' spelling of `.. code-block::`) | ✅ | Shares one AST node with `.. code-block::`, tagged with which directive wrote it. Its own option vocabulary is honored rather than Sphinx's: `:number-lines:` (with an optional start value), `:class:` and `:name:`. Each directive accepts only its own spellings, so a `:linenos:` on a `.. code::` is reported as an unknown option, and a `:number-lines:` on a `.. code-block::` likewise |
| `.. index::` (`single`/`pair`/`triple`/comma-shorthand entries, `!main` marker) | ✅ | — |
| `.. index::` `see`/`seealso` entry types | 🔶 | Parsed but not yet surfaced in the rendered general index |
| `.. note::`, `.. warning::`, `.. tip::` (admonitions) | ✅ | All docutils admonition kinds supported (`attention`, `caution`, `danger`, `error`, `hint`, `important`, `note`, `tip`, `warning`), plus generic `.. admonition:: Title` and a `:collapsible:` option |
| `.. seealso::` | ✅ | — |
| `.. versionadded::`, `.. versionchanged::`, `.. deprecated::` (version-change admonitions) | 🔶 | `.. versionremoved::` is not supported |
| `.. image::` | 🔶 | Every option supported (`:alt:`, `:height:`, `:width:`, `:scale:`, `:align:`, `:target:`, `:class:`, `:name:`, `:loading:`), for a document-relative path, a source-root-relative `/path`, or an external URL. `:loading: embed` really inlines the file as a `data:` URI, read by a separate per-document build step so the bytes never enter a `.ast` and an image edit re-renders only the pages that embed it (see `docs/decisions/007-image-assets.md`). Four deliberate narrowings: a bare `:scale:` with no `:width:`/`:height:` is dropped with an `image.scale-no-dimensions` warning, since docutils answers it by opening the file and this build never reads an image while rendering; the three *vertical* `:align:` values are refused, because docutils accepts them only inside a substitution definition, which is unimplemented here; `:loading: embed` on an external URL is refused (`image.embed-external`) and linked instead, since fetching it would make the build non-hermetic; and Sphinx's `.. image:: logo.*` candidate-selection wildcard is unsupported, presuming a source-tree glob that the explicitly-declared-inputs model deliberately does not have. Bundled images keep their source-root-relative path under `_images/` rather than being flattened to a basename with Sphinx's global collision counter |
| `.. figure::` (image with a caption/legend) | 🔶 | Every `.. image::` option above, plus `:figwidth:` and `:figclass:`, and the full body grammar: the first paragraph becomes the caption (parsed as inline markup, unlike the plain-text captions the code-block and table directives take), everything after it is the legend, and a leading empty comment (`..`) suppresses the caption so the whole body is legend. `:figwidth: image` is emitted as CSS `width: fit-content` rather than the image's measured pixel width — the same result, arrived at without opening the file. Shares every narrowing listed for `.. image::` |
| `.. include::` | 🔶 | Splices another file's reStructuredText into the document *at parse time*, so its sections, targets, toctree entries and index entries belong to the including document exactly as if typed there. All eleven options supported (`:start-line:`, `:end-line:`, `:start-after:`, `:end-before:`, `:literal:`, `:code:`, `:encoding:`, `:tab-width:`, `:number-lines:`, `:class:`, `:name:`); nested includes resolve relative to the file they are written in, and a cycle is reported with the whole chain rather than recursing. A diagnostic found inside an included file names *that* file, and a `.. noqa:` only silences diagnostics from the file it was written in (see `docs/decisions/008-source-transclusion.md`). Two deliberate narrowings: `:parser:` (pluggable alternate markup parsers) is unsupported, and `:encoding:` is limited to UTF-8 and its ASCII subset, matching `csv-table`. The file must be declared in the Bazel library's `parse_data` attribute, and must *not* also appear in `srcs`, which would publish it as a page of its own |
| `.. literalinclude::` | 🔶 | Lowers to the same AST node and renderer as `.. code-block::`, since every option it adds is a parse-time source transform. Supported: `:language:` (an option here, not the argument; absent inherits from `.. highlight::`), `:lines:`, `:start-after:`, `:end-before:`, `:start-at:`, `:end-at:`, `:prepend:`, `:append:`, `:dedent:`, `:lineno-match:`, `:linenos:`, `:lineno-start:`, `:emphasize-lines:`, `:caption:` (bare, it defaults to the filename), `:name:`, `:class:`, `:force:`, `:encoding:`, `:tab-width:`, and `:diff:`, which really does compute a unified diff during the build. Every way of selecting nothing — an unmatched `:start-after:`, a `:lines:` past the end of the file, a range that ends before it begins — is its own diagnostic rather than a silently empty block. Three deliberate narrowings: `:pyobject:` is refused with `literalinclude.pyobject-unsupported` (it would need a Python parser; `:start-after:`/`:end-before:` do the same job), `:lineno-match:` on a `:diff:` or a discontinuous `:lines:` is refused since there is no single line to number from, and `:encoding:` is UTF-8 only. Same `parse_data` requirement as `.. include::` |
| `.. doctest::`, `.. testcode::`, `.. testoutput::`, `.. testsetup::`, `.. testcleanup::` (`sphinx.ext.doctest`) | 🔶 | Parsed and rendered but not executed as part of the site build — execution is a separate, opt-in `bazel test` target (see `docs/decisions/002-doctest-execution.md`). `:pyversion:` is parsed but deliberately not evaluated at parse time |
| Doctest blocks (a text block starting with `>>> `, no directive) | ✅ | Unlike the `.. doctest::` directive family above, these are executed by default, matching Sphinx |
| `.. automodule::` / `.. autofunction::` (autodoc) | ❌ | Out of scope — would require importing user code to generate content |
| `.. contents::` (local/on-page table of contents) | ✅ | The full docutils spec (Sphinx defers to it as-is): the optional title argument, `:depth:`, `:local:`, `:backlinks:` (`entry`/`top`/`none`), `:class:` and `:name:`. `:local:` finds its enclosing section at render time via a backward scan, not via a stored index (see `crates/renderer/src/blocks/contents.rs`'s module doc comment); a `.. contents::` written *after* the sections it covers cannot backlink them, since rendering is a single left-to-right pass. Not to be confused with `:no-contents-entry:` below, a distinct Sphinx mechanism (`toc_object_entries`) for nesting object descriptions into the *sidebar* navigation, unrelated to this directive |
| `.. sectnum::` / `.. section-numbering::` (automatic section numbering) | ✅ | Both spellings share one implementation — docutils' own alias agrees on every option, unlike `.. code-block::`/`.. code::`. All four options supported (`:depth:`, `:start:`, `:prefix:`, `:suffix:`), numbering that one document's own sections independently of every other document, exactly as plain docutils does; an argument is diagnosed rather than dropped, matching docutils, which declares zero argument slots for this directive and really does reject one given anyway (as an inline directive error). Interacts with `.. toctree::`'s `:numbered:` the same way a nested `:numbered:` toctree is ignored inside an already-numbered subtree: a document already numbered by an ancestor `:numbered:` toctree ignores its own `.. sectnum::` entirely, and — unlike a `:numbered:` toctree — `.. sectnum::` never gives the document itself a number, only its sections, so a sectnum-numbered document's title heading stays unnumbered. Four deliberate narrowings: `:depth: 0` is unlimited rather than disabling numbering, matching `.. contents::`'s own `:depth:`; `:start: 0` is rejected (kept at the default of `1`) since there is no sensible number to display there, unlike `:depth:`; an unset `:suffix:` is the empty string rather than docutils' non-breaking space, since the renderer already appends its own separating space after a rendered number; and, like every other option in this build, `:prefix:`/`:suffix:` are trimmed of leading/trailing whitespace, so a value meant to carry a trailing/leading space (docutils' own examples often prepend `"Appendix "` this way) needs surrounding punctuation instead (see `//examples/sectnum`) |
| `.. topic::` (self-contained boxed section) | ❌ | — |
| `.. sidebar::` (parallel mini-document alongside the main text) | ❌ | — |
| `.. rubric::` (informal heading outside the document's section structure) | ❌ | — |
| `.. epigraph::` / `.. highlights::` / `.. pull-quote::` (block-quote styling variants) | ❌ | — |
| `.. compound::` / `.. container::` (generic grouping directives, no inherent styling) | ❌ | — |
| `.. parsed-literal::` (literal block with inline markup parsed) | ❌ | — |
| `.. raw::` (pass content untouched to a specific output format) | ❌ | — |
| `.. class::` / `.. rst-class::` (sets a CSS class on the following element) | ❌ | — |
| `.. role::` / `.. default-role::` (define/select a custom interpreted text role) | ❌ | — |
| `.. meta::` (HTML `<meta>` tags) | ❌ | — |
| `.. centered::` (deprecated in real Sphinx in favor of `.. rst-class:: centered`) | ❌ | — |
| `.. hlist::` (compact multi-column bullet list) | ❌ | — |
| `.. highlight::` (sets the default `.. code-block::` language for following blocks) | 🔶 | The language argument, `:linenothreshold:` and `:force:` are all supported, and apply to `::` literal blocks as well as to the directive forms. Deliberate narrowing: the language applies to the rest of the document in source order, where Sphinx scopes it to the enclosing block-level container — so a `.. highlight::` inside a list item or admonition body keeps applying after that body ends |
| `.. math::` (display math block) | 🔶 | `:label:`, `:name:` (a spelling of `:label:`), `:nowrap:`/`:no-wrap:` and `:class:` supported, as are the argument form (`.. math:: a = b`), blank-line-separated multi-equation bodies and `\\`-aligned multi-line equations. Three deviations: Sphinx wraps a multi-line equation in `split`, which the `MathML` converter does not implement, so `aligned` is emitted instead (same alignment); `math_number_all` and `math_numfig` are unsupported, so only labeled equations are numbered and numbers restart per document rather than being section-scoped (`(1.2)`); and under `:nowrap:` a self-numbering environment such as `align` restarts its own count in every directive, since each block is converted independently |
| `.. productionlist::` (formal grammar production rules) | ❌ | — |
| `.. only::` (conditionally includes content based on build tags) | ❌ | — |
| `.. sectionauthor::` / `.. moduleauthor::` / `.. codeauthor::` (author metadata; no visible output by default in real Sphinx either) | ❌ | — |
| `.. target-notes::` (footnote-style listing of link targets, for hardcopy output) | ❌ | — |

## Domains

### `py` domain

| Feature | Status | Notes |
|---------|--------|-------|
| `.. py:function::` (function object type) | ✅ | — |
| `:func:` cross-reference role | ✅ | Resolves to `.. py:function::`; the same role also resolves to `.. c:function::` in the `c` domain (see below) |
| `.. py:module::` (module object type) | ✅ | Options `:platform:`, `:synopsis:`, `:deprecated:` supported |
| `:mod:` cross-reference role | ✅ | Resolves to `.. py:module::` |
| `.. py:data::` (data object type) | ✅ | Options `:type:`, `:value:` supported |
| `:data:`/`:const:` cross-reference role | ✅ | Resolves to `.. py:data::` |
| `.. py:method::` (method object type) | 🔶 | Options `:classmethod:`, `:staticmethod:`, `:abstractmethod:`, `:async:` supported; `:property:`/`:no-index:` not supported |
| `:meth:` cross-reference role | ✅ | Resolves to `.. py:method::` |
| `.. classmethod::` / `.. staticmethod::` (legacy directive aliases for `.. py:method::`) | ✅ | — |
| `.. py:class::` (class object type) | 🔶 | `:canonical:`/`:no-index:` and other real-Sphinx options not supported |
| `:class:` cross-reference role | ✅ | Resolves to `.. py:class::` |
| `.. py:attribute::` (attribute object type) | ✅ | Options `:type:`, `:value:`, `:canonical:` supported; `:canonical:` is rendered as plain metadata only — no alias/redirect semantics |
| `:attr:` cross-reference role | ✅ | Resolves to `.. py:attribute::` |
| `.. py:exception::` (exception object type) | 🔶 | Signature-line-wrapping options (`:single-line-parameter-list:`, etc.) not supported — no multi-line signature wrapping exists |
| `:exc:` cross-reference role | ✅ | Resolves to `.. py:exception::` |
| `.. py:currentmodule::` (sets module context for subsequent cross-referencing without documenting a module itself) | ✅ | — |
| `:module:` option (available on every `py:*` object-description directive) | ✅ | — |
| `py` domain role-target aliasing (`:exc:`/`:class:` resolving against either a `.. class::` or `.. exception::` definition of the same name) | 🔶 | Real Sphinx's third alias, `:obj:`, is not supported — only `class`/`exception` alias each other |
| Nesting-based cross-reference qualification (object directives nested inside a `py:class`/`py:exception` body) | ✅ | — |
| Module-based cross-reference qualification (`py:function`/`py:class`/etc. documented as a sibling after `.. py:module::`) | ✅ | Still unresolved: a role whose requested type doesn't match the definition's (e.g. `:meth:`~random.seed`` pointing at a `py:function`) — real Sphinx also warns about this case |

### `c` domain

| Feature | Status | Notes |
|---------|--------|-------|
| `.. c:function::` (function object type) | ✅ | — |
| `:func:` cross-reference role | ✅ | Resolves to `.. c:function::`; the same role also resolves to `.. py:function::` in the `py` domain (see above) |
| `.. c:macro::` (macro object type) | ✅ | Aliased with `function` for cross-referencing, matching real Sphinx's own macro/function ambiguity |
| `:macro:` cross-reference role | ✅ | Resolves to `.. c:macro::` |
| `.. c:struct::` (struct object type) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect — that option controls Sphinx's `toc_object_entries` sidebar-nesting mechanism, which is unrelated to (and, like it, still unimplemented apart from) the now-implemented `.. contents::` directive. `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:struct:` cross-reference role | ✅ | Resolves to `.. c:struct::` |
| `.. c:union::` (union object type) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect — that option controls Sphinx's `toc_object_entries` sidebar-nesting mechanism, which is unrelated to (and, like it, still unimplemented apart from) the now-implemented `.. contents::` directive. `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:union:` cross-reference role | ✅ | Resolves to `.. c:union::` |
| `.. c:member::` (member object type) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect — that option controls Sphinx's `toc_object_entries` sidebar-nesting mechanism, which is unrelated to (and, like it, still unimplemented apart from) the now-implemented `.. contents::` directive. `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:member:` cross-reference role | ✅ | A `:c:macro:`/`:c:data:` naming collision resolves via alias with a soft type-mismatch warning rather than a broken link, matching real Sphinx's own permissive behavior here |
| `.. c:var::` (directive-name alias of `.. c:member::`) | 🔶 | `:no-contents-entry:` is parsed but has no rendering effect — that option controls Sphinx's `toc_object_entries` sidebar-nesting mechanism, which is unrelated to (and, like it, still unimplemented apart from) the now-implemented `.. contents::` directive. `no-typesetting` not modeled. `enum`/`enumerator` object types not implemented |
| `:var:` cross-reference role | ✅ | A `:c:macro:`/`:c:data:` naming collision resolves via alias with a soft type-mismatch warning rather than a broken link, matching real Sphinx's own permissive behavior here |
| `:data:` cross-reference role | ✅ | A `:c:macro:`/`:c:data:` naming collision resolves via alias with a soft type-mismatch warning rather than a broken link, matching real Sphinx's own permissive behavior here |
| `.. c:type::` (type object type) | ✅ | — |
| `:type:` cross-reference role | ✅ | Resolves to `.. c:type::` |
| `.. c:namespace::` (sets the active C namespace) | ✅ | — |
| `.. c:namespace-push::` (pushes a namespace onto the active stack) | ✅ | One deliberate deviation: an unmatched `c:namespace-push` inside a body is contained by that body's close rather than leaking out to a later unrelated pop |
| `.. c:namespace-pop::` (pops the most recently pushed namespace) | ✅ | See `.. c:namespace-push::`'s note for the one behavioral deviation in this family |
| Object name extraction from C declarations (shared by `c:function`/`c:macro` signature parsing, e.g. `char *foo(void)`, `int (*Py_tracefunc)(PyObject *obj)`) | 🔶 | Multiple declarators per signature, K&R definitions, a real expression grammar in array sizes/initializers, and macro wrappers like `PyAPI_FUNC(...)` are not supported — falls back to a name-extraction heuristic and emits a diagnostic when the declaration doesn't parse |

### `std` domain

| Feature | Status | Notes |
|---------|--------|-------|
| `.. option::` (cmdoption object type) | ✅ | Cross-reference keys are case-insensitive, so e.g. `-X` and `-x` would collide (pre-existing limitation, not std-domain-specific) |
| `.. cmdoption::` (legacy alias of `.. option::`) | ✅ | Cross-reference keys are case-insensitive, so e.g. `-X` and `-x` would collide (pre-existing limitation, not std-domain-specific) |
| `.. program::` (sets the "current program" context `.. option::`/`:option:` qualify against) | ✅ | — |
| `:option:` cross-reference role | ✅ | — |
| `.. envvar::` (environment-variable object type) | ❌ | — |
| `:envvar:` cross-reference role | ❌ | — |
| `.. confval::` (configuration-value object type) | ❌ | — |
| `:confval:` cross-reference role | ❌ | — |
| `.. describe::` (generic formatted description; no index entry or cross-reference target) | ❌ | — |
| `.. object::` (generic object description; no cross-reference target) | ❌ | — |
| `:token:` cross-reference role (references a grammar token defined by `.. productionlist::`) | ❌ | — |

### Cross-domain behavior

| Feature | Status | Notes |
|---------|--------|-------|
| Multiple signatures per definition directive (several argument lines under one directive) | ✅ | — |
| Default domain resolution for bare (unprefixed) directives/roles | ✅ | — |
| Target resolution order (leading-dot `refspecific` targets, and the suffix fallback) | ✅ | Two deliberate deviations from real Sphinx: (1) the requested object type is checked in every search tier, not just dot-prefixed targets, so a role like `:func:`Thread.run`` is reported rather than silently linked to a method; (2) an ambiguous suffix match does not resolve at all, where Sphinx warns but links the first candidate |
| Explicit `` :role:`title <target>` `` override for domain-object roles | ✅ | — |
| Cross-referencing domain objects (and targets/glossary terms) nested inside tables, lists, or admonition/version-change/seealso bodies | ✅ | — |
| Automatic general-index entries for domain object definitions | ✅ | Entry text uses a simplified format (e.g. `"Greeter.greet (method)"`) rather than Sphinx's fuller `"coroutine() (in module types)"` phrasing |
| `preview` subcommand default-domain awareness | 🔶 | `preview` accepts a `--default-domain` flag identical to `parse`'s, but the VS Code extension never queries the enclosing `rusty_sphinx_library`'s `default_domain` Bazel attribute or passes it through, so editor-driven previews always default to `py` |

## General Index

| Feature | Status | Notes |
|---------|--------|-------|
| Site-wide `genindex.html` page | ✅ | Own markup, not visually identical to Sphinx's default theme |
| "Index" sidebar link | ✅ | — |

## Document Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Sections and document tree (nested sections) | 🔶 | Heading levels are detected; no explicit section nesting in the AST |
| Transitions (`----`) | ✅ | No section-boundary validation, since sections aren't nested in the AST yet |
| Comments (`.. comment text`) | ✅ | — |
| Substitution definitions (`.. |name| replace::`/`unicode::`/`image::`) | 🔶 | `replace` (content inline-parsed as a paragraph, so it may itself carry markup or nested substitution references), `unicode` (decimal/hex/`U+`/`\u`/`\x`/XML-entity codepoints, plus `:trim:`/`:ltrim:`/`:rtrim:`) and `image` (reusing every `.. image::` option, additionally accepting the three vertical `:align:` values and refusing `:name:`) are fully supported, resolved by a whole-document pass that runs once parsing finishes so a reference may precede its definition. Matching is case-sensitive first, falling back to a case-insensitive one when exactly one candidate matches; a duplicate definition, an undefined reference and a circular `replace` chain are all diagnosed and degrade rather than failing the build. `.. date::` is deliberately unsupported: resolving it would bake the wall-clock time into a `.ast` file, which is a Bazel action cached on its inputs alone, breaking the hermeticity the rest of the build is built around. `.. raw::` is unsupported because the standalone `.. raw::` directive it would share a content model with is itself unimplemented (see the row below) |
| Footnotes and citations | ❌ | — |

## Diagnostics

Reporting behaviour rather than RST coverage. Sphinx has no equivalent of the
last two rows — they are rusty-sphinx extensions, described in
[ADR-003](docs/decisions/003-diagnostics.md).

| Feature | Status | Notes |
|---------|--------|-------|
| Source positions on warnings | ✅ | Every warning reports `path:line:column`, both parse-time and render-time. Positions are ranges internally; the terminal prints the start |
| Positions inside nested constructs | ✅ | Resolve correctly through directive bodies, all three list kinds, glossary definitions and table cells, at any nesting depth |
| Positions for `.. math::` LaTeX errors | 🔶 | The `.. math::` directive is the only one whose AST node carries a span, so a LaTeX error found while rendering can be placed at all. It points at the directive body's first line rather than at the offending character: by then the equation has been rejoined from several lines, so no finer position survives |
| Positions for `.. csv-table::` cell content | 🔶 | Rows built from CSV data correspond to no `.rst` line (a `:file:`'s data is not even in the document), so diagnostics raised inside a cell are reported without a position rather than against a wrong one |
| Stable diagnostic codes (`link.broken-ref`, `table.grid.no-columns`, …) | ✅ | One per reporting site; part of the documented surface once written into a `.. noqa:` |
| `.. noqa:` suppression comments | ✅ | **rusty-sphinx extension.** Silences the named codes for the block that follows; a bare `.. noqa` covers every code. Resolves against the enclosing block when nested. Suppresses the warning, the `--strict-links` failure and the sidecar entry alike |
| `noqa.unknown-code` for a mistyped id | ✅ | **rusty-sphinx extension.** A typo is reported rather than silently suppressing nothing |
| `noqa.unused` for a suppression that matched nothing | ❌ | Undecidable in one phase: parse-time and render-time diagnostics are evaluated in different processes |
