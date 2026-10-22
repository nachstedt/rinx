//! The admonition family: `.. note::` and its siblings, the generic
//! `.. admonition::`, `.. seealso::`, and the three version changes.
//!
//! What makes them one family is where their content may begin. None of them
//! but `.. admonition::` takes an argument in docutils' sense — a version
//! change's version aside — so the text after the `::` is the first line of
//! the content, and lines indented below it continue it. Each parser's
//! module holds its directive's entry in the directive table and its
//! `interpret_*`, which decides which part of the marker line is content; `crate::directives::body::directive_content` assembles that
//! line and the body into one positioned slice; the three parsers below only
//! read options off it and parse the rest.

mod admonition;
mod seealso;
mod version_change;

pub(super) use admonition::{
    ADMONITION_SPEC, GENERIC_ADMONITION_SPEC, interpret_admonition, interpret_generic_admonition,
};
pub(super) use seealso::{SEEALSO_SPEC, interpret_seealso};
pub(super) use version_change::{VERSION_CHANGE_SPEC, interpret_version_change};
