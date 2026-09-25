use rusty_sphinx_ast::{BarGrid, ChartValue, EntityBar, EntityBarSource};
use rusty_sphinx_filter::parse_filter;

use super::count_grid;
use crate::blocks::chart_test_support::{index, schema};

fn filter(text: &str) -> ChartValue {
    ChartValue::Filter(Some(
        parse_filter(text).expect("expected the filter to parse"),
    ))
}

fn bar(rows: Vec<Vec<ChartValue>>) -> EntityBar {
    EntityBar::new(EntityBarSource::EntityBar, BarGrid::new(rows).unwrap())
}

#[test]
fn test_every_cell_is_counted_in_its_own_place() {
    // Given — two series by two categories
    let bar = bar(vec![
        vec![filter(r#"type == "req""#), filter(r#"type == "test""#)],
        vec![filter(r#"status == "open""#), ChartValue::Count(9)],
    ]);

    // When
    let counted = count_grid(&bar, &index(), &schema());

    // Then
    assert_eq!(counted.values, [[3, 1], [2, 9]]);
}

#[test]
fn test_the_chart_filter_narrows_every_cell() {
    // Given — REQ_3 lives outside `specs/`
    let mut bar = bar(vec![vec![filter(r#"type == "req""#)]]);
    bar.filter = Some(parse_filter(r#""specs" in docname"#).unwrap());

    // When
    let counted = count_grid(&bar, &index(), &schema());

    // Then
    assert_eq!(counted.values, [[2]]);
}

#[test]
fn test_labels_are_resolved_with_their_ordinal_fallback() {
    // Given
    let rows = vec![vec![ChartValue::Count(1), ChartValue::Count(2)]];
    let bar = EntityBar::new(
        EntityBarSource::NeedBar,
        BarGrid::new(rows)
            .unwrap()
            .with_category_labels(vec![Some("Peter".to_string())]),
    );

    // When
    let counted = count_grid(&bar, &index(), &schema());

    // Then
    assert_eq!(counted.categories, ["Peter", "2"]);
    assert_eq!(counted.series, ["1"]);
}

#[test]
fn test_an_empty_grid_counts_nothing() {
    // Given
    let bar = bar(Vec::new());

    // When
    let counted = count_grid(&bar, &index(), &schema());

    // Then
    assert!(counted.values.is_empty());
    assert!(counted.categories.is_empty());
}
