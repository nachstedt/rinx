//! Object-type tags: the lightweight `domain:objtype` labels shared by
//! domain-object definitions and the cross-reference roles that point at them.
//!
//! [`py`], [`c`] and [`std_`] each enumerate one domain's own object types;
//! [`combined`] holds [`ObjectType`], the umbrella enum over all three, and
//! the role-alias and domain-dispatch logic that only makes sense once the
//! three are seen together.

mod c;
mod combined;
mod py;
mod std_;

pub use c::CObjectType;
pub use combined::ObjectType;
pub use py::PyObjectType;
pub use std_::StdObjectType;
