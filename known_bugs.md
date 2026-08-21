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
known to resolve under real Sphinx. The open bug below lives in files absent
from `.nitignore` and references targets absent from `nitpick_ignore`.

---

## 1. C-domain reference targets keep a trailing `()`

- **Warnings:** `1  Doc/c-api/object: 'Py_TYPE()' (referenced as c:function)`,
  `1  Doc/c-api/refcounting: 'Py_REFCNT()' (referenced as c:function)`,
  `1  Doc/c-api/refcounting: 'Py_SET_REFCNT()' (referenced as c:function)`,
  `2  Doc/c-api/typeobj: 'Py_SIZE()' (referenced as c:function)`,
  `1  Doc/c-api/structures: 'Py_TYPE()'`/`'Py_SIZE()'`, plus several
  `whatsnew/*` occurrences — roughly ten in all.
- **Component:** `crates/parser/src/inline.rs` (the domain-role handlers), or
  wherever a `c`-domain reference target is normalized before it reaches
  `DomainObjectResolver::resolve`.

CPython's docs routinely write a C function reference with empty parentheses,
`` :c:func:`Py_TYPE()` ``, to read as a call at the point of use. Real Sphinx's
C domain strips a trailing `()` from the target before looking it up, so these
resolve against the plain `Py_TYPE` declaration. rusty-sphinx keeps the parens
as part of the name, so the lookup misses and the reference breaks.

The affected files — `c-api/object.rst`, `c-api/refcounting.rst`,
`c-api/structures.rst`, `c-api/typeobj.rst` — are all absent from CPython's
`Doc/tools/.nitignore`, and the targets are absent from `Doc/conf.py`'s
`nitpick_ignore`, so real Sphinx resolves every one of them today.

This overlaps with, but is independent of, the `function`/`macro` alias pair:
some of these same symbols also collide on object type, and fixing the alias
table did not help the paren-suffixed spellings at all.

**Reproducer:**

```rst
.. c:function:: PyTypeObject *Py_TYPE(PyObject *o)

   Returns the object's type.

Use :c:func:`Py_TYPE()` to inspect an object's type.
```

**Suggested fix:** strip one trailing `()` from a `c`-domain reference target
during parsing, recording it as markup rather than part of the name — the same
"a sigil is markup, not part of a name" treatment the leading `.`/`~` prefixes
already get. Note the display text should keep the parens, as Sphinx does.

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
