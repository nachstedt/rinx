//! Resolving a `.. toctree::`'s written entries into the documents they name.
//!
//! Its own crate rather than a module of `rinx_analyzer` because three
//! phases need this and they are not allowed to depend on each other: the
//! analyzer builds the project index with it, the renderer walks the toctree
//! graph with it, and the worker's Bazel strict-deps validator checks
//! declared dependencies with it. Folding it into the analyzer would put the
//! renderer back to depending on the whole analyzer crate — exactly what
//! splitting `rinx_index` out was meant to stop.
//!
//! Each of those three expands against a *different* universe of documents,
//! which is why a `:glob:` is stored unexpanded all the way into the index
//! rather than being resolved once, early: expanding at parse or index time
//! would leave the strict-deps check either reimplementing the matcher against
//! its own narrower list, or not checking globs at all.
//!
//! [`resolve`] holds the entry resolution itself; [`glob`] holds the pattern
//! matching it delegates to, and [`pattern`] Sphinx's glob translation, which
//! the language server also matches a `conf.py`'s `exclude_patterns` with. [`normalize_path`] is re-exported
//! from `rinx_ast`, where it moved once image URIs needed the same
//! `.`/`..` resolution.

mod glob;
mod pattern;
mod resolve;

pub use pattern::SphinxPattern;

// Re-exported rather than defined here: resolving `.`/`..` is a plain path
// operation with no toctree in it, and `rinx_ast` needs the same
// function to resolve a document-relative image URI. Keeping the name
// available here means every existing caller — including the analyzer's own
// re-export — is unaffected by where it lives.
pub use resolve::{
    TocTarget, UnmatchedEntry, UnmatchedKind, expand_toctree, resolve_docname, strip_rst,
};
pub use rinx_ast::normalize_path;
