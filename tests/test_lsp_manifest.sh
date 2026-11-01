#!/bin/bash
# test_lsp_manifest.sh
# Tests the `lsp_manifest` output group of rinx_site (ADR-038 §2): that it
# describes every library's parse settings as the build applies them, that
# each file it names exists once the group is built, and that building the
# site alone does not write it.

set -euo pipefail

# Resolve from PATH rather than hardcoding an install location.
BAZEL=${BAZEL:-$(command -v bazel)}

echo "=== Testing the site's default outputs hold no manifest ==="
if $BAZEL cquery --output=files //examples:site 2>/dev/null | grep -q 'lsp_manifest.json$'; then
    echo "ERROR: building //examples:site writes the language server's manifest."
    exit 1
fi
echo "SUCCESS: the manifest is not a default output."

echo "=== Testing the manifest describes each library ==="
$BAZEL build --output_groups=lsp_manifest //examples:site > /dev/null 2>&1
MANIFEST=$($BAZEL cquery --output=files --output_groups=lsp_manifest //examples:site 2>/dev/null | grep 'lsp_manifest.json$')
EXECROOT=$($BAZEL info execution_root 2>/dev/null)
python3 - "$MANIFEST" "$EXECROOT" <<'PY'
import json
import os
import sys

manifest_path, execroot = sys.argv[1], sys.argv[2]
with open(manifest_path) as f:
    manifest = json.load(f)
libraries = {library["label"]: library for library in manifest["libraries"]}

def check(condition, message):
    if not condition:
        print(f"ERROR: {message}")
        sys.exit(1)

check(manifest["version"] == 1, f"version is {manifest['version']}, not 1")
check(manifest["site"]["label"] == "//examples:site", f"site is {manifest['site']['label']}")
check(libraries["//examples/default_role:configured_docs"]["default_role"] == "any",
      "configured_docs' default_role is not 'any'")
check(libraries["//examples/team_a:docs"]["default_domain"] == "c", "team_a's domain is not 'c'")
check(libraries["//examples/rst_prolog:docs"]["rst_prolog"] == {"source": "examples/rst_prolog/prolog.rst"},
      "rst_prolog's prolog is not recorded")
check(libraries["//examples/ignored_roles:docs"]["parse_config"] == {"source": "examples/ignored_roles/rinx_parser.toml"},
      "ignored_roles' parse_config is not recorded")
check(libraries["//examples/entities:docs"]["entity_schema"] == {"source": "examples/entities/entities.toml"},
      "the entities library's schema is not recorded")
check(libraries["//examples/jinja:docs"]["jinja"], "the jinja library is not recorded as templated")
check("examples/shared/parameters.rst" in libraries["//examples:root_docs"]["parse_data"],
      "root_docs' parse_data is not recorded")
check(manifest["site"]["entity_schema"] == {"source": "examples/entities/entities.toml"},
      "the site's schema is not recorded")
check([inv["name"] for inv in manifest["inventories"]] == ["python"], "the inventories are not recorded")
check(list(libraries) == sorted(libraries), "the libraries are not sorted by label")

def files(value):
    if isinstance(value, dict) and set(value) <= {"source", "generated"} and len(value) == 1:
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from files(item)
    elif isinstance(value, list):
        for item in value:
            yield from files(item)

for file in files(manifest):
    path = file.get("source") or os.path.join(execroot, file["generated"])
    check(os.path.isfile(path), f"the manifest names {file}, which does not exist")
print("SUCCESS: the manifest describes each library, and every file it names exists.")
PY

echo "=== Testing a generated inventory is materialized with the manifest ==="
$BAZEL build --output_groups=lsp_manifest //examples/intersphinx:sibling_site > /dev/null 2>&1
SIBLING=$($BAZEL cquery --output=files --output_groups=lsp_manifest //examples/intersphinx:sibling_site 2>/dev/null | grep 'lsp_manifest.json$')
python3 - "$SIBLING" "$EXECROOT" <<'PY'
import json
import os
import sys

manifest = json.load(open(sys.argv[1]))
generated = [inv["file"]["generated"] for inv in manifest["inventories"] if "generated" in inv["file"]]
if not generated:
    print("ERROR: the sibling site's inventory of //examples:site is not recorded as generated.")
    sys.exit(1)
for path in generated:
    if not os.path.isfile(os.path.join(sys.argv[2], path)):
        print(f"ERROR: the generated inventory {path} was not built with the manifest.")
        sys.exit(1)
print("SUCCESS: a generated inventory is built with the manifest.")
PY

exit 0
