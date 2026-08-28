//! The parser module converts RST text into an Abstract Syntax Tree (Document).

pub(crate) mod admonitions;
pub(crate) mod blocks;
pub(crate) mod bullet_list;
pub(crate) mod definition_list;
pub(crate) mod directives;
pub(crate) mod doctest;
pub(crate) mod domains;
pub(crate) mod enumerated_list;
pub(crate) mod escapes;
pub(crate) mod glossary;
pub(crate) mod headings;
pub(crate) mod index_directive;
pub(crate) mod index_ids;
pub(crate) mod inline;
pub(crate) mod list_table;
pub(crate) mod punctuation;
pub(crate) mod simple_table;
pub(crate) mod table;
pub(crate) mod typography;

pub use blocks::{parse, parse_with_domain};
