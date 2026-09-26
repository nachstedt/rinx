//! An entity as it appears *in a document*, as opposed to how its type is
//! declared.
//!
//! The split matters and runs right through the codebase: this module owns the
//! **instance** data — the id, the parsed attribute values, the section node
//! trees — while `rinx_entity` owns the **meta-model** that describes
//! what an instance may contain. It has to fall this way round, because the
//! meta-model crate depends on this one; the reverse would be a cycle.
//!
//! The practical rule: if it is written in an `.rst` file and survives into a
//! `.ast` file, it lives here. If it is written in the schema, it lives there.

mod attribute_value;
mod body;
mod id;
mod section;

pub use attribute_value::AttributeValue;
pub use body::EntityBody;
pub use id::{EntityId, EntityIdError};
pub use section::{EntitySection, SectionKind};
