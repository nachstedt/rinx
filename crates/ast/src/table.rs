use serde::{Deserialize, Serialize};

use crate::node::Node;

/// A single cell in a grid table, possibly spanning multiple grid columns/rows.
///
/// Only span-starting cells are represented (mirroring HTML's `colspan`/
/// `rowspan` model) — there is no placeholder entry for grid positions
/// covered by another cell's span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCell {
    /// Number of grid columns this cell spans (always >= 1).
    pub colspan: usize,
    /// Number of grid rows this cell spans (always >= 1).
    pub rowspan: usize,
    /// The cell's content, parsed as block-level RST nodes — a grid table
    /// cell is "a miniature document" per the RST spec.
    pub content: Vec<Node>,
}

/// A single row of a grid table, holding only its span-starting cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}
