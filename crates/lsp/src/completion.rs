//! Completing a reference role's target from the workspace index, and a
//! directive's name, option or option value from the parser's specs.
//!
//! [`context`] reads which role the cursor is in from its line, [`doc_name`]
//! writes a document's name the way `:doc:` reads it, and [`items`] turns a
//! folder's project index into the completion items for that role.
//! [`directive_context`] reads which directive name, option or value the
//! cursor is typing from the lines around it, and [`directive_items`] turns
//! the directive specs into the items for it. [`role_name_context`] reads
//! which role name the cursor is typing, and [`role_name_items`] turns the
//! parser's role table, the schema's roles and the document's own into the
//! items for it.

mod context;
mod directive_context;
mod directive_items;
mod doc_name;
mod items;
mod role_name_context;
mod role_name_items;

pub use context::role_at;
pub use directive_context::directive_at;
pub use directive_items::{DirectiveVocabulary, TextFormat, directive_items};
pub use items::completion_items;
pub use role_name_context::role_name_at;
pub use role_name_items::{RoleVocabulary, role_name_items};
