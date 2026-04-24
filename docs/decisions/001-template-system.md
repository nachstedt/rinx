# ADR-001: HTML Template System

**Status:** Accepted  
**Date:** 2026-04-19  

## Context

Today rusty-sphinx renders each `.rst` file into a raw HTML fragment — bare `<h1>`, `<p>`, `<ul>` elements with no surrounding page structure. There is no `<!DOCTYPE html>`, no `<head>`, no stylesheet, no navigation sidebar. The output is functional but not usable as a documentation site: it lacks visual structure, branding, and navigation between pages.

Sphinx solves this with a Jinja2-based theming system. Each page body is injected into a template (`layout.html`) that provides the page chrome — sidebar navigation, project title, CSS links, header, and footer. We need an equivalent mechanism for rusty-sphinx.

## Decision Drivers

- **Bazel cacheability:** The template mechanism must produce deterministic output and play well with Bazel's input-hashing cache.
- **Performance:** Template application should not become a bottleneck in the render phase.
- **Separation of concerns:** HTML structure should be editable independently of Rust code.
- **Customisability:** Users should be able to supply their own template without rebuilding the binary.
- **Minimal dependencies:** The solution should not introduce heavy or numerous new crate dependencies.

## Options Considered

### Option 1: Reuse Sphinx's Jinja2 Templates Directly

Use the existing Sphinx theme templates (e.g., Alabaster's `layout.html`) and evaluate them with the Python Jinja2 engine at build time.

**Rejected because:**
- Requires a Python runtime as a build dependency, breaking Bazel's hermetic build philosophy.
- Requires a Rust-to-Python FFI bridge (via PyO3 or subprocess), adding significant complexity.
- Sphinx templates call Python-native helper functions in the template context (`pathto()`, `toctree()`, `hasdoc()`). These are live Python callables, not data — a Jinja2-compatible engine in Rust cannot execute them without reimplementing Sphinx's entire context-building layer.
- The dependency and complexity cost vastly outweighs the benefit of template reuse.

### Option 2: Compile-Time Rust Templates (Askama/Maud or Hand-Written Strings)

Bake the template into the Rust binary using either a compile-time template engine like Askama (Jinja2-like syntax, compiled to Rust code) or hand-written string concatenation.

**Rejected because:**
- The template is compiled into the binary. Any change to the HTML layout — even a CSS class rename — requires a full binary rebuild and release.
- User customisation is impossible without Rust code changes. Users cannot supply their own template.
- For a built-in default template that rarely changes, this would technically work, but it forecloses the extensibility goal entirely.

### Option 3: Runtime Jinja2-Compatible Template Engine (MiniJinja) ✅

Use [MiniJinja](https://github.com/mitsuhiko/minijinja), a lightweight Rust-native Jinja2-compatible template engine, to evaluate an external `.html` template file at render time.

**Chosen because:**
- **Clean separation:** The HTML template lives in a separate `.html` file. Designers or users can edit it without touching Rust code.
- **Customisable without a binary rebuild:** A user-supplied template is just a different file path passed via the Bazel rule's `template` attribute. The binary does not change.
- **Equally cacheable in Bazel:** The template file is declared as an explicit input to every render action. Bazel tracks it and correctly invalidates cached HTML when it changes — identical cache behaviour to a compile-time approach.
- **Minimal dependencies:** MiniJinja's only mandatory dependency is `serde`, which rusty-sphinx already uses.
- **Familiar syntax:** Jinja2 syntax (`{{ body }}`, `{% for entry in nav_tree %}`) is widely known and well-supported by editor tooling.
- **Negligible runtime cost:** Template parsing is trivial compared to the I/O cost of reading/writing files in each render action.

## CSS Delivery

A single `default.css` file is emitted into the site output root directory. Each rendered HTML page references it via a computed relative path (e.g., `../../default.css` for a page two directories deep).

**Inline `<style>` blocks rejected because:** they duplicate the same CSS content across every page with no benefit. Relative paths work equally well for offline viewing — there is no advantage to inlining.

## Configuration

Site metadata — project name and version — is specified in a single `rusty_sphinx.toml` configuration file. File paths (template, CSS) are **not** part of the config file; they are passed as separate CLI flags (`--template`, `--config`) and as Bazel rule attributes (`template`, `config`, `css`).

**File paths in config rejected because:** Bazel runs build actions inside a sandbox where file paths are relocated (e.g., `bazel-out/darwin_x86_64-fastbuild/bin/...`). A config file referencing `template = "templates/default.html"` breaks because that relative path doesn't exist in the sandbox. Attempts to work around this (rewriting the config at build time) are fragile. Keeping file paths as CLI flags lets the build system resolve them correctly without touching the config content.

**Individual CLI flags for metadata rejected because:** accumulating `--project`, `--version` etc. makes the binary interface fragile and hard to extend. A config file keeps metadata stable: adding a new field requires no CLI changes.

## Navigation

The sidebar renders a hierarchical, toctree-aware navigation tree matching Sphinx's behaviour. The tree is built during the indexing phase (Phase 2) by following toctree relationships between documents. Each document that declares a `.. toctree::` becomes a parent, and its entries become children. The resulting `nav_tree` is part of the serialised `ProjectIndex` and passed as a template variable to MiniJinja.

This requires no additional Bazel actions — the nav tree flows through the existing `project.index` file that the render action already depends on.

## Consequences

- **Template changes trigger full re-render:** Because the template is an input to every render action, changing it invalidates all cached HTML. This is correct behaviour (the layout changed for every page) but should be understood by template authors.
- **Not Sphinx-template-compatible:** Our template variables (`{{ body }}`, `{{ nav_tree }}`, `{{ css_path }}`) are not source-compatible with Sphinx templates, which inject Python callables (`{{ toctree() }}`, `{{ pathto() }}`). Existing Sphinx themes cannot be used as drop-in replacements, but their HTML structure and CSS class names can serve as inspiration.
- **MiniJinja is a new dependency:** It is lightweight (only depends on `serde`) but is still a new crate in the dependency tree. It is actively maintained with 31M+ downloads.
