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

Opening a ``.rst`` file starts the language server, ``rinx lsp``. It is the binary the build runs, so the editor reports the build's own diagnostics: the same codes and the same messages, underlined in the editor and listed in the *Problems* view as you type. For example, ``.. foo::`` is reported as ``directive.unknown``, and the code links to its entry in :ref:`diagnostics`. A ``.. noqa:`` comment silences a diagnostic in the editor exactly as it does in the build.

A mistake inside a file that an open document pulls in with ``.. include::`` is underlined in that file, at its own line, even when the file itself is not open. The ``.. include::`` line itself carries a note saying how many problems the file brought in, with a link to each, so the problem is visible from the document you are editing; it is information rather than a warning, so a problem is not counted twice. Editing an included file, saved or not, re-checks the open documents that include it. While an open document includes a file, that file shows what was found through its includers rather than what it reports on its own, because a fragment on its own lacks what its includer defines before the ``.. include::``.

That holds for documents that are not open, too. The server reads every ``.rst`` file under each workspace folder when it starts, so a fragment opened on its own is checked as the documents including it read it, whether or not they are open.

That scan also builds the project index the server keeps current as you edit. The status bar shows its progress, then for example ``rinx: 512 docs indexed (0.8 s)``; clicking it opens the server's log. The scan skips hidden directories and does not follow a symlinked directory, so a Bazel workspace's ``bazel-*`` output links are not read twice; ``.gitignore`` is not consulted, since generated sources are often part of the documentation; a project says which files are documents in its ``conf.py`` (see :ref:`vscode-sphinx-projects`). A workspace folder with no ``conf.py`` is one project, with ``index`` as its root document. ``docs/dev/lsp-roadmap.md`` in the repository lists the steps, and ``docs/decisions/038-language-server.md`` describes the design.

The index answers completion. Typing ``:ref:`` and the backtick opening its target lists every label in the workspace folder with the title a bare reference to it shows, and the document defining it. Typing the same after ``:doc:`` lists every document with its title, named relative to the current document, as ``:doc:`` reads it; begin the name with ``/`` to list the names from the source root instead. Inside ``Title <target>`` only the target completes, once the ``<`` is written. A label just written in an unsaved buffer completes at once, and while the scan is still running the list is refreshed as you type. The role must still be on one line, and ``:external:`` references do not complete, since the server reads no inventory yet.

Hovering a reference shows where it leads: the title of what it names — a section's title, a document's title, an object's qualified name, the number a ``:numref:`` or ``:eq:`` shows, an entity's title — and the file it is in, as a link that opens it. A reference into another site's inventory shows that page's address instead. This covers ``:ref:``, ``:doc:``, ``:term:``, ``:numref:``, ``:eq:``, ``:any:``, ``:option:``, the domain roles and the entity roles, resolved exactly as the built page links them, in the scope the reference is written in. A reference the page would draw broken shows no hover, since its warning already says why. The hover reads the document as you type it, so it is current without waiting for a pause, but with the same limit as the rendering below: no inventories.

Go to Definition (F12, or Ctrl+click) on the same references opens the file of the document they lead to: the document a ``:doc:`` names, or the one holding a label, term, object or entity. It covers the same roles and resolves them the same way, so a reference the page would draw broken leads nowhere. For now it opens the document at its first line, not at the label or object itself. A reference into another site's inventory has no definition, since there is no source to open; its hover shows the address.

The index also decides which references are broken. What the parser finds is reported as you type; what only rendering the page finds comes a moment later, once you pause for 300 ms: a ``:ref:``, ``:doc:`` or domain reference that resolves nowhere (``link.broken-ref``, ``link.broken-doc``, …), a label two documents define, an equation that does not convert, a listing whose filter matches nothing. These are the build's own warnings, with the same codes and messages, and a ``.. noqa:`` silences them as it does in the build. Only the documents you have open are rendered, along with any closed document that includes an open file. Defining a missing label in another document, even in an unsaved buffer, clears the warning without your touching the reference. The page is rendered with what the server reads of the project's configuration, but with no inventories yet, so a reference that the build resolves through intersphinx is reported as broken. Diagrams are not compiled.

.. _vscode-sphinx-projects:

Sphinx projects
^^^^^^^^^^^^^^^

A directory holding a ``conf.py`` is a Sphinx project, and every file belongs to the nearest project above it. What lies under no ``conf.py`` stays the workspace folder's own project. The status bar then names the project, for example ``rinx: Sphinx project (docs/conf.py) · 512 docs (0.8 s)``, and its tooltip lists every project.

The server reads ``conf.py`` without running it, since running it would need the project's Python and its extensions, and would run code you have not chosen to trust. It takes each setting below where it is written as a literal: a string, a number, ``True``, ``False``, ``None``, or a list, tuple or dict of those. Every setting it cannot take as written is reported in ``conf.py`` itself:

- ``conf.unread-setting``: the value is computed or imported, so the setting keeps its default.
- ``conf.modified-setting``: the literal is changed afterwards, as by ``exclude_patterns.append(…)`` inside an ``if``. The literal applies; the change does not.
- ``conf.invalid-value``: the value is not one the setting takes, or not one rinx supports.
- ``conf.wildcard-import``: a ``from … import *``, which may set anything.
- ``conf.syntax-error``: where reading stopped; every setting after it keeps its default.

These are the settings read:

- ``root_doc`` (or ``master_doc``): the root document. As in Sphinx, ``contents`` stands in for a missing ``index``.
- ``source_suffix``: the suffixes of documents. Only reStructuredText's are read, so a Markdown suffix is reported, and a file under two suffixes is one document, the first by name.
- ``exclude_patterns`` and ``include_patterns``: which files are documents, matched as Sphinx matches them. ``templates_path``, ``html_static_path`` and ``html_extra_path`` are excluded too, as Sphinx excludes them. An excluded file you open is still checked, but it is in no index and is not rendered.
- ``default_role``: the role a bare `` `text` `` is read as.
- ``primary_domain``: ``py`` or ``c``. Any other domain, or ``None``, is reported, and ``py`` is used.
- ``numfig``, ``numfig_secnum_depth`` and ``highlight_language``: how pages are rendered.

Every other setting is ignored, and not reported. ``extensions``, ``intersphinx_mapping``, ``rst_prolog`` and the sphinx-needs settings are read by later steps. Saving ``conf.py``, or creating or deleting one, reads the folder's projects again.

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
