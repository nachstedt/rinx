//! Table content and layout.
//!
//! [`row`] holds the cell/row structure every table kind (grid, simple,
//! `list-table` and `csv-table`) parses into; [`align`] and [`widths`] hold
//! the two presentation options the option-bearing table directives can set
//! on it, and [`source`] records which of those two directives a given table
//! came from.

mod align;
mod row;
mod source;
mod widths;

pub use align::TableAlign;
pub use row::{TableCell, TableRow};
pub use source::TableSource;
pub use widths::TableWidths;
