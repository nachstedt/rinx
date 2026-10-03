//! The rinx language server, served by `rinx lsp`.
//!
//! One binary with the build, so the editor and the build cannot disagree about
//! how a document parses or what a diagnostic code means — see
//! `docs/decisions/038-language-server.md`, and `docs/dev/lsp-roadmap.md` for
//! the order the server grows in. It is synchronous, on rust-analyzer's
//! `lsp-server`, because the pipeline it drives is synchronous and CPU-bound.
//!
//! - `server` is the protocol loop and the pure handlers it dispatches to;
//! - `diagnostics` parses an open document and converts what the parser found;
//! - `position` converts rinx's 1-based character positions to the protocol's
//!   0-based, negotiated-unit ones;
//! - `documents` holds the text of every open document;
//! - `files` is the loader a file-reading directive goes through.

mod diagnostics;
mod documents;
mod files;
mod position;
mod server;

pub use position::PositionEncoding;
pub use server::run;

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
