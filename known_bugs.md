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
known to resolve under real Sphinx. All three bugs below live in files absent
from `.nitignore` and reference targets absent from `nitpick_ignore`.

---

## 1. Unqualified `c:member` references are not resolved against the enclosing C scope

- **Warning:** `7  Doc/c-api/long: 'digits' (referenced as c:member)` (also
  `value`, `negative`, `ndigits`, 1 occurrence each, same file and cause)
- **Component:** `crates/renderer/src/domain_resolution.rs`

`Doc/c-api/long.rst` declares

```rst
.. c:type:: PyLongExport

   * If :c:member:`digits` is ``NULL``, only use the :c:member:`value` member.

   .. c:member:: const void *digits
```

The analyzer indexes the definition correctly as `pylongexport.digits` — its
`CScope` qualifies nested declarations under the enclosing `c:type`. The
*resolver*, however, only ever receives a `PythonScope`; `CScope` is never
consulted at resolution time, so an unqualified `digits` written inside
`PyLongExport`'s own body is looked up as the bare top-level name `digits`
and misses.

Real Sphinx's C domain resolves this: `_resolve_xref_inner` starts from the
node's `c:parent_key` (the enclosing declaration) and calls
`Symbol.find_declaration(..., matchSelf=True)`, which walks up the nesting
scopes. The existing dot-prefixed suffix search in
`test_resolve_dot_prefixed_c_member_finds_nested_member_via_suffix_search`
covers `` :c:member:`.digits` ``, but a plain target uses
`TargetSearchOrder::LeastQualifiedFirst`, which does not allow the suffix
fallback.

**Reproducer** (parse → index → render emits `warning: broken domain object
'digits' (referenced as c:member)`):

```rst
.. c:type:: PyLongExport

   * If :c:member:`digits` is ``NULL``, only use the :c:member:`value` member.

   .. c:member:: int64_t value

      The native integer value.

   .. c:member:: const void *digits

      Read-only array of unsigned digits.
```

**Suggested fix:** thread the render-time `CScope` into
`DomainObjectResolver::resolve` for `c`-domain references and try the
enclosing-container prefixes (`PyLongExport.digits`, then `digits`) the way
`PythonScope::reference_candidates` already does for the `py` domain.

---

## 2. `.. decorator::` is not implemented, so decorators are never indexed

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

## 3. `:c:func:` does not accept a `.. c:macro::` definition

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
