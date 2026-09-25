#!/bin/bash
# test_diagram_opt_in.sh
# Verifies the `diagrams` attribute on rinx_library is enforced.
#
# Diagram compilation is opt-in per library, so a project that draws nothing
# pays nothing for it: Bazel cannot know before reading a document whether it
# holds a diagram, so the library has to say. The opt-in is only honest if
# forgetting it fails loudly — otherwise the page would ship with an `<img>`
# pointing at an SVG no action ever compiled.
#
# This asserts both halves: without the attribute, a library holding a diagram
# fails to build, naming the attribute; with it, the build succeeds. It then
# asserts the same for a *generated* picture — a `.. entity-flow::` is compiled
# by the very same action, so it needs the very same opt-in, and reports it
# under its own `entity-flow.*` code — and likewise a `.. entity-sequence::`,
# under `entity-sequence.*`.

set -euo pipefail

BAZEL=${BAZEL:-bazel}
BUILD_FILE=examples/team_a/BUILD.bazel
TARGET=//examples:site

cp "$BUILD_FILE" "$BUILD_FILE.bak"
restore() { mv "$BUILD_FILE.bak" "$BUILD_FILE"; }
trap restore EXIT

echo "=== Testing that a library with diagrams builds when it opts in ==="
if $BAZEL build "$TARGET" > /dev/null 2>&1; then
    echo "SUCCESS: The opted-in library built."
else
    echo "ERROR: The site failed to build with team_a opted in to diagrams."
    exit 1
fi

echo "=== Testing that the same library fails without the opt-in ==="
# Removing the attribute changes the parse action's arguments, so Bazel re-runs
# the parse — no need to also touch the .rst, unlike the image tests.
sed -i '/^    diagrams = True,$/d' "$BUILD_FILE"
if grep -q "diagrams = True" "$BUILD_FILE"; then
    echo "ERROR: could not remove the attribute from $BUILD_FILE; the test is broken."
    exit 1
fi

if build_output=$($BAZEL build "$TARGET" 2>&1); then
    echo "ERROR: The build succeeded although team_a holds diagrams and no longer opts in."
    exit 1
fi
if grep -q "uml.diagrams-disabled" <<<"$build_output" \
    && grep -q "diagrams = True" <<<"$build_output"; then
    echo "SUCCESS: The build failed, naming the attribute to set."
else
    echo "ERROR: The build failed, but not with the opt-in error."
    echo "$build_output" | tail -20
    exit 1
fi

restore
trap - EXIT

ENTITY_BUILD_FILE=examples/entities/BUILD.bazel
cp "$ENTITY_BUILD_FILE" "$ENTITY_BUILD_FILE.bak"
restore_entities() { mv "$ENTITY_BUILD_FILE.bak" "$ENTITY_BUILD_FILE"; }
trap restore_entities EXIT

echo "=== Testing that a generated flowchart needs the same opt-in ==="
sed -i '/^    diagrams = True,$/d' "$ENTITY_BUILD_FILE"
if grep -q "diagrams = True" "$ENTITY_BUILD_FILE"; then
    echo "ERROR: could not remove the attribute from $ENTITY_BUILD_FILE; the test is broken."
    exit 1
fi

if build_output=$($BAZEL build "$TARGET" 2>&1); then
    echo "ERROR: The build succeeded although entities/diagrams.rst draws a flowchart."
    exit 1
fi
if grep -q "entity-flow.diagrams-disabled" <<<"$build_output" \
    && grep -q "diagrams = True" <<<"$build_output"; then
    echo "SUCCESS: A flowchart without the opt-in failed under its own code."
else
    echo "ERROR: The build failed, but not with the flowchart's opt-in error."
    echo "$build_output" | tail -20
    exit 1
fi

echo "=== Testing that a generated sequence diagram needs the same opt-in ==="
# The same failed build: entities/diagrams.rst draws both kinds of picture.
if grep -q "entity-sequence.diagrams-disabled" <<<"$build_output"; then
    echo "SUCCESS: A sequence diagram without the opt-in failed under its own code."
else
    echo "ERROR: The build failed, but not with the sequence diagram's opt-in error."
    echo "$build_output" | tail -20
    exit 1
fi

exit 0
