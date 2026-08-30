//! The analyzer module represents the global indexing phase.
//!
//! It builds a `ProjectIndex` — a global symbol table containing cross-reference
//! targets, document titles, and a hierarchical navigation tree derived from
//! toctree directives.
//!
//! `document_index` does the per-document walk (and delegates domain objects
//! to `domain_object_index` and toctrees to `nav_tree`); `project_index`
//! merges every document's result into the one global index.
//! `path_normalization` resolves the `.`/`..` components in the document
//! paths a toctree may name.

mod document_index;
mod domain_object_index;
mod nav_tree;
mod path_normalization;
mod project_index;

pub use document_index::analyze;
pub use path_normalization::normalize_path;
pub use project_index::build_project_index;
