//! The parser module converts RST text into an Abstract Syntax Tree (Document).

pub(crate) mod blocks;
pub(crate) mod directives;
pub(crate) mod headings;
pub(crate) mod indent;
pub(crate) mod inline;

pub use blocks::{parse, parse_with_domain};
