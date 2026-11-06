# Notes on the language server (`rinx_lsp`) and the editors

- **Parity with the build is the promise** (ADR-038): the server reports what `rinx parse` warns, file by file (`crates/worker/tests/integration/lsp_parity.rs`). Reuse the build's code (parser, analyzer fold, renderer resolution, `render_document_page`) rather than reimplementing a question.
- `server/` follows the pure/IO split: handlers map state plus one message to the messages to send, tested directly and over `Connection::memory()`; `run.rs` is the thin loop.
- `position.rs` is the one place rinx's 1-based columns become the protocol's 0-based UTF-16/UTF-32 ones.
- `strictness.rs` is the one place a diagnostic's severity is decided, keyed on `DiagnosticSubject`, never the message.
- The server never touches the network. Inventories are fetched by the client on request, and only when it opts in.
- Projects are discovered from the folder (a Sphinx `conf.py`, read without running it through `rinx_pyconf`) or from a Bazel site's `lsp_manifest.json`. Every feature reads the resulting `ProjectModel`.
- The preview is a `rinx/preview` request rendered by the same call as the build's render action. The VS Code extension (`editors/vscode/`) only displays it; diagrams in the preview are drawn by `@plantuml/core` in the webview, never a JVM.
- Configuration snippets for other editors live in `editors/{neovim,helix,emacs}/` and are included by `docs/editors.rst` and run by `editors/run-tests.sh` — change a snippet and its test together.
