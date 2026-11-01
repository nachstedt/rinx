#!/usr/bin/env bash
# Smoke test of `rinx lsp` in Helix, configured by the languages.toml
# docs/editors.rst shows. Helix has no scripting interface, so this checks
# what can be seen from outside: that `hx --health` finds the server for
# reStructuredText, and that a real session — Helix in a detached tmux — gets
# the server's diagnostics and its quick fix, read from Helix's own log.
# Hover, definition and completion are checked by hand.
#
# Run by editors/run-tests.sh: smoke.sh <languages.toml> <workspace>
set -euo pipefail

languages=$1
workspace=$2
mkdir -p "${XDG_CONFIG_HOME:?}/helix"
cp "$languages" "$XDG_CONFIG_HOME/helix/languages.toml"

fail() {
  echo "FAILED: $1" >&2
  exit 1
}

# Coloured even into a pipe, so the escapes are stripped before matching.
health=$(hx --health rst | sed 's/\x1b\[[0-9;]*m//g')
echo "$health"
grep -q '✓ rinx' <<<"$health" || fail "hx --health finds no rinx for rst"
echo "ok: hx --health finds rinx for rst"

# A socket of its own, so the session neither joins nor disturbs a tmux server
# the user runs, and inherits this environment's PATH and XDG directories.
log=$(dirname "$workspace")/helix.log
socket=rinx-helix-$$
tmux -L "$socket" new-session -d -x 120 -y 40 -c "$workspace" "hx -vv --log '$log' index.rst"
trap 'tmux -L "$socket" kill-server 2>/dev/null || true' EXIT

deadline=$((SECONDS + 60))
until grep 'publishDiagnostics' "$log" 2>/dev/null | grep -q 'directive.unknown'; do
  if ((SECONDS >= deadline)); then
    cat "$log" >&2 || true
    fail "Helix shows no directive.unknown from rinx within 60 s"
  fi
  sleep 0.2
done
echo "ok: Helix receives the unknown directive's diagnostic"

# The code action menu on the directive's line asks for its quick fixes; the
# server's answer is in the log.
tmux -L "$socket" send-keys ':10' Enter
tmux -L "$socket" send-keys Space a
deadline=$((SECONDS + 20))
until grep -q 'Suppress directive.unknown with .. noqa:' "$log" 2>/dev/null; do
  if ((SECONDS >= deadline)); then
    cat "$log" >&2 || true
    fail "Helix is offered no noqa quick fix within 20 s"
  fi
  sleep 0.2
done
echo "ok: Helix is offered the noqa quick fix"
# Closes the menu, apart from the colon that follows so tmux does not join
# the two into Alt-colon.
tmux -L "$socket" send-keys Escape
sleep 0.5

# Back in normal mode. No Escape right before: tmux would join it and the
# colon into Alt-colon.
tmux -L "$socket" send-keys ':q!' Enter
deadline=$((SECONDS + 10))
while tmux -L "$socket" has-session 2>/dev/null; do
  ((SECONDS < deadline)) || fail "Helix did not quit"
  sleep 0.2
done
echo "ok: Helix quits"
