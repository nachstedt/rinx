//! The parser module converts RST text into an Abstract Syntax Tree (Document).

pub(crate) mod blocks;
pub(crate) mod context;
pub(crate) mod diagnostics;
pub(crate) mod directives;
pub(crate) mod explicit_title;
pub(crate) mod headings;
pub(crate) mod indent;
pub(crate) mod inline;

pub use blocks::{parse, parse_with_ctx, parse_with_domain};
pub use context::{CsvFileLoader, ParseCtx, RejectCsvFiles};
