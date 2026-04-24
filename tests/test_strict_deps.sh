#!/bin/bash
# test_strict_deps.sh
# Tests that the Bazel build correctly enforces strict toctree dependencies.

set -euo pipefail

BAZEL=${BAZEL:-/usr/local/bin/bazel}

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
exit 0
