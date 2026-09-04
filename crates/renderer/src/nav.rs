//! Rendering navigation: what a `.. toctree::` displays in a page's body, and
//! what the sidebar shows.
//!
//! Flat at the crate root, beside `resolution`, because both `blocks` (the
//! in-page toctree) and `page` (the sidebar) reach it — putting it under
//! either would misstate the dependency.
//!
//! [`expand`] walks the toctree graph in the index into a tree of entries,
//! applying `:maxdepth:`, `:titlesonly:` and `:includehidden:` and breaking
//! cycles; [`resolve`] turns one document or section into the title and href
//! an entry shows; [`secnumber`] looks up `:numbered:` numbers; [`entry`]
//! holds the resulting [`ResolvedNavEntry`]; and [`html`] writes it as the
//! `<ul>` an in-page toctree renders to.
//!
//! The sidebar and the in-page toctree share every one of those except
//! [`html`] — the sidebar hands its entries to the page template instead. That
//! sharing is the point: the two used to resolve titles from different places
//! and could label the same document differently on one page.

mod entry;
mod expand;
mod html;
mod relations;
mod resolve;
mod secnumber;

pub use entry::ResolvedNavEntry;

pub(crate) use entry::format_secnumber;
pub use relations::{PageLink, page_neighbors};

pub(crate) use expand::{expand_toctree_entries, sidebar_entries};
pub(crate) use html::write_nav_list;
