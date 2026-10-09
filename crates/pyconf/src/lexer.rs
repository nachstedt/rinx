//! Python's lexical grammar, complete enough to find where every token of
//! any valid module ends.
//!
//! [`tokenize`](tokenize::tokenize) drives a [`cursor::Cursor`] over the
//! source and turns it into tokens, logical lines and indentation;
//! [`string`] scans the one part of the grammar that is genuinely nested:
//! string literals, whose f-string fields may hold further strings.

mod cursor;
mod string;
mod tokenize;

pub use tokenize::{Tokenized, tokenize};
