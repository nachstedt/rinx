# Known Bugs

Confirmed rusty-sphinx resolver/parser bugs found while triaging the top 20
"Unresolved Domain-Object References" in `benchmark_result.txt` against the
CPython doc source. Unlike the entries in `scripts/domain_warnings_whitelist.json`,
these are cases where real Sphinx *would* resolve the reference — the warning
is a rusty-sphinx shortcoming, not a CPython doc inconsistency.

## ~~Explicit-title cross-reference syntax (`` `text <target>` ``) isn't parsed~~ — fixed

`parse_domain_object_target` (`crates/parser/src/inline.rs`) now splits an
explicit title (`` `Display text <target>` ``) off the target via the same
`split_explicit_title` helper `:ref:`/`:term:` already used, and un-escapes
backslash-escaped punctuation in the display text (e.g. `spawn\*` → `spawn*`)
via a new `unescape_rst_backslashes` helper. Covered by unit tests in the
same file (`test_parse_domain_object_target_explicit_title_*`,
`test_handle_inline_match_func_variant_explicit_title`,
`test_handle_attr_match_explicit_title_splits_display_from_target`) and by
an example in `examples/domains.rst`. See `spec_gaps.md`'s "Inline Markup"
section for the up-to-date status.

## `:c:data:` role loses its `c:` domain prefix

`DATA_ROLE_REGEX` (`crates/parser/src/inline.rs`) only allows an optional
`py:` domain prefix (`(?P<domain>py)`), not `c:`. Since the domain group is
optional and the match isn't anchored, `:c:data:`Py_mod_exec`` matches
starting after the stray `c:`, producing a bare `` :data:`Py_mod_exec` ``
with `domain = None` — which then falls back to the document's default
domain (`Domain::Py`). This is why the benchmark reports these as
"referenced as py:data" even though the source uses `:c:data:`.

rusty-sphinx also has no `c:data`/`c:var` object type yet (`CObjectType` only
has `Function` and `Macro`), so even a correctly-parsed `c:data` role
wouldn't resolve against a `.. c:macro::`-defined target today.

- Confirmed examples: `Py_mod_exec` (`c-api/module.rst`, defined via
  `.. c:macro:: Py_mod_exec`), `Py_tp_bases` (`c-api/type.rst`, defined via
  `.. c:macro:: Py_tp_bases`).
