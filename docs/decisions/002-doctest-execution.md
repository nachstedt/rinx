# ADR-002: Doctest Execution

**Status:** Accepted
**Date:** 2026-08-23

## Context

`sphinx.ext.doctest` gives five directives — `doctest`, `testcode`, `testoutput`,
`testsetup`, `testcleanup` — plus docutils' implicit doctest block (a text block
starting with `>>> `). Together they were the largest cluster of unsupported
directives in the CPython benchmark corpus.

They are unusual among the constructs rusty-sphinx implements, because they have
two halves with completely different requirements:

- **Rendering** is pure. The directive carries its content literally in the
  `.rst`; turning it into a `<pre class="language-pycon">` needs nothing but the
  AST.
- **Execution** asserts that *CPython* produces the stated output. CPython is the
  system under test, not an implementation detail.

ADR-001 rejected a Python runtime on the render path, and that objection still
stands. But documentation whose examples are never run rots silently, which is
the entire reason `make doctest` exists.

## Decision Drivers

- **ADR-001's boundary holds:** `bazel build //examples:site` must stay buildable
  with no Python toolchain registered.
- **A broken example must not block publishing.** A failing doctest is a bug in
  the documented code or the example, not a reason to stop shipping the site.
- **Prose edits must be cheap.** Documentation changes constantly, and almost
  none of those changes affect what the examples do. Re-running an interpreter
  over a whole corpus because someone fixed a typo is the failure mode to avoid.
- **No second oracle.** `doctest.OutputChecker` — `ELLIPSIS`,
  `NORMALIZE_WHITESPACE`, `<BLANKLINE>`, `IGNORE_EXCEPTION_DETAIL`, exception
  matching — must not be reimplemented.

## Decision

Split the two halves across Bazel's build/test line.

**Rendering happens in the normal pipeline.** `rusty_sphinx_library` parses these
directives and `rusty_sphinx_site` renders them, exactly like any other block. No
interpreter is involved.

**Execution is `rusty_sphinx_doctest_tests`**, an opt-in macro emitting one stock
`py_test` per `rusty_sphinx_library`. The Python toolchain is a dependency of
those test targets only; `tests/test_doctest_isolation.sh` turns that from a
design intention into a checked invariant.

**`scripts/doctest_runner.py` drives CPython's stdlib `doctest`** rather than
reimplementing its comparison semantics, the same way PlantUML diagrams are
compiled by `plantuml.jar` — rusty-sphinx does not own that language either. It
reproduces Sphinx's own deviations from the stdlib: `doctest_default_flags`, one
namespace per group, a failed setup skipping its group, `:pyversion:` compared
against the *running* interpreter, `:skipif:` evaluated in a context built from
global setup then global cleanup, and `DONT_ACCEPT_BLANKLINE` forced on for
`testcode`/`testoutput` pairs.

### The cache firewall

Between the two halves sits an `extract_doctests` build action, declared per
document by `rusty_sphinx_library` (`crates/worker/src/doctest_plan.rs`). It
projects the AST down to a `.doctests.json` plan holding **only what changes how
the code runs** — never `:hide:`, never the trim tri-state, never line numbers.

A prose edit therefore re-runs that cheap AST walk but leaves its bytes
identical, so Bazel's test-result cache stops the cascade and no interpreter
starts. `tests/test_doctest_cache_firewall.sh` asserts both directions, and
`doctest_plan.rs`'s own test asserts **byte** equality of plans across
presentation-only differences, not merely structural equality.

The plans live in a non-default output group (`doctest_plans`), so building a
site never produces them.

### One test target per library

`rusty_sphinx_library` is the unit of ownership everywhere else in this ruleset,
mirroring `cc_library`, and the doctest macro follows it. Someone who wants finer
granularity splits the library — which is what they would do for any other reason
too. `examples/BUILD.bazel` demonstrates this: the one document with executable
content lives in its own `doctest_docs` library.

## Options Considered

### Running doctests during the site build

**Rejected because:** it puts a Python runtime back on the render path, in direct
conflict with ADR-001, and it makes a broken example block publication of the
documentation that describes it.

### A cached build action, with the test target inspecting its result

**Rejected because** it is worse in three ways: `bazel build //...` builds test
targets' runfiles, so a wildcard build in CI would have executed every doctest;
build actions have no timeout, so one `input()` call hangs the build with no
`--test_timeout` to stop it; and `--cache_test_results=no` would have become a
silent no-op, reporting PASSED from a stale file.

None of it was needed for the caching win — Bazel already caches test results on
the test's runfiles, and the plan file is what those runfiles contain.

### A custom `rule(test = True)` instead of a macro over `py_test`

**Rejected because** it would have to hand-roll a launcher that rebuilds
`PYTHONPATH` from each dependency's `PyInfo.imports` and merges runfiles —
reimplementing rules_python's bootstrap, for no gain. Stock `py_test` targets get
all of that, plus test timeouts, `--runs_per_test`, `--flaky_test_attempts`,
JUnit XML and `--cache_test_results`, behaving exactly as they do everywhere
else.

### One test target per document

The original design. Bazel macros cannot read a target's providers at loading
time, so the macro could not learn the document list from the library it points
at.

**Rejected because** the workarounds cost more than the granularity was worth:
the `.rst` list had to be repeated in the BUILD file — once for the library and
once for the tests — and the two sides were connected by a `_doctest_plan_key`
naming convention duplicated in `rules/library.bzl` and `rules/doctest.bzl`, with
a per-document output group for every source. Neither duplication had any purpose
beyond addressing a single document's plan by label.

### Reimplementing `doctest` comparison semantics in Rust

**Rejected because** it would not remove the interpreter dependency — the code
still has to be executed — and would only add a second, silently divergent
oracle.

## Consequences

- **Doctests are opt-in.** A `rusty_sphinx_library` without a matching
  `rusty_sphinx_doctest_tests` target has its examples rendered but never run.
  This is deliberate: adding a Python toolchain requirement must be the user's
  choice.
- **A code edit in one document re-runs its library's other documents.** The
  granularity of the test-result cache is the library. Split the library if that
  becomes expensive.
- **A library's documents share one interpreter.** Process-global side effects
  (`sys.modules`, `os.chdir`, monkeypatching) can leak between documents of the
  same library. Per-*group* namespaces are unaffected — each group still gets a
  fresh namespace. This moves *closer* to Sphinx, whose `make doctest` runs a
  whole project in one process.
- **`doctest`'s hard-coded compile mode has to be patched.** `doctest` compiles
  every example with `compile(..., "single", ...)`, which rejects the
  multi-statement code a `testcode` block normally holds. The runner replaces the
  `compile` that `doctest`'s module globals resolve — the same workaround Sphinx
  uses. It is the only way to keep doctest's comparison and failure reporting
  while executing block code.
- **Autodoc stays out of scope, and this is not a contradiction.** The rule is
  not "Sphinx extensions are out of scope": `sphinx.ext.doctest` is in scope
  precisely because its parse/render half is pure. `automodule` has to import
  user modules to generate documentation *content*, which puts an interpreter
  back on the render path.
