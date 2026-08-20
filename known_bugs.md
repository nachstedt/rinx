# Known Bugs

Confirmed rusty-sphinx resolver/parser bugs found while triaging the top 20
"Unresolved Domain-Object References" in `benchmark_result.txt` against the
CPython doc source. Unlike the entries in `scripts/domain_warnings_whitelist.json`,
these are cases where real Sphinx _would_ resolve the reference — the warning
is a rusty-sphinx shortcoming, not a CPython doc inconsistency.

## How "real Sphinx would resolve it" was established

CPython ships `Doc/tools/.nitignore`, an explicit list of the only `.rst` files
allowed to fail Sphinx's nit-picky mode in CI. Any file **not** in that list
builds warning-free with `nitpicky = True`, so every cross-reference it
contains — minus the symbols listed in `Doc/conf.py`'s `nitpick_ignore` — is
known to resolve under real Sphinx. The first numbered bug below lives in a
file absent from `.nitignore` and references a target absent from
`nitpick_ignore` — the second entry is a separately-discovered latent gap
with no benchmark-observed warning (see its own note).

---

## 1. `:c:func:` does not accept a `.. c:macro::` definition

- **Warnings:** `4  Doc/c-api/gcsupport: 'Py_VISIT' (referenced as c:function)`,
  `3  Doc/extending/newtypes_tutorial: 'Py_VISIT' (referenced as c:function)`
  (and, further down the list, `Py_REFCNT`/`Py_TYPE` in `Doc/c-api/structures`)
- **Component:** `crates/ast/src/object_type.rs`
  (`ObjectType::role_alias_candidates`)

`Py_VISIT` is defined as `.. c:macro:: Py_VISIT(o)` in `c-api/gcsupport.rst`
and referenced from the same file as ``:c:func:`Py_VISIT```, because it is a
function-like macro. `role_alias_candidates` already models the C domain's
type-blindness for the `Macro` ↔ `Member` pair, citing real Sphinx's literal
`# TODO: check role type vs. object type` in `_resolve_xref_inner` — but
`C(Function)` currently aliases nothing, so the same looseness is not extended
to the `Function` ↔ `Macro` pair.

This is a second *confirmed* collision of exactly the kind that comment says
the alias table exists to model: `c-api/gcsupport.rst` is not in `.nitignore`,
so real Sphinx resolves it today.

**Reproducer:**

```rst
.. c:macro:: Py_VISIT(o)

   A macro.

A typical traverse function calls the :c:func:`Py_VISIT` macro.
```

**Suggested fix:** add `C(Macro)` to `C(Function)`'s alias list (and, for
symmetry with the existing pair, consider `C(Function)` in `C(Macro)`'s).

---

## 2. `c:function`/`c:macro` are qualified against the enclosing `PythonScope`, not `CScope` — FIXED

- **Component:** `crates/scope/src/python_scope.rs`, `crates/scope/src/c_scope.rs`,
  consumed from `crates/renderer/src/directives.rs` and
  `crates/analyzer/src/lib.rs`'s `uses_c_scope` match.

A `.. c:function::`/`.. c:macro::` nested inside a `.. py:class::`/
`.. py:exception::` body used to get prefixed with the enclosing Python
module+class name, because it fell through to `PythonScope::qualify` like
every other non-`c:struct`/`c:union`/`c:member`/`c:type` object. Real Sphinx's
C domain has no concept of a "current Python class" at all, so this was
wrong.

**Fix:** `c:function`/`c:macro` joined the `uses_c_scope` set in both
`index_domain_object` and `render_domain_object`, so every `c`-domain object
type now qualifies against `CScope` uniformly, never `PythonScope`. See
`test_analyze_c_function_nested_in_py_class_is_not_qualified_by_it` /
`test_render_formats_c_function_nested_in_py_class_is_not_qualified_by_it`.

This also fixed a related, previously-undocumented asymmetry: reference
*resolution* (`DomainObjectResolver::resolve`) already dispatched a `c:function`/
`c:macro` reference through `CScope` (it keys off `object_type.domain()`,
not the object's variant), so a definition qualified via `PythonScope` while
in-document references to it resolved via `CScope` were silently out of sync
whenever a class scope was in play. Both now agree.

**Trade-off at the time, since resolved — see entry 3 below:** because
`CScope`'s scope is shared by every `c`-domain object, `c:function`/`c:macro`
nested inside `c:struct`/`c:union`/`c:type` became qualified by that
container too, which they weren't before. That briefly regressed one real
CPython-docs case; implementing `c:namespace` closed it.

---

## 3. `c:macro` nested under `c:type` was qualified, unlike real CPython docs — FIXED

- **Component:** `crates/scope/src/c_scope.rs` (`CScope::set_namespace`/
  `push_namespace`/`pop_namespace`), `Directive::CNamespace`/`CNamespacePush`/
  `CNamespacePop` in `crates/ast/src/directive.rs`, parsed by
  `try_parse_scope_directive` (`crates/parser/src/directives.rs`) and applied
  in `crates/analyzer/src/lib.rs`'s `index_nodes` and
  `crates/renderer/src/lib.rs`'s `render_directive`.

CPython's `c-api/memory.rst` nests enum-style `.. c:macro::` constants (e.g.
`PYMEM_DOMAIN_RAW`) inside `.. c:type:: PyMemAllocatorDomain`, but references
and renders them under their **bare** name, because the source writes a
`.. c:namespace:: NULL` inside that body to reset the qualifier. Bug 2's fix
made `c:macro` consult `CScope` like every other `c`-domain object, so
without `c:namespace` support the constant wrongly indexed as
`PyMemAllocatorDomain.PYMEM_DOMAIN_RAW`.

**Fix:** the full `c:namespace`/`c:namespace-push`/`c:namespace-pop` family
is now implemented. `CScope` models the current scope as a stack of scopes
whose last entry is in effect; lexical nesting and the namespace directives
both operate on it, exactly as real Sphinx has both `CObject.before_content`/
`after_content` and its namespace directives read and write the single
`ref_context['c:parent_key']`. The body-close restore replaces the whole
stack by value rather than truncating to a saved depth, so a `c:namespace`
reset inside a body cannot strand the scope when that body closes. See
`test_analyze_c_namespace_null_inside_c_type_body_unqualifies_nested_macro`
and the `C Namespaces` section of `examples/domains.rst`.

One deliberate deviation: an unmatched `c:namespace-push` inside a
`c:struct`/`c:union`/`c:type` body is contained by that body's close, so it
cannot be consumed by a later unrelated `c:namespace-pop`. Sphinx's
`before_content`/`after_content` save only the current-scope pointer and not
its namespace stack, so it appears to leak there; containing is simpler and
more defensible for what is malformed input either way.

---

## Note on the whitelist's justifications

While triaging the above, the `.nitignore` oracle contradicted a claim that
several entries in `scripts/domain_warnings_whitelist.json` used to make: that
real Sphinx also fails to resolve a `` :func:`SomeClass` ``-style
role/object-type mismatch. It does not. `PythonDomain.find_obj` only filters
by object type when the target is dot-prefixed (`refspecific`); for a plain
target it takes the exact-name match whatever its type — the code path is
commented `# NOTE: searching for exact match, object type is not considered`.
That is corroborated empirically: `csv.rst`, `threading.rst`, `functions.rst`,
`functools.rst`, `stdtypes.rst`, `dataclasses.rst`, `decimal.rst` and
`bdb.rst` all contain such references and are all absent from `.nitignore`.

Flagging these is still the *intended* rusty-sphinx behaviour — see the
module doc of `crates/renderer/src/domain_resolution.rs`, deviation (1) — and
the underlying CPython markup really is inconsistent, so these belong in the
whitelist rather than here. Every affected entry has been reworded to say that
accurately, so a `` :func:`SomeClass` `` suppression in the whitelist now
records "real Sphinx links this type-blindly; we deliberately do not" rather
than the false "real Sphinx fails to resolve this too".

A related case worth knowing about: `` :data:`errno` `` in `library/ctypes.rst`
is not merely unresolved in real Sphinx — the type-blind exact-name match sends
it to the **`errno` module page**, because `library/errno.rst` declares
`.. module:: errno`. That is the concrete hazard behind
`domain_resolution.rs`'s note that Sphinx's "only exact matches allowed for
modules" rule is deliberately not reproduced here.
