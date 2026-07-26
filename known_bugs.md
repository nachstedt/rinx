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
like `c:member` and `c:type` commonly nest *other* directives inside their
body (struct/enum members, bit-flag constants) that genuinely define
cross-reference targets. Because rusty-sphinx doesn't yet implement either
container directive (`c:member`: 102 occurrences, `c:type`: 133 in the
benchmark's "Unsupported Directives Summary"), every domain-object definition
nested inside one is captured as inert text in the parent's opaque `body` and
never reaches the analyzer — so any reference to it is reported as unresolved
even though the object is, in fact, documented.

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

`list-table` used to be a third example here — `reference/datamodel.rst`'s
"Special read-only attributes" table nests `.. attribute:: method.__self__`
(and `.__func__`, `.__doc__`, `.__name__`, `.__module__`) inside table-cell
body content — but is now fixed: `list-table` is a recognized container
directive (`crates/parser/src/list_table.rs`) whose cell content is parsed
via the same `parse_blocks` grid-table cells already use, so nested
domain-object definitions are indexed like any other. See `spec_gaps.md`'s
`list-table` row.

Fixing `c:member`/`c:type` still isn't a matter of a small patch each — the
parser's directive dispatch would need to recursively re-parse an `Unknown`
directive's body for nested directives (or both would need to become
recognized container directives whose body is parsed like any other block
content, the same fix `list-table` got) before nested domain-object
definitions inside them could be indexed.
