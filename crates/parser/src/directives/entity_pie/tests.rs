use rusty_sphinx_ast::{ChartColor, Directive, EntityPie, ImageAlign, Node, SliceSource};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

/// The schema every test below parses against: two types, an enum attribute
/// and a relation, which is enough vocabulary for every filter a wedge needs.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
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

          [[entity_type.relation]]
          name = "verifies"
          to = ["req"]
          incoming = "verified_by"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn parse(rst: &str) -> rusty_sphinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rusty_sphinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("specs/boot", rst, &ctx)
}

/// The first pie chart in `rst`.
fn parse_pie(rst: &str) -> EntityPie {
    let doc = parse(rst);
    doc.nodes
        .iter()
        .find_map(|node| match node {
            Node::Directive(Directive::EntityPie(pie)) => Some((**pie).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no pie chart parsed from:\n{rst}"))
}

/// The diagnostic codes a parse reported, as their author-facing ids.
fn codes(rst: &str) -> Vec<String> {
    parse(rst)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// The messages a parse reported.
fn messages(rst: &str) -> Vec<String> {
    parse(rst)
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn test_a_chart_takes_its_title_from_the_argument() {
    // Given — unlike `.. entity-table::`, which refuses an argument
    let rst = ".. entity-pie:: Safety Artifacts by Type\n\n   type == \"req\"\n";

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.title.as_deref(), Some("Safety Artifacts by Type"));
}

#[test]
fn test_each_content_line_becomes_one_wedge() {
    // Given — the shape every `.. needpie::` in the benchmark corpus has
    let rst = concat!(
        ".. entity-pie:: By type\n",
        "\n",
        "   type == \"req\"\n",
        "   type == \"test\"\n",
    );

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.slices.len(), 2);
    assert!(matches!(pie.slices[0].source, SliceSource::Filter(Some(_))));
    assert!(matches!(pie.slices[1].source, SliceSource::Filter(Some(_))));
}

#[test]
fn test_labels_pair_with_the_wedges_by_position() {
    // Given
    let rst = concat!(
        ".. entity-pie:: By type\n",
        "   :labels: Requirements, Tests\n",
        "\n",
        "   type == \"req\"\n",
        "   type == \"test\"\n",
    );

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.slices[0].label.as_deref(), Some("Requirements"));
    assert_eq!(pie.slices[1].label.as_deref(), Some("Tests"));
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_a_line_that_is_a_number_becomes_a_wedge_of_that_size() {
    // Given — sphinx-needs' static-data form. A bare `12` is also a valid
    // filter, so the number must be tried first or the wedge would count the
    // whole project instead
    let rst = ".. entity-pie:: Budget\n\n   12\n   30\n";

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.slices[0].source, SliceSource::Count(12));
    assert_eq!(pie.slices[1].source, SliceSource::Count(30));
}

#[test]
fn test_the_needpie_spelling_parses_to_the_same_node() {
    // Given
    let rst = ".. needpie:: By type\n\n   type == \"req\"\n";

    // When
    let pie = parse_pie(rst);

    // Then — only the recorded spelling differs, so a diagnostic can quote it
    assert_eq!(pie.source.as_str(), "needpie");
    assert_eq!(pie.slices.len(), 1);
}

#[test]
fn test_a_broken_slice_filter_is_reported_at_the_column_it_breaks_at() {
    // Given — the reason a filter is parsed here and not where it is evaluated
    let rst = ".. entity-pie:: By type\n\n   type == \"req\" and len(title)\n";

    // When
    let doc = parse(rst);
    let diagnostic = doc
        .diagnostics
        .iter()
        .find(|d| d.code.as_str() == "entity-pie.invalid-filter")
        .expect("expected the broken filter to be reported");

    // Then — `len` starts at column 21 of the source line (3 of indent, 18 of
    // `type == "req" and `), and the message names the construct refused
    let span = diagnostic.span.expect("expected a span");
    assert_eq!(span.start.line, 3);
    assert_eq!(span.start.column, 22);
    assert!(
        diagnostic.message.contains("function calls"),
        "{diagnostic:?}"
    );
}

#[test]
fn test_a_broken_slice_filter_leaves_the_wedge_selecting_everything() {
    // Given — the rule every filtered directive here follows
    let rst = ".. entity-pie:: By type\n\n   len(title) > 0\n";

    // When
    let pie = parse_pie(rst);

    // Then — the wedge survives, so the labels still line up
    assert_eq!(pie.slices.len(), 1);
    assert_eq!(pie.slices[0].source, SliceSource::Filter(None));
}

#[test]
fn test_a_slice_filter_naming_an_unknown_field_is_reported() {
    // Given
    let rst = ".. entity-pie:: By type\n\n   priority == \"high\"\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-pie.unknown-field"]);
}

#[test]
fn test_a_chart_with_no_content_is_reported_while_parsing() {
    // Given — no index is needed to see that there is nothing to count
    let rst = ".. entity-pie:: By type\n   :labels: Requirements\n";

    // When
    let reported = codes(rst);

    // Then
    assert!(
        reported.contains(&"entity-pie.no-slices".to_string()),
        "{reported:?}"
    );
}

#[test]
fn test_more_labels_than_wedges_is_reported_and_the_surplus_dropped() {
    // Given — they pair by position, so the author cannot see the slip
    let rst = concat!(
        ".. entity-pie:: By type\n",
        "   :labels: Requirements, Tests, Specs\n",
        "\n",
        "   type == \"req\"\n",
        "   type == \"test\"\n",
    );

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.slices.len(), 2);
    assert_eq!(pie.slices[1].label.as_deref(), Some("Tests"));
    assert!(
        codes(rst).contains(&"entity-pie.label-count-mismatch".to_string()),
        "{:?}",
        codes(rst)
    );
}

#[test]
fn test_fewer_labels_than_wedges_still_draws_every_wedge() {
    // Given — losing the data over a naming mistake would be the larger failure
    let rst = concat!(
        ".. entity-pie:: By type\n",
        "   :labels: Requirements\n",
        "\n",
        "   type == \"req\"\n",
        "   type == \"test\"\n",
    );

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.slices.len(), 2);
    assert_eq!(pie.slices[0].label.as_deref(), Some("Requirements"));
    assert_eq!(pie.slices[1].label, None);
}

#[test]
fn test_the_chart_filter_narrows_every_wedge() {
    // Given
    let rst = concat!(
        ".. entity-pie:: By status\n",
        "   :filter: type == \"req\"\n",
        "\n",
        "   status == \"open\"\n",
    );

    // When
    let pie = parse_pie(rst);

    // Then
    assert!(pie.filter.is_some());
    assert_eq!(pie.slices.len(), 1);
}

#[test]
fn test_the_presentation_options_are_read() {
    // Given
    let rst = concat!(
        ".. entity-pie:: By type\n",
        "   :legend:\n",
        "   :colors: #4c72b0, teal\n",
        "   :text_color: white\n",
        "   :caption: How the project splits\n",
        "   :align: center\n",
        "   :width: 400px\n",
        "   :class: wide framed\n",
        "   :name: type-split\n",
        "\n",
        "   type == \"req\"\n",
    );

    // When
    let pie = parse_pie(rst);

    // Then
    assert!(pie.legend);
    assert_eq!(
        pie.colors,
        [
            ChartColor::parse("#4c72b0").unwrap(),
            ChartColor::parse("teal").unwrap()
        ]
    );
    assert_eq!(pie.text_color, Some(ChartColor::parse("white").unwrap()));
    assert_eq!(pie.caption.as_deref(), Some("How the project splits"));
    assert_eq!(pie.align, Some(ImageAlign::Center));
    assert_eq!(
        pie.width.as_ref().map(ToString::to_string).as_deref(),
        Some("400px")
    );
    assert_eq!(pie.classes, ["wide", "framed"]);
    assert_eq!(
        pie.name.as_ref().map(rusty_sphinx_ast::TargetName::as_str),
        Some("type-split")
    );
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_an_unusable_colour_is_dropped_and_reported_while_the_rest_are_kept() {
    // Given — losing every colour over one misspelling would change a chart
    // the author can see into one they cannot recognise
    let rst = concat!(
        ".. entity-pie:: By type\n",
        "   :colors: #4c72b0, chartreusey, teal\n",
        "\n",
        "   type == \"req\"\n",
    );

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.colors.len(), 2);
    assert_eq!(codes(rst), ["entity-pie.invalid-color"]);
}

#[test]
fn test_a_scale_without_a_width_is_reported_as_unusable() {
    // Given
    let rst = ".. entity-pie:: By type\n   :scale: 50\n\n   type == \"req\"\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-pie.unusable-scale"]);
}

#[test]
fn test_every_unsupported_needpie_option_is_refused_by_name() {
    // Given — silence is the one behaviour that cannot be right: an author who
    // asked for exploded wedges and got none has no way to find out why
    for (option, value) in [
        ("explode", "0.1, 0"),
        ("shadow", ""),
        ("style", "ggplot"),
        ("filter-func", "my_module:pick"),
    ] {
        let rst = format!(".. entity-pie:: By type\n   :{option}: {value}\n\n   type == \"req\"\n");

        // When
        let reported = codes(&rst);

        // Then
        assert_eq!(reported, ["entity-pie.unsupported-option"], "{option}");
        assert!(
            messages(&rst)[0].contains(&format!(":{option}:")),
            "{option}: {:?}",
            messages(&rst)
        );
    }
}

#[test]
fn test_an_option_nobody_claims_is_reported_as_unknown() {
    // Given
    let rst = ".. entity-pie:: By type\n   :wobble: 3\n\n   type == \"req\"\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["directive.entity-pie-unknown-option"]);
}

#[test]
fn test_an_align_this_build_cannot_draw_is_reported() {
    // Given
    let rst = ".. entity-pie:: By type\n   :align: sideways\n\n   type == \"req\"\n";

    // When
    let reported = codes(rst);

    // Then
    assert_eq!(reported, ["entity-pie.invalid-align"]);
}

#[test]
fn test_a_name_with_no_value_is_reported_rather_than_registered() {
    // Given
    let rst = ".. entity-pie:: By type\n   :name:\n\n   type == \"req\"\n";

    // When
    let pie = parse_pie(rst);

    // Then
    assert_eq!(pie.name, None);
    assert_eq!(codes(rst), ["entity-pie.empty-option-value"]);
}

#[test]
fn test_a_startswith_filter_parses_now_that_the_language_has_one() {
    // Given — eight corpus wedges are written this way, and every one of them
    // was refused as "attribute access" before
    let rst = ".. needpie:: FSRs by Subsystem\n\n   id.startswith(\"FSR_STEER\")\n";

    // When
    let pie = parse_pie(rst);

    // Then
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
    assert!(matches!(pie.slices[0].source, SliceSource::Filter(Some(_))));
}
