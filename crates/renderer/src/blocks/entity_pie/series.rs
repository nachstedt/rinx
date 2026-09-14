//! Turning a chart's wedges into counts, against the project index.
//!
//! Split from the markup for the reason `entity_table/rows.rs` is split from
//! its own: selecting is where every decision is, and it is testable without
//! reading a byte of SVG.
//!
//! This is also the piece a later `.. entity-bar::` reuses. A bar chart is the
//! same question with a second dimension — a grid of filters rather than a
//! list — so what it shares with a pie is *counting*, not geometry. Putting
//! the generality here rather than in one directive with a `:type:` switch is
//! the whole shape of `docs/decisions/017-entity-pie.md`.

use rusty_sphinx_ast::{EntityPie, SliceSource};
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_filter::Expr;
use rusty_sphinx_index::{EntitySubject, ProjectIndex};

use crate::pie_chart::PieWedge;

/// Counts each wedge, in the order the chart's content lines were written.
///
/// The index is walked **once**, not once per wedge: every entity is turned
/// into an [`EntitySubject`] a single time and offered to each filter in turn.
/// That matters because a chart is linear in the size of the project already,
/// and a project with many wedges should not multiply it.
///
/// [`EntitySubject`] is `rusty_sphinx_index`'s, the very type an
/// `.. entity-table::`'s rows and a diagram's `filter()` resolve names
/// through, so a filter cannot mean one thing in a table and another in a
/// chart.
pub(super) fn count_wedges(
    pie: &EntityPie,
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> Vec<PieWedge> {
    let mut counts = vec![0u64; pie.slices.len()];

    if pie.slices.iter().any(is_counted) {
        for (id, record) in &index.entities {
            let subject = EntitySubject {
                id,
                record,
                index,
                schema,
            };
            if !matches_optional(pie.filter.as_ref(), &subject) {
                continue;
            }
            for (at, slice) in pie.slices.iter().enumerate() {
                if let SliceSource::Filter(filter) = &slice.source
                    && matches_optional(filter.as_ref(), &subject)
                {
                    counts[at] += 1;
                }
            }
        }
    }

    pie.slices
        .iter()
        .enumerate()
        .map(|(at, slice)| PieWedge {
            label: slice.display_label(at),
            count: match &slice.source {
                SliceSource::Count(written) => *written,
                SliceSource::Filter(_) => counts[at],
            },
        })
        .collect()
}

/// Whether a wedge's size has to be counted from the index at all.
///
/// A chart whose every wedge is a written number never touches the index,
/// which is the one case a pie costs nothing to draw.
fn is_counted(slice: &rusty_sphinx_ast::PieSlice) -> bool {
    matches!(slice.source, SliceSource::Filter(_))
}

/// Whether a filter selects `subject`, where an absent filter selects it.
///
/// An absent filter is both an omitted `:filter:` and one that failed to
/// parse — the encoding the node uses, and the rule every filtered directive
/// here follows.
fn matches_optional(filter: Option<&Expr>, subject: &EntitySubject<'_>) -> bool {
    filter.is_none_or(|filter| filter.matches(subject))
}

#[cfg(test)]
mod tests;
