"""rusty_sphinx_doctest_tests macro.

Runs the doctest blocks of a `rusty_sphinx_library`'s documents, one Bazel test
per document.

# Why a macro over stock `py_test`, rather than a custom test rule

A custom `rule(test = True)` would have to hand-roll a launcher that rebuilds
`PYTHONPATH` from each dependency's `PyInfo.imports` and merges runfiles —
reimplementing rules_python's bootstrap, for no gain. Emitting stock `py_test`
targets gets all of that, plus test timeouts, `--runs_per_test`,
`--flaky_test_attempts`, JUnit XML and `--cache_test_results`, behaving exactly
as they do everywhere else.

# Why execution is a test, not a build action

An earlier design ran the doctests in a cached build action and left the test
target to inspect the result. That is worse in three ways: `bazel build //...`
builds test targets' runfiles, so the wildcard build CI runs would have executed
every doctest; build actions have no timeout, so one `input()` call hangs the
build with no `--test_timeout` to stop it; and `--cache_test_results=no` would
have become a silent no-op, reporting PASSED from a stale file.

None of that is needed to get the caching win. Bazel already caches test results
on the test's runfiles, so when a document's only data runfile is its
`.doctests.json` — whose bytes ignore prose, `:hide:` and trim options — a prose
edit leaves the key unchanged and `bazel test` reports `(cached)` without
starting an interpreter. The firewall is the extraction step, which stays a
build action.

# Keeping Python out of the build

The Python toolchain is a dependency of these test targets only. It must never
become one of `rusty_sphinx_library` or `rusty_sphinx_site`:
`bazel build //examples:site` has to stay buildable with no Python toolchain
registered. `tests/test_doctest_isolation.sh` enforces that.
"""

load("@rules_python//python:defs.bzl", "py_test")

def _doctest_plan_key(relative_path):
    """Mirrors `_doctest_plan_key` in rules/library.bzl.

    The two must agree: the rule derives its key from a source's
    package-relative path, and this derives the same key from the `srcs` string
    the BUILD file passes. Providers are not visible at loading time, so a
    shared naming convention is what connects them.
    """
    return relative_path.replace("/", "_").replace(".", "_").replace("-", "_")

def rusty_sphinx_doctest_tests(
        name,
        lib,
        srcs,
        py_deps = None,
        global_setup = None,
        global_cleanup = None,
        size = "small",
        tags = None,
        **kwargs):
    """Defines one doctest test per document, plus a suite over all of them.

    Args:
      name: Name of the generated `test_suite`. Individual tests are named
        `<name>_<document>`.
      lib: The `rusty_sphinx_library` whose documents to test.
      srcs: The same `.rst` list given to `lib`. Bazel macros cannot read a
        target's providers, so the document list has to be repeated here;
        define it once as a variable (or `glob`) and pass it to both. A name
        listed here but absent from `lib` fails the build; one omitted here is
        simply not tested.
      py_deps: `py_library` targets to put on the path, so documented code is
        importable. This is rules_python's ordinary `deps`, surfaced — not a
        third dependency mechanism of rusty-sphinx's own. Note it is unrelated
        to `rusty_sphinx_library`'s `deps`, which is strictly about toctrees.
      global_setup: A `.py` file run before every group, Sphinx's
        `doctest_global_setup`. A file label rather than a config field,
        because sandboxes relocate paths (see ADR-001).
      global_cleanup: A `.py` file run after every group.
      size: Bazel test size; `small` (60s) by default. A doctest that needs
        longer is usually a doctest that should not be one.
      tags: Applied to the generated tests *and* to the suite. Both are needed
        for `manual` to work: a `test_suite` that explicitly lists a target
        re-includes it in wildcard expansion, so tagging only the tests would
        leave them running under `bazel test //...`.
      **kwargs: Passed to each generated `py_test` (`timeout`, `flaky`, …).
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

    tests = []
    for src in srcs:
        if not src.endswith(".rst"):
            fail("rusty_sphinx_doctest_tests: srcs must be .rst files, got %r" % src)

        key = _doctest_plan_key(src.removesuffix(".rst"))
        plan_target = "%s_%s_plan" % (name, key)

        # Extract just this document's plan from the library's output group,
        # so it is the test's only data runfile and the cache key tracks one
        # document's test code.
        native.filegroup(
            name = plan_target,
            srcs = [lib],
            output_group = "doctest_plan_" + key,
            testonly = True,
        )

        test_name = "%s_%s" % (name, key)
        py_test(
            name = test_name,
            srcs = ["@rusty_sphinx//scripts:doctest_runner.py"],
            main = "doctest_runner.py",
            args = ["$(location :%s)" % plan_target] + shared_args,
            data = [":" + plan_target] + shared_data,
            deps = py_deps,
            size = size,
            tags = tags,
            **kwargs
        )
        tests.append(":" + test_name)

    native.test_suite(
        name = name,
        tests = tests,
        tags = tags,
    )
