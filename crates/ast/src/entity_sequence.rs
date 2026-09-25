//! The vocabulary a sequence diagram over the entity graph is written in.
//!
//! Two modules:
//!
//! - [`sequence`] — the node itself: where the walk starts, which relations
//!   carry messages, which receivers to keep, and the presentation options
//!   every diagram shares.
//! - [`source`] — which of the directive's two names was written.
//!
//! Like [`crate::entity_flow`], the node carries a *question* and nothing an
//! author wrote reaches `PlantUML`: the participants and messages are entities
//! declared in documents the one holding the directive has never heard of, so
//! the text is generated after indexing and hashed into the same
//! `_images/<hash>.svg` population every other diagram is.

mod sequence;
mod source;

pub use sequence::EntitySequence;
pub use source::EntitySequenceSource;
