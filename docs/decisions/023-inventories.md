# 23. Inventories: intersphinx as pinned build inputs

## Status

Accepted.

## Context

Sphinx writes an `objects.inv` next to every HTML site: every target the site
defines — name, `domain:role`, priority, URI, display name — in a small
zlib-compressed text format. Intersphinx is the other half: a project lists
other sites in `intersphinx_mapping`, Sphinx downloads their inventories while
building, and a reference no document defines resolves into them. It is how
nearly every Python project's documentation links to `dict`, and it was the
largest reason a real project could not build here: a `:py:class:` naming a
standard-library type was a broken link, and a rusty-sphinx site was a link
target nobody else could reach.

Three things here differ from Sphinx and needed deciding: where an inventory
comes from, which phase reads it, and how much of it survives into the index.

## Decision

### 1. An inventory is a declared, pinned input — nothing is fetched

Sphinx fetches `https://…/objects.inv` while building. A sandboxed action may
not, and an input that changes under a build without the build changing is
exactly what Bazel's cache cannot see. So an inventory is an ordinary label,
declared by a new rule:

```python
rusty_sphinx_inventory(
    name = "python",
    src = "@python_objects_inv//file",   # http_file with sha256, or vendored
    base_url = "https://docs.python.org/3.13/",
)

rusty_sphinx_site(..., inventories = [":python"])
```

`src` is any file: a vendored copy, an `http_file` pinned by `sha256`, or —
the case this also opens — another `rusty_sphinx_site`'s `inventory` output
group. Updating a pin is an explicit change to the build. This is the stance
`.. needimport::` takes on URLs (ADR-016 §5), for the same reason.

`base_url` is metadata about the file, so it lives on the rule rather than in
`rusty_sphinx.toml` (ADR-001: the path goes on the command line, the name and
its meaning travel with it). It may be absolute, or relative to the consuming
site's root for sites deployed side by side, in which case each page's link is
made relative to that page, as an internal link is.

The rule's name is Sphinx's `intersphinx_mapping` key, which documents write
in `:external+python:` and in a `python:` target prefix. `InventoryName`
refuses the characters that would end the name early in either spelling.

### 2. The index action reads inventories, and the index holds them

The inventories reach one action — the index — and are stored in
`ProjectIndex::external_inventories`, in declaration order. Rendering then
resolves against one universe, and the live preview, which merges a fresh
document into a stale index, finds external targets without being handed any
file. Giving each render action the files instead was rejected: every page
would parse every inventory, and the preview would need a second input it has
no way to discover.

The field is a build input, not document data, so `ProjectIndex::merge` keeps
the base index's inventories rather than merging them — a single document's
analysis never has any.

### 3. Only what some document names survives into the index

Measured with Python 3.13's full inventory (18,676 entries) and a
one-paragraph page, release build:

| | index file | one render action (median of 20) |
|---|---|---|
| no inventory | 476 B | 4.7 ms |
| whole inventory stored | 2.4 MB | 22.8 ms |
| pruned to written targets | 997 B | 5.5 ms |

Stored whole, every render action pays ~18 ms to parse targets it will never
look up — on a ~500-page site like CPython's, around nine CPU-seconds on a
build that otherwise takes 15.6 s. So the index action keeps only the entries
some reference could reach (`ExternalInventory::retain_referenced`). That is
exact rather than heuristic because an external lookup is: it tries the target
as written, the part after a `name:` prefix, and for an option the
`program.option` form — never a scope-qualified name. The written targets are
read off each AST's serialized form rather than its typed tree, so a container
added later cannot hide its references from the pruning. The index action
itself, reading and pruning that inventory, took 57 ms.

The cost is borne by the live preview only: a reference to an external target
no document named when the index was last built stays unresolved there until
the next build — the same kind of staleness the preview already has for
titles.

### 4. Resolution order is Sphinx's, and local always comes first

A role first searches this site, exactly as before. Only if that fails:

1. the target as written, in every inventory, in declaration order — the first
   inventory listing it wins, so the order is the author's to choose;
2. if the target has a `name:` prefix naming a declared inventory, the rest in
   that inventory alone.

That is Sphinx's `_resolve_reference_detect_inventory`. Step 1 comes first so a
target that merely contains a colon still resolves as written. Two
inventories listing the same name are *not* reported as ambiguous, unlike an
ambiguous local suffix match: the winner is decided by an order the author
wrote, not by an accident of the index, and a prefix or `:external+name:`
picks the other.

`:external:role:` skips the local search, and `:external+name:role:` searches
one inventory only. The prefix is markup, so the parser strips it and records
an `InventorySelector` on the node, like a leading `.` becomes a
`TargetSearchOrder`. It applies to every role that resolves against an index —
`:ref:`, `:term:`, `:option:` and the domain roles.

The entry types a role accepts are its local aliases
(`ObjectType::role_alias_candidates`) plus `py:property` for `:attr:`, which
Sphinx's Python domain lets that role reach and this build has no directive
for.

An external link carries Sphinx's `reference external` classes and its
`(in Project vVersion)` tooltip, so a stylesheet written for Sphinx applies.

### 5. Every site writes an `objects.inv`, projected from the index

`rusty_sphinx_site` always runs an `inventory` action next to `genindex`,
writing `_site_out/objects.inv` and exposing it in an `inventory` output group
so another site can depend on the one file. It is a projection of
`ProjectIndex`: documents (`std:doc`), labels (`std:label`, with the title a
bare `:ref:` shows), glossary terms, options and domain objects, plus
`genindex`. Every URI comes from the function the renderer uses for a link to
the same target, and `examples/` checks that each of the site's entries lands
on an existing `id`.

Writing it forced two fixes that are not about inventories:

- A bare `:ref:` now shows the title of the section (or the caption of the
  figure, table or code block) its label is written above, as Sphinx does,
  recorded in `ProjectIndex::target_titles`. The inventory's display names
  come from the same map.
- A target whose anchor is not its name — an entity, at `entity-<id>` — is
  recorded in `ProjectIndex::target_anchors`. Before this, a local `:ref:` to
  an entity linked to an `id` that did not exist.

`domain_objects` keys are lowercased `TargetName`s, which is right for lookup
and wrong for publishing: Sphinx resolves Python names case-sensitively. The
definition's spelling is kept beside the key in `domain_object_spellings`.

The format itself lives in the leaf crate `rusty_sphinx_inventory`, since the
worker writes one, the index holds one and the AST names one. A real
`sphinx-build` 9.1.0 output is checked in as its compatibility fixture: the
reader loads it, and re-writing it reproduces Sphinx's body line for line.

## Consequences

- A reference into the standard library, or any declared project, resolves,
  and a rusty-sphinx site is linkable from Sphinx projects and from other
  rusty-sphinx sites.
- `.ast` files carry an optional `inventory` selector on four inline nodes; an
  ordinary role serializes exactly as before.
- Editing an inventory re-runs the index and every render, as any index input
  does. Its pruned bytes are what the renders see, so an inventory update that
  changes nothing a document names changes no render input.

## Narrowings

- **Anchors differ from Sphinx's.** A Python object's anchor here is
  `py:class:pkg.greeter`, not `pkg.Greeter`. Harmless, since the inventory
  carries the real URI, but a hand-written link into a rusty-sphinx page copied
  from a Sphinx one will miss.
- **Untitled labels are listed.** Sphinx lists only labels with a title; this
  build lists every internal target (showing its name), since an entity or a
  `:name:`d directive is still worth linking to from another site.
- **Not every entry type is written.** Equations are not listed, as Sphinx
  does not list them either; `py-modindex`, `modindex` and `search` are not,
  because this build has no such pages.
- **`:doc:` and `:any:` do not exist**, so neither resolves externally, and no
  equivalent of `intersphinx_disabled_reftypes` is needed yet.
- **Version 1 inventories are refused by name**, and a body line Sphinx would
  skip is reported with its line number rather than skipped silently.
