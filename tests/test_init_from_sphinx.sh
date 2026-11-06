#!/bin/bash
# test_init_from_sphinx.sh
# Tests `rinx init --from-sphinx` (roadmap #37): it migrates the Sphinx
# project in tests/sphinx_project — includes, an image, a download, a CSV and
# a literalinclude, `rst_prolog`, `exclude_patterns` and sphinx-needs — into a
# Bazel package that builds with no edit, publishes the documents Sphinx
# would and only those, and refuses to run a second time over its own files.
#
# The copy is migrated inside this repository, whose MODULE.bazel is rinx's
# own: init leaves it untouched, and the build shares the repository's cache.

set -euo pipefail

# Resolve from PATH rather than hardcoding an install location.
BAZEL=${BAZEL:-$(command -v bazel)}

PACKAGE=tests/sphinx_project_migrated
rm -rf "$PACKAGE"
cp -r tests/sphinx_project "$PACKAGE"
cleanup() {
    rm -rf "$PACKAGE"
}
trap cleanup EXIT

$BAZEL build //:rinx > /dev/null 2>&1
RINX=$($BAZEL cquery --output=files //:rinx 2>/dev/null | grep -v -- "-exec/" | head -n 1)
MODULE_BEFORE=$(cat MODULE.bazel)

echo "=== Migrating the Sphinx project ==="
REPORT=$("$RINX" init --from-sphinx "$PACKAGE")
for file in BUILD.bazel rinx.toml entities.toml rst_prolog.txt; do
    if [ ! -f "$PACKAGE/$file" ]; then
        echo "ERROR: init wrote no $file:"
        echo "$REPORT"
        exit 1
    fi
done
if [ "$(cat MODULE.bazel)" != "$MODULE_BEFORE" ]; then
    echo "ERROR: init changed rinx's own MODULE.bazel."
    exit 1
fi
if ! grep -q 'root_doc = "tests/sphinx_project_migrated/index"' "$PACKAGE/rinx.toml"; then
    echo "ERROR: rinx.toml does not name the root document from the workspace's root:"
    cat "$PACKAGE/rinx.toml"
    exit 1
fi
echo "SUCCESS: init wrote the build, and left the enclosing module alone."

echo "=== Building the migrated project ==="
if ! OUTPUT=$($BAZEL build "//$PACKAGE:site" 2>&1); then
    echo "ERROR: the migrated project does not build:"
    echo "$OUTPUT"
    exit 1
fi
SITE="$($BAZEL info bazel-bin 2>/dev/null)/$PACKAGE/site_site_out/$PACKAGE"
for page in index.html guide.html; do
    if [ ! -f "$SITE/$page" ]; then
        echo "ERROR: the site has no $page."
        exit 1
    fi
done
for page in drafts/old.html includes/snippet.html; do
    if [ -f "$SITE/$page" ]; then
        echo "ERROR: the site publishes $page, which Sphinx excludes."
        exit 1
    fi
done
if ! grep -q 'Sphinx Fixture is migrated' "$SITE/index.html"; then
    echo "ERROR: the prolog's substitution did not reach the page."
    exit 1
fi
if ! grep -q 'Spliced in by' "$SITE/index.html"; then
    echo "ERROR: the included fragment did not reach the page."
    exit 1
fi
echo "SUCCESS: the migrated project builds, with exactly Sphinx's documents."

echo "=== Migrating again ==="
if OUTPUT=$("$RINX" init --from-sphinx "$PACKAGE" 2>&1); then
    echo "ERROR: a second init succeeded over the files the first one wrote."
    exit 1
fi
if ! grep -q 'refusing to overwrite' <<< "$OUTPUT"; then
    echo "ERROR: a second init failed, but not by refusing to overwrite:"
    echo "$OUTPUT"
    exit 1
fi
echo "SUCCESS: init refuses to overwrite what it wrote."
