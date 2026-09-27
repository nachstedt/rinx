# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Before 1.0, a minor version bump may change the Bazel rules and the CLI in
incompatible ways.

## [Unreleased]

### Added

- A version switcher for the default template: `[version_switcher] json_url`
  in `rinx.toml` names a pydata-sphinx-theme-style `versions.json`, and every
  page gets a menu of the published versions plus a banner on any version
  other than the preferred one. A page finds its version by its own address,
  so builds stay identical across the directories they are published to.

## [0.1.0] - 2026-09-26

The first public release. The documentation is at
<https://nachstedt.github.io/rinx/>.

### Added

- Bazel rules `rinx_library`, `rinx_site`, `rinx_inventory` and
  `rinx_doctest_tests`. Parse, index and render each run as a separate,
  cacheable action, and a toctree that points at an undeclared library fails
  the build.
- reStructuredText: sections, bullet, enumerated and definition lists, grid and
  simple tables, `list-table`, `csv-table`, admonitions, substitutions,
  `include`, `literalinclude`, images and figures.
- Sphinx: `toctree`, `:ref:` and `:term:`, glossaries, the Python, C and
  standard domains, the general index, and sphinx-simplepdf's
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
