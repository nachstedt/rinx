//! The directives this build implements, as data: each one's names and
//! purpose, paired with how its invocation is interpreted.
//!
//! The table is the parser's authority, not a mirror of it. A directive's
//! [`DirectiveSpec`] lives in the module that parses it (`toctree.rs`'s
//! `SPEC`, …), built from [`vocabulary`] alone, beside the `interpret_*` that
//! turns its invocation into nodes. [`registry`]'s one list pairs the two for
//! every directive; `dispatch.rs` finds a written name's entry there and calls
//! its interpreter, so no name reaches a parser without an entry, and
//! `is_builtin_directive_name` — what an entity schema may not shadow — is
//! derived from the same entries rather than kept as a list beside them.
//!
//! The dependencies run one way: [`vocabulary`] names no directive module,
//! every directive module uses it, and [`registry`] uses both.
//!
//! Two vocabularies stay outside the table, because whether a name means
//! anything depends on more than the name: a domain object or scope directive
//! is resolved against the document's default domain
//! ([`super::domains::object_type`], [`super::scope`]), and an entity type or
//! section against the project's schema. Their names are reserved all the
//! same.

mod registry;
mod vocabulary;

pub(crate) use registry::registered_directive;
pub use registry::{directive_specs, domain_directive_specs, is_builtin_directive_name};
pub use vocabulary::{Arguments, DirectiveSpec, OptionSet, OptionSpec, OptionSupport, OptionValue};
