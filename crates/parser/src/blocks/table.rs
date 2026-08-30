//! RST *grid tables* — the `+---+---+` / `|` ASCII-art syntax, as opposed to
//! the whitespace-column form handled by [`super::simple_table`].
//!
//! [`grid`] resolves the character art into rows; [`geometry`] holds the
//! column-boundary, divider and cell-extraction maths it is phrased in.

mod geometry;
mod grid;

pub(crate) use geometry::{NormalizedCell, normalize_cell_lines};
pub(super) use grid::try_parse_grid_table;
