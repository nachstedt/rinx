#!/bin/bash
# test_image_data.sh
# Tests that a picture shown by `.. image::`/`.. figure::` must be declared in
# the library's `images` attribute — that is, that the declaration is what gets
# the file bundled into the site and embedded into the pages that ask for it,
# rather than the build merely finding it in the source tree.
#
# Unlike `csv_data`, an image is not a parse-action input: parsing a document
# succeeds whether or not the picture exists. The failure comes from the site's
# `validate_images` action, and — for a `:loading: embed` image — from the
# per-document `embed_assets` action that cannot read what was never declared.

set -euo pipefail

# Resolved from PATH rather than hardcoded, so the script runs wherever bazel
# is installed; override with BAZEL=... to pin a specific one.
BAZEL=${BAZEL:-bazel}

BUILD_FILE=examples/BUILD.bazel
DOC_FILE=examples/images.rst

restore() {
    [ -f "$BUILD_FILE.bak" ] && mv "$BUILD_FILE.bak" "$BUILD_FILE"
    [ -f "$DOC_FILE.bak" ] && mv "$DOC_FILE.bak" "$DOC_FILE"
    rm -f "$BUILD_FILE.tmp" "$DOC_FILE.tmp"
}
trap restore EXIT

echo "=== Testing correctly declared images ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Valid build succeeded."
else
    echo "ERROR: Valid build failed unexpectedly."
    exit 1
fi

echo "=== Testing undeclared images ==="
cp "$BUILD_FILE" "$BUILD_FILE.bak"
cp "$DOC_FILE" "$DOC_FILE.bak"

# Drop the declaration. `examples/images.rst` and `examples/figures.rst` still
# show the file, so the build must fail with it missing from the sandbox.
sed -i.tmp 's/    images = \["data\/logo.svg"\],//g' "$BUILD_FILE"

# Bazel does not invalidate an action when an input is merely *removed* — the
# incremental check only verifies that the inputs still present are unchanged.
# Editing the document itself changes a digest Bazel does check, which forces
# the affected actions to re-run so the missing file is actually observed.
echo "" >> "$DOC_FILE"
echo ".. This comment exists only to force a re-parse; see tests/test_image_data.sh" >> "$DOC_FILE"

if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "ERROR: Build succeeded but it should have FAILED with data/logo.svg undeclared."
    exit 1
else
    echo "SUCCESS: Build successfully failed as expected for undeclared images."
fi

exit 0
