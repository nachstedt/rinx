//! Cross-reference resolution against the [`rusty_sphinx_index::ProjectIndex`].
//!
//! [`domain_object`] resolves domain-object roles (`:py:func:`, `:c:struct:`,
//! …) through the scope tiers and object-type aliasing Sphinx's domains
//! define; [`option`] resolves `:option:` roles, whose search has neither (see
//! its doc comment). Both are built once per page and read from
//! [`crate::blocks`] and [`crate::inline`] alike, which is why they live here
//! rather than inside either tree.

mod domain_object;
mod option;

pub(crate) use domain_object::{DomainObjectResolution, DomainObjectResolver};
pub(crate) use option::{OptionResolution, OptionResolver};
