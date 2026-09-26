use rinx_ast::{
    BarArrangement, BarOrientation, ChartColor, ChartValue, Directive, EntityBar, ImageAlign, Node,
};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

/// The schema every test below parses against: two types and an enum
/// attribute, enough vocabulary for every filter a cell needs.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]

        [[entity_type]]
        name = "test"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn parse(rst: &str) -> rinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("specs/report", rst, &ctx)
}

/// The first bar chart in `rst`.
fn parse_bar(rst: &str) -> EntityBar {
    let doc = parse(rst);
    doc.nodes
        .iter()
        .find_map(|node| match node {
            Node::Directive(Directive::EntityBar(bar)) => Some((**bar).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no bar chart parsed from:\n{rst}"))
}

/// The diagnostic codes a parse reported, as their author-facing ids.
fn codes(rst: &str) -> Vec<String> {
    parse(rst)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// The first diagnostic reported under `code`.
fn diagnostic(rst: &str, code: &str) -> rinx_ast::Diagnostic {
    parse(rst)
        .diagnostics
        .into_iter()
        .find(|d| d.code.as_str() == code)
        .unwrap_or_else(|| panic!("expected {code} from:\n{rst}"))
}

fn counts(rows: &[&[u64]]) -> Vec<Vec<ChartValue>> {
    rows.iter()
        .map(|row| row.iter().map(|count| ChartValue::Count(*count)).collect())
        .collect()
}

#[test]
fn test_each_content_line_is_a_series_and_each_cell_a_category() {
    // Given
    let rst = ".. entity-bar:: Budget\n\n   1, 2, 3\n   4, 5, 6\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.title.as_deref(), Some("Budget"));
    assert_eq!(bar.grid.values(), counts(&[&[1, 2, 3], &[4, 5, 6]]));
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_a_cell_that_is_not_a_number_is_a_filter() {
    // Given
    let rst = ".. entity-bar:: By type\n\n   type == \"req\", 7\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert!(matches!(
        bar.grid.values()[0][0],
        ChartValue::Filter(Some(_))
    ));
    assert_eq!(bar.grid.values()[0][1], ChartValue::Count(7));
}

#[test]
fn test_the_corpus_shape_takes_both_label_sets_from_the_data() {
    // Given — the one `.. needbar::` in the benchmark corpus, abridged
    let rst = concat!(
        ".. needbar:: Object authors\n",
        "   :legend:\n",
        "   :xlabels: FROM_DATA\n",
        "   :ylabels: FROM_DATA\n",
        "   :show_sum:\n",
        "   :show_top_sum:\n",
        "   :stacked:\n",
        "\n",
        "   , Peter, Sarah\n",
        "   SW Reqs, type == \"req\", type == \"req\" and status == \"open\"\n",
        "   Tests, type == \"test\", 3\n",
    );

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.series_count(), 2);
    assert_eq!(bar.grid.category_count(), 2);
    assert_eq!(bar.grid.display_series_label(1), "Tests");
    assert_eq!(bar.grid.display_category_label(0), "Peter");
    assert_eq!(bar.grid.values()[1][1], ChartValue::Count(3));
    assert!(bar.legend);
    assert_eq!(bar.arrangement, BarArrangement::Stacked);
    assert!(bar.value_labels.inside && bar.value_labels.at_end);
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_category_labels_from_the_data_keep_the_first_cell_without_series_labels() {
    // Given — only the header row is labels, so its first cell is one too
    let rst = ".. entity-bar::\n   :xlabels: FROM_DATA\n\n   a, b\n   1, 2\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.display_category_label(0), "a");
    assert_eq!(bar.grid.values(), counts(&[&[1, 2]]));
}

#[test]
fn test_series_labels_from_the_data_take_each_rows_first_cell() {
    // Given
    let rst = ".. entity-bar::\n   :ylabels: FROM_DATA\n\n   Reqs, 1, 2\n   Tests, 3, 4\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.display_series_label(0), "Reqs");
    assert_eq!(bar.grid.values(), counts(&[&[1, 2], &[3, 4]]));
}

#[test]
fn test_written_labels_pair_by_position() {
    // Given
    let rst = concat!(
        ".. entity-bar::\n",
        "   :xlabels: Q1, Q2\n",
        "   :ylabels: Reqs, Tests\n",
        "\n",
        "   1, 2\n",
        "   3, 4\n",
    );

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.display_category_label(1), "Q2");
    assert_eq!(bar.grid.display_series_label(1), "Tests");
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_a_label_count_that_disagrees_with_the_grid_is_reported() {
    // Given
    let rst = ".. entity-bar::\n   :xlabels: Q1, Q2, Q3\n\n   1, 2\n";

    // When
    let reported = diagnostic(rst, "entity-bar.label-count-mismatch");

    // Then — pointed at the option line, where the fix is
    assert_eq!(reported.span.unwrap().start.line, 2);
    assert_eq!(parse_bar(rst).grid.category_count(), 2);
}

#[test]
fn test_unlabelled_rows_and_columns_are_numbered_like_sphinx_needs() {
    // Given
    let rst = ".. entity-bar::\n\n   1, 2\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.display_category_label(1), "2");
    assert_eq!(bar.grid.display_series_label(0), "1");
}

#[test]
fn test_a_short_row_is_padded_with_zeros_and_reported_on_its_line() {
    // Given — sphinx-needs raises here; losing the whole chart over one short
    // row would be the larger failure
    let rst = ".. entity-bar::\n\n   1, 2, 3\n   4\n";

    // When
    let bar = parse_bar(rst);
    let reported = diagnostic(rst, "entity-bar.ragged-row");

    // Then
    assert_eq!(bar.grid.values(), counts(&[&[1, 2, 3], &[4, 0, 0]]));
    assert_eq!(reported.span.unwrap().start.line, 4);
}

#[test]
fn test_a_long_row_widens_the_grid_rather_than_losing_cells() {
    // Given
    let rst = ".. entity-bar::\n\n   1\n   2, 3\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.values(), counts(&[&[1, 0], &[2, 3]]));
    assert_eq!(codes(rst), ["entity-bar.ragged-row"]);
}

#[test]
fn test_a_separator_lets_a_filter_hold_a_comma() {
    // Given
    let rst = concat!(
        ".. entity-bar::\n",
        "   :separator: ;\n",
        "\n",
        "   \"a,b\" in title; 2\n",
    );

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.category_count(), 2);
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_transpose_swaps_the_grid_and_its_labels() {
    // Given
    let rst = concat!(
        ".. entity-bar::\n",
        "   :xlabels: Q1, Q2, Q3\n",
        "   :ylabels: Reqs\n",
        "   :transpose:\n",
        "\n",
        "   1, 2, 3\n",
    );

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.grid.values(), counts(&[&[1], &[2], &[3]]));
    assert_eq!(bar.grid.display_series_label(2), "Q3");
    assert_eq!(bar.grid.display_category_label(0), "Reqs");
}

#[test]
fn test_a_broken_cell_filter_is_reported_at_the_column_it_breaks_at() {
    // Given — the second cell starts at column 7 (3 of indent, `1, ` 3 more)
    let rst = ".. entity-bar::\n\n   1, len(title)\n";

    // When
    let reported = diagnostic(rst, "entity-bar.invalid-filter");

    // Then
    let span = reported.span.expect("expected a span");
    assert_eq!(span.start.line, 3);
    assert_eq!(span.start.column, 7);
    assert_eq!(parse_bar(rst).grid.values()[0][1], ChartValue::Filter(None));
}

#[test]
fn test_a_cell_filter_naming_an_unknown_field_is_reported() {
    // Given
    let rst = ".. entity-bar::\n\n   priority == \"high\"\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-bar.unknown-field"]);
}

#[test]
fn test_the_chart_filter_is_read() {
    // Given
    let rst = ".. entity-bar::\n   :filter: type == \"req\"\n\n   1\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert!(bar.filter.is_some());
}

#[test]
fn test_a_chart_with_no_data_is_reported() {
    // Given — a header row alone leaves nothing to draw
    let rst = ".. entity-bar::\n   :xlabels: FROM_DATA\n\n   a, b\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-bar.no-data"]);
}

#[test]
fn test_the_presentation_options_are_read() {
    // Given
    let rst = concat!(
        ".. needbar:: Report\n",
        "   :horizontal:\n",
        "   :colors: navy, #f00\n",
        "   :text_color: gray\n",
        "   :x_axis_title: Author\n",
        "   :y_axis_title: Count\n",
        "   :xlabels_rotation: 45\n",
        "   :ylabels_rotation: 90\n",
        "   :sum_rotation: 405\n",
        "   :align: right\n",
        "   :width: 60%\n",
        "   :caption: Who wrote what\n",
        "   :class: wide\n",
        "   :name: report-chart\n",
        "\n",
        "   1\n",
    );

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(bar.orientation, BarOrientation::Horizontal);
    assert_eq!(bar.colors.len(), 2);
    assert_eq!(bar.text_color, Some(ChartColor::parse("gray").unwrap()));
    assert_eq!(bar.x_axis_title.as_deref(), Some("Author"));
    assert_eq!(bar.y_axis_title.as_deref(), Some("Count"));
    assert_eq!(bar.xlabels_rotation.unwrap().degrees(), 45);
    assert_eq!(bar.ylabels_rotation.unwrap().degrees(), 90);
    assert_eq!(bar.sum_rotation.unwrap().degrees(), 45);
    assert_eq!(bar.align, Some(ImageAlign::Right));
    assert_eq!(bar.caption.as_deref(), Some("Who wrote what"));
    assert_eq!(bar.classes, ["wide"]);
    assert_eq!(
        bar.name.as_ref().map(rinx_ast::TargetName::as_str),
        Some("report-chart")
    );
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_hyphenated_spellings_of_the_multiword_flags_are_accepted() {
    // Given — sphinx-needs writes underscores, this build hyphens
    let rst = ".. entity-bar::\n   :show-top-sum:\n   :x-axis-title: Author\n\n   1\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert!(bar.value_labels.at_end);
    assert_eq!(bar.x_axis_title.as_deref(), Some("Author"));
}

#[test]
fn test_a_rotation_that_is_not_whole_degrees_is_reported() {
    // Given
    let rst = ".. entity-bar::\n   :xlabels_rotation: -45\n\n   1\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(codes(rst), ["entity-bar.invalid-rotation"]);
    assert_eq!(bar.xlabels_rotation, None);
}

#[test]
fn test_every_unsupported_needbar_option_is_refused_by_name() {
    // Given
    let rst = concat!(
        ".. needbar::\n",
        "   :style: ggplot\n",
        "   :status: open\n",
        "   :tags: a\n",
        "   :types: req\n",
        "   :cypher: MATCH (n)\n",
        "\n",
        "   1\n",
    );

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-bar.unsupported-option"; 5]);
    assert!(
        diagnostic(rst, "entity-bar.unsupported-option")
            .message
            .contains(":style:")
    );
}

#[test]
fn test_types_is_refused_with_the_exact_filter_to_write() {
    // Given
    let rst = ".. needbar::\n   :types: req\n\n   1\n";

    // When
    let reported = diagnostic(rst, "entity-bar.unsupported-option");

    // Then
    assert!(
        reported.message.contains(r#"type == "req""#),
        "{}",
        reported.message
    );
}

#[test]
fn test_an_option_nobody_claims_is_reported_as_unknown() {
    // Given
    let rst = ".. entity-bar::\n   :explode:\n\n   1\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["directive.entity-bar-unknown-option"]);
}

#[test]
fn test_an_empty_separator_is_reported_and_the_comma_kept() {
    // Given
    let rst = ".. entity-bar::\n   :separator:\n\n   1, 2\n";

    // When
    let bar = parse_bar(rst);

    // Then
    assert_eq!(codes(rst), ["entity-bar.empty-option-value"]);
    assert_eq!(bar.grid.category_count(), 2);
}

#[test]
fn test_a_scale_without_a_width_is_reported_as_unusable() {
    // Given
    let rst = ".. entity-bar::\n   :scale: 50\n\n   1\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-bar.unusable-scale"]);
}
