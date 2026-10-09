#!/usr/bin/env bash
# Runs one fuzz target for a number of seconds, with the seeds and options it
# needs, so CI's smoke run, the nightly run and a local run cannot differ:
#
#   ./run.sh parse 60
#   ./run.sh lsp_session 600
#   ./run.sh pyconf 60
#
# New inputs collect in corpus/<target>; a crashing one lands in
# artifacts/<target>/, and `cargo fuzz run <target> <file>` replays it.
set -euo pipefail
cd "$(dirname "$0")"

target=$1
seconds=$2
mkdir -p "corpus/$target"
# cargo-fuzz defaults to the triple it was itself built for, which for a
# prebuilt download (as CI installs) is musl, where the sanitizers do not work.
host=$(rustc -vV | sed -n 's/^host: //p')
case "$target" in
  # This repository's own documents as seeds, and markup tokens as a
  # dictionary, so the parser's constructs are reached rather than guessed.
  parse)
    exec cargo fuzz run --target "$host" parse "corpus/parse" ../examples ../docs -- \
      -dict=dictionaries/rst.dict -max_len=4096 -max_total_time="$seconds"
    ;;
  lsp_session)
    exec cargo fuzz run --target "$host" lsp_session "corpus/lsp_session" -- -max_total_time="$seconds"
    ;;
  # Real `conf.py` files as seeds, so the reader starts from the shapes
  # projects write.
  pyconf)
    exec cargo fuzz run --target "$host" pyconf "corpus/pyconf" ../crates/pyconf/testdata -- \
      -max_len=8192 -max_total_time="$seconds"
    ;;
  *)
    echo "unknown fuzz target '$target'" >&2
    exit 2
    ;;
esac
