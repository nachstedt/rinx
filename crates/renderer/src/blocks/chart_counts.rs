//! Turning a chart's values into numbers, against the project index.
//!
//! The layer every chart over the entity graph shares: a pie's wedges and a
//! bar chart's cells are both [`ChartValue`]s, and both are counted here. It
//! knows nothing about wedges, bars or geometry, which is the whole shape of
//! `docs/decisions/017-entity-pie.md`: the two directives differ in
//! presentation, so the generality lives in the *counting* underneath them.
//!
//! Flat under `blocks/` rather than inside either chart's directory because
//! both of them call it.

use rinx_ast::ChartValue;
use rinx_entity::EntitySchema;
use rinx_filter::Expr;
use rinx_index::{EntitySubject, ProjectIndex};

/// Counts each value, in the order given, among the entities `scope` selects.
///
/// The index is walked **once**, not once per value: every entity is turned
/// into an [`EntitySubject`] a single time and offered to each filter in turn.
/// That matters because a chart is linear in the size of the project already,
/// and a bar chart's grid can easily hold a dozen filters.
///
/// [`EntitySubject`] is `rinx_index`'s, the very type an
/// `.. entity-table::`'s rows and a diagram's `filter()` resolve names
/// through, so a filter cannot mean one thing in a table and another in a
/// chart.
///
/// A written number is passed through untouched, and a chart made only of
/// them never reads the index at all — the one case a chart costs nothing.
pub(super) fn count_values(
    scope: Option<&Expr>,
    values: &[&ChartValue],
    index: &ProjectIndex,
    schema: &EntitySchema,
) -> Vec<u64> {
    let mut counts: Vec<u64> = values
        .iter()
        .map(|value| match value {
            ChartValue::Count(written) => *written,
            ChartValue::Filter(_) => 0,
        })
        .collect();
    if !values.iter().any(|value| value.is_counted()) {
        return counts;
    }

    for (id, record) in &index.entities {
        let subject = EntitySubject {
            id,
            record,
            index,
            schema,
        };
        if !matches_optional(scope, &subject) {
            continue;
        }
        for (count, value) in counts.iter_mut().zip(values) {
            if let ChartValue::Filter(filter) = value
                && matches_optional(filter.as_ref(), &subject)
            {
                *count += 1;
            }
        }
    }
    counts
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
mod tests {
    use rinx_filter::parse_filter;

    use super::*;
    use crate::blocks::chart_test_support::{index, schema};

    fn filter(text: &str) -> ChartValue {
        ChartValue::Filter(Some(
            parse_filter(text).expect("expected the filter to parse"),
        ))
    }

    fn count(values: &[ChartValue], scope: Option<&str>) -> Vec<u64> {
        let scope = scope.map(|text| parse_filter(text).expect("expected the scope to parse"));
        let values: Vec<&ChartValue> = values.iter().collect();
        count_values(scope.as_ref(), &values, &index(), &schema())
    }

    #[test]
    fn test_each_filter_counts_the_entities_it_selects() {
        // Given
        let values = [filter(r#"type == "req""#), filter(r#"type == "test""#)];

        // When
        let counted = count(&values, None);

        // Then
        assert_eq!(counted, [3, 1]);
    }

    #[test]
    fn test_a_written_number_is_passed_through() {
        // Given
        let values = [ChartValue::Count(9), filter(r#"type == "test""#)];

        // When
        let counted = count(&values, None);

        // Then
        assert_eq!(counted, [9, 1]);
    }

    #[test]
    fn test_the_scope_narrows_every_filter_but_not_a_written_number() {
        // Given — REQ_3 is `closed` but lives outside `specs/`
        let values = [filter(r#"status == "closed""#), ChartValue::Count(5)];

        // When
        let counted = count(&values, Some(r#""specs" in docname"#));

        // Then
        assert_eq!(counted, [0, 5]);
    }

    #[test]
    fn test_a_filter_that_could_not_be_parsed_counts_everything_in_scope() {
        // Given
        let values = [ChartValue::Filter(None)];

        // When
        let counted = count(&values, Some(r#"type == "req""#));

        // Then
        assert_eq!(counted, [3]);
    }

    #[test]
    fn test_written_numbers_alone_never_read_the_index() {
        // Given — an index with no entities, which a filter would count as 0
        let values = [ChartValue::Count(4)];

        // When
        let counted = count_values(None, &[&values[0]], &ProjectIndex::default(), &schema());

        // Then
        assert_eq!(counted, [4]);
    }
}
