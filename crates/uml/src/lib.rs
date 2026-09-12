//! Turning a diagram's template into the `PlantUML` text that gets compiled.
//!
//! The renderer is the only caller. It expands each diagram once, links the
//! page's `<img>` to the hash of the result, and hands the text back for the
//! render subcommand to write as `<hash>.puml` — so the picture a page names and
//! the file compiled for it come out of one call, in one process.
//!
//! It is its own crate anyway, rather than a module of the renderer, because it
//! owns a different kind of work: evaluating a template against an owned
//! snapshot of the entity graph, with none of the HTML vocabulary around it.
//! (It began life with two callers; a separate expansion action wrote the
//! `.puml` files. That action repeated work the render already did and cost
//! every document of every project an extra process, so it was removed — see
//! `docs/decisions/012-entity-diagrams.md`.)
//!
//! Determinism is still load-bearing: the hash *is* the filename, and an
//! unchanged diagram must hash the same on the next build, or its compile
//! action misses the cache and a JVM starts for nothing.
//!
//! ## Why expansion cannot happen earlier
//!
//! A templated diagram asks the entity graph questions, and the entities it
//! asks about live in documents the one holding the diagram has never heard
//! of. Only [`ProjectIndex`](rusty_sphinx_index::ProjectIndex) knows them all,
//! and it does not exist until every document has been parsed. So diagram
//! compilation happens after indexing, at the site level — which is why a
//! `rusty_sphinx_library` no longer compiles diagrams of its own.

mod context;
mod error;
mod expand;
mod snapshot;
mod template;

pub use context::UmlContext;
pub use error::UmlError;
pub use expand::expand;
