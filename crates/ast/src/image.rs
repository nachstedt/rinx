//! The vocabulary `.. image::` and `.. figure::` are written in.
//!
//! Four modules, split by the question each answers about an image:
//!
//! - [`uri`] — where its bytes come from ([`ImageUri`]) and where clicking it
//!   goes ([`ImageTarget`]), plus the one resolution function every later
//!   phase turns a document-relative path into a project path with.
//! - [`length`] — how big it is: the measurements `:height:`, `:width:` and
//!   `:figwidth:` are written in, each opaque and validated on construction
//!   because the text is re-emitted straight into CSS.
//! - [`align`] — where it sits relative to the text.
//! - [`options`] — the nine options both directives share, bundled so one
//!   parser and one renderer can serve both.
//! - [`figure`] — what `.. figure::` adds on top: its own two options and its
//!   caption/legend body.

mod align;
mod figure;
mod length;
mod options;
mod uri;

pub use align::{ImageAlign, is_vertical_name};
pub use figure::Figure;
pub use length::{
    FigureWidth, InvalidLength, Length, LengthOrPercentage, LengthUnit, Percentage, scaled_width,
};
pub use options::{ImageLoading, ImageOptions};
pub use uri::{ImageTarget, ImageUri};
