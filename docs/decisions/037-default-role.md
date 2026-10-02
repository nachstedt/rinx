# 37. Interpreted text and the default role

## Status

Accepted.

## Context

docutils calls text between single backquotes *interpreted text*. A role may
be written before it (``:sub:`2` ``), after it (`` `2`:sub: ``), or not at
all, in which case the text is read as the *default role*. Until now rinx read
only the first form. A bare `` `Dune` `` and a suffix role were both left as
text, backquotes included.

What docutils 0.21 and Sphinx 9.1 do with the other two forms:

- **Recognition is the inline-markup scan's.** The backquote opens under the
  same start-string rules as `*`, and the first backquote that may close it
  does, under the end-string rules. One `` `(?P<suffix>(:role:)?(__?)?) ``
  pattern reads what follows. A closing backquote followed by `_` or `__`
  makes the whole construct a hyperlink reference, not interpreted text.
- **A role on both sides is an error.** docutils warns "Multiple roles in
  interpreted text". It gives "Mismatch: both interpreted text role … and
  reference suffix" for a role together with `_`, and shows the source as a
  `problematic` node.
- **The default is `title-reference`**, with `title` and `t` as its other
  names. It is a generic role that docutils' and Sphinx's HTML writers draw
  as `<cite>`.
- **Sphinx's `default_role` setting replaces it for every document.** It is
  registered as the role named `''` at the start of each document and
  unregistered at the end. An unknown name gets the warning "default role %s
  not found", and docutils' own default stays.
- **`.. default-role:: name` replaces it for the rest of the document.**
  Sphinx overrides docutils' directive (`sphinx/directives/__init__.py`). With
  no argument, it *unregisters* the role named `''`. That leaves docutils'
  `title-reference`, not the configured default. An unknown name is an error,
  and the default stays.

## Decision

### A default role is a role the table already knows

Neither form gets a regex or a node of its own. `inline/interpreted.rs`
rewrites both to the prefix spelling ``:role:`text` ``. It finds the first
`SIMPLE_ROLE_REGEXES` entry matching the whole spelling, the entry the scan
itself would pick, and calls that entry's handler. So `` `x` `` under
`.. default-role:: py:func` is exactly ``:py:func:`x` ``: same node, same
default-domain resolution, same explicit-title split, same span.

The same lookup also decides what a valid default role is: `find_role_kind`
spells ``:name:`x` `` and asks the table. A name matched only by the
catch-all shape counts only if the schema declares it or a `.. role::` has
defined it by then. Any role rinx implements can therefore be the default,
including an entity role or a derived one. And adding a role to the table
makes it available as a default with no second list to update.

### `title-reference` is a node

`InlineNode::TitleReference(String)` is plain text with smart typography, like
`Script`. It renders as `<cite>`, and `inline_plain_text` flattens it in a
section title. `DefaultRole` keeps `title-reference` as its own variant, built
directly without a rewrite. `DefaultRole::parse` folds all three spellings
into that variant, because the table maps them to the `title-reference` kind.

### Recognition sits beside emphasis

`markup.rs`'s scan opens interpreted text with a single backquote, never the
first of two. A literal is tried first at the same position, so it wins. The
closes are computed once per scan, as for every other marker, with what
follows each one: nothing, a role, a reference suffix, or both. As in
docutils' pattern, the longest reading that ends where markup may end wins.
So `` `x`:sub:y `` is bare text followed by `:sub:y`.

Two cases are left to the role table:

- **The first close ends a reference** (`` `x`_ ``). This is no match, so a
  title cannot run on into the next pair of backquotes.
- **A role is written right before the opener**, in docutils' wider name
  shape, as in ``:a.b:`x` ``. The table reads it, or leaves it as text when it
  does not know the role.

### Configured on the library, overridden by the document

The configured default is the `default_role` attribute of `rinx_library`,
passed as `--default-role`. It is not a `rinx.toml` setting, because it
changes the `.ast`, which makes it a parse input. It is also a property of the
library's content, like `default_domain`.

The worker validates the name against the schema. An unknown one fails the
parse, where Sphinx only warns: it is a build-configuration fault, and with a
mistyped default every bare `` `text` `` of the library would silently become
a title instead.

The document's choice is state, so it lives in `DocumentRoles`. That is the
table `.. role::` already fills (renamed from `CustomRoles`), with the same
semantics:

- it applies from where it is written to the end of the document;
- a choice made inside an `.. include::`d fragment carries into the including
  document;
- an excluded `.. if-builder::` block, which is never parsed, has no effect.

`ParseCtx::default_role` answers with the document's choice if there is one,
and the configured one otherwise.

### Refusals use the existing pass

A role on both sides, or a role with a reference suffix, becomes an
`InlineNode::RefusedRole` (`RoleRefusal::MultipleRoles`/`RoleAndReference`).
`report_refused_roles` reports it as `interpreted.multiple-roles` or
`interpreted.role-and-reference` and lowers it to its source text, as
docutils' `problematic` node shows it. The prefix case is detected after the
table's match, in `text.rs`. The suffix case is detected by the scan.

The directive reports these codes:

- `default-role.unknown-role` for a name no role answers to;
- `default-role.invalid-argument` for more than one word;
- `default-role.unexpected-content` for options or a body.

The first two leave the default as it was; the last one still applies the
argument.

### Deliberate deviations

- **Role names are case-sensitive**, as for every built-in role (ADR-034):
  `` `x`:Sub: `` and `.. default-role:: Any` are not recognized. A role
  defined with `.. role::` is the exception, since its lookup ignores case
  already.
- **An unknown suffix role is left as written, unreported**, as an unknown
  prefix role already is. docutils reports both.
- **An unknown `default_role` fails the build.** Sphinx warns and falls back.
- **The three title-reference names are reserved**, so `.. role:: t(code)` is
  refused as `role.builtin-name`.
- **A section title holding a title reference shows it as plain text** in the
  navigation and the page `<title>`, as it does for emphasis.

## Consequences

- Two example pages, `examples/sectnum/prefix_suffix.rst` and
  `examples/entities/diagrams.rst`, had used single backquotes where they
  meant a literal. They now say so with double backquotes. A project with the
  same habit will now see citations, as it would under Sphinx.
- Sphinx's most common `default_role`, `py:obj`, fails with an unknown-role
  error until `:obj:` is implemented. After that it works with no further
  change.
- The live preview accepts `--default-role` too. Like `--default-domain`, the
  VS Code extension does not yet pass it.
