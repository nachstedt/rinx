#!/usr/bin/env bash
#
# Prepares a release: checks the tag agrees with every version the repository
# declares, writes the source archive the Bazel Central Registry entry points
# at, and prints the release notes on stdout.
#
# Called by bazel-contrib's release_ruleset workflow (see release.yaml), which
# fixes this path. Runnable locally too: `.github/workflows/release_prep.sh v0.1.0`.

set -o errexit -o nounset -o pipefail

TAG=$1
VERSION=${TAG#v}

fail() {
  echo "release_prep: $*" >&2
  exit 1
}

[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "tag '$TAG' is not of the form vX.Y.Z"

# Everything below is irreversible once published, so a version that disagrees
# anywhere stops the release here, before any of it happens.
module_version=$(sed -n 's/^    version = "\(.*\)",$/\1/p' MODULE.bazel | head -1)
[[ "$module_version" == "$VERSION" ]] ||
  fail "MODULE.bazel declares version '$module_version', the tag says '$VERSION'"

cargo_version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml)
[[ "$cargo_version" == "$VERSION" ]] ||
  fail "Cargo.toml's [workspace.package] declares version '$cargo_version', the tag says '$VERSION'"

stale=$(sed -n '/^\[workspace.dependencies\]/,/^\[/p' Cargo.toml |
  grep 'version = ' | grep -v "version = \"$VERSION\"" || true)
[[ -z "$stale" ]] ||
  fail "Cargo.toml's [workspace.dependencies] still name another version:"$'\n'"$stale"

# The release notes are the changelog's section for this version, which must
# have been dated — a section still reading "Unreleased" was not finished.
notes=$(awk -v v="$VERSION" '
  $0 ~ "^## \\[" v "\\]" { found = 1; heading = $0; next }
  found && /^## \[/ { exit }
  # The link definitions closing the file belong to no version.
  found && /^\[[^]]+\]: / { exit }
  found { print }
  END { if (!found) exit 1; if (heading ~ /Unreleased/) exit 2 }
' CHANGELOG.md) || fail "CHANGELOG.md has no dated '## [$VERSION] - YYYY-MM-DD' section"

# The prefix matches what GitHub puts in its own source archives, so switching
# between the two never changes a user's strip_prefix.
PREFIX="rinx-${VERSION}"
ARCHIVE="rinx-${TAG}.tar.gz"
git archive --format=tar --prefix="${PREFIX}/" "${TAG}" | gzip -n > "$ARCHIVE"

cat <<NOTES
${notes}

## Bazel

Add to your \`MODULE.bazel\`:

\`\`\`starlark
bazel_dep(name = "rinx", version = "${VERSION}")
\`\`\`
NOTES
