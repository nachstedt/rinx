//! The project-wide index: a global symbol table (`ProjectIndex`) built from
//! all documents in a project — cross-reference targets, document titles, the
//! toctree graph and each document's section outline, glossary terms, domain objects,
//! numbered equations, and general-index entries. Pure data, no indexing/traversal logic — see
//! `rusty_sphinx_analyzer` for how a `ProjectIndex` gets built.

mod document_outline;
mod entity_record;
mod entity_subject;
mod entity_update_history;
mod entity_update_record;
mod equation_location;
mod gen_index_entry;
mod href;
mod project_index;
mod section_numbers;
mod target_location;

pub use document_outline::{DocumentOutline, DocumentToctree, OutlineSection};
pub use entity_record::EntityRecord;
pub use entity_subject::EntitySubject;
pub use entity_update_history::{
    AppliedFieldUpdate, AppliedRelationUpdate, AttributeFieldHistory, EntityFieldHistory,
    RelationFieldHistory,
};
pub use entity_update_record::EntityUpdateRecord;
pub use equation_location::EquationLocation;
pub use gen_index_entry::GenIndexEntry;
pub use href::{entity_anchor, relative_doc_href};
pub use project_index::{DuplicateEntityId, MergeConflicts, ProjectIndex};
pub use section_numbers::DocumentNumbers;
pub use target_location::TargetLocation;
