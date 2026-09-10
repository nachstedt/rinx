//! The vocabulary a listing directive over the entity graph is written in.
//!
//! Two modules:
//!
//! - [`table`] — the node itself: the filter it selects with, the columns it
//!   shows and the presentation options it shares with every other table.
//! - [`source`] — which of the directive's two names was written.
//!
//! Unlike the entity *instances* in [`crate::entity`], nothing here is data an
//! author wrote about their project: a listing directive is a question asked of
//! the graph, so what survives into the `.ast` is the question, already parsed.

mod source;
mod table;

pub use source::EntityTableSource;
pub use table::EntityTable;
