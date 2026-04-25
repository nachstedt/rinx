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

1. **Cloning**: The script automatically performs a shallow clone of the CPython repository into `benchmark_data/cpython/`.
2. **Bazel Project Generation**: A `BUILD.bazel` file is generated on the fly inside the `Doc/` directory of the clone, utilizing a `glob(["**/*.rst"])` statement to automatically capture all reStructuredText files into a single `rusty_sphinx_library` target. It also generates a `rusty_sphinx_site` target to assemble the HTML.
3. **Execution**: The script runs `bazel build --keep_going //benchmark_data/cpython/Doc:site`. This triggers `rusty-sphinx` to parse, validate, and render every `.rst` file into HTML in parallel. *Note: Because the `rusty-sphinx` parser currently doesn't understand directives like `:maxdepth:`, strict toctree validation fails. Consequently, HTML generation is blocked until parser support improves.*
4. **Analysis**: Once the build completes (even with failures), the script traverses the generated Abstract Syntax Tree (`.ast`) JSON files located in `bazel-bin/`. It tallies up the `Directive::Unknown` nodes and outputs a frequency map.

## Interpreting Results

The script will output two critical pieces of information:

### 1. Rendering Time
You will see a line like:
```
Bazel build succeeded in X.XX seconds.
```
This is the raw time taken by Bazel to execute the `rusty-sphinx` pipeline across the entire CPython documentation suite. Since Bazel runs these in parallel, this highlights the concurrency benefits of our architecture.

### 2. Unsupported Directives Summary
The final output is a sorted list of Sphinx directives that `rusty-sphinx` encountered but does not yet know how to process correctly.

```
Unsupported Directives Summary:
-------------------------------
versionadded: 1542
code-block: 840
note: 430
...
```

This list serves as a prioritized roadmap for feature implementation. Implementing the most frequent missing directives will rapidly increase our compatibility with real-world Sphinx codebases.
