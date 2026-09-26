//! The analyzer module represents the global indexing phase.
//!
//! It builds a `ProjectIndex` — a global symbol table containing cross-reference
//! targets, document titles, and a hierarchical navigation tree derived from
//! toctree directives.
//!
//! `document_index` does the per-document walk (and delegates domain objects
//! to `domain_object_index` and toctrees to `nav_tree`); `project_index`
//! merges every document's result into the one global index.
//! `rinx_ast`'s `normalize_path`, re-exported below, resolves the
//! `.`/`..` components in the document paths a toctree may name.

mod document_index;
mod domain_object_index;
mod entity_index;
mod entity_update_apply;
mod equation_numbering;
mod nav_diagnostics;
mod outline;
mod page_order;
mod project_index;
mod section_numbering;

pub use document_index::analyze;
// `normalize_path` lives in `rinx_ast`, which is the only crate both
// its callers — toctree entry resolution and image URI resolution — are
// allowed to share; re-exported here so the worker's existing import keeps
// working.
pub use nav_diagnostics::DocumentDiagnostics;
pub use project_index::{ProjectIndexBuild, build_project_index, build_project_index_reporting};
pub use rinx_toctree::normalize_path;
