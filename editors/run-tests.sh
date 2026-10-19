#!/usr/bin/env bash
# Runs one editor's language-server test: the editor, configured by the
# snippet docs/editors.rst shows, against a rinx binary over a copy of
# editors/test-fixtures/workspace — the same command the `Editors` CI jobs run.
#
#   editors/run-tests.sh neovim|emacs|helix
#
# RINX_BINARY names the binary (default: target/debug/rinx). The snippets start
# `rinx` from the PATH, so its directory is put first on the PATH rather than
# the snippet being edited: what is tested is what is published.
set -euo pipefail

editor=${1:?usage: editors/run-tests.sh neovim|emacs|helix}
here=$(cd "$(dirname "$0")" && pwd)
binary=$(realpath "${RINX_BINARY:-$here/../target/debug/rinx}")
[[ $(basename "$binary") == rinx ]] || { echo "RINX_BINARY must be named rinx: $binary" >&2; exit 2; }

# Outside the repository, so no editor takes the repository's root for the
# workspace's: the fixture's conf.py is its root marker. The editors' own
# configuration, state and logs go beside it rather than into the user's.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
workspace=$scratch/workspace
mkdir "$workspace"
cp -R "$here/test-fixtures/workspace/." "$workspace"
for dir in config data state cache; do mkdir "$scratch/$dir"; done
export XDG_CONFIG_HOME=$scratch/config XDG_DATA_HOME=$scratch/data
export XDG_STATE_HOME=$scratch/state XDG_CACHE_HOME=$scratch/cache
PATH="$(dirname "$binary"):$PATH"
export PATH

case $editor in
  neovim)
    nvim --headless --clean -l "$here/neovim/tests/lsp_test.lua" "$here/neovim/rinx.lua" "$workspace"
    ;;
  emacs)
    RINX_TEST_WORKSPACE=$workspace emacs --batch -Q \
      -l "$here/emacs/rinx-eglot.el" \
      -l "$here/emacs/tests/rinx-eglot-test.el" \
      -f ert-run-tests-batch-and-exit
    ;;
  helix)
    "$here/helix/tests/smoke.sh" "$here/helix/languages.toml" "$workspace"
    ;;
  *)
    echo "unknown editor '$editor'; expected neovim, emacs or helix" >&2
    exit 2
    ;;
esac
