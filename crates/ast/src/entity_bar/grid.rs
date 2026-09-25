//! The two-dimensional body of a bar chart: its values, and what each row and
//! column is called.
//!
//! "Parse, don't validate": a [`BarGrid`] is always rectangular and always has
//! exactly one label slot per row and per column, so the renderer indexes it
//! without asking. What a written body does when it is *not* rectangular — and
//! sphinx-needs raises on that — is the parser's decision, made where the
//! offending line can be pointed at.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::chart_value::ChartValue;

/// A bar chart's values, one row per **series** and one column per
/// **category**.
///
/// That orientation is sphinx-needs' own: a row is one legend entry, drawn in
/// one colour, and a column is one tick along the category axis. `:transpose:`
/// is applied by the parser through [`BarGrid::transposed`], so the stored
/// grid is already in the orientation it is drawn in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BarGrid {
    /// One label slot per row; `None` where none was written.
    series: Vec<Option<String>>,
    /// One label slot per column; `None` where none was written.
    categories: Vec<Option<String>>,
    /// The values, row by row, every row as long as `categories`.
    values: Vec<Vec<ChartValue>>,
}

/// Why a set of rows could not become a grid: they were not all one length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaggedGrid {
    /// The length of the first row, which every other row is measured by.
    pub expected: usize,
    /// The first row whose length differs, by position.
    pub row: usize,
    /// That row's length.
    pub found: usize,
}

impl fmt::Display for RaggedGrid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "row {} has {} value(s) but the first row has {}",
            self.row + 1,
            self.found,
            self.expected
        )
    }
}

impl BarGrid {
    /// A grid of `values`, with no labels yet.
    ///
    /// # Errors
    ///
    /// Returns [`RaggedGrid`] naming the first row whose length differs from
    /// the first row's.
    pub fn new(values: Vec<Vec<ChartValue>>) -> Result<Self, RaggedGrid> {
        let columns = values.first().map_or(0, Vec::len);
        if let Some((row, found)) = values
            .iter()
            .enumerate()
            .find(|(_, row)| row.len() != columns)
            .map(|(at, row)| (at, row.len()))
        {
            return Err(RaggedGrid {
                expected: columns,
                row,
                found,
            });
        }
        Ok(Self {
            series: vec![None; values.len()],
            categories: vec![None; columns],
            values,
        })
    }

    /// Names the rows by position; surplus labels are dropped and a row with
    /// no label left keeps `None`.
    ///
    /// Position is the whole interface between a written label list and the
    /// body, so a length mismatch is the parser's to report — this only keeps
    /// the one-slot-per-row invariant whatever it is given.
    #[must_use]
    pub fn with_series_labels(mut self, labels: Vec<Option<String>>) -> Self {
        pair_by_position(&mut self.series, labels);
        self
    }

    /// Names the columns by position, under [`BarGrid::with_series_labels`]'
    /// rule.
    #[must_use]
    pub fn with_category_labels(mut self, labels: Vec<Option<String>>) -> Self {
        pair_by_position(&mut self.categories, labels);
        self
    }

    /// The grid with rows and columns swapped, labels included — what
    /// sphinx-needs' `:transpose:` does.
    #[must_use]
    pub fn transposed(self) -> Self {
        let values = (0..self.categories.len())
            .map(|column| self.values.iter().map(|row| row[column].clone()).collect())
            .collect();
        Self {
            series: self.categories,
            categories: self.series,
            values,
        }
    }

    /// How many series (rows) the chart draws.
    #[must_use]
    pub fn series_count(&self) -> usize {
        self.series.len()
    }

    /// How many categories (columns) the chart draws.
    #[must_use]
    pub fn category_count(&self) -> usize {
        self.categories.len()
    }

    /// The values, row by row.
    #[must_use]
    pub fn values(&self) -> &[Vec<ChartValue>] {
        &self.values
    }

    /// The label to draw for series `at`, falling back to its ordinal.
    ///
    /// The fallback is sphinx-needs' own — `1`, `2`, … — so a migrated chart
    /// without labels reads as it did there.
    #[must_use]
    pub fn display_series_label(&self, at: usize) -> String {
        display_label(&self.series, at)
    }

    /// The label to draw for category `at`, under the same fallback.
    #[must_use]
    pub fn display_category_label(&self, at: usize) -> String {
        display_label(&self.categories, at)
    }
}

/// Fills `slots` from `labels` by position, leaving the rest as they were.
fn pair_by_position(slots: &mut [Option<String>], labels: Vec<Option<String>>) {
    for (slot, label) in slots.iter_mut().zip(labels) {
        *slot = label;
    }
}

/// The written label at `at`, or its one-based ordinal.
fn display_label(labels: &[Option<String>], at: usize) -> String {
    labels
        .get(at)
        .cloned()
        .flatten()
        .unwrap_or_else(|| (at + 1).to_string())
}

impl<'de> Deserialize<'de> for BarGrid {
    /// Re-checks the shape on load, as every opaque type here does: a `.ast`
    /// is a file on disk, and nothing guarantees this build wrote it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            series: Vec<Option<String>>,
            categories: Vec<Option<String>>,
            values: Vec<Vec<ChartValue>>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let shaped = raw.series.len() == raw.values.len()
            && raw
                .values
                .iter()
                .all(|row| row.len() == raw.categories.len());
        if !shaped {
            return Err(serde::de::Error::custom(
                "a bar chart's grid must have one label slot per row and per column, \
                 and every row as long as the category list",
            ));
        }
        Ok(Self {
            series: raw.series,
            categories: raw.categories,
            values: raw.values,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(rows: &[&[u64]]) -> Vec<Vec<ChartValue>> {
        rows.iter()
            .map(|row| row.iter().map(|count| ChartValue::Count(*count)).collect())
            .collect()
    }

    fn named(labels: &[&str]) -> Vec<Option<String>> {
        labels
            .iter()
            .map(|label| Some((*label).to_string()))
            .collect()
    }

    #[test]
    fn test_a_rectangular_body_becomes_a_grid_of_that_shape() {
        // Given
        let values = counts(&[&[1, 2, 3], &[4, 5, 6]]);

        // When
        let grid = BarGrid::new(values).unwrap();

        // Then
        assert_eq!(grid.series_count(), 2);
        assert_eq!(grid.category_count(), 3);
    }

    #[test]
    fn test_a_ragged_body_is_refused_naming_the_first_bad_row() {
        // Given
        let values = counts(&[&[1, 2, 3], &[4, 5, 6], &[7]]);

        // When
        let error = BarGrid::new(values).unwrap_err();

        // Then
        assert_eq!(
            error,
            RaggedGrid {
                expected: 3,
                row: 2,
                found: 1
            }
        );
    }

    #[test]
    fn test_an_empty_body_is_an_empty_grid() {
        // Given
        let values = Vec::new();

        // When
        let grid = BarGrid::new(values).unwrap();

        // Then
        assert_eq!(grid.series_count(), 0);
        assert_eq!(grid.category_count(), 0);
    }

    #[test]
    fn test_unlabelled_rows_and_columns_fall_back_to_their_ordinals() {
        // Given
        let grid = BarGrid::new(counts(&[&[1, 2]])).unwrap();

        // When
        let labels = (grid.display_series_label(0), grid.display_category_label(1));

        // Then
        assert_eq!(labels, ("1".to_string(), "2".to_string()));
    }

    #[test]
    fn test_labels_pair_with_rows_and_columns_by_position() {
        // Given
        let grid = BarGrid::new(counts(&[&[1, 2], &[3, 4]])).unwrap();

        // When
        let grid = grid
            .with_series_labels(named(&["Reqs", "Tests"]))
            .with_category_labels(named(&["Peter", "Sarah"]));

        // Then
        assert_eq!(grid.display_series_label(1), "Tests");
        assert_eq!(grid.display_category_label(0), "Peter");
    }

    #[test]
    fn test_surplus_labels_are_dropped_and_missing_ones_fall_back() {
        // Given
        let grid = BarGrid::new(counts(&[&[1, 2, 3]])).unwrap();

        // When
        let grid = grid
            .with_series_labels(named(&["Reqs", "surplus"]))
            .with_category_labels(named(&["Peter"]));

        // Then
        assert_eq!(grid.series_count(), 1);
        assert_eq!(grid.display_category_label(0), "Peter");
        assert_eq!(grid.display_category_label(2), "3");
    }

    #[test]
    fn test_transposing_swaps_values_and_labels() {
        // Given
        let grid = BarGrid::new(counts(&[&[1, 2, 3], &[4, 5, 6]]))
            .unwrap()
            .with_series_labels(named(&["a", "b"]))
            .with_category_labels(named(&["x", "y", "z"]));

        // When
        let transposed = grid.transposed();

        // Then
        assert_eq!(transposed.values(), counts(&[&[1, 4], &[2, 5], &[3, 6]]));
        assert_eq!(transposed.display_series_label(2), "z");
        assert_eq!(transposed.display_category_label(1), "b");
    }

    #[test]
    fn test_a_grid_survives_a_serialization_round_trip() {
        // Given
        let grid = BarGrid::new(vec![vec![ChartValue::Count(1), ChartValue::Filter(None)]])
            .unwrap()
            .with_series_labels(named(&["Reqs"]));

        // When
        let json = serde_json::to_string(&grid).unwrap();
        let decoded: BarGrid = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, grid);
    }

    #[test]
    fn test_loading_a_misshapen_grid_is_refused() {
        // Given — a `.ast` whose rows disagree with its category list
        let json = r#"{"series":[null],"categories":[null,null],"values":[[{"Count":1}]]}"#;

        // When
        let decoded = serde_json::from_str::<BarGrid>(json);

        // Then
        assert!(decoded.is_err());
    }
}
