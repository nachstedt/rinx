# 34. The `:rfc:`, `:cve:` and `:cwe:` roles

## Status

Accepted.

## Context

ADR-030 implemented Sphinx's `:pep:` and expected `:rfc:` to reuse it.
Sphinx 9.1 has three more roles of exactly that shape — `RFC`, `CVE` and
`CWE`, all `ReferenceRole`s — and docutils has `:rfc-reference:` beside
`:pep-reference:`. Until now all four fell through to the catch-all role
pattern and rendered as their source text.

Each of Sphinx's roles yields the same three nodes `:pep:` does: a
general-index entry, an `index-N` anchor from the per-document counter, and
`<a class="… reference external"><strong>…</strong></a>`, an explicit title
replacing the text. They differ in five places:

| | `:pep:` | `:rfc:` | `:cve:` | `:cwe:` |
|---|---|---|---|---|
| Index group | Python Enhancement Proposals | RFC | Common Vulnerabilities and Exposures | Common Weakness Enumeration |
| Text and subentry | `PEP <target>` | `RFC <target>`, or `RFC 2324 Section 2.3` for a `section-`/`appendix-`/`page-` anchor (`_format_rfc_target`) | `CVE <target>` | `CWE <target>` |
| URL | `pep_base_url` + `pep-%04d/` | `rfc_base_url` + `rfc%d.html` | `https://www.cve.org/CVERecord?id=CVE-` + target | `https://cwe.mitre.org/data/definitions/` + `%d.html` |
| Refused when | `int()` fails | `int()` fails | never | `int()` fails |
| Class | `pep` | `rfc` | `cve` | `cwe` |

Any `#fragment` is appended to the URL. Sphinx's `rfc_base_url` default is
`https://datatracker.ietf.org/doc/html/`, replacing docutils'.

docutils' `:rfc-reference:` differs from `:pep-reference:` in letting a
`#section` through to the URL; its number must be at least 1, and it shows
`RFC ` and the number as `int()` reads it, never the section.

## Decision

### One node for the four registries

`InlineNode::PepReference` became `InlineNode::RegistryReference`, holding a
`RegistryTarget` — an enum over `PepTarget`, `RfcTarget`, `CveTarget` and
`CweTarget` — beside the title, the anchor and the span. ADR-033 rejected
folding `:pep-reference:` into `:pep:`'s node because most of its fields
would mean nothing for it; here every field means the same for all four
roles, and only the target differs. As four sibling nodes, the anchor pass,
the analyzer, the renderer's dispatch, `inline_plain_text`, `span`,
`with_span`, `renders_as_link` and the unescaping pass would each grow four
identical arms; as one, each grew none.

`RegistryTarget` answers what the rest of the build asks: its `Registry`
(whose `role_name`, `label` and `index_group` are Sphinx's), `display_text`
(the link text and the index subentry, which Sphinx makes equal) and
`page_path` (relative to the registry's base). `PepTarget`, `RfcTarget` and
`CweTarget` share one number-and-fragment reader, so they cannot disagree
about what a number is; ADR-030's ASCII-only deviation carries over to all
three, and to `:rfc-reference:`.

The parser matches the four with one pattern, `:(pep|rfc|cve|cwe):`, and one
handler. The `.ast` format changed with the node: a target serializes as
`{"rfc": "2324"}`.

### A refusal names its registry; a code names the construct

`RoleRefusal::PepTarget` became `RoleRefusal::RegistryTarget { registry,
target }`, and the refusal pass maps the registry to its own code —
`pep.invalid-number`, `rfc.invalid-number`, `cve.invalid-id`,
`cwe.invalid-number` — because each role has its own rule. All four word
their message from one `InvalidRegistryTarget`, opening with Sphinx's
"invalid … number".

### `:cve:` is validated, where Sphinx validates nothing

Sphinx's `CVE` role never refuses: it appends any target to `…?id=CVE-`.
The commonest mistake, writing the identifier in full — `:cve:`CVE-2024-3094``
— therefore links `id=CVE-CVE-2024-3094` and shows "CVE CVE-2024-3094"
without a word. Here a target must be the identifier's year (four ASCII
digits), a `-` and a sequence number (at least four ASCII digits), with an
optional `#` and anchor, and anything else is `cve.invalid-id`. A target
beginning `CVE-`, in any case, gets its own message telling the author to
drop the prefix.

The price is that a document Sphinx builds without a warning can warn here.
That is the deviation this project prefers: the refused text was a broken
link in Sphinx too, only a silent one, and a `.. noqa: cve.invalid-id`
accepts it deliberately.

### Two base URLs, two fixed addresses

`rfc_base_url` joins `pep_base_url` in `rinx.toml`, for the reason ADR-030
gave: a URL the browser follows, read while rendering, so pointing a site at a
mirror re-parses nothing. Both are one `RegistryBaseUrl<S>`, told apart by a
marker type naming the setting's key, default and what is appended to it, so
one index cannot be passed where the other is expected and each error names
its own key. CVE's and CWE's addresses are constants in the renderer, as
Sphinx hard-codes them; making them settings is left until a project needs
it.

### `:rfc-reference:` is a sibling node, as `:pep-reference:` is

`InlineNode::DocutilsRfcReference` holding a `DocutilsRfcNumber`, for
ADR-033's reasons: no anchor, no index entry, no title, so it never meets the
anchor pass or the analyzer. It shares `rfc_base_url`, the digit reader and
the refusal pass (`rfc-reference.invalid-number`, in docutils' wording).

## Consequences

- `rfc`, `cve`, `cwe` and `rfc-reference` are fixed role names, so a
  `.. role::` cannot claim them.
- Role names stay case-sensitive, for the reason ADR-030 gives.
- All four registries number their anchors after the document's `.. index::`
  anchors, as ADR-030 recorded for `:pep:`.
- docutils' `rfc_references` setting, which turns bare "RFC 2822" text into
  links and which Sphinx leaves off, is not supported.
