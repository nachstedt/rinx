#!/bin/bash
# test_dependency_quick_fix.sh
# Tests the language server's quick fix for a toctree entry outside `deps`
# (roadmap #21, ADR-041): with `team_b` removed from `//examples:root_docs`'
# deps, `rinx lsp --check` offers adding it back, the edit it carries restores
# the build file exactly, and the site builds again.

set -euo pipefail

# Resolve from PATH rather than hardcoding an install location.
BAZEL=${BAZEL:-$(command -v bazel)}

SCRATCH=$(mktemp -d)
cp examples/BUILD.bazel "$SCRATCH/BUILD.bazel.orig"
restore() {
    cp "$SCRATCH/BUILD.bazel.orig" examples/BUILD.bazel
    rm -rf "$SCRATCH"
}
trap restore EXIT

echo "=== Removing a dependency the root's toctree needs ==="
sed -i.tmp '/^        "\/\/examples\/team_b:docs",$/d' examples/BUILD.bazel
rm -f examples/BUILD.bazel.tmp
if cmp -s examples/BUILD.bazel "$SCRATCH/BUILD.bazel.orig"; then
    echo "ERROR: the dependency on team_b was not found to remove."
    exit 1
fi

# Two builds: `--output_groups` would replace the binary's default output too.
$BAZEL build //:rinx > /dev/null 2>&1
$BAZEL build --output_groups=lsp_manifest //examples:site > /dev/null 2>&1
RINX=$($BAZEL cquery --output=files //:rinx 2>/dev/null | grep -v -- "-exec/" | head -n 1)
MANIFEST=$($BAZEL cquery --output=files --output_groups=lsp_manifest //examples:site 2>/dev/null | grep 'lsp_manifest.json$')

echo "=== Applying the quick fix the language server offers ==="
"$RINX" lsp --check "$PWD" --manifest "$PWD/$MANIFEST" > "$SCRATCH/check.json"
python3 - "$SCRATCH/check.json" "$PWD/examples/BUILD.bazel" <<'PY'
import json
import sys
from urllib.parse import unquote, urlparse

check_path, build_file = sys.argv[1], sys.argv[2]
with open(check_path) as f:
    check = json.load(f)

wanted = "Add //examples/team_b:docs to the deps of //examples:root_docs"
fixes = [
    fix
    for fixes in check.get("quick_fixes", {}).values()
    for fix in fixes
    if fix["title"] == wanted
]
if not fixes:
    titles = [fix["title"] for fixes in check.get("quick_fixes", {}).values() for fix in fixes]
    print(f"ERROR: no quick fix titled {wanted!r}; offered: {titles}")
    sys.exit(1)

changes = fixes[0]["edit"]["changes"]
paths = {unquote(urlparse(uri).path): edits for uri, edits in changes.items()}
if list(paths) != [build_file]:
    print(f"ERROR: the fix edits {list(paths)}, not {build_file}")
    sys.exit(1)

with open(build_file) as f:
    text = f.read()
lines = text.split("\n")

def offset(position):
    # The build file is ASCII, so a UTF-16 column is a character index.
    return sum(len(line) + 1 for line in lines[: position["line"]]) + position["character"]

for edit in sorted(paths[build_file], key=lambda edit: offset(edit["range"]["start"]), reverse=True):
    start, end = offset(edit["range"]["start"]), offset(edit["range"]["end"])
    text = text[:start] + edit["newText"] + text[end:]
with open(build_file, "w") as f:
    f.write(text)
print("SUCCESS: the quick fix is offered and applied.")
PY

if cmp -s examples/BUILD.bazel "$SCRATCH/BUILD.bazel.orig"; then
    echo "SUCCESS: the fixed build file is the original."
else
    echo "ERROR: the fixed build file differs from the original:"
    diff "$SCRATCH/BUILD.bazel.orig" examples/BUILD.bazel || true
    exit 1
fi

echo "=== Building the site from the fixed build file ==="
if $BAZEL build //examples:site > /dev/null 2>&1; then
    echo "SUCCESS: the site builds again."
else
    echo "ERROR: the site does not build from the fixed build file."
    exit 1
fi
