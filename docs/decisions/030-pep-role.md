# 30. The `:pep:` role

## Status

Accepted.

## Context

`` :pep:`8` `` links a Python Enhancement Proposal. Until now the role was
parsed as nothing: the catch-all role pattern matched it and left the markup
as text, so every mention in CPython's documentation rendered as its source.

Sphinx's `sphinx.roles.PEP` is a `ReferenceRole`, and one role yields three
nodes:

- **A general-index entry**, `single: Python Enhancement Proposals; PEP 8`:
  one group, one subentry per mention, the target as written — fragment
  included, and never the explicit title.
- **The anchor that entry links to**, `<span class="target" id="index-0">`,
  numbered from the same per-document counter `.. index::` uses.
- **The link**, `<a class="pep reference external"
  href="https://peps.python.org/pep-0008/"><strong>PEP 8</strong></a>`. The
  target is split at its first `#`; the part before is `int()`'d and padded to
  four digits, the part after is the URL's fragment. The text is `PEP ` and
  the whole target as written, so `:pep:`8#naming`` shows "PEP 8#naming";
  an explicit title replaces it, with no `PEP ` prefix.

The PEP index is docutils' `pep_base_url` setting, default
`https://peps.python.org/`, which a Sphinx project can only change in a
`docutils.conf`; `pep_file_url_template` is ignored, since Sphinx hard-codes
`pep-%04d/`. A target `int()` rejects is an "invalid PEP number" error and a
`problematic` node showing the source.

## Decision

### One node carries all three

`InlineNode::PepReference` holds a `PepTarget`, the explicit title, the index
anchor's id and a span. Three nodes would let a later phase separate an
anchor from the entry pointing at it; one cannot. The node renders as a link
(`renders_as_link`), so a `.. button-link::` label flattens it as it does
every other reference.

### `PepTarget` is parsed, not validated

`PepTarget` is the written text, split and read once: the number, the
fragment, and `page_path()` (`pep-0008/#naming`) as the one place the number
is padded. It serializes as the written text and re-parses on load. Only
ASCII digits are a number — `int()` also takes a sign, underscores and
non-ASCII digits, which name no PEP anybody wrote, so they are refused rather
than reproduced.

### The URL is built while rendering

The `.ast` holds the page, not the URL. `pep_base_url` is a `rinx.toml` key
(`PepBaseUrl`, checked by the same `check_browser_address` the version
switcher's `json_url` uses, plus a required trailing slash): it is a URL the
browser follows, not a path, so it respects the config's no-paths rule, and
reading it while rendering means pointing a site at a mirror re-parses no
document. The trailing slash is required rather than added, because docutils
appends to the value as written and a guess would hide which was meant.

### Anchors are minted after parsing, in the `.. index::` sequence

`assign_pep_index_ids` runs after `assign_index_ids` and after substitutions
are resolved, continuing the same counter, so no id repeats and each use of a
`replace` definition holding a `:pep:` gets an anchor of its own. It is a
second walk rather than one interleaved with the first because an inline
role can sit where `assign_index_ids` does not look — a glossary definition,
a dropdown's title — so the ids' order differs from Sphinx's when both kinds
appear. An anchor's name is not something a reader links to.

The analyzer finds the roles with `for_each_inline_list`, and the parser
numbers them with `for_each_inline_list_mut`. Both moved from the parser into
`rinx_ast` for this, and one macro generates both from one body, so an entry
exists for exactly the roles that have an anchor. The future `:index:` role
needs the same reach.

### A refused role is one node for every role

A target that is not a number becomes an `InlineNode::RefusedRole`, which the
whole-document pass reports as `pep.invalid-number` at the role and lowers to
the role's source text, as Sphinx's `problematic` node shows it.
`RefusedRole` replaces `:numref:`'s `RefusedNumberReference` (ADR-028): a
`RoleRefusal` enum says which role was refused and carries what its
diagnostic and lowering need, so the next role that can be refused (`:rfc:`
will be) adds a variant, not a node and a pass.

## Consequences

- `:rfc:` can reuse all of it: the refusal pass, the anchor pass, the inline
  walkers and the render shape. It is left for its own change.
- docutils matches role names case-insensitively, so `:PEP:` reaches Sphinx's
  role; here it does not. That is a property of every role, not this one, and
  belongs in a change that introduces it for all of them.
- docutils' own `:pep-reference:` role, which Sphinx does not override and
  which renders without the index entry and the `<strong>`, is not supported.
