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
known to resolve under real Sphinx. A bug earns a place below only if it lives
in files absent from `.nitignore` and references targets absent from
`nitpick_ignore`.

---

## The `:module:` option on Python domain objects is ignored

**Status:** open.
**Symptom:** `Doc/library/multiprocessing.shared_memory: 'multiprocessing.managers.SharedMemoryManager' (referenced as py:class)`, 4 occurrences.

Sphinx's Python-domain object directives accept a `:module:` option that
overrides the module a definition is filed under, independently of the
enclosing `.. module::`/`.. currentmodule::`. CPython uses it to document a
class on a *different* module's page:

```rst
.. module:: multiprocessing.shared_memory

...

.. class:: SharedMemoryManager([address[, authkey]])
   :module: multiprocessing.managers

   A subclass of :class:`multiprocessing.managers.BaseManager` which can be
   used for the management of shared memory blocks across processes.
```

Real Sphinx indexes that as `multiprocessing.managers.SharedMemoryManager`, so
the four `` :class:`~multiprocessing.managers.SharedMemoryManager` ``
references in the same file resolve. `Doc/library/multiprocessing.shared_memory.rst`
is absent from `.nitignore`, confirming they resolve warning-free today.

rusty-sphinx has no handling for the option at all. Two consequences, both
visible in the built artifacts:

1. **Wrong index key.** `bazel-bin/Doc/site.project.index` holds
   `multiprocessing.shared_memory.sharedmemorymanager`, filed under the
   enclosing `.. module::` instead of the override, so every reference to the
   real name misses. The nested `.. method::`s inherit the same wrong prefix
   (`multiprocessing.shared_memory.sharedmemorymanager.shareablelist` etc.).
2. **The option leaks into the rendered body.** The parsed AST for that
   directive starts with
   `{"Paragraph": [{"Text": ":module: multiprocessing.managers"}]}` — the
   unrecognized option line is not consumed as an option and is parsed as the
   first paragraph of the docstring, so it is rendered as prose on the page.

The fix belongs with the other per-directive option extraction in
`crates/parser/src/domains.rs` — `extract_common_object_description_options`
handles the `:no-index:` family and `extract_module_options` handles
`:platform:`/`:synopsis:`/`:deprecated:`; `:module:` needs an equivalent that
feeds the analyzer's `PythonScope` module component for that one object rather
than just being stripped. Sphinx applies it to every `py:` object directive,
not only `py:class`.

`Doc/library/ctypes.rst` uses `:module: ctypes.util` the same way six times,
and `benchmark_result.txt` shows the matching fallout: unresolved
`ctypes.util.dllist`, `ctypes.util.struct`, `ctypes.util.wrap_dll_function`
and `ctypes.util.find_library` references from ctypes.rst and the whatsnew
pages. The same fix clears those.

---

## Note on the whitelist's justifications

While triaging, the `.nitignore` oracle contradicted a claim that
several entries in `scripts/domain_warnings_whitelist.json` used to make: that
real Sphinx also fails to resolve a `` :func:`SomeClass` ``-style
role/object-type mismatch. It does not. `PythonDomain.find_obj` only filters
by object type when the target is dot-prefixed (`refspecific`); for a plain
target it takes the exact-name match whatever its type — the code path is
commented `# NOTE: searching for exact match, object type is not considered`.
That is corroborated empirically: `csv.rst`, `threading.rst`, `functions.rst`,
`functools.rst`, `stdtypes.rst`, `dataclasses.rst`, `decimal.rst`,
`pickle.rst`, `string.rst`, `warnings.rst`, `unittest.mock-examples.rst`,
`datamodel.rst` and `bdb.rst` all contain such references and are all absent
from `.nitignore`.

Flagging these is still the *intended* rusty-sphinx behaviour — see the
module doc of `crates/renderer/src/domain_resolution.rs`, deviation (1) — and
the underlying CPython markup really is inconsistent, so these belong in the
whitelist rather than here. Every affected entry is worded to say that
accurately, so a `` :func:`SomeClass` `` suppression in the whitelist records
"real Sphinx links this type-blindly; we deliberately do not" rather than the
false "real Sphinx fails to resolve this too".

A related case worth knowing about: `` :data:`errno` `` in `library/ctypes.rst`
is not merely unresolved in real Sphinx — the type-blind exact-name match sends
it to the **`errno` module page**, because `library/errno.rst` declares
`.. module:: errno`. That is the concrete hazard behind
`domain_resolution.rs`'s note that Sphinx's "only exact matches allowed for
modules" rule is deliberately not reproduced here.

---

## Fixed

- **C-domain reference targets keep a trailing `()`.**
  `strip_trailing_call_parens` in `crates/parser/src/inline.rs` now keeps the
  parens out of the lookup name and in the display text, per domain;
  `spec_gaps.md` records the details.
