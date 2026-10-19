A Bazel project as `rinx lsp --check --manifest` reads it, for
`tests/integration/lsp.rs`. `site.lsp_manifest.json` is written by hand, as
`rules/site.bzl`'s `lsp_manifest` output group would write it for this tree,
naming only source files; there is no `BUILD.bazel`, because no build runs.
