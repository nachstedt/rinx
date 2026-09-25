# 13. Templated sources: rendering an `.rst` as Jinja before parsing it

## Status

Accepted.

## Context

Real Sphinx projects template their documents, and there is no Sphinx feature
for it. What they do instead is connect the `source-read` event in `conf.py`
and run every file through Jinja2 on its way to docutils — the recipe from Eric
Holscher's "Integrating Jinja with RST", reached for to share a boilerplate
header, to generate repetitive sections from data, or to vary a page by build.

useblocks' sphinx-needs demo — the corpus `bazel run //scripts:benchmark_entities`
builds — opens 24 of its 31 documents with exactly two lines:

```text
{% set page="index.rst" %}
{% include "demo_page_header.rst" with context %}
```

rinx cannot run a `conf.py`, and will not: a documentation build whose
inputs are arbitrary Python is not a build Bazel can cache. Before this change
the two lines were not recognised as anything, so they parsed as the prose they
literally are and shipped as a stray `{% ... %}` paragraph at the top of 24
pages, with the shared header they exist to pull in appearing on none of them.

## Decision

### The transform is a declared feature, and it is opt-in

`rinx_library` gains `jinja = True` and a `jinja_context` string dict,
which is the `html_context` a Sphinx project passes to the same render. The
flags reach `parse` and `preview` as `--jinja` and `--jinja-context k=v ...`,
through the `ParseInputs` both already share.

Opt-in rather than always on, unlike the extension directives of ADR 11. A
directive name is a construct nobody writes by accident; `{{` and `{%` are
ordinary characters, and a page *about* templating — the CPython corpus has
several — would become a syntax error. Within an opted-in library the same
concern is handled by a cheaper rule: a document holding no Jinja delimiter at
all is not rendered, so only the documents that use the feature are subject to
it.

### The templates are `parse_data`, and nothing new

A template is a file the parser reads while parsing, which is the exact
definition of `parse_data` (ADR 8). It joins the parse action's inputs, an
undeclared one is absent from the sandbox and fails the build, and — the same
trap `.. include::` has — a template listed in `srcs` as well would be
published as a page of its own. No fifth dependency mechanism was added.

### A template name resolves from the source root

This is the one place where `{% include %}` and `.. include::` deliberately
disagree. A `.. include::` resolves against the file it is written in, as
docutils does. A template name resolves against the **source root**, as Jinja
does: the `FileSystemLoader` a `conf.py` builds is rooted at the Sphinx source
directory, so a project's shared header is named the same way from every
document however deep — which is what the corpus's 24 documents rely on.

### The rendered source is never materialized

It lives inside the parse action, between the read and the parse, and dies with
the process. Making it a pipeline phase of its own would mean emitting two
files — the rendered `.rst` and a serialized line map, which cannot be
reconstructed from the text — to split a microsecond transform off an action
that already reads the same inputs. There is no cache firewall to win here, the
way there is for the doctest plans (ADR 2) or the embedded image assets
(ADR 7): both actions would take identical inputs.

`parse` takes `--dump-rendered <path>` for the times somebody needs to see it
(`-` writes to standard output). No Bazel rule passes it, and `preview` does
not have it: its standard output *is* the page the editor reads.

### Every rendered line remembers where it was written

This is the part that is not "call MiniJinja".

A Jinja pass moves lines. An `{% include %}` splices a whole file in, so
everything below it shifts by however long that file is, and `MiniJinja` offers
no source map. Ignoring that would break more than a warning's cosmetics: the
parser's positions, `.. noqa:` suppression, and the `Span` stored on every AST
node for the *renderer* to report a broken `:ref:` against in a different
process all run through those numbers. ADR 8's rule applies unchanged —
confidently wrong is worse than positionless — but positionless would cost 24
pages every line number they have.

So `rinx_template` injects a `(template, line)` marker at the start of
every line that does not begin inside a tag, renders, and reads the markers
back off the output. Two rules decide the rest:

- **The last marker on a rendered line wins.** An `{% include %}` on its own
  line renders as that line's marker immediately followed by the included
  file's first line and its marker; the text visible there is the included
  file's, so that is what it is attributed to.
- **A line with no marker inherits the one above it**, unchanged rather than
  incremented: it was produced by a construct spanning several lines, and the
  construct is the thing worth pointing at.

`ParseCtx` gains that map, and it is not an offset. `Origin` is affine and
composes through every nested parse; a template map cannot, so it is consulted
once, at the end of `ParseCtx::position`, after every nested offset has been
added. `position` therefore returns a `SourcePoint` — a position *and* the file
it was measured in — and building a `Span` moved onto it as `SourcePoint::to`.
That keeps ADR 8's invariant intact in a stronger form: a span cannot be
measured in one file and named after another, because the file arrives already
attached to the point.

An `.. include::` fragment is **not** Jinja-rendered — Sphinx's `source-read`
fires for documents, not for transcluded text — so `ParseCtx::included` clears
the map and positions restart at line 1 of that file, exactly as before.

### A template that will not render is a diagnostic, not a failure

The parse then continues on the *unrendered* text. A broken template costs the
page its expansion and nothing else, which is the error-resilience the whole
parser is written for and what the live preview needs over a half-typed
document. Seven codes in the `jinja.*` family name the cases. The one thing
that does fail the build is a template that could not be *read*, and that
happens through the machinery `parse_data` already had: the loader records the
failure, and `cmd_parse` refuses to write an `.ast` when it did.

## Consequences

- `rinx_template` is a new leaf crate, a peer of `rinx_cdecl`
  and `rinx_filter`: a text-to-text step that depends on nothing of
  ours and learns what it cannot do itself through an injected
  `TemplateLoader`, which `rinx_parser` implements over the file loader
  an `.. include::` already reads through.
- The live preview gets the feature for free, since it shares `ParseInputs` —
  though the VS Code extension cannot yet *discover* that a site's library sets
  `jinja = True`, so a previewed template renders only when the setting is
  passed by hand.
- On the benchmark corpus the stray preamble paragraphs are gone, the shared
  header is spliced into all 24 documents, and `if-builder: 24` now appears in
  the unsupported-directive tally: the header's body is behind
  `.. if-builder:: html` (sphinx-simplepdf's), and an unknown directive never
  parses its body. Supporting it is a separate change.

## Narrowings

- **Whitespace-control modifiers (`{%-`, `-%}`) are refused** with
  `jinja.whitespace-control`. A line marker is not whitespace, so it would
  defeat the trim they ask for, and in reStructuredText a silently changed
  indent changes what a block contains.
- **A template name must be a quoted literal**
  (`jinja.dynamic-template-name`). Every file an action reads is declared
  before it runs, so a name that only exists mid-render could never be one of
  them — it would fail inside the sandbox instead, which is a worse place to
  learn it.
- **An undefined value is reported** (`jinja.undefined-value`), where Jinja2
  substitutes the empty string. A page silently missing a value it asked for
  looks finished, is wrong, and leaves nothing to grep for.
- **Templates resolve by the name written**, globally: two documents writing
  the same include name mean the same template.
- **Columns are rendered-text columns.** Lines are exact; on a line whose width
  a `{{ }}` substitution changed, the column is measured after substitution.
