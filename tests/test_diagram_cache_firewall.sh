#!/bin/bash
# test_diagram_cache_firewall.sh
# The most important test of the diagram pipeline's move behind the index.
#
# Diagrams are expanded and compiled at the *site* level, because a templated
# one asks questions only the project index can answer. That puts the index in
# every expansion's inputs, so any edit anywhere in the project re-runs every
# expansion. What must NOT happen is a JVM starting for each of them: the
# expander is deterministic, so a document whose diagrams did not change
# produces byte-identical .puml files, leaving the PlantUMLCompile action key
# unchanged and its result cached.
#
# This asserts both halves: an edit elsewhere must NOT recompile a diagram, and
# an edit to the diagram's own source MUST.

set -euo pipefail

BAZEL=${BAZEL:-bazel}
# The document holding the diagrams, and one in a *different* library, so the
# "edit elsewhere" half is a genuine cross-document edit rather than a
# same-document one the .ast would absorb anyway.
DIAGRAM_DOC=examples/team_a/index.rst
OTHER_DOC=examples/team_b/index.rst
TARGET=//examples:site

cp "$DIAGRAM_DOC" "$DIAGRAM_DOC.bak"
cp "$OTHER_DOC" "$OTHER_DOC.bak"
restore() {
    mv "$DIAGRAM_DOC.bak" "$DIAGRAM_DOC"
    mv "$OTHER_DOC.bak" "$OTHER_DOC"
}
trap restore EXIT

# Warm the cache so the baseline is a known-built state.
$BAZEL build "$TARGET" > /dev/null 2>&1

# Builds and reports whether PlantUML ran, keeping a build failure
# distinguishable from a cache miss: under `pipefail` a failed build and a
# genuine miss would both make a piped `grep` non-zero, so the caller would
# blame the firewall for either.
#
# `-s` prints each executed action; an action served from the cache is not
# executed, so its absence is the signal.
plantuml_ran() {
    if ! build_output=$($BAZEL build -s "$TARGET" 2>&1); then
        echo "ERROR: 'bazel build $TARGET' failed. That says nothing about the"
        echo "       cache firewall — fix the failure below first."
        echo "$build_output" | tail -20
        exit 1
    fi
    grep -q "SUBCOMMAND.*PlantUMLCompile\|action 'Generating diagrams" <<<"$build_output"
}

echo "=== Testing that an edit in another document does NOT recompile diagrams ==="
# Unique per run: appending the *same* paragraph every time leaves the edited
# variant in the machine's cache, so every later run would pass whether or not
# the firewall works — a false pass hiding exactly the regression this catches.
printf '\nA paragraph unique to this run (%s), in a document with no diagrams.\n' \
    "$(date +%s%N)" >> "$OTHER_DOC"

if plantuml_ran; then
    echo "ERROR: PlantUML re-ran after an edit to an unrelated document."
    echo "       Every expansion re-runs by design — the index is an input —"
    echo "       but the .puml bytes must be identical, so nothing downstream"
    echo "       should have re-run. Something non-deterministic or"
    echo "       document-dependent has leaked into crates/uml's expander."
    exit 1
else
    echo "SUCCESS: The compiled diagrams were reused; no JVM started."
fi

echo "=== Testing that editing a diagram DOES recompile it ==="
restore
cp "$DIAGRAM_DOC" "$DIAGRAM_DOC.bak"
cp "$OTHER_DOC" "$OTHER_DOC.bak"
# `Alice -> Bob` appears in the first diagram; changing it changes the text
# that gets compiled, and so the hash the SVG is named by.
sed -i 's/Alice -> Bob: Auth Request/Alice -> Bob: Authentication Request/' "$DIAGRAM_DOC"

if plantuml_ran; then
    echo "SUCCESS: PlantUML re-ran after the diagram's source changed."
else
    echo "ERROR: The diagram was not recompiled even though its source changed."
    echo "       Its page now points at an SVG nothing produced."
    exit 1
fi

exit 0
