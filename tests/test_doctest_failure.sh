#!/bin/bash
# test_doctest_failure.sh
# Tests that a wrong expected output fails `bazel test`, and that the very same
# document still renders to HTML. That pairing is the whole point of keeping
# rendering and execution on opposite sides of Bazel's build/test line: a broken
# example must not stop the documentation from being published.

set -euo pipefail

BAZEL=${BAZEL:-bazel}

echo "=== Testing that a failing doctest fails the test target ==="
if $BAZEL test //tests/doctest_failure:doctests > /dev/null 2>&1; then
    echo "ERROR: The test target passed, but its expected output is wrong on purpose."
    exit 1
else
    echo "SUCCESS: Test target failed as expected."
fi

echo "=== Testing that the same document still builds as HTML ==="
if $BAZEL build //tests/doctest_failure:site > /dev/null 2>&1; then
    echo "SUCCESS: Site built despite the failing doctest."
else
    echo "ERROR: Site build failed. A broken example must not block publishing."
    exit 1
fi

exit 0
