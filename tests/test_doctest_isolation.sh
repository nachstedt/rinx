#!/bin/bash
# test_doctest_isolation.sh
# Enforces ADR-002's binding scope limit: the Python toolchain is a dependency
# of the doctest *test* targets only, never of building documentation.
#
# ADR-001 rejected a Python runtime on the render path. Executing doctests
# genuinely needs an interpreter, but that dependency has to stay on the test
# side of the build/test line, or the objection ADR-001 raised comes back. This
# turns "opt-in" from a design intention into a checked invariant.

set -euo pipefail

BAZEL=${BAZEL:-bazel}

echo "=== Testing that the doctest runner is not a dependency of the site ==="
if $BAZEL cquery 'deps(//examples:site)' 2>/dev/null | grep -q "doctest_runner"; then
    echo "ERROR: The Python runner reached the site's dependency graph."
    exit 1
else
    echo "SUCCESS: No Python runner in the site's dependencies."
fi

echo "=== Testing that building the site produces no doctest plans ==="
# The extraction action is declared for every document but lives in a
# non-default output group, so a build that does not ask for it must not run it.
# Deleting the outputs first makes this an execution-level check rather than an
# assertion about the analysis graph.
find -L bazel-bin/examples -name '*.doctests.json' -delete 2>/dev/null || true

$BAZEL build //examples:site > /dev/null 2>&1
plans_after_site=$(find -L bazel-bin/examples -name '*.doctests.json' 2>/dev/null | wc -l)

if [ "$plans_after_site" -ne 0 ]; then
    echo "ERROR: Building the site produced $plans_after_site doctest plan(s)."
    echo "       The extraction outputs have leaked into DefaultInfo."
    exit 1
fi
echo "SUCCESS: Site build produced no doctest plans."

echo "=== Testing that the plans are still produced on demand ==="
$BAZEL build //examples:root_docs --output_groups=doctest_plans > /dev/null 2>&1
plans_on_demand=$(find -L bazel-bin/examples -name '*.doctests.json' 2>/dev/null | wc -l)

if [ "$plans_on_demand" -eq 0 ]; then
    echo "ERROR: Requesting the doctest_plans output group produced nothing."
    exit 1
fi
echo "SUCCESS: Requesting the output group produced $plans_on_demand plan(s)."

exit 0
