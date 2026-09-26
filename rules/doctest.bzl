"""rinx_doctest_tests macro.

Runs the doctest blocks of a `rinx_library`'s documents as one Bazel
test. See `docs/decisions/002-doctest-execution.md` for the full rationale; the
notes below are what you need before editing this file.

# Why one test per library, not per document

`rinx_library` is the unit of ownership everywhere else in this ruleset,
exactly like `cc_library`, and a macro cannot read its providers at loading
time. Testing per document therefore meant repeating the document list in the
BUILD file and connecting the two sides by a duplicated naming convention. Per
library, the macro just consumes the library's aggregate `doctest_plans` output
group and neither duplication exists.

Someone who wants finer granularity splits the library, which is the same thing
they would do for any other reason — `examples/BUILD.bazel` keeps the one
document with executable content in its own `doctest_docs` library for exactly
this reason.

Two consequences, both accepted: editing test code in one document re-runs that
library's other documents too, and a library's documents share one interpreter,
so process-global side effects (`sys.modules`, `os.chdir`, monkeypatching) can
leak between them. Per-*group* namespaces are unaffected. The second moves
*closer* to Sphinx, whose `make doctest` runs a whole project in one process.

# Why a macro over stock `py_test`, and why a test at all

Both argued in full in the ADR. In short: a custom `rule(test = True)` would have
to reimplement rules_python's bootstrap for no gain, and running the doctests in
a build action instead would execute them under `bazel build //...`, lose
`--test_timeout`, and make `--cache_test_results=no` a silent no-op.

Bazel already caches test results on the test's runfiles, so when the only data
runfiles are the library's `.doctests.json` plans — whose bytes ignore prose,
`:hide:` and trim options — a prose edit leaves the key unchanged and
`bazel test` reports `(cached)` without starting an interpreter. The firewall is
the extraction step, which stays a build action.

# Keeping Python out of the build

The Python toolchain is a dependency of these test targets only. It must never
become one of `rinx_library` or `rinx_site`:
`bazel build //examples:site` has to stay buildable with no Python toolchain
registered. `tests/test_doctest_isolation.sh` enforces that.
"""

load("@rules_python//python:defs.bzl", "py_test")

def rinx_doctest_tests(
        name,
        lib,
        py_deps = None,
        global_setup = None,
        global_cleanup = None,
        size = "small",
        tags = None,
        **kwargs):
    """Defines the doctest test for one `rinx_library`.

    Args:
      name: Name of the generated `py_test`.
      lib: The `rinx_library` whose documents to test. Every document it
        owns is tested; its `deps` are not, since each library carries the test
        target for its own documents.
      py_deps: `py_library` targets to put on the path, so documented code is
        importable. This is rules_python's ordinary `deps`, surfaced — not a
        third dependency mechanism of rinx's own. Note it is unrelated
        to `rinx_library`'s `deps`, which is strictly about toctrees.
      global_setup: A `.py` file run before every group, Sphinx's
        `doctest_global_setup`. A file label rather than a config field,
        because sandboxes relocate paths (see ADR-001).
      global_cleanup: A `.py` file run after every group.
      size: Bazel test size; `small` (60s) by default. A doctest that needs
        longer is usually a doctest that should not be one.
      tags: Applied to the generated test, e.g. `manual` to keep it out of
        `bazel test //...`.
      **kwargs: Passed to the generated `py_test` (`timeout`, `flaky`, …).
    """
    py_deps = py_deps or []
    tags = tags or []

    shared_args = []
    shared_data = []
    if global_setup:
        shared_args += ["--global-setup", "$(location %s)" % global_setup]
        shared_data.append(global_setup)
    if global_cleanup:
        shared_args += ["--global-cleanup", "$(location %s)" % global_cleanup]
        shared_data.append(global_cleanup)

    # The library's plans are its only data runfiles, so the test-result cache
    # key tracks its test code and nothing else.
    plan_target = "%s_plans" % name
    native.filegroup(
        name = plan_target,
        srcs = [lib],
        output_group = "doctest_plans",
        testonly = True,
    )

    py_test(
        name = name,
        srcs = ["@rinx//scripts:doctest_runner.py"],
        main = "doctest_runner.py",
        # `locations`, plural: one argument per document in the library.
        args = ["$(locations :%s)" % plan_target] + shared_args,
        data = [":" + plan_target] + shared_data,
        deps = py_deps,
        size = size,
        tags = tags,
        **kwargs
    )
