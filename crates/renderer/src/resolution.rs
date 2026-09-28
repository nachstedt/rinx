//! Cross-reference resolution against the [`rinx_index::ProjectIndex`].
//!
//! [`domain_object`] resolves domain-object roles (`:py:func:`, `:c:struct:`,
//! …) through the scope tiers and object-type aliasing Sphinx's domains
//! define; [`option`] resolves `:option:` roles, whose search has neither (see
//! its doc comment); [`document`] resolves a document name, for `:doc:` and
//! `:any:` alike; [`any`] resolves `:any:` by asking all three, and the index,
//! about every kind of target at once. The first two are built once per page,
//! and all are read from [`crate::blocks`] and [`crate::inline`] alike, which
//! is why they live here rather than inside either tree.

mod any;
mod document;
mod domain_object;
mod entity;
mod external;
mod option;

pub(crate) use any::{AnyHit, AnyResolution, AnyResolver};
pub(crate) use document::resolve_document;
pub(crate) use domain_object::{DomainObjectResolution, DomainObjectResolver};
pub(crate) use entity::{EntityResolution, EntityResolver};
pub(crate) use external::{
    ExternalHit, external_entry_types, resolve_external, resolve_external_any, unresolved_kind,
};
pub(crate) use option::{OptionResolution, OptionResolver};
