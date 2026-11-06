# Notes on the Bazel rules (`rules/`)

- **`deps` is strict and toctree-only**: `validate_toctree` checks a document's toctree entries against it. A `.. toctree::` inside an included fragment still needs `deps`, since the check runs on the merged AST. A glob survives unexpanded into the index, because each phase expands it against a different set of documents.
- **`parse_data`** is every file the parser reads (`csv-table :file:`, `.. include::`, `.. literalinclude::`, Jinja templates). An `.rst` reached by `.. include::` must not also be in `srcs`, or it is published as a page too. A Jinja template name resolves against the source root, not the including document.
- **`images` and `downloads` are not parse inputs**: they ride the provider to the site, which copies them keeping their source-root-relative path; an undeclared one fails `validate_assets`.
- What a library failed to declare (toctree dep, diagram without `diagrams = True`, image, download) is an `error:` that fails the build and is never subject to `.. noqa:`.
- **Cache firewalls**: doctest plans (a non-default `doctest_plans` output group consumed by `rinx_doctest_tests`), `:loading: embed` images (`embed_assets` → `<doc>.embeds.json`), and diagram `.puml` files. Each keeps only what the costly step depends on; keep it that way, or every prose edit reruns it. Doctests run per library, so a library is split (as `examples/` does with `doctest_docs`) to narrow a rerun.
- Shell tests under `tests/` that remove an input must also edit the referencing `.rst`: Bazel does not rerun an action when an input is merely removed.
- `aspect_rules_js` must stay a `dev_dependency`, and the root package must stay loadable by every consumer (so `assets/` cannot become a package without breaking `@rinx//:assets/default.css`).
