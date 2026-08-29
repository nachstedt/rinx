#!/bin/bash
# test_csv_data.sh
# Tests that a file read by `.. csv-table:: :file:` must be declared in the
# library's `csv_data` attribute — that is, that the declaration is what puts
# the file in the parse action's sandbox, rather than the build merely finding
# it in the source tree.

set -euo pipefail

# Resolved from PATH rather than hardcoded, so the script runs wherever bazel
# is installed; override with BAZEL=... to pin a specific one.
BAZEL=${BAZEL:-bazel}

BUILD_FILE=examples/BUILD.bazel
DOC_FILE=examples/csv_table.rst

restore() {
    [ -f "$BUILD_FILE.bak" ] && mv "$BUILD_FILE.bak" "$BUILD_FILE"
    [ -f "$DOC_FILE.bak" ] && mv "$DOC_FILE.bak" "$DOC_FILE"
    rm -f "$BUILD_FILE.tmp" "$DOC_FILE.tmp"
}
trap restore EXIT

echo "=== Testing correctly declared csv_data ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Valid build succeeded."
else
    echo "ERROR: Valid build failed unexpectedly."
    exit 1
fi

echo "=== Testing undeclared csv_data ==="
cp "$BUILD_FILE" "$BUILD_FILE.bak"
cp "$DOC_FILE" "$DOC_FILE.bak"

# Drop the declaration. `examples/csv_table.rst` still reads the file, so the
# parse action must fail with it missing from the sandbox.
sed -i.tmp 's/    csv_data = \["data\/fruits.csv"\],//g' "$BUILD_FILE"

# Bazel does not invalidate an action when an input is merely *removed* — the
# incremental check only verifies that the inputs still present are unchanged.
# Editing the document itself changes a digest Bazel does check, which forces
# the parse action to re-run so the missing file is actually observed.
echo "" >> "$DOC_FILE"
echo ".. This comment exists only to force a re-parse; see tests/test_csv_data.sh" >> "$DOC_FILE"

if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "ERROR: Build succeeded but it should have FAILED with data/fruits.csv undeclared."
    exit 1
else
    echo "SUCCESS: Build successfully failed as expected for undeclared csv_data."
fi

exit 0
