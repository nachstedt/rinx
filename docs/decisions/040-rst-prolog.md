# 40. `rst_prolog` and `rst_epilog`

## Status

Accepted.

## Context

A Sphinx project can set `rst_prolog` and `rst_epilog` in `conf.py`: two
strings of reStructuredText that Sphinx adds to the start and the end of every
document. Legacy projects mostly use them for substitutions, `.. role::`
definitions and link targets that every page shares. rinx supported neither,
so every `|name|` defined there was a `substitution.undefined`, in the build
and in the editor (ADR-038's open question).

What Sphinx 9.1 does (`sphinx.util.rst.prepend_prolog`/`append_epilog`,
called from `RSTParser.decorate`):

- **The prolog goes after the docinfo.** The leading lines matching a field
  (`:orphan:`, `:tocdepth: 2`) stay first, so they are still metadata. A blank
  line is inserted after the docinfo and after the prolog.
- **The epilog goes after the last line**, after a blank line.
- **Positions name the text, not the document.** The inserted lines carry the
  source `<rst_prolog>` or `<rst_epilog>` and their line within the text, so a
  warning in them reads `<rst_prolog>:2: …`.
- **Only documents get them.** `decorate` runs once per document parse, so the
  text an `.. include::` splices in is not decorated again.
- **After `source-read`.** A Jinja hook on `source-read` has rendered the
  document before the prolog is added, so the prolog is not rendered.

## Decision

### Parsed as segments, not pasted into the text

`parse_with_ctx` parses three line slices in turn: the prolog, the document's
own lines (after its metadata), and the epilog. All three share the document's
adornment order, `Diagnostics` and `DocumentRoles` table, and every pass after
the block parse (index ids, substitutions, refused roles) runs over all of
them.

This gives the tree pasting would give. Sphinx puts a blank line at each seam,
and a blank line ends every block that could otherwise run across it. A
`.. role::` or `.. default-role::` in the prolog still applies to the
document, because the shared role table is filled in order. What it saves is
the line arithmetic: the document's own lines are never moved, so none of its
positions needs rebasing. The Jinja `TemplateMap` keeps working for the same
reason. The splice lives in `parser/blocks/splice.rs`.

### A segment is named by a `FileId`

Each non-empty segment interns a name in the document's `source_files` table,
and its spans carry that `FileId`, with lines counted from 1 in the text. That
is the mechanism an `.. include::`d fragment already uses. A warning therefore
names the text at its own line, and a `.. noqa:` written in a prolog applies
only there. The context for it is `ParseCtx::spliced`, not `included`: an
`.. include::` written in a prolog resolves against the document, because the
text is part of the document, as it is in Sphinx.

### Files in the build, strings in `conf.py`

The build takes the texts as files: `rinx_library(rst_prolog = "prolog.rst",
rst_epilog = "epilog.rst")`, passed to `parse` and `preview` as
`--rst-prolog`/`--rst-epilog` and declared as inputs of every parse action.
A file can be opened and highlighted as reStructuredText, and a diagnostic in
it names a real file and line: the build warns
`docs/prolog.rst:2:1 (included from docs/guide.rst): …`. A string attribute
would have meant multi-line Starlark strings and a `<rst_prolog>` that no
editor can open.

They are library attributes, not `rinx.toml` settings, for the reason
`default_role` is (ADR-037): they change the `.ast`, so they are parse inputs.
Editing one re-parses every document of the library.

The language server reads the strings from `conf.py`, through
`rinx_pyconf`, and names them as Sphinx does: `<rst_prolog>` and
`<rst_epilog>`. Those names are no path, so the editor places a diagnostic in
them at the start of the document it was parsed with, through the fallback
every unnameable file already gets.

## Consequences

- A project's shared substitutions, roles and targets work in both front ends.
- Every document's `.ast` grows by the prolog's and the epilog's nodes, and
  its `source_files` by their names. That is what Sphinx's doctree holds too.
- The language server reads only literal strings. CPython's `rst_epilog` is an
  f-string over an imported `version`, extended by `+=` inside an `if`, so it
  is reported as `conf.unread-setting` and its substitutions stay undefined.
  Running `conf.py` (roadmap #16) is what would read it.
- A mistake in a prolog is reported once per document, as in Sphinx.

## Alternatives considered

- **Pasting the text into the document's lines.** This is what Sphinx does,
  but every position below the prolog would then need an offset, and a
  per-line source map like Jinja's for the prolog's own lines. Segments give
  the same tree without either.
- **Positionless (`ctx.synthetic()`) segments.** This would be simpler, but a
  mistake in a prolog would then point at no line of anything.
- **String attributes on `rinx_library`.** These are closer to `conf.py`, but
  rejected for the reasons under "Files in the build, strings in `conf.py`".
