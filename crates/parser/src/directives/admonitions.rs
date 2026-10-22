//! The admonition family: `.. note::` and its siblings, the generic
//! `.. admonition::`, `.. seealso::`, and the three version changes.
//!
//! What makes them one family is where their content may begin. None of them
//! but `.. admonition::` takes an argument in docutils' sense — a version
//! change's version aside — so docutils reads the text after the `::` as the
//! first line of the content, and lines indented below it continue it. The
//! reader (`super::reader`) splits every block that way from the shape in
//! each module's directive table entry; the parsers below only read the
//! options and parse the content they are handed.

mod admonition;
mod seealso;
mod version_change;

pub(super) use admonition::{
    ADMONITION_SPEC, GENERIC_ADMONITION_SPEC, interpret_admonition, interpret_generic_admonition,
};
pub(super) use seealso::{SEEALSO_SPEC, interpret_seealso};
pub(super) use version_change::{VERSION_CHANGE_SPEC, interpret_version_change};
