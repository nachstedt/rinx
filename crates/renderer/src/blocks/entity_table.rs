//! `.. entity-table::` / `.. needtable::` — a table of the entities matching
//! a filter.
//!
//! The one block renderer whose content comes from *other documents*. Every
//! other one draws what its node carries; this one asks the project index a
//! question the node was written to ask, which is why it is a render-time
//! construct at all: the entities it lists are declared in documents the
//! parser of this one never saw.
//!
//! Three modules:
//!
//! - [`subject`] — how a field name resolves against one entity record, which
//!   is the half of the field vocabulary `rusty_sphinx_entity` cannot own.
//! - [`rows`] — selecting, ordering and reducing the index to cells.
//! - [`html`] — the table markup, over the same shell every other table
//!   directive uses.

mod html;
mod rows;
mod subject;

pub(super) use html::render_entity_table;
