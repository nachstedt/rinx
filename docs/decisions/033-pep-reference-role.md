# 33. docutils' `:pep-reference:` role

## Status

Accepted.

## Context

ADR-030 implemented Sphinx's `:pep:` and left docutils' own
`:pep-reference:` unsupported, so `` :pep-reference:`8` `` fell through to
the catch-all role pattern and rendered as its source text.

Sphinx registers only `pep`, so `pep-reference` still reaches docutils'
`pep_reference_role`, which differs from `:pep:` in every visible way:

| | `:pep:` (Sphinx) | `:pep-reference:` (docutils) |
|---|---|---|
| Index entry and `index-N` anchor | yes | no |
| HTML | `<a class="pep reference external" …><strong>PEP 8</strong></a>` | `<a class="reference external" …>PEP 8</a>` |
| Page | `pep-0008/`, plus any `#fragment` | `pep-0008`, docutils' `pep_file_url_template` `pep-%04d`, no trailing slash |
| `8#naming`, `Title <8>` | a fragment, an explicit title | refused: `int()` fails |
| Range | any number | 0 to 9999 |
| Refusal | "invalid PEP number" | `PEP number must be a number from 0 to 9999; "x" is invalid.` |

Both read docutils' `pep_base_url` setting.

## Decision

### A node of its own

`InlineNode::DocutilsPepReference` holds a `DocutilsPepNumber` and a span.
A flag on `PepReference` was rejected: the title, the fragment and the anchor
would all mean nothing when it was set, and the anchor pass and the analyzer
would each have to filter it out. As its own node it never reaches
`assign_pep_index_ids` or `index_pep_references`, so it gets no anchor and no
index entry without either of them knowing it exists.

### `DocutilsPepNumber` is parsed, not validated

It holds the written text, because docutils shows it (`08` reads "PEP 08"),
and a number at most 9999. Its digits go through the reader `PepTarget`
uses, so the two roles cannot disagree about what a number is, and the
ASCII-only deviation ADR-030 records carries over. `page_path()` is the one
place the number is padded.

### Shared with `:pep:`: the base URL and the refusal pass

The link is built while rendering from `rinx.toml`'s `pep_base_url`, as
docutils reads one setting for both roles. The page template is fixed at
`pep-%04d`: making `pep_file_url_template` configurable is left until a
project needs it.

A refused target is an `InlineNode::RefusedRole` with a
`RoleRefusal::DocutilsPepNumber`, reported by the existing pass as
`pep-reference.invalid-number` and lowered to the role's source text. The
code is its own rather than `pep.invalid-number` because a code names the
construct, and this is a different construct with a different rule.

## Consequences

- `pep-reference` is a fixed role name, so `.. role:: pep-reference(code)` is
  refused as a built-in name.
- Role names stay case-sensitive, for the reason ADR-030 gives.
- `.. role:: x(pep-reference)` cannot derive from it, since custom roles
  derive only from `code`; docutils' `pep_references` setting, which turns
  bare "PEP 8" text into links and which Sphinx leaves off, is not supported.
