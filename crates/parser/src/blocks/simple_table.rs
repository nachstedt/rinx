//! Parsing for RST *simple tables* — the whitespace-column `=====  =====`
//! syntax, as opposed to the `+---+---+` ASCII art handled by
//! [`super::table`]. Both produce the same [`rusty_sphinx_ast::Node::Table`].
//!
//! The structure follows docutils' `SimpleTableParser`, including its central
//! trick: the top border, the header/body rule and the bottom border are all
//! rewritten from `=` to `-` up front, so every structural line in the table
//! is uniformly a *column-span underline* and one code path handles all of
//! them. A row's column layout always comes from the line that terminates it.
//!
//! Simple tables cannot express row spans, so every cell here has
//! `rowspan: 1`.
//!
//! [`borders`] recognizes the structural lines, [`layout`] derives the column
//! geometry from them, [`rows`] lowers a row's lines into cells, and
//! [`parse`] drives the whole walk.

mod borders;
mod layout;
mod parse;
mod rows;

pub(super) use parse::try_parse_simple_table;
