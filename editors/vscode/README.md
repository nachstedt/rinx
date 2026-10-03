# Rinx for VS Code

Diagnostics and a live preview for reStructuredText documentation built with
[rinx](https://github.com/nachstedt/rinx), a fast, Bazel-native
re-implementation of Sphinx.

- **Diagnostics as you type.** The language server (`rinx lsp`) is the same
  binary that builds your site, so the editor reports exactly what the build
  reports, with the same codes.
- **Live preview.** `Rinx: Show Preview` renders the open document the way
  the site will.

## Requirements

A `rinx` binary. The extension finds one in this order:

1. the `rinx.binaryPath` setting;
2. a Bazel workspace's `@rinx//:rinx`;
3. `rinx` on the `PATH`.

`cargo install rinx` provides one.

## Documentation

See [the VS Code guide](https://nachstedt.github.io/rinx/latest/docs/vscode.html).
