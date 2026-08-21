# Benchmarking Rusty-Sphinx

This document explains how to benchmark the `rusty-sphinx` documentation generator against a large real-world Sphinx project.

We use the **CPython Documentation** as our primary benchmark target because of its size, complexity, and widespread use of various Sphinx extensions and syntax.

## Running the Benchmark

The benchmark is managed by a Python script integrated into the Bazel workspace.

You can execute it by running:

```bash
bazel run //scripts:benchmark
```

### What happens under the hood?

1. **Cloning**: The script automatically performs a shallow clone of the CPython repository into a temporary directory (`rusty_sphinx_benchmark_cpython` under the system temp dir).
2. **Bazel Project Generation**: A `BUILD.bazel` file is generated on the fly inside the `Doc/` directory of the clone, utilizing a `glob(["**/*.rst"])` statement to automatically capture all reStructuredText files into a single `rusty_sphinx_library` target. It also generates a `rusty_sphinx_site` target to assemble the HTML.
3. **Execution**: The script runs `bazel build //Doc:site`. This triggers `rusty-sphinx` to parse, validate, and render every `.rst` file into HTML in parallel. The build currently succeeds outright against CPython's docs — toctree validation passes and HTML is produced for every page.
4. **Analysis**: Once the build completes, the script traverses the generated Abstract Syntax Tree (`.ast`) JSON files located in `bazel-bin/`. It tallies up `Directive::Unknown` nodes (directives the parser doesn't recognize), `Toctree.ignored_options` (recognized toctree options the parser doesn't yet act on, e.g. `:caption:`), and per-document parser diagnostics, and prints each as a frequency map.

## Interpreting Results

The full analysis is far too long for a terminal, so the script writes it to **`benchmark_result.txt`** in the workspace root and prints only a compact **Benchmark Summary** (distinct/occurrence counts per category) to the screen, ending with a pointer to that file. Everything described below — the frequency tables and the domain-object listings — lives in `benchmark_result.txt`; the terminal shows just the counts. (`benchmark_result.txt` is git-ignored.)

### 1. Rendering Time
You will see a line like:
```
Bazel build succeeded in X.XX seconds.
```
This is the raw time taken by Bazel to execute the `rusty-sphinx` pipeline across the entire CPython documentation suite. Since Bazel runs these in parallel, this highlights the concurrency benefits of our architecture.

The script runs the inner `bazel build //Doc:site` with its output captured (so it can time and parse it), which means that build log is *not* streamed to your terminal. It is instead written to `bazel_build.log` in the generated workspace (`$TMPDIR/rusty_sphinx_benchmark_cpython/bazel_build.log`), on both success and failure — inspect it there to see exactly what the inner build printed. The Bazel timing profile is written alongside it as `profile.json.gz` (drop it into https://ui.perfetto.dev/ or `chrome://tracing`).

### 2. Unsupported Directives Summary
A sorted list of Sphinx directives that `rusty-sphinx` encountered but doesn't yet recognize (surfaced as `Directive::Unknown` nodes in the AST).

```
Unsupported Directives Summary:
-------------------------------
doctest: 472
availability: 394
option: 356
audit-event: 191
...
```

This list serves as a prioritized roadmap for feature implementation. Implementing the most frequent missing directives will rapidly increase our compatibility with real-world Sphinx codebases.

### 3. Ignored Toctree Options / Parser Diagnostics Summaries
Two smaller summaries follow: options seen on `.. toctree::` directives that are recognized but not yet acted upon (e.g. `:caption:`, `:numbered:`, `:hidden:`), and aggregated parser diagnostics (e.g. malformed grid tables) emitted per document. Both are minor compared to the unsupported-directives list, but flag smaller gaps worth closing.

### 4. Domain-Object Warnings vs. the Whitelist
The render step reports **domain-object** cross-references (`:func:`, `:py:class:`, `:c:type:`, …) that either fail to resolve or resolve only via an object-type alias fallback. Against a large corpus like CPython these number in the hundreds, and most are *expected* — references to stdlib/C-API symbols this doc set doesn't index. To separate those from genuine regressions, the benchmark diffs them against a checked-in whitelist.

**How it's wired:** each `render` action writes a machine-readable sidecar (`--warnings-output`, one `*.warnings.json` per document) into `bazel-bin/Doc/site_warnings/`. These are a non-default output group (`domain_warnings`) — they never land in the published site bundle, but they're produced on every build. `scripts/benchmark.py` globs them and compares against the whitelist.

**The whitelist** lives at `scripts/domain_warnings_whitelist.json` and is **hand-authored** — there is no record/regenerate mode. Its shape:

```json
{
  "entries": [
    {
      "doc_path": "Doc/library/xmlrpc.client",
      "kind": "object_type_mismatch",
      "target": "Fault",
      "comment": "Referenced as :exc: but defined as a class upstream; expected."
    }
  ]
}
```

The two `kind` values distinguish the two failure modes:
- `domain_object_reference` — the reference **didn't resolve at all**. Its `requested_type` records the object type the role asked for (the "missed type"), e.g. `py:class`.
- `object_type_mismatch` — the reference **did** resolve, but to a different object type than the role asked for (via the alias fallback, e.g. `class`/`exception` or `c`'s `function`/`macro`). It carries both `requested_type` and `resolved_type`.

An entry is matched to a warning by the exact triple `(doc_path, kind, target)` — the `requested_type`/`resolved_type` fields are informational, not part of the identity. `comment` is a free-text note explaining why the warning is accepted; it's yours to write and is preserved across runs.

The benchmark prints the new (non-whitelisted) warnings as two separate, most-frequent-first sections so the two failure modes don't drown each other out — the leading number on each line is that warning's occurrence count across the corpus:
- **Unresolved Domain-Object References** — `domain_object_reference` warnings, shown as `doc: 'target' (referenced as <missed type>)`.
- **Domain-Object Type Mismatches** — `object_type_mismatch` warnings, shown as `doc: 'target' (<requested> -> <resolved>)`.

It then prints a count of occurrences suppressed by the whitelist, and a **Stale Whitelist Entries** list (entries that no longer match any emitted warning).

**Auto-pruning:** stale entries are removed from the whitelist file automatically, but only when the warning data is trustworthy — the Bazel build succeeded **and** at least one sidecar was found. Otherwise pruning is skipped (and says why), so a broken or empty build can never silently delete your accepted entries along with their comments. This is why the build no longer runs under `--keep_going`: a failed render must fail the whole build rather than under-report a document's warnings and get its whitelist entries pruned as "stale".

The benchmark always exits 0 — it reports and prunes, but never gates the build on new warnings.
