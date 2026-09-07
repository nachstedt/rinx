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
TARGET=//examples:doctest_docs_doctests

cp "$DOC" "$DOC.bak"
restore() { mv "$DOC.bak" "$DOC"; }
trap restore EXIT

# Warm the cache so the baseline is a known-cached state.
$BAZEL test "$TARGET" > /dev/null 2>&1

# Runs `bazel test` and reports whether the result came from the cache, keeping
# a build or test failure distinguishable from a cache miss. Piping straight
# into `grep -q` cannot: under `pipefail` a failing `bazel test` and a genuine
# cache miss both make the pipeline non-zero, so the caller would blame the
# plan for either.
bazel_test_was_cached() {
    if ! test_output=$($BAZEL test "$TARGET" 2>&1); then
        echo "ERROR: 'bazel test $TARGET' failed. That says nothing about the"
        echo "       cache firewall — fix the failure below first."
        echo "$test_output" | tail -20
        exit 1
    fi
    grep -q "(cached)" <<<"$test_output"
}

echo "=== Testing that a prose edit does NOT re-run the doctests ==="
# Unique per run. Appending the *same* paragraph every time leaves the edited
# variant in the machine's cache, so every later run finds a cached result and
# passes whether or not the firewall works — a false pass that hides exactly
# the regression this test exists to catch.
printf '\nAn added paragraph unique to this run (%s), changing no test code.\n' \
    "$(date +%s%N)" >> "$DOC"

if bazel_test_was_cached; then
    echo "SUCCESS: Test result was reused; no interpreter ran."
else
    echo "ERROR: The test re-ran after a prose-only edit."
    echo "       The plan is no longer a pure projection of the test code —"
    echo "       something presentational has leaked into doctest_plan.rs."
    echo "$test_output" | tail -20
    exit 1
fi

echo "=== Testing that editing the test code DOES re-run the doctests ==="
restore
cp "$DOC" "$DOC.bak"
# `1 + 1` appears in the first doctest block; changing it changes the plan.
sed -i 's/>>> 1 + 1$/>>> 1 + 1 + 0/' "$DOC"

if bazel_test_was_cached; then
    echo "ERROR: The test was cached even though its code changed."
    exit 1
else
    echo "SUCCESS: Test re-ran after a code edit."
fi

exit 0
