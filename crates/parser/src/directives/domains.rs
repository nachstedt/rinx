//! Parsing the body of a domain-object directive (`.. py:function::`,
//! `.. c:struct::`, `.. option::`, …), once
//! [`object_type::resolve_domain_object_type`] has recognized one.
//!
//! [`object`] holds the three-way dispatch; each domain then owns its own
//! object-type dispatch and the helpers only it needs — [`py`] the `:module:`
//! option line, [`c`] up-front signature parsing and the shared
//! object-description flags. [`body`] is the one genuinely cross-domain
//! helper, used wherever a type has no options of its own to strip first.

mod body;
mod c;
mod object;
pub(super) mod object_type;
mod py;
mod std_;

pub(super) use object::parse_domain_object;
