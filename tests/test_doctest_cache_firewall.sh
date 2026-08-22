#!/bin/bash
# test_doctest_cache_firewall.sh
# The most important test of the doctest feature.
#
# Doctest execution is cached on the extracted `.doctests.json` plan, which is a
# projection of the AST keeping only what changes how the code runs. Editing
# prose therefore re-runs the (cheap) extraction but leaves its bytes identical,
# so Bazel's test-result cache stops the cascade and no interpreter starts.
#
# This asserts both halves: a prose edit must NOT re-run the test, and a change
# to the test code MUST.

set -euo pipefail

BAZEL=${BAZEL:-bazel}
DOC=examples/doctests.rst
TARGET=//examples:root_docs_doctests_doctests

cp "$DOC" "$DOC.bak"
restore() { mv "$DOC.bak" "$DOC"; }
trap restore EXIT

# Warm the cache so the baseline is a known-cached state.
$BAZEL test "$TARGET" > /dev/null 2>&1

echo "=== Testing that a prose edit does NOT re-run the doctests ==="
printf '\nAn added paragraph that changes the document but no test code.\n' >> "$DOC"

if $BAZEL test "$TARGET" 2>&1 | grep -q "(cached)"; then
    echo "SUCCESS: Test result was reused; no interpreter ran."
else
    echo "ERROR: The test re-ran after a prose-only edit."
    echo "       The plan is no longer a pure projection of the test code —"
    echo "       something presentational has leaked into doctest_plan.rs."
    exit 1
fi

echo "=== Testing that editing the test code DOES re-run the doctests ==="
restore
cp "$DOC" "$DOC.bak"
# `1 + 1` appears in the first doctest block; changing it changes the plan.
sed -i 's/>>> 1 + 1$/>>> 1 + 1 + 0/' "$DOC"

if $BAZEL test "$TARGET" 2>&1 | grep -q "(cached)"; then
    echo "ERROR: The test was cached even though its code changed."
    exit 1
else
    echo "SUCCESS: Test re-ran after a code edit."
fi

exit 0
