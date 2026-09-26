#!/bin/bash
# test_jinja.sh
# Tests the two halves of the opt-in Jinja pass on a `rinx_library`:
#
#   1. A template an `{% include %}` names must be declared in `parse_data`,
#      exactly like every other file the parser reads. Undeclared, it is absent
#      from the sandbox and the build fails rather than shipping a page with
#      its shared header silently missing.
#   2. Without `jinja = True`, the same sources are parsed exactly as written —
#      the markup renders as the text it literally is, and nothing is read.
#      This is what keeps the feature from changing any other library.

set -euo pipefail

# Resolved from PATH rather than hardcoded, so the script runs wherever bazel
# is installed; override with BAZEL=... to pin a specific one.
BAZEL=${BAZEL:-bazel}

BUILD_FILE=examples/jinja/BUILD.bazel
DOC=examples/jinja/loops.rst
PAGE=bazel-bin/examples/site_site_out/examples/jinja/loops.html

restore() {
    for f in "$BUILD_FILE" "$DOC"; do
        if [ -f "$f.bak" ]; then mv "$f.bak" "$f"; fi
    done
    rm -f "$BUILD_FILE.tmp" "$DOC.tmp"
}
trap restore EXIT

# Bazel does not invalidate an action when an input is merely *removed* — the
# incremental check only verifies that the inputs still present are unchanged.
# Editing the document itself changes a digest Bazel does check, which forces
# the parse action to re-run so the missing file is actually observed.
force_reparse() {
    printf '\n.. This comment exists only to force a re-parse; see tests/test_jinja.sh\n' >> "$1"
}

echo "=== Testing a correctly declared template ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Valid build succeeded."
else
    echo "ERROR: Valid build failed unexpectedly."
    exit 1
fi

if ! grep -q "release 0.1" "$PAGE"; then
    echo "ERROR: the shared header was not rendered into $PAGE."
    exit 1
fi
echo "SUCCESS: the shared header reached the page."

echo "=== Testing an undeclared template ==="
cp "$BUILD_FILE" "$BUILD_FILE.bak"
cp "$DOC" "$DOC.bak"
sed -i.tmp '\|parse_data = \["_templates/page_header.rst"\],|d' "$BUILD_FILE"
if grep -q "parse_data = " "$BUILD_FILE"; then
    echo "ERROR: failed to remove parse_data from $BUILD_FILE; the test would not prove anything."
    exit 1
fi
force_reparse "$DOC"

if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "ERROR: Build succeeded but should have FAILED with the template undeclared."
    exit 1
fi
echo "SUCCESS: Build failed as expected for an undeclared template."
restore

echo "=== Testing a library that did not opt in ==="
cp "$BUILD_FILE" "$BUILD_FILE.bak"
cp "$DOC" "$DOC.bak"
sed -i.tmp '/jinja = True,/d' "$BUILD_FILE"
force_reparse "$DOC"

if ! $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "ERROR: Build failed; without the opt-in the markup is ordinary text and must still build."
    exit 1
fi
if ! grep -q "{% for name, purpose in" "$PAGE"; then
    echo "ERROR: without 'jinja = True' the markup should have rendered as the text it is."
    exit 1
fi
echo "SUCCESS: without the opt-in the sources are parsed exactly as written."
restore

# Leave the tree building again, so a later `bazel build` is not surprised.
$BAZEL build //examples:site > /dev/null 2>&1

echo "=== All Jinja cases behaved correctly ==="
exit 0
