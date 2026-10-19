The `e2e-bazel` tests' workspace: a Bazel workspace (`workspace/MODULE.bazel`)
with one site, whose `site.lsp_manifest.json` is written by hand as
`rules/site.bzl`'s `lsp_manifest` output group would write it. `fake-bazel/bazel`
stands in for Bazel on the extension host's PATH, so no build runs.
