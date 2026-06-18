//! The parser module converts RST text into an Abstract Syntax Tree (Document).

pub(super) mod admonitions;
pub(super) mod blocks;
pub(super) mod bullet_list;
pub(super) mod directives;
pub(super) mod glossary;
pub(super) mod headings;
pub(super) mod inline;

pub use blocks::parse;
