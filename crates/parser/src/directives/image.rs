//! The two image directives, `.. image::` and `.. figure::`.
//!
//! [`options`] holds the nine options they spell identically, layered on the
//! generic `:name: value` scan the way `directives::table_options` layers the
//! table directives' shared five. [`image_directive`] and [`figure`] then add
//! what is particular to each: an `.. image::` is an argument and nothing
//! else, while a `.. figure::` adds two options of its own and a body that
//! splits into a caption and a legend.

mod figure;
mod image_directive;
mod options;

pub(super) use figure::parse_figure_directive;
pub(super) use image_directive::{parse_image_directive, report_option_conflicts};
// Reused by `super::substitution` to parse a `.. |name| image::` definition
// with the same option vocabulary, gated to the substitution-only narrowings
// by `ImageContext::Substitution`.
pub(super) use options::{ImageContext, parse_common_image_options};
