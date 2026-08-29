//! The *data* table directives — tables written as data plus options rather
//! than as character art.
//!
//! `.. list-table::` lives here today, and lowers to
//! [`rusty_sphinx_ast::Directive::DataTable`]. The tree splits along the line
//! between what a directive shares with its siblings and what is its own:
//!
//! - [`options`] scans the leading `:option:` lines off a directive body and
//!   consumes the options the data-table directives share; anything it does
//!   not recognize is handed back for the caller to accept or diagnose.
//! - [`widths`] resolves the `:widths:` option, which needs the table's column
//!   count before it can be validated.
//! - [`list`] parses the nested-bullet-list row syntax.

mod list;
mod options;
mod widths;

pub(super) use list::parse_list_table;
