# 46. Rename rewrites every place or none

## Status

Accepted.

## Context

Roadmap #31 gives the language server F2: rename a label, an entity id or an
equation's label everywhere the project writes it. Find-references (#30)
already lists every use of a definition, resolved as the page links it, and
ADR-045 gives every definition its site. What a rename adds is a write, and a
write that goes wrong does what no read can: it leaves a link broken in a file
the author is not looking at, with nothing on the screen to say so.

Three things stood between the uses #30 lists and a safe rewrite:

- **Where the name stands.** A use's span is the whole construct, the role
  `` :ref:`Title <install>` ``, and a definition's site its whole line.
- **Uses #30 did not list.** A named reference (`install_`) resolves within
  its page through `DocumentHyperlinkTargets`, not the index, and an
  `.. entity-update::` argument is no relation option. Renaming the label
  would have broken both.
- **Whether the text still says what the analysis read.** The server's
  analysis of a closed document comes from the disk as it last read it; a
  Jinja-rendered document's columns are off by every substitution on its line
  (issue #321); an `.. include::` places a fragment's uses in another file.

## Decision

### The parser says where each name was written

Two preparatory pull requests (#387, #390) record what the server would
otherwise have to find again with a second reader of the grammar:

- `rinx_ast::Written<T>` (since replaced by `Named<T>`, a handle into the
  parse's `SourceMap`; see ADR-045's second amendment) is a value with the
  span of exactly its characters:
  a label after `.. _`, every directive's `:name:`, a `.. math::` `:label:`,
  an entity's `:id:` and an `.. entity-update::`'s id argument. The span is
  never serialized and equality ignores it, so the `.ast` and the `.index`
  are unchanged (ADR-045's amendment). An entity id has one only when it was
  written as `:id:`, which is how a composed, generated or imported id is told
  apart.
- `rinx_renderer::written_uses` lists named references with the alias chain
  each follows in its own page, and the server lists each id argument of an
  `.. entity-update::`.

A role's target is found inside its backquotes by
`rinx_parser::role_target_range`, built on the very bracket search
`split_explicit_title` reads a role with.

### Every place is checked, and a rename is all or nothing

The server collects the definition's name and every use, narrows each to the
name's characters by how it was written (`UseForm`: a role, a named reference,
an id on a relation line, an exact name), reads each from the open buffer or
the disk, and requires what stands there to name the definition still: the
label as the parser normalizes it, or the id. If any place fails, the rename
is refused, naming the file and line. One rule thereby covers a stale disk, a
Jinja line, an escape the narrowing did not model and a construct it does not
know, without a case for each.

The same holds for the other refusals, each an error the editor shows: a
document, section, term or domain object; a name two documents define; an id
not written as `:id:`; a new name already defined, or one a label cannot hold;
a use in a `conf.py`'s `rst_prolog` or `rst_epilog`; a place outside every
workspace folder.

### A named reference's text is its name

In `` `Install`_ `` the text is both what the reader sees and the name it
refers to; docutils has no separate target. A rename rewrites it, changing the
visible text, as renaming any identifier changes what is written. A bare
`install_` gains backquotes when the new name is not a simple reference name.
A reference that reaches the label through an alias (`docs_` with
`.. _docs: install_`) keeps naming the alias, whose own line is the use that
is rewritten.

### An entity is one definition, however it is named

`UseTarget::of` names the entity for a `:ref:` to an entity's label, as for an
entity role, so find-references and rename treat `` :ref:`req_1` `` and
`` :need:`REQ_1` `` alike. A rename writes the new id into both; a label is
case-insensitive, so the id's own spelling serves.

## Consequences

- An edit is computed against the text each file holds now and versioned per
  open document when the client takes document changes, so an editor applies
  it to exactly what was checked.
- Measured on CPython's 556 documents with a release build, renaming
  `types-genericalias` (55 edits in 12 files) takes 59 ms, 15 ms once every
  document's uses are resolved.
- Ids written inside a `:filter:`, a diagram template (`ref()`, `flow()`), a
  `needs.json` or a `jinja_context`, and another site linking in through its
  `objects.inv`, are not renamed: none of them is a use the project resolves.
- A reference inside a directive the build does not know, such as CPython's
  `.. describe::`, is not read by the build either, and is not renamed.

## Alternatives considered

- **Edit whatever was found, and let the build report what broke.** Faster to
  build, but the failure lands in a later build on a line the author never
  saw, which is the silent degradation the diagnostics exist to prevent.
- **An alias form keeping a named reference's visible text**
  (`` `Install <new_>`_ ``). It preserves the page but introduces a spelling
  nobody wrote, and a rename that keeps the old name on screen does not
  rename.
- **The server finding names again on each line.** A second reader of every
  name's grammar, which would drift from the parser's exactly where it
  matters; recording the span where the parser reads the name costs nothing.
