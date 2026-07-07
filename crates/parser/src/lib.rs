//! The parser module converts RST text into an Abstract Syntax Tree (Document).

pub(crate) mod admonitions;
pub(crate) mod blocks;
pub(crate) mod bullet_list;
pub(crate) mod definition_list;
pub(crate) mod directives;
pub(crate) mod domains;
pub(crate) mod glossary;
pub(crate) mod headings;
pub(crate) mod inline;
pub(crate) mod typography;

pub use blocks::{parse, parse_with_domain};
