//! The vocabulary a flowchart over the entity graph is written in.
//!
//! Three modules:
//!
//! - [`flow`] — the node itself: the filter selecting what to draw, which
//!   relations become edges, and the presentation options every diagram
//!   shares.
//! - [`source`] — which of the directive's two names was written.
//! - [`direction`] — the two layout directions `PlantUML` actually has.
//!
//! Like [`crate::entity_table`] and unlike [`crate::uml`], the node carries a
//! *question* and nothing an author wrote: a flowchart's `PlantUML` text is
//! **generated** from the entities matching its filter, which live in
//! documents the one holding the directive has never heard of. The text is
//! therefore produced after indexing, by the same expander a written diagram
//! goes through, and hashed into the same `_images/<hash>.svg` population — a
//! second population would be a second thing that can drift out of step with
//! what the build compiles.

mod direction;
mod flow;
mod source;

pub use direction::{FlowDirection, InvalidFlowDirection};
pub use flow::EntityFlow;
pub use source::EntityFlowSource;
