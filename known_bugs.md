# Known Bugs

Confirmed rusty-sphinx resolver/parser bugs found while triaging the top 20
"Unresolved Domain-Object References" in `benchmark_result.txt` against the
CPython doc source. Unlike the entries in `scripts/domain_warnings_whitelist.json`,
these are cases where real Sphinx *would* resolve the reference — the warning
is a rusty-sphinx shortcoming, not a CPython doc inconsistency.

## Domain-object definitions nested inside not-yet-implemented directives are silently dropped

`try_parse_directive` (`crates/parser/src/directives.rs`) falls back to
`Directive::Unknown { name, argument, body }` for any directive it doesn't
specifically recognize, and `body` is just `join_body_lines(&body_lines)` — a
flat string, never re-parsed for nested directives. Real Sphinx directives
like `list-table`, `c:member`, and `c:type` commonly nest *other* directives
inside their body (table cells, struct/enum members, bit-flag constants) that
genuinely define cross-reference targets. Because rusty-sphinx doesn't yet
implement any of those three container directives (`list-table`: 68
occurrences, `c:member`: 102, `c:type`: 133 in the benchmark's "Unsupported
Directives Summary"), every domain-object definition nested inside one is
captured as inert text in the parent's opaque `body` and never reaches the
analyzer — so any reference to it is reported as unresolved even though the
object is, in fact, documented.

- `list-table`: `reference/datamodel.rst`'s "Special read-only attributes"
  table nests `.. attribute:: method.__self__` (and `.__func__`, `.__doc__`,
  `.__name__`, `.__module__`) inside table-cell body content; all five are
  swallowed into the `list-table` directive's raw `body` string.
- `c:member`: `c-api/typeobj.rst` documents the `PyTypeObject.tp_flags`
  bitmask values (`Py_TPFLAGS_HAVE_GC`, `Py_TPFLAGS_HEAPTYPE`, etc.) as
  `.. c:macro::` directives nested inside `.. c:member:: unsigned long
  PyTypeObject.tp_flags`'s body, preceded by a `.. c:namespace:: NULL` reset
  so they're indexed as bare top-level names rather than
  `PyTypeObject.tp_flags`-qualified. Since rusty-sphinx doesn't implement
  `c:member`, none of the nested macros are ever extracted.
- `c:type`: `c-api/memory.rst` documents `PyMemAllocatorDomain`'s enum values
  (`PYMEM_DOMAIN_RAW`, `PYMEM_DOMAIN_MEM`, `PYMEM_DOMAIN_OBJ`) the same way —
  nested `.. c:macro::` directives (after a `.. c:namespace:: NULL` reset)
  inside `.. c:type:: PyMemAllocatorDomain`'s body — and rusty-sphinx doesn't
  implement `c:type` either, so these are swallowed too.

Fixing any one of `list-table`/`c:member`/`c:type` on its own wouldn't be
enough — the parser's directive dispatch would need to recursively re-parse
an `Unknown` directive's body for nested directives (or these three would
need to become recognized container directives whose body is parsed like any
other block content) before nested domain-object definitions inside them
could be indexed.
