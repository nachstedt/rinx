# 35. The `:index:` role, and `see`/`seealso` in the general index

## Status

Accepted.

## Context

ADR-030 expected an `:index:` role to need the same reach as `:pep:`. Until
now the role fell through to the catch-all role pattern and rendered as its
source text, and nothing it names reached the general index.

Sphinx 9.1's `IndexRole` (`sphinx/domains/index.py`) yields three nodes: an
`addnodes.index` holding the entries, a `target` with an `index-N` id from the
per-document counter `.. index::` uses, and a plain `Text` node showing the
title, with no markup inside it and no link. Which entries it makes depends on
the form:

| Written | Entries | Shown |
|---|---|---|
| `` :index:`foo; bar` `` | `single: foo; bar` | `foo; bar` |
| `` :index:`a, b` `` | `single: a, b`: no comma split | `a, b` |
| `` :index:`!foo` `` | `single: foo`, main | `foo` |
| `` :index:`title <pair: a; b>` `` | `process_index_entry(target)`, one `.. index::` line | `title` |

A malformed entry, such as `pair: a`, gets a warning while Sphinx builds the
index. The text and anchor stay on the page.

`see: a; b` and `seealso: a; b` are listed under `a` as the subentry `see b`
or `see also b`, with no link (`IndexEntries.create_index`). The `.. index::`
directive already parsed them, with two faults: the general index dropped
them (🔶 in the compatibility table), and the parser read `entry <target>`
where Sphinx splits `entry; target` at the first `;`.

## Decision

### One node for all three

`InlineNode::IndexReference` holds the title, the entries and the anchor
together, for the reason `RegistryReference` does: nothing can separate the
anchor from the entries linking to it. The title is already final, with the
`!` stripped and escapes resolved, so no later phase re-derives it.

### The directive's grammar, not a copy

The role parses its target with the `.. index::` directive's own functions.
With an explicit title, the target is one `parse_index_line`. Without one, it
is `parse_typed_entry(Single, …)`, which is what makes `a, b` a single term.
To make that possible, both functions return a
`Result<_, rinx_ast::InvalidIndexEntry>` instead of pushing a diagnostic in
the directive's wording. An inline handler has nowhere to report, and the
directive and the role report under different codes. `InvalidIndexEntry` is
in `rinx_ast` because a refused role carries it in the AST until the refusal
pass reports it.

Sharing the grammar also exposed a bug in it: in a comma-separated bare line
Sphinx reads `!` per value, so `` :index:`t <spam, !eggs>` `` marks only
*eggs* main. rinx had marked every value main for a leading `!` and indexed
`!eggs` literally. Both the directive and the role now follow Sphinx.

An empty `single:` (`` :index:`!` ``, or `.. index:: single:`) is now refused
too, as Sphinx's `_split_into(1, …)` refuses it. It used to make an entry with
an empty term.

### Its own diagnostic family

A malformed entry is `index-role.invalid-{single,pair,triple,see,seealso}`,
not the directive's `index.*`. The grammar is shared, but a code names the
construct the author wrote, as `pep.*` and `pep-reference.*` do. So a
`.. noqa: index.invalid-pair` written for a directive does not silence a role
on the same line range.

### Reported while parsing, shown as Sphinx shows it

The handler turns a malformed entry into a `RefusedRole` with
`RoleRefusal::IndexEntry`. The refusal pass reports it and lowers it to an
`IndexReference` with the title and no entries, which is what Sphinx's page
shows. Lowering to the source text, as the registry roles do, would show
markup Sphinx never shows. The refusal pass now runs *before* inline anchors
are minted, so a lowered role still gets the anchor Sphinx gives it.

### One anchor pass for both inline kinds

`assign_registry_index_ids` became `assign_inline_index_ids`. It numbers
registry and `:index:` roles in document order among themselves, after the
directives' anchors, carrying over ADR-030's ordering deviation. It runs after
substitutions, so each use of a `replace` definition gets its own anchor. The
analyzer's matching walk (`index_inline_index_entries`) files a role's entries
through the same `index_genindex_entries` the directive uses, so the two
cannot record an entry differently.

### Redirects are their own index data

`ProjectIndex::genindex_redirects` holds `GenIndexRedirect { primary, kind,
target }`, kept apart from `genindex_entries`. A redirect has no location, so
modelling it as a `GenIndexEntry` with an optional anchor would allow a linked
entry without a place to link to. The general index files each redirect as a
location-less subentry of its primary (`GenIndexRedirect::subentry_text`, in
Sphinx's wording). A target naming no entry is not checked, as in Sphinx.
`ProjectIndex::has_genindex_entries` counts both, and is now the one check
every page asks before linking the index.

`see`/`seealso` values are now split at the first `;`. The old `<target>`
form was never Sphinx's, and a document using it now gets
`index.invalid-see`.

## Consequences

- `index` is a fixed role name, so a `.. role::` cannot claim it.
- The `.ast` and `.index` formats changed (a new inline node, a new index
  field), so every cached parse and index is invalidated once.
- The directive's diagnostics now use Sphinx's wording,
  `invalid pair index entry 'a'`, as the role's do.
