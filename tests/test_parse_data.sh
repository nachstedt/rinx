#!/bin/bash
# test_parse_data.sh
# Tests that every file the *parser* reads must be declared in the library's
# `parse_data` attribute — that the declaration is what puts the file in the
# parse action's sandbox, rather than the build merely finding it in the source
# tree.
#
# Four directives read files at parse time, and each is checked here:
#   - `.. csv-table::` with `:file:`   (examples/csv_table.rst)
#   - `.. include::`                   (examples/includes.rst)
#   - `.. literalinclude::`            (examples/includes.rst)
#   - `.. needimport::`                (examples/entities/imported.rst)
#
# The last one lives in a different package, which is why the BUILD file to
# edit is a parameter rather than a constant: `entities.toml` is a parse-time
# input too, so the entity documents have a library of their own.

set -euo pipefail

# Resolved from PATH rather than hardcoded, so the script runs wherever bazel
# is installed; override with BAZEL=... to pin a specific one.
BAZEL=${BAZEL:-bazel}

ROOT_BUILD=examples/BUILD.bazel
ENTITY_BUILD=examples/entities/BUILD.bazel
CSV_DOC=examples/csv_table.rst
INCLUDE_DOC=examples/includes.rst
IMPORT_DOC=examples/entities/imported.rst

# Every file this run has copied aside, so a failure at any point puts the
# working tree back exactly as it found it.
EDITED=("$ROOT_BUILD" "$ENTITY_BUILD" "$CSV_DOC" "$INCLUDE_DOC" "$IMPORT_DOC")

restore() {
    for f in "${EDITED[@]}"; do
        if [ -f "$f.bak" ]; then mv "$f.bak" "$f"; fi
        rm -f "$f.tmp"
    done
}
trap restore EXIT

# Bazel does not invalidate an action when an input is merely *removed* — the
# incremental check only verifies that the inputs still present are unchanged.
# Editing the document itself changes a digest Bazel does check, which forces
# the parse action to re-run so the missing file is actually observed.
force_reparse() {
    printf '\n.. This comment exists only to force a re-parse; see tests/test_parse_data.sh\n' >> "$1"
}

# Drops one `parse_data` entry from `build_file`, forces the document that
# reads it to re-parse, and asserts the build fails.
expect_failure_without() {
    local build_file="$1" entry="$2" doc="$3" label="$4"
    echo "=== Testing undeclared $label ==="
    cp "$build_file" "$build_file.bak"
    cp "$doc" "$doc.bak"

    # Removes the entry from whatever list holds it, whether the list is
    # written one item per line or inline — a single-element `parse_data` is
    # spelled `["needs.json"]`, with no trailing comma to anchor on. Comment
    # lines are skipped: they name these paths too, and leaving them in place
    # is what keeps the diff honest.
    sed -i.tmp -E "/^[[:space:]]*#/! s|\"$entry\",?[[:space:]]*||" "$build_file"
    # Compared against the copy rather than grepped for, since the surrounding
    # comments would match a grep and a test that removed nothing would then
    # prove nothing.
    if cmp -s "$build_file" "$build_file.bak"; then
        echo "ERROR: failed to remove '$entry' from $build_file; the test would not prove anything."
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

expect_failure_without "$ROOT_BUILD"   "data/fruits.csv"       "$CSV_DOC"     "csv-table :file:"
expect_failure_without "$ROOT_BUILD"   "shared/parameters.rst" "$INCLUDE_DOC" "include source"
expect_failure_without "$ROOT_BUILD"   "shared/greeter.py"     "$INCLUDE_DOC" "literalinclude source"
expect_failure_without "$ENTITY_BUILD" "needs.json"            "$IMPORT_DOC"  "needimport source"

echo "=== All parse_data cases behaved correctly ==="
exit 0
