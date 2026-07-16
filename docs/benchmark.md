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

The script outputs three pieces of information:

### 1. Rendering Time
You will see a line like:
```
Bazel build succeeded in X.XX seconds.
```
This is the raw time taken by Bazel to execute the `rusty-sphinx` pipeline across the entire CPython documentation suite. Since Bazel runs these in parallel, this highlights the concurrency benefits of our architecture.

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
