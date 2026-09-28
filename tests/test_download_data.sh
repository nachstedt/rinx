#!/bin/bash
# test_download_data.sh
# Tests that a file linked with `:download:` must be declared in the library's
# `downloads` attribute — that is, that the declaration is what gets the file
# copied into the site's `_downloads/` directory, rather than the build merely
# finding it in the source tree.
#
# Like an image, a download is not a parse-action input, and unlike one it is
# not a render input either: nothing reads it but the copy. The failure comes
# from the site's `validate_assets` action, reported as `download.undeclared`.

set -euo pipefail

# Resolved from PATH rather than hardcoded, so the script runs wherever bazel
# is installed; override with BAZEL=... to pin a specific one.
BAZEL=${BAZEL:-bazel}

BUILD_FILE=examples/BUILD.bazel

restore() {
    [ -f "$BUILD_FILE.bak" ] && mv "$BUILD_FILE.bak" "$BUILD_FILE"
    rm -f "$BUILD_FILE.tmp"
}
trap restore EXIT

echo "=== Testing correctly declared downloads ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: Valid build succeeded."
else
    echo "ERROR: Valid build failed unexpectedly."
    exit 1
fi

BUNDLED="$($BAZEL info bazel-bin 2>/dev/null)/examples/site_site_out/_downloads/examples/data/fruits.csv"
if [ -f "$BUNDLED" ]; then
    echo "SUCCESS: The file landed at its source-root-relative path under _downloads/."
else
    echo "ERROR: Expected $BUNDLED to exist."
    exit 1
fi

echo "=== Testing undeclared downloads ==="
cp "$BUILD_FILE" "$BUILD_FILE.bak"

# Drop the whole declaration. `examples/downloads.rst` still links the files,
# so the build must fail with them missing from the bundle. No document edit
# is needed to force a re-run, unlike tests/test_image_data.sh: the
# validation action loses its `_downloads/` input, which changes its key.
sed -i.tmp '/^    downloads = \[/,/^    \],/d' "$BUILD_FILE"
if grep -q "downloads = \[" "$BUILD_FILE"; then
    echo "ERROR: Failed to remove the downloads attribute from $BUILD_FILE."
    exit 1
fi

if OUTPUT=$($BAZEL build //examples:site 2>&1); then
    echo "ERROR: Build succeeded but it should have FAILED with the downloads undeclared."
    exit 1
elif echo "$OUTPUT" | grep -q "download.undeclared"; then
    echo "SUCCESS: Build failed as expected, reporting download.undeclared."
else
    echo "ERROR: Build failed, but not with download.undeclared:"
    echo "$OUTPUT" | tail -20
    exit 1
fi

exit 0
