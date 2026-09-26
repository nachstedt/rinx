//! A project's user-definable entity meta-model.
//!
//! An *entity* is the shape that a sphinx-needs requirement and `CPython`'s
//! `.. audit-event::` both already have: a typed, identified, attributed thing
//! that can be referenced from prose and can point at other such things. This
//! crate holds the declaration of that shape — the schema a project writes —
//! and the validation every later phase performs against it.
//!
//! Four kinds of declaration answer four different questions, and keeping them
//! distinct is the heart of the model:
//!
//! - [`attribute`] — **values**: typed, validated, stored in the project
//!   index, filterable, and never parsed as RST.
//! - [`section`] — **documents**: fully-parsed RST, neither indexed nor
//!   filterable. This is what makes a requirement's verification criteria a
//!   real piece of prose rather than a string.
//! - [`relation`] — **edges** to other entities, declared on the type that
//!   carries them, with the back-links they imply derived in [`backlinks`].
//! - [`role`] — how prose *points at* an entity. Optional sugar: every entity
//!   is reachable by `:ref:` regardless, and a role adds a type check plus the
//!   spelling an existing project already writes.
//!
//! Its own crate, parallel to `rinx_scope` and `rinx_toctree`,
//! because the phases that need it are forbidden to depend on each other: the
//! parser reads the vocabulary, the analyzer indexes against it, the renderer
//! presents it, and the worker loads it. The renderer's `SiteConfig` could not
//! host it — the analyzer is not allowed to depend on the renderer.
//!
//! [`EntitySchema`] is the entry point, built only by [`load`] or
//! [`EntitySchema::empty`], so a schema in hand has already been checked.
//!
//! The *instance* types an entity turns into — `EntityId`, `AttributeValue`,
//! `EntityBody`, `EntitySection` — live in `rinx_ast` instead, and are
//! deliberately not re-exported here: they are written in an `.rst` file and
//! survive into a `.ast` file, so the AST owns them. This crate owns only what
//! the schema declares.

pub mod argument;
pub mod attribute;
pub mod backlinks;
pub mod entity_type;
pub mod error;
pub mod field;
pub mod id;
pub mod json_schema;
pub mod load;
pub mod needs_json;
pub mod pattern;
pub mod relation;
pub mod role;
pub mod schema;
pub mod section;

pub use argument::{ArgumentError, ArgumentSpec, ArgumentSplit};
pub use attribute::{
    AttributeParseError, AttributeSchema, AttributeType, parse_attribute_value, split_list,
};
pub use backlinks::{BacklinkSource, BacklinkSpec, BacklinkTable, derive_backlinks};
pub use entity_type::{EntityType, ID_OPTION};
pub use error::{DeclarationKind, SchemaError, SchemaErrors};
pub use field::{BUILTIN_FIELDS, is_builtin_field};
pub use id::{IdContext, IdDerivationError, IdPatternMismatch, IdSpec};
pub use json_schema::{SCHEMA_PATH, entity_json_schema, entity_json_schema_text};
pub use load::{NoReservedNames, ReservedDirectiveNames, load_schema};
pub use needs_json::{
    INTERNAL_FIELDS, NeedsFile, NeedsVersion, RawNeed, VersionError, field_is_list, field_is_null,
    field_text, is_internal_field, read_needs_json,
};
pub use pattern::ValuePattern;
pub use relation::RelationSpec;
pub use role::{BUILTIN_ROLE, RoleSpec};
pub use schema::EntitySchema;
pub use section::SectionSpec;
