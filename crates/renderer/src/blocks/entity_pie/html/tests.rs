use std::collections::BTreeMap;

use rinx_ast::{
    AttributeValue, ChartColor, Directive, Document, EntityId, EntityPie, EntityPieSource,
    ImageAlign, LengthOrPercentage, Node, PieSlice, TargetName,
};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rinx_filter::parse_filter;
use rinx_index::{EntityRecord, ProjectIndex};

use crate::{EmbeddedAssets, RenderOutput, blocks::EntityTemplates, config::SiteConfig};

fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
          [[entity_type.attribute]]
          name = "status"
          type = "string"

        [[entity_type]]
        name = "test"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn id(raw: &str) -> EntityId {
    EntityId::new(raw).unwrap()
}

/// Two requirements and one test.
fn index() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for (name, status) in [("REQ_1", "open"), ("REQ_2", "closed")] {
        index.entities.insert(
            id(name),
            EntityRecord {
                type_name: "req".to_string(),
                doc_path: "specs/boot".to_string(),
                title: Some("A requirement".to_string()),
                attributes: BTreeMap::from([(
                    "status".to_string(),
                    AttributeValue::String(status.to_string()),
                )]),
                outgoing: BTreeMap::new(),
                uml: BTreeMap::new(),
            },
        );
    }
    index.entities.insert(
        id("TEST_1"),
        EntityRecord {
            type_name: "test".to_string(),
            doc_path: "tests/boot".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
        },
    );
    index
}

/// A chart whose wedges are `filters`.
fn pie(filters: &[&str]) -> EntityPie {
    let mut pie = EntityPie::new(EntityPieSource::EntityPie);
    pie.slices = filters
        .iter()
        .map(|text| {
            PieSlice::from_filter(Some(
                parse_filter(text).expect("expected the filter to parse"),
            ))
        })
        .collect();
    pie
}

/// Renders `pie` through the public entry point — so a test also proves the
/// directive is wired into the node dispatcher.
fn render(pie: &EntityPie) -> RenderOutput {
    let doc = Document::new(
        "specs/boot".to_string(),
        vec![Node::Directive(Directive::EntityPie(Box::new(pie.clone())))],
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

fn html_of(pie: &EntityPie) -> String {
    render(pie).html
}

#[test]
fn test_the_chart_carries_one_class_for_both_spellings() {
    // Given — two documents using different names for one directive should
    // not need two stylesheets
    let mut needpie = pie(&[r#"type == "req""#]);
    needpie.source = EntityPieSource::NeedPie;

    // When
    let (ours, theirs) = (html_of(&pie(&[r#"type == "req""#])), html_of(&needpie));

    // Then
    assert!(ours.contains("class=\"entity-pie\""), "{ours}");
    assert!(theirs.contains("class=\"entity-pie\""), "{theirs}");
}

#[test]
fn test_the_svg_goes_straight_into_the_page() {
    // Given — the whole reason a chart needs no compile action
    let pie = pie(&[r#"type == "req""#]);

    // When
    let html = html_of(&pie);

    // Then
    assert!(html.contains("<svg"), "{html}");
    assert!(!html.contains("<img"), "a chart must not point at a file");
}

#[test]
fn test_a_chart_records_no_diagram_source_to_compile() {
    // Given — nothing here reaches PlantUML, so no `.puml` is written and no
    // library needs `diagrams = True`
    let pie = pie(&[r#"type == "req""#]);

    // When
    let output = render(&pie);

    // Then
    assert!(output.diagram_sources.is_empty());
}

#[test]
fn test_the_title_is_shown_above_the_chart() {
    // Given
    let mut pie = pie(&[r#"type == "req""#]);
    pie.title = Some("Safety Artifacts by Type".to_string());

    // When
    let html = html_of(&pie);

    // Then
    assert!(html.contains("Safety Artifacts by Type"), "{html}");
    assert!(html.contains("entity-pie-title"), "{html}");
}

#[test]
fn test_the_caption_is_shown_under_the_chart() {
    // Given
    let mut pie = pie(&[r#"type == "req""#]);
    pie.caption = Some("How the project splits".to_string());

    // When
    let html = html_of(&pie);

    // Then
    assert!(
        html.contains("<p class=\"caption\">How the project splits</p>"),
        "{html}"
    );
}

#[test]
fn test_the_name_becomes_the_wrappers_id_so_a_ref_lands_on_it() {
    // Given
    let mut pie = pie(&[r#"type == "req""#]);
    pie.name = Some(TargetName::new("type-split"));

    // When
    let html = html_of(&pie);

    // Then
    assert!(html.contains("id=\"type-split\""), "{html}");
}

#[test]
fn test_the_wrapper_carries_the_alignment_and_the_authors_own_classes() {
    // Given
    let mut pie = pie(&[r#"type == "req""#]);
    pie.align = Some(ImageAlign::Center);
    pie.classes = vec!["wide".to_string()];

    // When
    let html = html_of(&pie);

    // Then
    assert!(
        html.contains("class=\"entity-pie align-center wide\""),
        "{html}"
    );
}

#[test]
fn test_the_width_is_applied_to_the_figure() {
    // Given
    let mut pie = pie(&[r#"type == "req""#]);
    pie.width = Some(LengthOrPercentage::new("400px").unwrap());

    // When
    let html = html_of(&pie);

    // Then
    assert!(html.contains("width: 400px"), "{html}");
}

#[test]
fn test_a_title_with_markup_characters_is_escaped() {
    // Given
    let mut pie = pie(&[r#"type == "req""#]);
    pie.title = Some("Reqs & <Tests>".to_string());

    // When
    let html = html_of(&pie);

    // Then
    assert!(html.contains("Reqs &amp; &lt;Tests&gt;"), "{html}");
}

#[test]
fn test_a_chart_whose_wedges_all_count_zero_is_reported_and_not_drawn() {
    // Given — an empty chart is far more often a filter that no longer
    // matches than a deliberate statement
    let pie = pie(&[r#"type == "nothing""#]);

    // When
    let output = render(&pie);

    // Then
    assert_eq!(output.empty_listing_errors.len(), 1);
    assert_eq!(
        output.empty_listing_errors[0].code().as_str(),
        "entity-pie.empty-result"
    );
    assert!(!output.html.contains("<svg"), "{}", output.html);
}

#[test]
fn test_the_empty_report_quotes_the_spelling_the_author_wrote() {
    // Given
    let mut pie = pie(&[r#"type == "nothing""#]);
    pie.source = EntityPieSource::NeedPie;

    // When
    let output = render(&pie);

    // Then
    assert!(
        output.empty_listing_errors[0]
            .message()
            .starts_with("needpie:"),
        "{}",
        output.empty_listing_errors[0].message()
    );
}

#[test]
fn test_a_chart_that_counts_something_reports_nothing() {
    // Given
    let pie = pie(&[r#"type == "req""#, r#"type == "test""#]);

    // When
    let output = render(&pie);

    // Then
    assert!(output.empty_listing_errors.is_empty());
}

#[test]
fn test_the_written_colours_reach_the_drawing() {
    // Given
    let mut pie = pie(&[r#"type == "req""#, r#"type == "test""#]);
    pie.colors = vec![
        ChartColor::parse("#ff0000").unwrap(),
        ChartColor::parse("#00ff00").unwrap(),
    ];

    // When
    let html = html_of(&pie);

    // Then
    assert!(html.contains("#FF0000"), "{html}");
    assert!(html.contains("#00FF00"), "{html}");
}
