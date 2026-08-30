//! Table content and layout.
//!
//! [`row`] holds the cell/row structure every table kind (grid, simple and
//! `list-table`) parses into; [`align`] and [`list_widths`] hold the two
//! presentation options a `list-table` directive can set on it.

mod align;
mod list_widths;
mod row;

pub use align::TableAlign;
pub use list_widths::ListTableWidths;
pub use row::{TableCell, TableRow};
