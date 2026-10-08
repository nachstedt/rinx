//! What the server knows about the workspace beyond the open documents.
//!
//! [`discover`] finds a folder's documents, [`scan`] parses and analyses them
//! in parallel when the server starts, and [`folder`] keeps one analysis per
//! document and folds them into the folder's project index. Each workspace
//! folder is a project of its own until a `conf.py` or a Bazel manifest says
//! where projects begin (roadmap #10, #18).

mod discover;
mod folder;
mod scan;

pub use discover::{discover_sources, is_discoverable, is_visible_under};
pub use folder::{IndexedDocument, WorkspaceFolder};
#[cfg(test)]
pub use scan::parse_folder_documents;
pub use scan::scan_folder;
