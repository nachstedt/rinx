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

1. **Cloning**: The script automatically performs a shallow clone of the CPython repository into a temporary directory (`rusty_sphinx_benchmark_cpython` under the system temp dir), at the **release tag** named by `PYTHON_VERSION` in `scripts/benchmark.py` — not at `main`.

### Why the version is pinned

`PYTHON_VERSION` is the single place the benchmark's Python version is decided, and it pins *both* halves of the corpus so they cannot drift apart:

- the CPython release tag whose `Doc/` tree is cloned (`v3.14.2`), and
- the interpreter the generated workspace resolves, via a `python.toolchain(python_version = ...)` written into the injected `MODULE.bazel`.

This matters for two reasons. First, **doctests**: the script used to clone `main` while the interpreter came from rusty-sphinx's own `MODULE.bazel`, so the documentation described a development version whose APIs the interpreter did not have. Every doctest exercising a newly added API failed for a reason that had nothing to do with rusty-sphinx (`re.Pattern.prefixmatch`, `IPv4Network.next_network`, `PrettyPrinter(expand=...)`, `shlex.quote(force=...)` were all seen). Second, **reproducibility**: a benchmark against a moving branch produces numbers that change on their own, so a delta in `benchmark_result.txt` could never be attributed to a local change with confidence. A tag makes the corpus fixed.

Overriding it for a one-off comparison:

```bash
bazel run //scripts:benchmark -- --python-version 3.13.11
```

A version must exist on **both** sides to be usable: as a `v<version>` tag in the CPython repository, and as an entry in rules_python's `TOOL_VERSIONS` (`python/versions.bzl`) for the rules_python release this workspace depends on. CPython ships later 3.14.x tags than rules_python 2.0.0 has interpreters for, which is why the default is 3.14.2 rather than the newest patch release.

Note that `scripts/domain_warnings_whitelist.json` is tied to the pinned corpus: entries for documents that do not exist at that tag are pruned automatically. Changing `PYTHON_VERSION` will therefore churn the whitelist.
2. **Bazel Project Generation**: A `BUILD.bazel` file is generated on the fly inside the `Doc/` directory of the clone, utilizing a `glob(["**/*.rst"])` statement to automatically capture all reStructuredText files into a single `rusty_sphinx_library` target. It also generates a `rusty_sphinx_site` target to assemble the HTML.
3. **Warm-up build**: A second, one-document site (`bench_warmup/`, generated beside `Doc/`) is built first. Its only purpose is to compile the `rusty-sphinx` binary and resolve the Rust, Java and Python toolchains *before* the clock starts on the documentation build — see "Rendering Time" below for why this is a separate build rather than a `bazel build @rusty_sphinx//:rusty_sphinx_worker`.
4. **Execution**: The script then runs `bazel build //Doc:site`. This triggers `rusty-sphinx` to parse, validate, and render every `.rst` file into HTML in parallel. The build currently succeeds outright against CPython's docs — toctree validation passes and HTML is produced for every page.
5. **Analysis**: Once the build completes, the script traverses the generated Abstract Syntax Tree (`.ast`) JSON files located in `bazel-bin/`. It tallies up `Directive::Unknown` nodes (directives the parser doesn't recognize), `Toctree.ignored_options` (recognized toctree options the parser doesn't yet act on, e.g. `:caption:`), and per-document parser diagnostics, and prints each as a frequency map.

## Interpreting Results

The full analysis is far too long for a terminal, so the script writes it to **`benchmark_result.txt`** in the workspace root and prints only a compact **Benchmark Summary** (distinct/occurrence counts per category) to the screen, ending with a pointer to that file. Everything described below — the frequency tables and the domain-object listings — lives in `benchmark_result.txt`; the terminal shows just the counts. (`benchmark_result.txt` is git-ignored.)

### 1. Rendering Time
You will see two timings, one per `bazel build`:
```
Dependency build finished in Y.YY seconds.
Documentation build succeeded in X.XX seconds.
(Excludes Y.YY seconds spent building rusty-sphinx itself.)
```
`X.XX` is the number the benchmark is about: the raw time taken by Bazel to execute the `rusty-sphinx` pipeline across the entire CPython documentation suite. Since Bazel runs these in parallel, this highlights the concurrency benefits of our architecture. `Y.YY` is the cost of compiling `rusty-sphinx` and fetching its toolchains, which says nothing about documentation throughput and varies wildly with how warm the Bazel cache happened to be (it dominates everything after a `--clean`).

Keeping the two apart is why the warm-up site exists. Building `@rusty_sphinx//:rusty_sphinx_worker` directly would not do: build tools are compiled in Bazel's *exec* configuration, so that would warm a differently-configured binary and leave the real one to be compiled inside the timed step. Building a trivial site warms exactly the configurations the corpus build reuses, at the cost of one tiny document.

Each build's output is captured (so the script can time and parse it), which means neither log is streamed to your terminal. They are written to `bazel_deps_build.log` and `bazel_build.log` in the generated workspace (`$TMPDIR/rusty_sphinx_benchmark_cpython/`), on both success and failure — inspect them there to see exactly what the inner builds printed. The Bazel timing profile is written alongside it as `profile.json.gz` (drop it into https://ui.perfetto.dev/ or `chrome://tracing`).

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
