#!/bin/bash
# test_strict_links.sh
# Tests that broken cross-references are reported as warnings by default and
# only fail the build when a site opts into strict_links = True.

set -euo pipefail

# Resolve from PATH rather than hardcoding an install location.
BAZEL=${BAZEL:-$(command -v bazel)}

echo "=== Testing baseline build succeeds ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Baseline build succeeded."
else
    echo "ERROR: Baseline build failed unexpectedly."
    exit 1
fi

# Temporarily introduce a broken :ref: into an example doc and back up the
# files we're about to mutate, mirroring test_strict_deps.sh.
cp examples/comments.rst examples/comments.rst.bak
cp examples/BUILD.bazel examples/BUILD.bazel.bak

restore() {
    mv examples/comments.rst.bak examples/comments.rst
    mv examples/BUILD.bazel.bak examples/BUILD.bazel
}
trap restore EXIT

echo ':ref:`this-target-does-not-exist`' >> examples/comments.rst

# examples/BUILD.bazel already sets strict_links = True on //examples:site.
echo "=== Testing strict_links = True fails the build on a broken link ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "ERROR: Build succeeded but it should have FAILED due to strict_links + a broken reference."
    exit 1
else
    echo "SUCCESS: Build successfully failed as expected under strict_links."
fi

echo "=== Testing non-strict build warns but still succeeds ==="
sed -i.tmp '/strict_links = True,/d' examples/BUILD.bazel
rm -f examples/BUILD.bazel.tmp

if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Build with a broken link succeeded (warning-only) as expected."
else
    echo "ERROR: Build failed but should only have warned (strict_links disabled)."
    exit 1
fi

exit 0
