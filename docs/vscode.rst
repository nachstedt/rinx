.. _vscode:

VS Code Extension Architecture for Rinx
=======================================

This document outlines the proposed architecture for a high-performance VS Code extension providing live previews for ``rinx`` documentation. The extension also runs the ``rinx`` language server, which underlines problems as you type (see :ref:`vscode-diagnostics` below).

Core Philosophy
---------------

The extension prioritizes **instant feedback** (sub-100ms latency) by bypassing the full Bazel build graph during content editing. It leverages the modular design of the ``rinx`` binary to perform "dirty" but fast renders.

Architecture Overview
---------------------

The extension acts as an orchestrator between the VS Code editor, the ``rinx`` CLI, and the Bazel build artifacts.

1. Configuration
~~~~~~~~~~~~~~~~

The extension provides the following settings:

- ``rinx.binaryPath``: Path to the ``rinx`` binary, for the preview and the language server. When unset, the binary is discovered through Bazel, falling back to ``rinx`` on the ``PATH``.
- ``rinx.configPath``: Path to the ``rinx.toml`` config file (auto-detected from workspace).
- ``rinx.templatePath``: Path to the HTML template used for rendering.
- ``rinx.indexPath``: Path to the project index (default: auto-detected from ``bazel-bin``).
- ``rinx.previewMode``: ``onSave`` or ``onType`` (default: ``onType``).
- ``rinx.trace.server``: ``off``, ``messages`` or ``verbose``; traces the language server's protocol traffic into the *Rinx* output channel.

2. The Preview Pipeline (As-You-Type)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

When the user edits a ``.rst`` file, the extension executes the following pipeline:

1. **Parse**: The current editor content is piped into ``rinx parse``.
2. **Local Analysis**: The resulting AST is analyzed locally using ``rinx index`` (on just this one file) to extract current targets and the document title.
3. **Index Merging**: The local data is merged into the "stale" global index loaded from the ``bazel-bin`` directory. This ensures same-file references are always correct.
4. **Render**: The AST is rendered into HTML using the merged index.
5. **Webview Update**: The HTML is pushed to a VS Code Webview panel.

.. _vscode-diagnostics:

3. Diagnostics
~~~~~~~~~~~~~~

Opening a ``.rst`` file starts the language server, ``rinx lsp``. It is the binary the build runs, so the editor reports the build's own diagnostics: the same codes and the same messages, underlined in the editor and listed in the *Problems* view as you type. For example, ``.. foo::`` is reported as ``directive.unknown``.

For now the server knows each open document on its own: there is no project index yet, no ``.. noqa:`` filtering, and no reporting inside included files. ``docs/dev/lsp-roadmap.md`` in the repository lists the steps that add these, and ``docs/decisions/038-language-server.md`` describes the design.

4. Change Detection & Background Reconcile
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

To keep the global state (Sidebar, cross-references to other files) eventually consistent, the extension monitors the local index:

- **Global Change Detection**: If a change in the local file affects **Global Metadata** (e.g., the document's ``<h1>`` title changed, a target was added/removed, or the ``.. toctree::`` was modified), the extension detects this by comparing the new local index against the cached global index.
- **Background Build**: Upon detecting a global change, the extension triggers a background ``bazel build //Doc:site``.
- **Silent Refresh**: Once Bazel completes, the extension re-loads the global index and refreshes the preview to reflect the now-correct global state (e.g., updated sidebar titles).

Caveats & Required Changes
--------------------------

Caveat 1: The CLI Requires File Paths, Not Stdin
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The current ``parse`` subcommand expects ``--input <file> --output <file>``. For as-you-type previews, the extension would need to either:

- Write the editor buffer to a temporary file on every keystroke (adds I/O overhead and complexity), or
- Add ``--stdin`` / ``--stdout`` support to the binary so the extension can pipe content directly.

**Recommendation**: Add a ``preview`` subcommand that reads RST from stdin, accepts an ``--index`` path, and writes the final HTML to stdout. This collapses the parse → index → render pipeline into a single process invocation, avoiding three separate spawns per keystroke.

Caveat 2: Index Merging Does Not Exist Yet
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The document describes "merging" a local single-file index into the global index. Currently, ``process_index`` builds a fresh ``ProjectIndex`` from a list of AST files — there is no "merge a partial update into an existing serialized index" capability.

**Recommendation**: If using the single ``preview`` subcommand approach, the merging can happen in-process: deserialize the global index, run ``analyze()`` on the current document, and overwrite the relevant entries before rendering. No new CLI surface is needed — but the logic must be implemented in the binary.

Caveat 3: The Render Step Requires Config and Template Files
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The ``render`` subcommand needs ``--config <config.toml>`` and ``--template <template.html>``. The extension must know where these files live. In Bazel projects, their paths are specified in ``BUILD.bazel`` and may use default labels like ``@@//:templates/default.html``.

**Recommendation**: The extension should auto-detect the config and template by:
1. Searching for ``rinx.toml`` upward from the active file.
2. Falling back to reasonable defaults or prompting the user.
3. Exposing ``configPath`` and ``templatePath`` as settings for manual override.

Caveat 4: PlantUML Diagrams Are Not Rendered in Preview
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The preview pipeline skips the PlantUML compilation step (which requires a JVM). The HTML will contain ``<img>`` tags pointing to SVG files that may not exist yet.

**Recommendation**: This is acceptable for a "content preview." The extension could:
- Show a placeholder for missing diagram images.
- Optionally trigger PlantUML compilation on save (not on every keystroke) for users who want diagram previews.

Caveat 5: Process Spawn Overhead on Every Keystroke
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Spawning a new ``rinx`` process for every keystroke (even with debouncing) incurs OS-level overhead: fork, exec, dynamic linker, argument parsing.

**Recommendation**: Consider a **long-running server mode** for the binary. The extension would start ``rinx serve`` once, then communicate via stdin/stdout JSON-RPC or a local socket. This eliminates per-request spawn costs and allows the binary to keep the global index in memory. This can be deferred to a later iteration — simple process spawning with ~50ms debounce will likely be fast enough for v1.

Caveat 6: No Initial Index Without a Prior Bazel Build
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

If the user opens a project that has never been built, there is no ``site.project.index`` in ``bazel-bin``. The extension would fail to load the global index.

**Recommendation**: The extension should handle a missing index gracefully by:
- Using an empty ``ProjectIndex`` (cross-references and sidebar will be empty, but the page content renders fine).
- Showing a notification: *"No project index found. Run a full build for cross-references and navigation."*
- Optionally triggering the initial ``bazel build`` in the background.

Caveat 6b: External Links Come From the Last Build's Index
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

References into other sites (intersphinx, see :ref:`intersphinx`) resolve in the preview without any extra input, because the declared inventories are stored in the global index the preview merges into. But the index keeps only the external targets some document referenced when it was built, so a reference to an external target nothing else names stays unresolved in the preview until the next build — the same eventual consistency as titles and cross-references.

Caveat 7: ``doc_path`` Must Be Relative to the Sphinx Root
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The renderer uses ``doc_path`` (e.g., ``library/os.rst``) to compute relative links for CSS, images, and cross-references. The extension must correctly compute this relative path from the workspace root or the Sphinx doc root — not use the absolute filesystem path.

**Recommendation**: Derive ``doc_path`` by stripping the detected doc root (e.g., the directory containing ``rinx.toml``) from the absolute file path.

Benefits
--------

- **Live Feedback**: Users see their text and local links update as they type.
- **Low Overhead**: No full-project re-indexing is required for 99% of edits (content changes).
- **Correctness**: Leverages the same binary and logic used in the production build, ensuring "what you see is what you get."
- **Native Experience**: No need to manage a separate dev-server or browser window.

Future Enhancements
-------------------

- **Long-Running Server Mode**: A ``rinx serve`` command that keeps the index in memory and accepts render requests over a local socket. This eliminates process spawn overhead and enables sub-10ms feedback.
- **Scroll Sync**: Bidirectional scrolling between the ``.rst`` editor and the preview.
- **Diagram Preview**: Optional PlantUML rendering on save for users who want to see diagrams in the preview.

Development and Local Testing
-----------------------------

1. Repository Structure
~~~~~~~~~~~~~~~~~~~~~~~
The extension should be developed in a dedicated directory at the project root:
- ``editors/vscode/``: Contains the TypeScript source code, ``package.json``, and VS Code configuration.

2. Prerequisites
~~~~~~~~~~~~~~~~
- **Node.js & npm**: Required for building the extension.
- **VS Code**: The target editor.
- ``vsce``: The VS Code Extension Manager, installed as a development dependency by ``npm install``.

3. Local Development Workflow
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
To develop and debug the extension without publishing:
1.  **Open the directory**: Open ``editors/vscode`` in a new VS Code window.
2.  **Install dependencies**: Run ``npm install``.
3.  **Launch Extension Development Host**: Press ``F5`` (or go to *Run and Debug* -> *Launch Extension*). This opens a new VS Code window with the extension loaded.
4.  **Live Debugging**: You can set breakpoints in the TypeScript source and use the *Debug Console* to inspect the extension's behavior.

4. Checks and Tests
~~~~~~~~~~~~~~~~~~~
CI runs all of these on every pull request, from ``editors/vscode``:

- ``npm run typecheck``, ``npm run lint`` and ``npm run format:check``. The linter is oxlint, with type-aware rules: the extension is on TypeScript 7, which ESLint's TypeScript support cannot load.
- ``npm test`` runs the tests inside a real VS Code through ``@vscode/test-cli``, configured in ``.vscode-test.mjs``. ``VSCODE_VERSION=min`` picks the oldest VS Code that ``engines.vscode`` allows, and ``VSCODE_VERSION=insiders`` picks the next one. ``npm test -- --coverage`` also measures coverage, which ``node scripts/check-coverage.mjs <floor>`` checks.
- The tests come in four labels, and ``npm test -- --label <name>`` runs one. ``unit`` needs nothing else. ``e2e`` opens a copy of ``test-fixtures/workspace`` and talks to the real ``rinx lsp``: the binary named by ``RINX_BINARY``, or else ``target/debug/rinx``, so run ``cargo build`` first. ``e2e-no-server`` checks that the extension survives a binary that does not exist. ``e2e-vsix`` exists only when ``RINX_VSIX`` names a packaged extension: it runs the ``e2e`` tests against that VSIX installed into VS Code, rather than against the checkout.

5. Packaging and Local Installation
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
To test the extension as a "production" build:
1.  **Package the extension**: Run ``npm run package`` inside ``editors/vscode``. This bundles the extension with esbuild into ``dist/`` and writes ``rinx.vsix``. ``npm run package:check`` compares the packaged files against ``vsix-files.txt``. Every CI run also uploads the VSIX as the ``rinx-vsix`` artifact.
2.  **Install locally**: Run ``code --install-extension rinx.vsix``.
3.  **Configure**: Set ``rinx.binaryPath`` in your global VS Code settings to point to your locally built binary, such as ``target/debug/rinx`` after ``cargo build``. Reload the window after rebuilding it, so the language server restarts.

6. Bazel Integration (Optional)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
While standard VS Code development tools are recommended, the final packaging step can be integrated into Bazel using a ``genrule`` that invokes ``vsce package``, ensuring that the extension version matches the project version.
