#!/bin/bash
# test_domain_indices.sh
# Tests that `domain_indices` on rinx_site is load-bearing: with
# "py-modindex" the site writes py-modindex.html and every page links it; without
# it the page is not an output at all and no page links it; and a name this build
# does not write fails the build while analyzing (ADR-032).

set -euo pipefail

# Resolve from PATH rather than hardcoding an install location.
BAZEL=${BAZEL:-$(command -v bazel)}

# The site's declared outputs. Asked of Bazel rather than read from bazel-bin,
# where a file a previous build wrote outlives the output that declared it.
site_outputs() {
    $BAZEL cquery --output=files //examples:site 2>/dev/null
}

PAGE=bazel-bin/examples/site_site_out/examples/domains.html

echo "=== Testing an enabled module index is written and linked ==="
$BAZEL build //examples:site > /dev/null 2>&1
if ! site_outputs | grep -q '_site_out/py-modindex.html$'; then
    echo "ERROR: domain_indices = [\"py-modindex\"] but py-modindex.html is not an output."
    exit 1
fi
if ! grep -q 'py-modindex.html">Module Index</a>' "$PAGE"; then
    echo "ERROR: a page of a site writing the module index does not link it."
    exit 1
fi
echo "SUCCESS: py-modindex.html is written and linked."

cp examples/BUILD.bazel examples/BUILD.bazel.bak
restore() {
    mv examples/BUILD.bazel.bak examples/BUILD.bazel
}
trap restore EXIT

echo "=== Testing a site without domain_indices writes and links no module index ==="
sed -i.tmp '/domain_indices = \["py-modindex"\],/d' examples/BUILD.bazel
rm -f examples/BUILD.bazel.tmp
$BAZEL build //examples:site > /dev/null 2>&1
if site_outputs | grep -q 'py-modindex.html$'; then
    echo "ERROR: py-modindex.html is an output although domain_indices is unset."
    exit 1
fi
if grep -q 'py-modindex.html">Module Index</a>' "$PAGE"; then
    echo "ERROR: a page links a module index the site does not write."
    exit 1
fi
echo "SUCCESS: no module index is written or linked."

echo "=== Testing an unknown domain index fails the build ==="
cp examples/BUILD.bazel.bak examples/BUILD.bazel
sed -i.tmp 's/domain_indices = \["py-modindex"\],/domain_indices = ["c-modindex"],/' examples/BUILD.bazel
rm -f examples/BUILD.bazel.tmp
if OUTPUT=$($BAZEL build //examples:site 2>&1); then
    echo "ERROR: Build succeeded but should have FAILED on an unknown domain index."
    exit 1
fi
if ! grep -q "names 'c-modindex'; this build writes: py-modindex" <<< "$OUTPUT"; then
    echo "ERROR: the build failed, but not by naming the unknown domain index:"
    echo "$OUTPUT" | tail -5
    exit 1
fi
echo "SUCCESS: the unknown domain index is refused by name."

exit 0
