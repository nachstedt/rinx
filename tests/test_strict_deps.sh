#!/bin/bash
# test_strict_deps.sh
# Tests that the Bazel build correctly enforces strict toctree dependencies.

set -euo pipefail

# Resolve from PATH rather than hardcoding an install location.
BAZEL=${BAZEL:-$(command -v bazel)}

echo "=== Testing correctly configured dependencies ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Valid build succeeded."
else
    echo "ERROR: Valid build failed unexpectedly."
    exit 1
fi

echo "=== Testing broken dependencies ==="
# We will temporarily tamper with the BUILD file to test the failure
cp examples/BUILD.bazel examples/BUILD.bazel.bak

sed -i.tmp 's/        "\/\/examples\/team_b:docs",//g' examples/BUILD.bazel
rm -f examples/BUILD.bazel.tmp

if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "ERROR: Build succeeded but it should have FAILED due to missing strict dependency on team_b."
    mv examples/BUILD.bazel.bak examples/BUILD.bazel
    exit 1
else
    echo "SUCCESS: Build successfully failed as expected for missing toctree dependency."
fi

# Restore the correct BUILD file
mv examples/BUILD.bazel.bak examples/BUILD.bazel

echo "=== Testing a :glob: that matches no declared dependency ==="
# A glob is expanded against the *declared* dependencies here, exactly as it is
# expanded against the whole project later, so a pattern reaching nothing
# declared still fails the build. Editing the source file (rather than removing
# an input) is what forces the parse and validation actions to re-run.
cp examples/toctree/index.rst examples/toctree/index.rst.bak

sed -i.tmp 's/   globbed_\*/   no_such_prefix_*/' examples/toctree/index.rst
rm -f examples/toctree/index.rst.tmp

if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "ERROR: Build succeeded but it should have FAILED: the glob matches no declared dependency."
    mv examples/toctree/index.rst.bak examples/toctree/index.rst
    exit 1
else
    echo "SUCCESS: Build successfully failed as expected for an unsatisfiable toctree glob."
fi

mv examples/toctree/index.rst.bak examples/toctree/index.rst
exit 0
