//! Resolving a `.. toctree::`'s written entries into the documents they name.
//!
//! Its own crate rather than a module of `rusty_sphinx_analyzer` because three
//! phases need this and they are not allowed to depend on each other: the
//! analyzer builds the project index with it, the renderer walks the toctree
//! graph with it, and the worker's Bazel strict-deps validator checks
//! declared dependencies with it. Folding it into the analyzer would put the
//! renderer back to depending on the whole analyzer crate — exactly what
//! splitting `rusty_sphinx_index` out was meant to stop.
//!
//! Each of those three expands against a *different* universe of documents,
//! which is why a `:glob:` is stored unexpanded all the way into the index
//! rather than being resolved once, early: expanding at parse or index time
//! would leave the strict-deps check either reimplementing the matcher against
//! its own narrower list, or not checking globs at all.
//!
//! [`resolve`] holds the entry resolution itself; [`glob`] holds the pattern
//! matching it delegates to, and the two places that matching is deliberately
//! narrower than the library performing it.

mod glob;
mod path_normalization;
mod resolve;

pub use path_normalization::normalize_path;
pub use resolve::{
    TocTarget, UnmatchedEntry, UnmatchedKind, expand_toctree, resolve_docname, strip_rst,
};
