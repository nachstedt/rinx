# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Before 1.0, a minor version bump may change the Bazel rules and the CLI in
incompatible ways.

## [Unreleased]

### Added

- Roles: `:any:`, `:doc:`, `:download:` (with a `downloads` attribute on
  `rinx_library`, bundled under `_downloads/`), `:numref:` (with `numfig`,
  `numfig_secnum_depth` and `numfig_format` in `rinx.toml`), `:code:` and
  `.. role:: name(code)`, `:pep:`, `:rfc:`, `:cve:` and `:cwe:` (with
  `pep_base_url` and `rfc_base_url`), docutils' `:pep-reference:` and
  `:rfc-reference:`, `:index:`, and `:sub:`/`:sup:`.
- A default role: `.. default-role::`, suffix roles (`` `text`:role: ``) and a
  `default_role` attribute on `rinx_library`.
- `see` and `seealso` entries in the general index.
- The Python Module Index, with `:mod:` tooltips, package folding and
  `modindex_common_prefix`; the site's `domain_indices` attribute decides
  whether it is written.
- `rst_prolog` and `rst_epilog` attributes on `rinx_library`, and
  `parse_config`, a `rinx_parser.toml` whose `ignored_roles` are shown as
  their text.
- The default theme's sidebar folds every branch off the current page's path,
  and `collapse_navigation` in `rinx.toml` trims it to that branch as Sphinx
  does.
- A version switcher for the default template: `[version_switcher] json_url`
  in `rinx.toml` names a pydata-sphinx-theme-style `versions.json`, and every
  page gets a menu of the published versions plus a banner on any version
  other than the preferred one. A page finds its version by its own address,
  so builds stay identical across the directories they are published to.
- `rinx init --from-sphinx <dir>` migrates a Sphinx project into a rinx build:
  its `BUILD.bazel`, `rinx.toml`, `entities.toml`, prolog and epilog files and
  `MODULE.bazel` dependency. Editors offer the same as a source action.
- `rinx needs_schema` converts a sphinx-needs configuration (a
  `ubproject.toml` or `conf.py`'s `needs_*` settings) into an entity schema.
- A language server, `rinx lsp`, for plain Sphinx projects (reading `conf.py`
  without running it, or by running it in a trusted workspace when allowed)
  and for Bazel sites (through `rinx_site`'s new `lsp_manifest` output group):
  - the build's diagnostics as you type, `.. noqa:` honoured, each code linked
    to its documentation, including those inside included fragments and the
    index phase's; their severity follows the project's declared extensions;
  - intersphinx inventories, `rst_prolog`/`rst_epilog`, sphinx-needs schemas
    and Jinja-templated Bazel libraries;
  - completion of role targets, role names, directive names, options and
    option values, and an entity relation's ids;
  - hover, go to definition, find references, rename of labels, entity ids
    and equation labels, workspace symbol search, the document outline,
    folding and semantic highlighting;
  - quick fixes adding a missing toctree dependency to `BUILD.bazel` and
    inserting a `.. noqa:`;
  - `rinx lsp --check <folder>`, the same analysis from the command line;
  - configurations for Neovim, Helix and Emacs, tested by CI.
- The VS Code extension runs the language server: its preview is rendered by
  the server, shows images and diagrams (PlantUML compiled to JavaScript, no
  JVM), follows links and scrolls together with the editor, and opens from the
  editor's title bar. *Rinx: Analyse Project* lists what a project writes that
  rinx does not support, and `npm run package:bundled` bundles the binary into
  the VSIX.

### Changed

- A name two documents define — label, glossary term, domain object, equation
  label or entity id — now defines nothing: every claimant is warned and a
  reference to it is `link.ambiguous-target`. Before, the last document merged
  won, depending on the order of the index action's inputs.
- Diagrams are laid out with ELK instead of a host Graphviz, PlantUML runs on
  a pinned JDK 21, and an error picture PlantUML draws with exit code 0 fails
  `validate_assets`.
- Hyperlinks are read as docutils reads them: a relative embedded URI
  (`<#anchor>`, `<page.html>`) links as written, and aliases and indirect
  targets resolve within their own document.
- A directive's diagnostics are reported on its marker or first content line.
- Unknown roles are reported, as `interpreted.unknown-role`, or as
  `interpreted.unsupported-role` for a docutils or Sphinx role rinx lacks.

### Removed

- The `preview` subcommand. The editor's preview is now served by `rinx lsp`.

### Fixed

- `:no-index:` is honoured on every domain object.
- Admonition content written on the directive line is kept.
- Malformed and misplaced section titles are reported.
- A lone `\r` line ending no longer shifts diagnostics below it in the editor,
  and the inline parser no longer panics on some non-ASCII text before `-_`.

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
