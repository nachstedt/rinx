//! `.. entity-table::` / `.. needtable::` — a table of the entities matching
//! a filter.
//!
//! The one block renderer whose content comes from *other documents*. Every
//! other one draws what its node carries; this one asks the project index a
//! question the node was written to ask, which is why it is a render-time
//! construct at all: the entities it lists are declared in documents the
//! parser of this one never saw.
//!
//! Two modules:
//!
//! - [`rows`] — selecting, ordering and reducing the index to cells.
//! - [`html`] — the table markup, over the same shell every other table
//!   directive uses.
//!
//! How a field *name* resolves against one entity is
//! `rinx_index::EntitySubject`, which used to live here. It moved when
//! a diagram template's `filter()` started resolving the same names: a filter
//! must mean one thing across the whole build, and two copies of these rules
//! is the only way it could come to mean two.

mod html;
mod rows;

pub(super) use html::render_entity_table;
