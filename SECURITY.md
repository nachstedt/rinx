# Security Policy

## Supported versions

rinx is pre-1.0. Security fixes are made on `main` and shipped in the next
release; older releases are not patched.

## Reporting a vulnerability

Please do not open a public issue. Report it privately through GitHub's
[private vulnerability reporting](https://github.com/nachstedt/rinx/security/advisories/new)
instead.

Include what an attacker controls (a `.rst` document, a `needs.json`, an
`objects.inv`, a `BUILD.bazel` attribute, ...), what happens, and ideally a
minimal input that reproduces it. You can expect an acknowledgement within a
week.

rinx processes documentation sources that are usually part of the same
repository as the build, so the most relevant problems are ones reachable from
document content: crashes, unbounded resource use, or HTML injection into the
generated pages.
