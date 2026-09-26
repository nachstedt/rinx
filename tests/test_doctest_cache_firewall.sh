#!/bin/bash
# test_doctest_cache_firewall.sh
# The most important test of the doctest feature.
#
# Doctest execution is cached on the extracted `.doctests.json` plan, which is a
# projection of the AST keeping only what changes how the code runs. Editing
# prose therefore re-runs the (cheap) extraction but leaves its bytes identical,
# so the test action's key is unchanged and no interpreter starts.
#
# This asserts both halves: a prose edit must NOT re-run the test, and a change
# to the test code MUST.

set -euo pipefail

BAZEL=${BAZEL:-bazel}
DOC=examples/doctests.rst
TARGET=//examples:doctest_docs_doctests

cp "$DOC" "$DOC.bak"
restore() { mv "$DOC.bak" "$DOC"; }
trap restore EXIT

# Warm the cache so the baseline is a known-built state.
$BAZEL test "$TARGET" > /dev/null 2>&1

# Runs `bazel test` and reports whether the test action had to run, keeping a
# build or test failure distinguishable from a cache miss: under `pipefail` a
# failed `bazel test` and a genuine miss would both make a piped `grep` non-zero,
# so the caller would blame the plan for either.
#
# `-s` prints every action whose key changed. An unchanged one is skipped by the
# local action cache and is not printed; one whose result a disk or remote cache
# supplies still is. That is the question a firewall asks — did the key change —
# which `(cached)` cannot answer: Bazel prints it for a result from *any* cache,
# so it depends on what an earlier run left behind (CI restores a disk cache).
doctests_ran() {
    if ! test_output=$($BAZEL test -s "$TARGET" 2>&1); then
        echo "ERROR: 'bazel test $TARGET' failed. That says nothing about the"
        echo "       cache firewall — fix the failure below first."
        echo "$test_output" | tail -20
        exit 1
    fi
    grep -q "SUBCOMMAND.*action 'Testing $TARGET'" <<<"$test_output"
}

echo "=== Testing that a prose edit does NOT re-run the doctests ==="
printf '\nAn added paragraph, changing no test code.\n' >> "$DOC"

if doctests_ran; then
    echo "ERROR: The test re-ran after a prose-only edit."
    echo "       The plan is no longer a pure projection of the test code —"
    echo "       something presentational has leaked into doctest_plan.rs."
    echo "$test_output" | tail -20
    exit 1
else
    echo "SUCCESS: The test action's key was unchanged; no interpreter ran."
fi

echo "=== Testing that editing the test code DOES re-run the doctests ==="
restore
cp "$DOC" "$DOC.bak"
# `1 + 1` appears in the first doctest block; changing it changes the plan.
sed -i 's/>>> 1 + 1$/>>> 1 + 1 + 0/' "$DOC"

if doctests_ran; then
    echo "SUCCESS: Test re-ran after a code edit."
else
    echo "ERROR: The test was not re-run even though its code changed."
    exit 1
fi

exit 0
