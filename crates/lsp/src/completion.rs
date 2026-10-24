//! Completing a role's target from the workspace index, a directive's name,
//! option or option value from the parser's specs, and a role's name from
//! its role table.
//!
//! [`context`] reads which role's target the cursor is in from its line,
//! [`doc_name`] writes a document's name the way `:doc:` reads it, and
//! [`targets`] turns a project index into the targets that role may name,
//! each checked by the resolver the page is rendered with.
//! [`directive_context`] reads which directive name, option or value the
//! cursor is typing from the lines around it, and [`directive_items`] turns
//! the directive specs into the items for it — for a relation's value, the
//! project's entity ids from [`entity_id_items`]. [`role_name_context`] reads
//! which role name the cursor is typing, and [`role_name_items`] turns the
//! parser's role table, the schema's roles and the document's own into the
//! items for it.

mod context;
mod directive_context;
mod directive_items;
mod doc_name;
mod entity_id_items;
mod role_name_context;
mod role_name_items;
mod targets;

pub use context::{RoleContext, WrittenRole, role_at};
pub use directive_context::{DirectiveContext, directive_at, identity_marker_line};
pub use directive_items::{DirectiveVocabulary, TextFormat, directive_items, relation_target_at};
pub use role_name_context::role_name_at;
pub use role_name_items::{RoleVocabulary, role_name_items};
pub use targets::{PLACEHOLDER, TargetQuestion, is_scoped, probe_text, target_items};
