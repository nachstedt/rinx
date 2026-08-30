//! The two *data* table directives — `.. list-table::` and `.. csv-table::`.
//!
//! Both spell a table out as data plus options rather than as character art,
//! and both lower to the same [`rusty_sphinx_ast::Directive::DataTable`]. Only
//! the way their rows are written differs, so the tree splits along exactly
//! that line:
//!
//! - [`options`] scans the leading `:option:` lines off a directive body and
//!   consumes the seven options the two directives share; anything it does not
//!   recognize is handed back for the caller to accept or diagnose.
//! - [`widths`] resolves the `:widths:` option, which both use and which needs
//!   the table's column count before it can be validated.
//! - [`list`] parses the nested-bullet-list row syntax.
//! - [`csv`] parses the CSV row syntax, with [`csv_dialect`] translating
//!   `:delim:`/`:quote:`/`:escape:`/`:keepspace:` into a reader configuration.

mod csv;
mod csv_dialect;
mod list;
mod options;
mod widths;

pub(super) use csv::parse_csv_table;
pub(super) use list::parse_list_table;
