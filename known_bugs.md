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
known to resolve under real Sphinx. Both numbered bugs below live in files
absent from `.nitignore` and reference targets absent from `nitpick_ignore`
— the third entry is a separately-discovered latent gap with no
benchmark-observed warning (see its own note).

---

## 1. `.. decorator::` is not implemented, so decorators are never indexed

- **Warning:** `5  Doc/reference/datamodel: 'classmethod' (referenced as py:class)`
- **Component:** `crates/parser/src/directives.rs` (`decorator: 69` in the
  benchmark's "Unsupported Directives Summary")

`classmethod`, `staticmethod`, `property`-style builtins and every decorator
in the stdlib docs are declared with `.. decorator:: name`, which rusty-sphinx
does not parse — so they never enter the index at all and *every* reference to
them breaks, whatever role is used.

Real Sphinx maps `PyDecoratorFunction`/`PyDecoratorMethod` onto the
`py:function` / `py:method` object types (`PyDecoratorFunction.run()` sets
`self.name = 'py:function'` before delegating), so
`.. decorator:: classmethod` registers exactly as if it had been written
`.. function:: classmethod`.

**Reproducer:**

```rst
.. decorator:: classmethod

   Transform a method into a class method.

Retrieving a :class:`classmethod` object.
```

**Suggested fix:** parse `decorator` / `decoratormethod` as aliases producing
`DomainObjectBody::PyFunction` / `PyMethod`. Note this alone will not silence
this particular warning — the reference uses `:class:` against what becomes a
`py:function`, which is the separate deliberate strictness deviation noted at
the bottom of this file — but it fixes the `:func:`/`:deco:` references, which
are the majority.

---

## 2. `:c:func:` does not accept a `.. c:macro::` definition

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

## 3. `c:function`/`c:macro` are qualified against the enclosing `PythonScope`, not `CScope`

- **Component:** `crates/scope/src/python_scope.rs` (`PythonScope::qualify`'s
  domain gating: `include_module = domain == Domain::Py || !self.classes.is_empty()`),
  consumed from `crates/renderer/src/directives.rs` and
  `crates/analyzer/src/lib.rs` wherever `c:function`/`c:macro` fall through to
  the non-`uses_c_scope` branch.

A `.. c:function::`/`.. c:macro::` nested inside a `.. py:class::`/
`.. py:exception::` body (so `PythonScope`'s class stack is non-empty when
the C object is qualified) currently gets prefixed with that enclosing
Python module+class name, per
`test_qualify_includes_module_for_c_domain_when_class_scope_present` in
`crates/scope/src/python_scope.rs`.

Real Sphinx's C domain has no concept of a "current Python class" at all —
the only namespacing mechanism a `c:function`/`c:macro` respects is
`.. c:namespace::`, which rusty-sphinx does not implement. A C function
nested inside a Python class in real Sphinx registers under its own bare
name, unaffected by the enclosing class.

No CPython doc source is currently known to trigger this — real docs never
nest a `c:function`/`c:macro` inside a `py:class` body — so unlike the two
bugs above this has no benchmark-observed symptom. Flagging it as a latent
correctness gap found while investigating (and fixing) an earlier bug in
this file about unqualified `c:member` reference resolution, not a triaged
warning.

**Suggested fix:** not yet investigated in depth. Candidates: stop
consulting `PythonScope`'s module/class parts for `Domain::C` objects
entirely (route `c:function`/`c:macro` through `CScope` for consistency with
the rest of the `c` domain, even though they don't currently establish or
consume container nesting), or simply always qualify them to their own
bare/dotted name regardless of any enclosing `PythonScope`.

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
