//! The analyzer module represents the global indexing phase.
//!
//! It builds a `ProjectIndex` — a global symbol table containing cross-reference
//! targets, document titles, and a hierarchical navigation tree derived from
//! toctree directives.

mod document_index;
mod domain_object_index;
mod nav_tree;
mod project_index;
mod utils;

pub use document_index::analyze;
pub use project_index::build_project_index;
pub use utils::normalize_path;
