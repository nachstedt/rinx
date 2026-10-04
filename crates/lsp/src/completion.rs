//! Completing a reference role's target from the workspace index.
//!
//! [`context`] reads which role the cursor is in from its line, [`doc_name`]
//! writes a document's name the way `:doc:` reads it, and [`items`] turns a
//! folder's project index into the completion items for that role.

mod context;
mod doc_name;
mod items;

pub use context::role_at;
pub use items::completion_items;
