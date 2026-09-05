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
pub(super) use image_directive::parse_image_directive;
