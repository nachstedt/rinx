//! The two *data* table directives — `.. list-table::` and `.. csv-table::`.
//!
//! Both spell a table out as data plus options rather than as character art,
//! and both lower to the same [`rinx_ast::Directive::DataTable`]. Only
//! the way their rows are written differs, so the tree splits along exactly
//! that line:
//!
//! - [`options`] layers the two options the two directives share beyond the
//!   five every option-bearing table directive has — those five (and the
//!   `:widths:` resolution both use) live in `crate::directives::table_options`
//!   and `crate::directives::table_widths` instead of here, since `.. table::`
//!   needs them too and produces no `Directive::DataTable` at all.
//! - [`list`] parses the nested-bullet-list row syntax.
//! - [`csv`] parses the CSV row syntax, with [`csv_dialect`] translating
//!   `:delim:`/`:quote:`/`:escape:`/`:keepspace:` into a reader configuration.

mod csv;
mod csv_dialect;
mod list;
mod options;

pub(super) use csv::parse_csv_table;
pub(super) use list::parse_list_table;
