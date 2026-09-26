use rinx_ast::{BarArrangement, BarOrientation, BarValueLabels, ChartColor};

use super::*;

fn names(labels: &[&str]) -> Vec<String> {
    labels.iter().map(|label| (*label).to_string()).collect()
}

/// Draws a chart of `values` in `colors`, after `adjust` has had its say about
/// the rest.
fn drawn_in(
    values: &[Vec<u64>],
    colors: &[ChartColor],
    adjust: impl FnOnce(&mut BarSpec<'_>),
) -> Option<String> {
    let series = names(&["Reqs", "Tests"][..values.len().min(2)]);
    let series: Vec<String> = (0..values.len())
        .map(|at| series.get(at).cloned().unwrap_or_else(|| format!("S{at}")))
        .collect();
    let categories: Vec<String> = (0..values.first().map_or(0, Vec::len))
        .map(|at| format!("Author {at}"))
        .collect();
    let mut spec = BarSpec {
        series: &series,
        categories: &categories,
        values,
        arrangement: BarArrangement::Grouped,
        orientation: BarOrientation::Vertical,
        value_labels: BarValueLabels::default(),
        legend: false,
        colors,
        text_color: None,
        x_axis_title: None,
        y_axis_title: None,
        xlabels_rotation: 0,
        ylabels_rotation: 0,
        sum_rotation: 0,
    };
    adjust(&mut spec);
    render_bar_svg(&spec)
}

/// Draws a chart of `values` in the default palette.
fn drawn(values: &[Vec<u64>], adjust: impl FnOnce(&mut BarSpec<'_>)) -> Option<String> {
    drawn_in(values, &[], adjust)
}

fn chart() -> Vec<Vec<u64>> {
    vec![vec![3, 5], vec![1, 0]]
}

#[test]
fn test_a_chart_is_drawn_as_an_svg_element() {
    // Given
    let values = chart();

    // When
    let svg = drawn(&values, |_| {}).expect("expected a chart");

    // Then
    assert!(svg.trim_start().starts_with("<svg"), "{svg}");
    assert!(svg.trim_end().ends_with("</svg>"), "{svg}");
}

#[test]
fn test_the_same_counts_draw_the_same_bytes() {
    // Given — a rendered page is a build artefact cached on its inputs
    let values = chart();

    // When
    let first = drawn(&values, |_| {});
    let second = drawn(&values, |_| {});

    // Then
    assert_eq!(first, second);
}

#[test]
fn test_every_nonzero_value_is_one_bar() {
    // Given — three of the four values are nonzero; the palette's first two
    // colours tell the series apart
    let values = chart();

    // When
    let svg = drawn(&values, |_| {}).unwrap();

    // Then
    assert_eq!(svg.matches("fill=\"#4C72B0\"").count(), 2, "{svg}");
    assert_eq!(svg.matches("fill=\"#DD852C\"").count(), 1, "{svg}");
}

#[test]
fn test_a_chart_whose_every_value_is_zero_is_not_drawn() {
    // Given
    let values = vec![vec![0, 0]];

    // When
    let svg = drawn(&values, |_| {});

    // Then
    assert_eq!(svg, None);
}

#[test]
fn test_a_chart_with_no_cells_is_not_drawn() {
    // Given
    let values: Vec<Vec<u64>> = vec![vec![]];

    // When
    let svg = drawn(&values, |_| {});

    // Then
    assert_eq!(svg, None);
}

#[test]
fn test_each_category_is_named_under_its_tick() {
    // Given
    let values = chart();

    // When
    let svg = drawn(&values, |_| {}).unwrap();

    // Then
    assert!(svg.contains(">Author 0</text>"), "{svg}");
    assert!(svg.contains(">Author 1</text>"), "{svg}");
}

#[test]
fn test_the_value_axis_is_labelled_in_whole_steps() {
    // Given — the tallest bar is 5, so the axis runs to 6 in steps of 1
    let values = chart();

    // When
    let svg = drawn(&values, |_| {}).unwrap();

    // Then
    assert!(svg.contains(">6</text>"), "{svg}");
    assert!(!svg.contains(">7</text>"), "{svg}");
}

#[test]
fn test_turned_category_labels_are_rotated_by_the_exact_angle() {
    // Given — the angle plotters cannot draw
    let values = chart();

    // When
    let svg = drawn(&values, |spec| spec.xlabels_rotation = 45).unwrap();

    // Then
    assert!(svg.contains("rotate(-45, "), "{svg}");
}

#[test]
fn test_values_inside_the_bars_are_written_when_asked_for() {
    // Given
    let values = vec![vec![13]];

    // When
    let plain = drawn(&values, |_| {}).unwrap();
    let labelled = drawn(&values, |spec| spec.value_labels.inside = true).unwrap();

    // Then — the axis counts in twos, so a 13 can only be the bar's own value
    assert!(!plain.contains(">13</text>"), "{plain}");
    assert!(labelled.contains(">13</text>"), "{labelled}");
}

#[test]
fn test_a_stack_is_topped_with_its_total() {
    // Given — 3 + 1 in the first category, and no bar anywhere is 4
    let values = chart();

    // When
    let svg = drawn(&values, |spec| {
        spec.arrangement = BarArrangement::Stacked;
        spec.value_labels.at_end = true;
    })
    .unwrap();

    // Then
    assert!(svg.contains(">4</text>"), "{svg}");
}

#[test]
fn test_side_by_side_each_bar_is_topped_with_its_own_value() {
    // Given
    let values = chart();

    // When
    let svg = drawn(&values, |spec| spec.value_labels.at_end = true).unwrap();

    // Then — the zero is written too, as matplotlib's `bar_label` writes it
    assert_eq!(svg.matches(">0</text>").count(), 2, "{svg}");
}

#[test]
fn test_a_horizontal_chart_differs_from_a_vertical_one() {
    // Given
    let values = chart();

    // When
    let vertical = drawn(&values, |_| {}).unwrap();
    let horizontal = drawn(&values, |spec| {
        spec.orientation = BarOrientation::Horizontal;
    })
    .unwrap();

    // Then
    assert_ne!(vertical, horizontal);
}

#[test]
fn test_the_legend_names_each_series() {
    // Given
    let values = chart();

    // When
    let svg = drawn(&values, |spec| spec.legend = true).unwrap();

    // Then
    assert!(svg.contains(">Reqs</text>"), "{svg}");
    assert!(svg.contains(">Tests</text>"), "{svg}");
}

#[test]
fn test_axis_titles_are_written() {
    // Given
    let values = chart();

    // When
    let svg = drawn(&values, |spec| {
        spec.x_axis_title = Some("Author");
        spec.y_axis_title = Some("Count");
    })
    .unwrap();

    // Then
    assert!(svg.contains(">Author</text>"), "{svg}");
    assert!(svg.contains(">Count</text>"), "{svg}");
    assert!(svg.contains("rotate(-90, "), "{svg}");
}

#[test]
fn test_the_written_colours_and_text_colour_are_the_ones_drawn() {
    // Given
    let values = chart();
    let colors = [ChartColor::parse("#123456").unwrap()];

    // When
    let svg = drawn_in(&values, &colors, |spec| {
        spec.text_color = Some(ChartColor::parse("#abcdef").unwrap());
    })
    .unwrap();

    // Then
    assert!(svg.contains("#123456"), "{svg}");
    assert!(svg.contains("fill=\"#ABCDEF\""), "{svg}");
}

#[test]
fn test_a_very_long_label_crowds_the_plot_rather_than_failing() {
    // Given
    let long = "x".repeat(500);
    let values = vec![vec![1]];
    let categories = vec![long];
    let series = names(&["Reqs"]);

    // When
    let svg = render_bar_svg(&BarSpec {
        series: &series,
        categories: &categories,
        values: &values,
        arrangement: BarArrangement::Grouped,
        orientation: BarOrientation::Horizontal,
        value_labels: BarValueLabels::default(),
        legend: true,
        colors: &[],
        text_color: None,
        x_axis_title: None,
        y_axis_title: None,
        xlabels_rotation: 0,
        ylabels_rotation: 0,
        sum_rotation: 0,
    });

    // Then
    assert!(svg.is_some());
}

#[test]
fn test_a_series_past_the_written_colours_is_drawn_from_the_palette() {
    // Given — one written colour for two series
    let values = chart();
    let colors = [ChartColor::parse("#123456").unwrap()];

    // When
    let svg = drawn_in(&values, &colors, |_| {}).unwrap();

    // Then — the second series takes the palette's first colour
    assert!(svg.contains("fill=\"#4C72B0\""), "{svg}");
}
