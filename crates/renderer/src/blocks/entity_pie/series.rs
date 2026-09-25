//! Turning a chart's wedges into counts, against the project index.
//!
//! Split from the markup for the reason `entity_table/rows.rs` is split from
//! its own: selecting is where every decision is, and it is testable without
//! reading a byte of SVG. The counting itself is [`count_values`], shared with
//! `.. entity-bar::`'s cells; this module only pairs each count with its
//! wedge's label.

use rusty_sphinx_ast::{ChartValue, EntityPie};
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_index::ProjectIndex;

use crate::blocks::chart_counts::count_values;
use crate::chart::PieWedge;

/// Counts each wedge, in the order the chart's content lines were written.
pub(super) fn count_wedges(
    pie: &EntityPie,
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> Vec<PieWedge> {
    let values: Vec<&ChartValue> = pie.slices.iter().map(|slice| &slice.source).collect();
    let counts = count_values(pie.filter.as_ref(), &values, index, schema);
    pie.slices
        .iter()
        .zip(counts)
        .enumerate()
        .map(|(at, (slice, count))| PieWedge {
            label: slice.display_label(at),
            count,
        })
        .collect()
}

#[cfg(test)]
mod tests;
