//! The project-wide index: a global symbol table (`ProjectIndex`) built from
//! all documents in a project — cross-reference targets, document titles, the
//! toctree graph and each document's section outline, glossary terms, domain objects,
//! numbered equations, general-index entries, and the other sites' inventories
//! it links into. Pure data, no indexing/traversal logic — see
//! `rinx_analyzer` for how a `ProjectIndex` gets built.

mod document_outline;
mod element_numbering;
mod entity_record;
mod entity_subject;
mod entity_update_history;
mod entity_update_record;
mod equation_location;
mod external_inventory;
mod gen_index_entry;
mod href;
mod module_entry;
mod project_index;
mod section_numbers;
mod target_location;

pub use document_outline::{DocumentOutline, DocumentToctree, OutlineSection};
pub use element_numbering::{
    DEFAULT_NUMFIG_SECNUM_DEPTH, ElementNumbers, NumberingStep, NumrefSubject, NumrefTarget,
    join_number,
};
pub use entity_record::EntityRecord;
pub use entity_subject::EntitySubject;
pub use entity_update_history::{
    AppliedFieldUpdate, AppliedRelationUpdate, AttributeFieldHistory, EntityFieldHistory,
    RelationFieldHistory,
};
pub use entity_update_record::EntityUpdateRecord;
pub use equation_location::EquationLocation;
pub use external_inventory::{ExternalInventory, ExternalTarget};
pub use gen_index_entry::GenIndexEntry;
pub use href::{entity_anchor, relative_doc_href};
pub use module_entry::ModuleEntry;
pub use project_index::{DuplicateEntityId, MergeConflicts, ProjectIndex};
pub use section_numbers::DocumentNumbers;
pub use target_location::TargetLocation;
