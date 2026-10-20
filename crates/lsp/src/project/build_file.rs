//! Adding labels to a `rinx_library`'s `deps` in its build file (roadmap #21,
//! ADR-041).
//!
//! The edit is computed from the file's text rather than delegated to
//! buildozer, so the server runs no tool and the client gets an ordinary,
//! undoable edit. A build file is Starlark, which is lexically Python, so
//! [`rinx_pyconf::tokenize`] splits it; on top of the tokens this module finds
//! one call by its `name = "…"` argument and one list literal. Whatever it
//! cannot read as written — a computed name, `deps = COMMON + […]`, a
//! `select` — is refused rather than guessed at, since a macro's arguments
//! can mean anything.
//!
//! Comments are no tokens, so they stay where they are: every edit inserts
//! text beside the list's items, except the rewrite of a single-line list,
//! which cannot hold a comment.
//!
//! [`calls`] reads the tokens; [`deps_edit`] makes the edit, spelling and
//! ordering labels as [`labels`] says; [`package`] finds the file a label's
//! package is declared in; and [`srcs`] answers which library of a package
//! lists a file, for a library no site collects yet.

mod calls;
mod deps_edit;
mod labels;
mod package;
mod srcs;

pub use deps_edit::add_deps;
pub use package::build_file_of;
pub use srcs::library_listing;
