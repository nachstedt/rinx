//! The vocabulary `.. entity-update::` (and its sphinx-needs spelling,
//! `.. needextend::`) is written in.
//!
//! Four modules:
//!
//! - [`source`] — which of the directive's two names was written.
//! - [`target`] — the argument, which may resolve to a single entity's id or
//!   to a filter over the whole project. Both readings are kept: only the
//!   merged project, not this document's parser, can tell which one applies.
//! - [`mutation`] — one field mutation the directive asks for: which field,
//!   which of the four operations, and (for three of them) what value.
//! - [`update`] — the node itself, tying the above together with `:strict:`
//!   and the justification body.
//!
//! Unlike an entity *instance* (`crate::entity`), what survives into the
//! `.ast` here is a *question with intended effects*, not authored data: the
//! entities this directive touches are resolved, and its mutations applied,
//! only once the whole project is merged — see
//! `rusty_sphinx_analyzer::apply_entity_updates` and
//! `docs/decisions/019-entity-update.md`.

mod mutation;
mod source;
mod target;
mod update;

pub use mutation::{FieldMutation, FieldMutationMode};
pub use source::EntityUpdateSource;
pub use target::UpdateTarget;
pub use update::EntityUpdate;
