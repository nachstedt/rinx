//! The rinx language server, served by `rinx lsp`.
//!
//! One binary with the build, so the editor and the build cannot disagree about
//! how a document parses or what a diagnostic code means — see
//! `docs/decisions/038-language-server.md`, and `docs/dev/lsp-roadmap.md` for
//! the order the server grows in. It is synchronous, on rust-analyzer's
//! `lsp-server`, because the pipeline it drives is synchronous and CPU-bound.
//!
//! - `server` is the protocol loop and the pure handlers it dispatches to;
//! - `completion` completes a `:ref:` or `:doc:` target from the index;
//! - `reference_at` finds the reference under the cursor, `hover` shows where
//!   it leads — `entity_hover` an entity's fields, also on the directive
//!   `entity_at` finds — and `definition` opens the document it leads to;
//! - `references` lists the uses of a definition, and `rename` says where in
//!   each its name stands and when renaming it is refused;
//! - `diagnostics` parses an open document and converts what the parser found;
//! - `render` renders a document for the diagnostics only a render finds, and
//!   `preview` renders its whole page for the editor's preview, whose links
//!   `preview_link` follows back to their sources;
//! - `position` converts rinx's 1-based character positions to the protocol's
//!   0-based, negotiated-unit ones;
//! - `documents` holds the text of every open document;
//! - `include_summary` notes on an `.. include::` the problems it brought in;
//! - `includes` decides what a file shows when open documents include it;
//! - `files` is the loader a file-reading directive goes through;
//! - `workspace_symbol` searches every project's definitions by name;
//! - `uri` converts between the protocol's `file:` URIs and paths;
//! - `project` finds the workspace's projects, reads their `conf.py`, and
//!   scans and keeps each one's project index.

mod code_action;
mod completion;
mod definition;
mod diagnostics;
mod document_symbol;
mod documents;
mod entity_at;
mod entity_hover;
mod files;
mod folding_range;
mod hover;
mod include_summary;
mod includes;
mod position;
mod preview;
mod preview_link;
mod progress;
mod project;
mod reference_at;
mod references;
mod rename;
mod render;
mod semantic_tokens;
mod server;
#[cfg(all(test, unix))]
mod test_scripts;
mod uri;
mod workspace_symbol;

pub use position::PositionEncoding;
pub use server::{CheckOptions, FolderCheck, ReferencesAt, check_folder, run};

use anyhow::Result;
use lsp_server::Connection;

/// Serves the language server protocol over standard input and output until
/// the client sends `exit`.
///
/// # Errors
///
/// Fails when the client breaks protocol or the stdio threads fail.
pub fn run_stdio() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();
    run(&connection)?;
    // Dropping the connection closes the writer's channel, which is what lets
    // its thread finish.
    drop(connection);
    io_threads.join()?;
    Ok(())
}
