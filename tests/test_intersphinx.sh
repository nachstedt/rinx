#!/bin/bash
# test_intersphinx.sh
# Tests that a site links into another site through the objects.inv that site
# writes, that the `inventories` declaration is load-bearing, and that a file
# which is not an inventory fails the build rather than silently resolving
# nothing.

set -euo pipefail

# Resolve from PATH rather than hardcoding an install location.
BAZEL=${BAZEL:-$(command -v bazel)}
BAZEL_BIN=$($BAZEL info bazel-bin 2>/dev/null)
SIBLING_PAGE="$BAZEL_BIN/examples/intersphinx/sibling_site_site_out/examples/intersphinx/sibling/index.html"

echo "=== Testing baseline builds succeed and write/read inventories ==="
if ! $BAZEL build //examples:site //examples/intersphinx:sibling_site > /dev/null 2>&1; then
    echo "ERROR: Baseline build failed unexpectedly."
    exit 1
fi
if [ ! -f "$BAZEL_BIN/examples/site_site_out/objects.inv" ]; then
    echo "ERROR: //examples:site wrote no objects.inv."
    exit 1
fi
if ! grep -q 'class="reference external" href="[./]*site_site_out/examples/index.html#home-index"' "$SIBLING_PAGE"; then
    echo "ERROR: the sibling site does not link into //examples:site through its inventory."
    exit 1
fi
echo "SUCCESS: objects.inv written, and the sibling site links through it."

cp examples/intersphinx/BUILD.bazel examples/intersphinx/BUILD.bazel.bak
restore() {
    mv examples/intersphinx/BUILD.bazel.bak examples/intersphinx/BUILD.bazel
}
trap restore EXIT

echo "=== Testing a missing inventory declaration breaks the links ==="
# Removing the declaration changes the index action's command line, so Bazel
# re-runs it and every render; the sibling site builds with strict_links.
sed -i.tmp '/inventories = \[":examples_site"\],/d' examples/intersphinx/BUILD.bazel
rm -f examples/intersphinx/BUILD.bazel.tmp
if OUTPUT=$($BAZEL build //examples/intersphinx:sibling_site 2>&1); then
    echo "ERROR: Build succeeded but should have FAILED: its references have nowhere to resolve."
    exit 1
fi
if ! grep -q 'link.broken-ref' <<< "$OUTPUT"; then
    echo "ERROR: Build failed, but not with a broken-link warning:"
    echo "$OUTPUT"
    exit 1
fi
echo "SUCCESS: Without the declaration the references are broken links."
cp examples/intersphinx/BUILD.bazel.bak examples/intersphinx/BUILD.bazel

echo "=== Testing a file that is no inventory fails the build ==="
sed -i.tmp 's|src = "python.inv",|src = "README.md",|' examples/intersphinx/BUILD.bazel
rm -f examples/intersphinx/BUILD.bazel.tmp
if OUTPUT=$($BAZEL build //examples:site 2>&1); then
    echo "ERROR: Build succeeded but should have FAILED on a malformed inventory."
    exit 1
fi
if ! grep -q 'not a Sphinx inventory' <<< "$OUTPUT"; then
    echo "ERROR: Build failed, but without naming the malformed inventory:"
    echo "$OUTPUT"
    exit 1
fi
echo "SUCCESS: A malformed inventory fails the index action by name."

exit 0
