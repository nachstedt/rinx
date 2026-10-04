//! Parsing the body of a domain-object directive (`.. py:function::`,
//! `.. c:struct::`, `.. option::`, …), once
//! [`object_type::resolve_domain_object_type`] has recognized one.
//!
//! [`object`] holds the three-way dispatch, and joins a signature wrapped
//! with a trailing backslash; each domain then owns its own object-type
//! dispatch and the helpers only it needs — [`c`] up-front signature parsing.
//! [`body`] is the one genuinely cross-domain helper: every type but
//! `py:module` parses its option block and content through it.

mod body;
mod c;
mod object;
pub(super) mod object_type;
mod py;
mod std_;

pub(super) use object::{DirectiveSignatures, parse_domain_object};
