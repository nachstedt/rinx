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

- **Nested `.. plantuml::` diagrams were silently dropped, and validation did
  not catch it.** Both `process_extract_diagrams` and `process_validate_images`
  in `crates/worker/src/main.rs` iterated only `doc.nodes`, so a diagram inside
  any container — an admonition, a `seealso`, a list item, a table cell, a
  domain-object body — produced no `.puml`, hence no `.svg`, hence a dangling
  `<img src="_images/<hash>.svg">` in the output. Because the *validator* had
  the identical blind spot, the `.images.validated` sentinel passed anyway, so
  the "documentation builds should fail loudly on missing diagram images"
  guarantee did not hold for nested diagrams. Both now share a single
  `collect_plantuml_contents` helper built on the new
  `rusty_sphinx_ast::walk_nodes` pre-order walker (`crates/ast/src/visit.rs`),
  so the compiled set and the validated set cannot drift apart again. The
  walker's inner match is exhaustive, so a future `Node`/`Directive` variant
  carrying child nodes is a compile error rather than another silent gap.
  `examples/team_a/index.rst` covers the case with a diagram inside a
  `.. note::`.

- **C-domain reference targets keep a trailing `()`.**
  `strip_trailing_call_parens` in `crates/parser/src/inline.rs` now keeps the
  parens out of the lookup name and in the display text, per domain;
  `spec_gaps.md` records the details.

- **The `:module:` option on Python domain objects was ignored.**
  Every `py:*` object-description directive except `py:module` itself now
  parses a leading `:module:` option line (`crates/parser/src/domains.rs`,
  alongside each type's other options) into a new `module: Option<String>`
  field on `DomainObjectBody`, exposed via `DomainObjectBody::module_override`.
  Both the analyzer's `index_domain_object` and the renderer's
  `render_domain_object` push it onto `PythonScope` for the duration of the
  object's own qualification and nested body via the new
  `PythonScope::push_module_override`/`restore_module`, mirroring real
  Sphinx's `PyObject.before_content()`/`after_content()` push/pop of
  `ref_context['py:module']` — restored afterward so a later sibling with no
  override of its own reverts to the enclosing `.. module::`/
  `.. currentmodule::`. This fixes both consequences noted above: the wrong
  index key (`multiprocessing.shared_memory.sharedmemorymanager` instead of
  `multiprocessing.managers.sharedmemorymanager`) and the option leaking into
  the rendered body as a stray paragraph. An empty `:module:` value clears
  the module instead of setting it, matching real Sphinx's falsy-`modname`
  check in `add_target_and_index`. See the "The `:module:` Option Override"
  section of `examples/domains.rst` and `spec_gaps.md` for details.
