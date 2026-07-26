# Known Bugs

Confirmed rusty-sphinx resolver/parser bugs found while triaging the top 20
"Unresolved Domain-Object References" in `benchmark_result.txt` against the
CPython doc source. Unlike the entries in `scripts/domain_warnings_whitelist.json`,
these are cases where real Sphinx *would* resolve the reference — the warning
is a rusty-sphinx shortcoming, not a CPython doc inconsistency.

## Explicit-title cross-reference syntax (`` `text <target>` ``) isn't parsed

Sphinx's explicit-title role syntax — `` :func:`spawn\* <spawnl>` `` displays
"spawn\*" but resolves against target "spawnl" — isn't recognized at all.
`parse_domain_object_target` (`crates/parser/src/inline.rs`) only strips a
leading `~` or `!`; it has no logic to split on `<...>`, so the entire raw
string (including the display text and angle brackets) is treated as the
literal target name.

- Confirmed example: `spawn\* <spawnl>` (`library/os.rst`).
- Likely affects other entries further down `benchmark_result.txt` with the
  same `text <target>` shape, e.g. `compat32 <email.policy.Compat32>`,
  `ttk.Treeview <tkinter.ttk.Treeview>`, `data <data_filter>`,
  `cur.execute(...) <Cursor.execute>` — not yet individually verified.

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
