# rinx

[![CI](https://github.com/nachstedt/rinx/actions/workflows/ci.yml/badge.svg)](https://github.com/nachstedt/rinx/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rinx.svg)](https://crates.io/crates/rinx)
[![Docs](https://img.shields.io/badge/docs-nachstedt.github.io%2Frinx-blue.svg)](https://nachstedt.github.io/rinx/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**rinx** is a Rust re-implementation of (a subset of) the
[Sphinx](https://www.sphinx-doc.org/) documentation generator, built to be a
first-class [Bazel](https://bazel.build/) build step rather than an external
tool wrapped by one. It turns reStructuredText into an HTML site with
cross-references, toctree navigation, syntax highlighting, math and diagrams,
and it models requirements and other project-specific constructs the way
[sphinx-needs](https://sphinx-needs.com/) does.

> **Status:** early and pre-1.0. The rule and CLI interfaces may still change.
> [`docs/dev/spec_gaps.md`](docs/dev/spec_gaps.md) tracks which
> reStructuredText and Sphinx features are implemented and which are missing.

## Why

A Sphinx build is a single process: it re-reads the whole project and decides
for itself what is out of date. rinx splits the work into small,
deterministic steps (parse, index, render), each of them a separate Bazel
action. So:

- **Only what changed is rebuilt.** Editing one page re-parses that page
  only. Results are cached like any other Bazel output, including remote
  caches.
- **Documentation composes like code.** A `rinx_library` per team or
  component, a `rinx_site` that assembles them, and dependency checking that
  fails the build when a toctree points at a library that isn't declared.
- **Fast.** CPython's documentation (about 500 documents) builds as a Bazel
  target; see [benchmarking](https://nachstedt.github.io/rinx/docs/benchmark.html).
- **Resilient parsing.** A half-written document still produces a page, which
  is what makes the VS Code live preview possible.

## Features

- reStructuredText: sections, lists, tables (grid, simple, `list-table`,
  `csv-table`), admonitions, substitutions,
  `.. include::`, `.. literalinclude::`, images and figures.
- Sphinx: `toctree`, `:ref:`/`:doc:`/`:term:`, glossaries, the Python, C and
  standard domains, the general index, `objects.inv` output and
  [intersphinx](https://nachstedt.github.io/rinx/docs/intersphinx.html) linking between sites.
- Code blocks highlighted at build time, math rendered to MathML, and doctests
  executed by `bazel test` rather than during the build.
- **Entities**: a project's own directives (`.. req::`, `.. spec::`, ...)
  declared in a schema, with typed attributes, relations and derived
  back-links, plus tables, flowcharts, sequence diagrams and charts over them.
  `needimport` reads sphinx-needs' `needs.json`. See
  [Entities](https://nachstedt.github.io/rinx/docs/entities.html).
- PlantUML diagrams, opted into per library so projects without diagrams pay
  nothing for them.
- Jinja templating of sources, and `sphinx-design`'s `dropdown`, `grid` and
  `button-link`.

## Quick start

Add rinx to your `MODULE.bazel`; it is in the
[Bazel Central Registry](https://registry.bazel.build/modules/rinx):

```python
bazel_dep(name = "rinx", version = "0.1.0")
```

Then declare your documents in a `BUILD.bazel`:

```python
load("@rinx//:defs.bzl", "rinx_library", "rinx_site")

rinx_library(
    name = "docs",
    srcs = glob(["**/*.rst"]),
)

rinx_site(
    name = "site",
    deps = [":docs"],
)
```

`bazel build //:site` writes the HTML site to `bazel-bin/`. The site's
`config` (a `rinx.toml` with the project name and version), `template` and
`css` are optional and default to the bundled ones.

The [`examples/`](examples/) directory is a complete multi-team project that
uses every feature. Build it with `bazel build //examples:site`.

## Documentation

The documentation is at **<https://nachstedt.github.io/rinx/>**. It is built
by rinx itself from [`docs/`](docs/):
[getting started](https://nachstedt.github.io/rinx/docs/getting_started.html),
[the Bazel rules](https://nachstedt.github.io/rinx/docs/rules.html),
[supported syntax](https://nachstedt.github.io/rinx/docs/syntax.html),
[entities](https://nachstedt.github.io/rinx/docs/entities.html),
[intersphinx](https://nachstedt.github.io/rinx/docs/intersphinx.html),
[the VS Code preview](https://nachstedt.github.io/rinx/docs/vscode.html) and
[benchmarking](https://nachstedt.github.io/rinx/docs/benchmark.html).
The [example site](https://nachstedt.github.io/rinx/example-site/examples/index.html)
shows every supported construct rendered.

For the design, see
[`docs/decisions/`](docs/decisions/), the architecture decision records, and
[`docs/dev/`](docs/dev/), the developer notes.

## Development

Day-to-day development uses plain Cargo; Bazel wraps the same binary.

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --tests   # must be warning-free
cargo fmt

bazel build //examples:site        # the example project, end to end
bash tests/test_strict_deps.sh     # one of the build-level tests in tests/
```

Changing a Cargo dependency requires `CARGO_BAZEL_REPIN=1` on the next Bazel
build. [`CONTRIBUTING.md`](CONTRIBUTING.md) explains how to propose a change;
[`CLAUDE.md`](CLAUDE.md) and [`guidelines.md`](guidelines.md) describe the code
layout and conventions in detail.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option. Third-party components and their notices are listed in
[`THIRD_PARTY.md`](THIRD_PARTY.md).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
