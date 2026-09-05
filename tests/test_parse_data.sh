#!/bin/bash
# test_parse_data.sh
# Tests that every file the *parser* reads must be declared in the library's
# `parse_data` attribute — that the declaration is what puts the file in the
# parse action's sandbox, rather than the build merely finding it in the source
# tree.
#
# Three directives read files at parse time, and each is checked here:
#   - `.. csv-table::` with `:file:`   (examples/csv_table.rst)
#   - `.. include::`                   (examples/includes.rst)
#   - `.. literalinclude::`            (examples/includes.rst)

set -euo pipefail

# Resolved from PATH rather than hardcoded, so the script runs wherever bazel
# is installed; override with BAZEL=... to pin a specific one.
BAZEL=${BAZEL:-bazel}

BUILD_FILE=examples/BUILD.bazel
CSV_DOC=examples/csv_table.rst
INCLUDE_DOC=examples/includes.rst

restore() {
    for f in "$BUILD_FILE" "$CSV_DOC" "$INCLUDE_DOC"; do
        if [ -f "$f.bak" ]; then mv "$f.bak" "$f"; fi
    done
    rm -f "$BUILD_FILE.tmp" "$CSV_DOC.tmp" "$INCLUDE_DOC.tmp"
}
trap restore EXIT

# Bazel does not invalidate an action when an input is merely *removed* — the
# incremental check only verifies that the inputs still present are unchanged.
# Editing the document itself changes a digest Bazel does check, which forces
# the parse action to re-run so the missing file is actually observed.
force_reparse() {
    printf '\n.. This comment exists only to force a re-parse; see tests/test_parse_data.sh\n' >> "$1"
}

# Drops one `parse_data` entry, forces the document that reads it to re-parse,
# and asserts the build fails.
expect_failure_without() {
    local entry="$1" doc="$2" label="$3"
    echo "=== Testing undeclared $label ==="
    cp "$BUILD_FILE" "$BUILD_FILE.bak"
    cp "$doc" "$doc.bak"

    sed -i.tmp "\|\"$entry\",|d" "$BUILD_FILE"
    # Only the list entry matters; the surrounding comments name these paths
    # too, and leaving those in place is what keeps the diff honest.
    if grep -q "\"$entry\"," "$BUILD_FILE"; then
        echo "ERROR: failed to remove '$entry' from $BUILD_FILE; the test would not prove anything."
        exit 1
    fi
    force_reparse "$doc"

    if $BAZEL build //examples:site > /dev/null 2>&1; then
        echo "ERROR: Build succeeded but should have FAILED with $entry undeclared."
        exit 1
    fi
    echo "SUCCESS: Build failed as expected for undeclared $label."

    restore
}

echo "=== Testing correctly declared parse_data ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Valid build succeeded."
else
    echo "ERROR: Valid build failed unexpectedly."
    exit 1
fi

expect_failure_without "data/fruits.csv"          "$CSV_DOC"     "csv-table :file:"
expect_failure_without "shared/parameters.rst"    "$INCLUDE_DOC" "include source"
expect_failure_without "shared/greeter.py"        "$INCLUDE_DOC" "literalinclude source"

echo "=== All parse_data cases behaved correctly ==="
exit 0
