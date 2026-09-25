//! Turning a bar chart's grid into counts, against the project index.
//!
//! Split from the markup for the reason the pie's `series.rs` is: selecting is
//! where every decision is, and it is testable without reading a byte of SVG.

use rusty_sphinx_ast::{ChartValue, EntityBar};
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_index::ProjectIndex;

use crate::blocks::chart_counts::count_values;

/// A bar chart's grid with every cell counted and every label resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CountedGrid {
    /// One label per series (row).
    pub series: Vec<String>,
    /// One label per category (column).
    pub categories: Vec<String>,
    /// One row per series, one count per category.
    pub values: Vec<Vec<u64>>,
}

/// Counts every cell of `bar`'s grid, in one walk of the index.
///
/// The grid is flattened for [`count_values`] and folded back afterwards, so a
/// chart of a dozen filters still walks the project once.
pub(super) fn count_grid(
    bar: &EntityBar,
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> CountedGrid {
    let grid = &bar.grid;
    let cells: Vec<&ChartValue> = grid.values().iter().flatten().collect();
    let counts = count_values(bar.filter.as_ref(), &cells, index, schema);
    let values = counts
        .chunks(grid.category_count().max(1))
        .take(grid.series_count())
        .map(<[u64]>::to_vec)
        .collect();
    CountedGrid {
        series: (0..grid.series_count())
            .map(|at| grid.display_series_label(at))
            .collect(),
        categories: (0..grid.category_count())
            .map(|at| grid.display_category_label(at))
            .collect(),
        values,
    }
}

#[cfg(test)]
mod tests;
