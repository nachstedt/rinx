# 29. The `:code:` role and `.. role::`

## Status

Accepted.

## Context

`` :code:`x = 1` `` marks inline text as code. Until now the role was parsed
as nothing: the catch-all role pattern matched it and left the markup as text.

On its own, `:code:` has no language. Highlighted inline code exists in
docutils and Sphinx only through a role *derived* from it:

```rst
.. role:: python(code)
   :language: python

Call :python:`print("hi")`.
```

So supporting `:code:` fully means supporting `.. role::`, at least for that
base. The behaviour was captured from `sphinx-build` 9.1:

- **The content is interpreted text, not a literal.** Sphinx's `code_role`
  builds a `literal` node from text that has been through backslash-escape
  handling, so `` :code:`a\*b \\ c` `` shows `a*b \ c`. An inline
  ` ``literal`` ` keeps its backslashes.
- **The class list is `code`, then — with a language — `highlight`, the
  role's classes, and the language unless a class already is**, followed by
  `highlight-<lang>`. A role written without `:class:` has its own name as
  its class.
- **`:class:` names are normalized**: `Foo_Bar` becomes `foo-bar`.
- **Role names are case-insensitive**, and a role applies only from its
  definition onwards. A use before the definition is an "Unknown interpreted
  text role" error. A role does not carry into the next document.
- **An unknown language still gets the `highlight` classes** and a
  `misc.highlighting_failure` warning. The text is shown plain.

## Decision

### A node of its own

`InlineNode::Code` carries the text, a `ResolvedLanguage` and the classes, and
a span. It is not an `InlineNode::Literal`, which has no language or classes
and keeps its backslashes. The span exists for the reason `Math`'s does: a
language with no grammar is only found while rendering, and the warning needs
a position. `ResolvedLanguage::None` is plain `:code:`. The language is always
written to the `.ast`, because `ResolvedLanguage`'s own default is Sphinx's
`highlight_language` (Python), which is not what a missing value means here.

### Custom roles are document-ordered parse state

A `.. role::` changes how the rest of the document parses. The block parser is
already sequential, and each paragraph's inline scan runs when the paragraph
is reached. So a table filled in by `.. role::` and read by the inline scan
gives docutils' semantics directly. That table, `CustomRoles`, is created once
per document by `parse_with_ctx` and borrowed by `ParseCtx`. Its interior
mutability is contained in that one type. Every nested context shares it, so a
role defined in an `.. include::`d fragment applies after the include, as in
docutils.

A whole-document pass was the alternative. The inline scan would emit an
unresolved-role node, the definition would stay in the tree as a node, and a
later walk in document order would lower one against the other. That is what
substitutions do, but substitutions are order-*independent*. For roles it
would add two node types, one of which every traversal (analyzer, renderer,
`walk_nodes`, …) would have to learn about, only to reproduce ordering that
the parse already has.

`.. role::` therefore contributes no node. It is dispatched with the splicing
directives, the one dispatcher that may answer with any number of nodes,
including none.

### Refuse, do not shadow

The inline scan tries every fixed-spelling role before the catch-all pattern
that custom and entity roles share. A custom role named `ref`, or `code`,
could never fire. So `.. role::` refuses such a name as `role.builtin-name`
rather than defining a role that silently does nothing. docutils would let it
replace the built-in role. The set of names is not a list: `is_fixed_role_name`
asks the role table itself, so a role added later is reserved automatically.
Entity roles are refused too, in any letter case
(`EntitySchema::has_role_ignoring_case`), because a custom role's name is
case-insensitive and an entity role's is not. Without that, a schema role
spelled `Req` and a `.. role:: req(code)` would split the spellings between
them without a warning. They are also asked *before* the custom table while
scanning, so an entity role keeps its exact spelling even if a conflicting
role somehow got defined.

### Only `code` as a base

Any other base, or a role with no base (`.. role:: red`, a class-only span),
is refused as `role.unsupported-base` and not defined. A half-supported base
would render differently from Sphinx with no warning. The other bases can be
added one at a time, each with its own node.

### Rendering

`renderer/src/inline/code.rs` highlights through the same `Highlighter` code
blocks use, and falls back to escaped text on failure. A failure reports
under `code-role.*`, not `code-block.*`: `HighlightError` now records which
construct was highlighted, because a code names the construct and a `.. noqa:`
for one must not silence the other. `hl-code`, which hands the element to the
generated theme's palette, is added only when tokens were actually coloured.
The stylesheet's light inline-code colours now apply only to `code` without
`hl-code`, and the rule stays at its old specificity.

## Consequences

- `:code:` renders as `<code class="code">`, without docutils'
  `docutils literal notranslate` classes or the `<span class="pre">` around
  each word, matching how this build draws an inline literal.
- `ParseCtx` now holds one piece of mutable state. Code that builds a context
  outside `parse_with_ctx` has no table, so a `.. role::` parsed there defines
  nothing. That affects only tests of individual block parsers.
- `.. default-role::` remains unsupported. It would change how bare
  `` `text` `` parses, which is a separate construct.
