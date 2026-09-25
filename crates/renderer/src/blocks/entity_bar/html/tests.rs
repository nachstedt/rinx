use rinx_ast::{
    BarArrangement, BarGrid, ChartValue, DiagnosticCode, Directive, Document, EntityBar,
    EntityBarSource, LabelRotation, Node, TargetName,
};
use rinx_filter::parse_filter;

use crate::blocks::chart_test_support::{index, schema};
use crate::{EmbeddedAssets, RenderOutput, blocks::EntityTemplates, config::SiteConfig};

/// A one-series chart whose cells are `filters`.
fn bar(filters: &[&str]) -> EntityBar {
    let row = filters
        .iter()
        .map(|text| {
            ChartValue::Filter(Some(
                parse_filter(text).expect("expected the filter to parse"),
            ))
        })
        .collect();
    EntityBar::new(EntityBarSource::EntityBar, BarGrid::new(vec![row]).unwrap())
}

/// Renders `bar` through the public entry point — so a test also proves the
/// directive is wired into the node dispatcher.
fn render(bar: &EntityBar) -> RenderOutput {
    let doc = Document::new(
        "specs/boot".to_string(),
        vec![Node::Directive(Directive::EntityBar(Box::new(bar.clone())))],
    );
    crate::render_with_assets(
        &doc,
        &index(),
        "specs/boot",
        &SiteConfig::default(),
        &EmbeddedAssets::new(),
        &schema(),
        &EntityTemplates::new(),
    )
}

#[test]
fn test_the_svg_goes_straight_into_the_page_in_the_charts_figure() {
    // Given
    let bar = bar(&[r#"type == "req""#, r#"type == "test""#]);

    // When
    let output = render(&bar);

    // Then — nothing is compiled, so no diagram source is recorded either
    assert!(
        output.html.contains("<div class=\"entity-bar\">"),
        "{}",
        output.html
    );
    assert!(
        output
            .html
            .contains("<div class=\"entity-bar-figure\"><svg"),
        "{}",
        output.html
    );
    assert!(output.diagram_sources.is_empty());
}

#[test]
fn test_both_spellings_carry_one_class() {
    // Given
    let mut needbar = bar(&[r#"type == "req""#]);
    needbar.source = EntityBarSource::NeedBar;

    // When
    let html = render(&needbar).html;

    // Then
    assert!(html.contains("<div class=\"entity-bar\">"), "{html}");
}

#[test]
fn test_the_placement_options_reach_the_figure() {
    // Given
    let mut bar = bar(&[r#"type == "req""#]);
    bar.title = Some("Authors".to_string());
    bar.caption = Some("Who wrote what".to_string());
    bar.name = Some(TargetName::new("authors-chart"));

    // When
    let html = render(&bar).html;

    // Then
    assert!(html.contains("entity-bar-title\">Authors</p>"), "{html}");
    assert!(
        html.contains("<p class=\"caption\">Who wrote what</p>"),
        "{html}"
    );
    assert!(html.contains("id=\"authors-chart\""), "{html}");
}

#[test]
fn test_the_counts_and_labels_reach_the_drawing() {
    // Given — three requirements and one test, named by category
    let row = vec![
        ChartValue::Filter(Some(parse_filter(r#"type == "req""#).unwrap())),
        ChartValue::Filter(Some(parse_filter(r#"type == "test""#).unwrap())),
    ];
    let mut bar = EntityBar::new(
        EntityBarSource::EntityBar,
        BarGrid::new(vec![row])
            .unwrap()
            .with_category_labels(vec![Some("Reqs".to_string()), Some("Tests".to_string())]),
    );
    bar.value_labels.inside = true;
    bar.xlabels_rotation = Some(LabelRotation::parse("30").unwrap());

    // When
    let html = render(&bar).html;

    // Then
    assert!(html.contains(">Reqs</text>"), "{html}");
    assert!(html.contains(">3</text>"), "{html}");
    assert!(html.contains("rotate(-30, "), "{html}");
}

#[test]
fn test_a_chart_whose_cells_all_count_zero_is_reported_and_not_drawn() {
    // Given
    let mut bar = bar(&[r#"type == "nothing""#]);
    bar.arrangement = BarArrangement::Stacked;

    // When
    let output = render(&bar);

    // Then
    assert!(!output.html.contains("entity-bar"), "{}", output.html);
    assert_eq!(output.empty_listing_errors.len(), 1);
    assert_eq!(
        output.empty_listing_errors[0].code(),
        DiagnosticCode::EntityBarEmptyResult
    );
}

#[test]
fn test_the_empty_report_quotes_the_spelling_the_author_wrote() {
    // Given
    let mut bar = bar(&[r#"type == "nothing""#]);
    bar.source = EntityBarSource::NeedBar;

    // When
    let output = render(&bar);

    // Then
    assert!(
        output.empty_listing_errors[0]
            .message()
            .starts_with("needbar:")
    );
}
