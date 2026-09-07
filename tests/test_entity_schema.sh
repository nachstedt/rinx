#!/bin/bash
# test_entity_schema.sh
# Tests that the `entity_schema` attribute is load-bearing on both rules.
#
# Two directions, because the schema reaches two different phases:
#
#   1. Removing it from `rusty_sphinx_library` must make `.. req::` an unknown
#      directive again — the parser recognises an entity only through the
#      schema. The example site sets `strict_links`, so the references to those
#      entities then fail the build outright rather than degrading quietly.
#   2. Removing it from `rusty_sphinx_site` while the library still declares it
#      must be *reported*, not silently accepted: the documents were parsed
#      against a vocabulary the index is not using, which is what
#      `entity.schema-mismatch` exists to catch.

set -euo pipefail

# Resolved from PATH rather than hardcoded, so the script runs wherever bazel
# is installed; override with BAZEL=... to pin a specific one.
BAZEL=${BAZEL:-bazel}

SITE_BUILD=examples/BUILD.bazel
LIB_BUILD=examples/entities/BUILD.bazel
DOC=examples/entities/requirements.rst

restore() {
    for f in "$SITE_BUILD" "$LIB_BUILD" "$DOC"; do
        if [ -f "$f.bak" ]; then mv "$f.bak" "$f"; fi
    done
    rm -f "$SITE_BUILD.tmp" "$LIB_BUILD.tmp" "$DOC.tmp"
}
trap restore EXIT

# Bazel does not invalidate an action when an input is merely *removed* — the
# incremental check only verifies that the inputs still present are unchanged.
# Editing the document itself changes a digest Bazel does check, which forces
# the parse action to re-run so the missing schema is actually observed.
force_reparse() {
    printf '\n.. This comment exists only to force a re-parse; see tests/test_entity_schema.sh\n' >> "$1"
}

echo "=== Baseline: the site builds and renders the entities ==="
"$BAZEL" build //examples:site > /dev/null 2>&1
RENDERED=$("$BAZEL" info bazel-bin)/examples/site_site_out/examples/entities/requirements.html
if ! grep -q 'class="entity entity-req"' "$RENDERED"; then
    echo "FAIL: baseline site does not render an entity"
    exit 1
fi
echo "PASS: entities render when the schema is declared"

echo
echo "=== 1. Dropping entity_schema from the library ==="
cp "$LIB_BUILD" "$LIB_BUILD.bak"
cp "$DOC" "$DOC.bak"
grep -v 'entity_schema = "entities.toml",' "$LIB_BUILD" > "$LIB_BUILD.tmp"
mv "$LIB_BUILD.tmp" "$LIB_BUILD"
force_reparse "$DOC"

# Without the vocabulary the entities are never defined, so every reference to
# one dangles — and the example site's `strict_links` turns that into a build
# failure rather than a quiet gap in the page.
OUTPUT=$("$BAZEL" build //examples:site 2>&1 || true)
if ! echo "$OUTPUT" | grep -q "entity.unknown-target"; then
    echo "FAIL: dropping the library's entity_schema did not break the entity references"
    echo "$OUTPUT" | tail -20
    exit 1
fi
echo "PASS: without the library's entity_schema, the entities are not recognised"

mv "$LIB_BUILD.bak" "$LIB_BUILD"
mv "$DOC.bak" "$DOC"

echo
echo "=== 2. Dropping entity_schema from the site ==="
cp "$SITE_BUILD" "$SITE_BUILD.bak"
grep -v 'entity_schema = "//examples/entities:entities.toml",' "$SITE_BUILD" > "$SITE_BUILD.tmp"
mv "$SITE_BUILD.tmp" "$SITE_BUILD"

# The index action re-runs because its own arguments changed, so no forced
# edit is needed here.
OUTPUT=$("$BAZEL" build //examples:site 2>&1 || true)
if ! echo "$OUTPUT" | grep -q "entity.schema-mismatch"; then
    echo "FAIL: the site indexed documents parsed against another schema without saying so"
    echo "$OUTPUT" | tail -20
    exit 1
fi
echo "PASS: a schema the site does not share is reported as entity.schema-mismatch"

mv "$SITE_BUILD.bak" "$SITE_BUILD"

echo
echo "=== Restoring and rebuilding ==="
"$BAZEL" build //examples:site > /dev/null 2>&1
echo "PASS: the site builds again"
echo
echo "All entity_schema tests passed."
