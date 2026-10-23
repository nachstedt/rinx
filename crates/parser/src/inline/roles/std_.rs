//! Per-role-type handlers for the `std`-domain inline roles that name a
//! target of their own kind: `:ref:` a label, `:term:` a glossary term,
//! `:option:` a program's option, and `:program:` a program, which it only
//! shows. Named with a trailing underscore to avoid shadowing the `std` crate.
//! Dispatched to from [`crate::inline::dispatch::handle_inline_match`].

pub(crate) mod option;
pub(crate) mod program;
pub(crate) mod ref_;
pub(crate) mod term;
