//! Converting a sphinx-needs configuration into an entity schema.
//!
//! A sphinx-needs project declares its vocabulary — need types, fields, link
//! types — as data: `needs_*` settings in `conf.py`, or a `[needs]` table of
//! the `ubproject.toml` that `needs_from_toml` names, optionally with a
//! `schemas.json` of JSON-Schema validation rules. That converts into entity
//! schema declarations mechanically, which is what lets the language server
//! understand a sphinx-needs project with no `entities.toml` of its own, and
//! what `rinx needs_schema` writes one from.
//!
//! The pieces, in the order the conversion runs:
//!
//! - [`config`]'s [`NeedsConfig`] is the configuration, one shape for both of
//!   its sources; [`ordered_table`] keeps its tables in written order, which
//!   decides the order attributes are listed in.
//! - [`schemas_json`] reads the rules of a `schemas.json` that narrow onto
//!   one need type.
//! - [`convert`]'s [`convert_needs`] yields the loader's own [`RawSchema`],
//!   so [`crate::load_raw_schema`] checks it as it checks a written schema,
//!   plus a [`report`] of everything with no equivalent in the entity model —
//!   collected rather than dropped, since that list is as much the point as
//!   the schema.
//! - [`render`] writes the declarations as an `entities.toml`.
//!
//! All of it is pure: text and values in, declarations and a report out.
//! Reading files is the caller's job.
//!
//! [`RawSchema`]: crate::RawSchema

pub mod config;
pub mod convert;
pub mod ordered_table;
pub mod render;
pub mod report;
pub mod schemas_json;

pub use config::{
    DEFAULT_TOML_ROOT, ExtraLink, ExtraOption, FieldSchema, NEEDS_SETTINGS, NeedField, NeedLink,
    NeedStatus, NeedType, NeedsConfig, NeedsConfigError,
};
pub use convert::convert_needs;
pub use ordered_table::OrderedTable;
pub use render::render_schema_toml;
pub use report::{ReportCategory, ReportEntry};
pub use schemas_json::SchemasFile;
