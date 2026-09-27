# 25. The `:any:` role: one question, answered while rendering

## Status

Accepted.

## Context

`` :any:`target` `` names a target without saying what kind of thing it is.
Sphinx's `ReferencesResolver.resolve_anyref` asks the `std` domain first — a
label, an option, a glossary term, a document — and then every other domain in
alphabetical order, collects every hit, and links the first, warning when there
was more than one. Projects use it where the kind is obvious to the reader, and
above all as their `default_role`, which makes every single-backquoted name a
cross-reference.

Every other cross-reference role here already had a resolver, so the questions
were where `:any:`'s search happens, how a hit is drawn, and what an ambiguous
target does. The behaviour was checked against `sphinx-build` 9.1 on a project
holding one target of every kind.

## Decision

### 1. The AST records the question, not an answer

The parser produces an `InlineNode::AnyReference` holding the title, the target
as written, and the `!` and `:external:` markup, and nothing else. What the
target *is* — a label, a term, a function — is a fact about the whole project,
which only the merged index knows; lowering to one of the specific reference
nodes at parse time would need the index the parser never has.

The target is kept verbatim otherwise. `~` is not markup to Sphinx's `:any:`
(it looks `~pkg.run` up as written and finds nothing), and a trailing `()` is
markup only to the domain-object kinds, so the resolver strips it for those
alone rather than the parser stripping it for every kind.

### 2. Each kind is searched by the resolver its own role uses

`renderer::resolution::AnyResolver` asks the index and the existing
`DomainObjectResolver` and `OptionResolver` — through `names_ending_in` and
`resolve_local` — rather than re-implementing their searches, so `:any:` finds
what the dedicated role would have found: an option with the ambient program,
a term case-insensitively, a document relative to the referencing one. Domain
objects follow `find_obj`'s fuzzy mode: the first scope tier naming any object
of the domain wins, and only a `py` name falls back to a dotted-suffix match.

Another site's inventory is consulted only when nothing local answers, over
every entry type it lists, and the first listing wins, as in intersphinx.

### 3. A hit is drawn by its own role's markup

The per-role renderers' href construction was extracted into
`label_href`, `term_href`, `domain_object_href` and `write_equation_link`, which
the dedicated roles and `:any:` share. A resolved `:any:` therefore produces
the same link its own role would, with Sphinx's extra `any` class where that
markup has a class list, and an equation still shows its number.

### 4. An ambiguous target links nothing

Several hits are reported as `link.ambiguous-any` and drawn as a broken link,
listing the role that would name each candidate alone
(`` :py:func:`pkg.close` ``). Sphinx links the first hit, but which hit comes
first is an accident of search order the reader cannot see — the same reasoning
that already leaves an ambiguous dot-prefixed domain reference unresolved. Under
`strict_links = True` it fails the build like any broken link; a `.. noqa:`
silences it.

## Consequences

- Three deliberate deviations from Sphinx 9, all recorded in
  `docs/compatibility.rst`: ambiguity is not resolved; glossary terms are found
  (Sphinx 9's `:any:` misses them, its lookup keying terms differently from how
  they are stored); and a label above no heading is found, as `:ref:` finds it
  here.
- Entities need nothing of their own: each is already a label.
- `default_role`, the most common way `:any:` is used, is still unsupported;
  with this node in place it is a matter of the inline scan producing one.
- The inventory pruning (ADR-023) keeps every name an `:any:` writes, so the
  external fallback is exact for this role too.
