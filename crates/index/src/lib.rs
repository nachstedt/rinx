//! The project-wide index: a global symbol table (`ProjectIndex`) built from
//! all documents in a project — cross-reference targets, document titles, the
//! toctree-derived nav tree, glossary terms, domain objects, and
//! general-index entries. Pure data, no indexing/traversal logic — see
//! `rusty_sphinx_analyzer` for how a `ProjectIndex` gets built.

mod gen_index_entry;
mod nav_entry;
mod project_index;
mod target_location;

pub use gen_index_entry::GenIndexEntry;
pub use nav_entry::NavEntry;
pub use project_index::ProjectIndex;
pub use target_location::TargetLocation;
