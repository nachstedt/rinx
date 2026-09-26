# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Before 1.0, a minor version bump may change the Bazel rules and the CLI in
incompatible ways.

## [Unreleased]

## [0.1.0] - Unreleased

The first public release.

### Added

- Bazel rules `rinx_library`, `rinx_site`, `rinx_inventory` and
  `rinx_doctest_tests`. Parse, index and render each run as a separate,
  cacheable action, and a toctree that points at an undeclared library fails
  the build.
- reStructuredText: sections, bullet, enumerated and definition lists, grid and
  simple tables, `list-table`, `csv-table`, admonitions, substitutions,
  `include`, `literalinclude`, images and figures.
- Sphinx: `toctree`, `:ref:`, `:doc:` and `:term:`, glossaries, the Python, C
  and standard domains, the general index, and sphinx-simplepdf's
  `.. if-builder::`.
- `objects.inv` output, and intersphinx linking between sites through pinned
  inventories.
- Code blocks highlighted at build time, math rendered to MathML, and doctests
  executed by `bazel test` rather than during the build.
- Entities: project-defined directives declared in a schema, with typed
  attributes, relations and derived back-links, listed and drawn by
  `entity-table`, `entity-flow`, `entity-sequence`, `entity-pie` and
  `entity-bar` (with the sphinx-needs spellings as aliases), and
  `needimport` for sphinx-needs' `needs.json`.
- PlantUML diagrams, opted into per library.
- Jinja templating of sources, and sphinx-design's `dropdown`, `grid` and
  `button-link`.
- A `preview` subcommand and a VS Code extension for live preview.

[Unreleased]: https://github.com/nachstedt/rinx/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nachstedt/rinx/releases/tag/v0.1.0
