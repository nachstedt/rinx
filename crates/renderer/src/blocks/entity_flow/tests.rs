use std::collections::BTreeMap;

use rusty_sphinx_ast::{
    EntityFlow, EntityFlowSource, EntityId, ImageAlign, LengthOrPercentage, ResolvedLanguage,
    TargetName,
};
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use crate::EmbeddedAssets;
use crate::blocks::render_test_support::with_ctx_for;

use super::*;

/// A project with two entities, one linking to the other.
fn project() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for (id, targets) in [("REQ_001", vec![]), ("TEST_001", vec!["REQ_001"])] {
        index.entities.insert(
            EntityId::new(id).expect("a valid id"),
            EntityRecord {
                type_name: "req".to_string(),
                doc_path: "specs".to_string(),
                title: Some(id.to_string()),
                attributes: BTreeMap::new(),
                outgoing: BTreeMap::from([(
                    "links".to_string(),
                    targets
                        .iter()
                        .map(|id| EntityId::new(id).expect("a valid id"))
                        .collect::<Vec<_>>(),
                )]),
                uml: BTreeMap::new(),
            },
        );
    }
    index
}

/// A flowchart drawing the `links` relation, which the empty test schema does
/// not declare — named explicitly so the picture has edges to assert about.
fn linking_flow() -> EntityFlow {
    EntityFlow {
        relations: Some(vec!["links".to_string()]),
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    }
}

/// Renders `flow` over `index` and returns the HTML.
fn render_over(index: &ProjectIndex, flow: &EntityFlow) -> String {
    with_ctx_for(
        index,
        "index.rst",
        ResolvedLanguage::default(),
        &EmbeddedAssets::new(),
        |ctx| {
            let mut html = String::new();
            render_entity_flow(&mut html, flow, ctx);
            html
        },
    )
}

/// Renders `flow` and returns what the render recorded for the build.
fn recorded(index: &ProjectIndex, flow: &EntityFlow) -> (Vec<HashedContent>, Vec<DiagramError>) {
    with_ctx_for(
        index,
        "index.rst",
        ResolvedLanguage::default(),
        &EmbeddedAssets::new(),
        |ctx| {
            let mut html = String::new();
            render_entity_flow(&mut html, flow, ctx);
            (ctx.diagram_sources.clone(), ctx.diagram_errors.clone())
        },
    )
}

#[test]
fn test_a_flowchart_renders_the_same_figure_a_written_diagram_does() {
    // Given — two pictures on one page should not be tellable apart by how
    // their text came about
    let index = project();

    // When
    let html = render_over(&index, &linking_flow());

    // Then
    assert!(html.contains("<div class=\"plantuml-diagram\">"), "{html}");
    assert!(html.contains("alt=\"PlantUML Diagram\""), "{html}");
    assert!(html.contains("_images/"), "{html}");
    assert!(html.contains(".svg"), "{html}");
}

#[test]
fn test_the_page_points_at_the_hash_of_the_text_the_build_is_given() {
    // Given — a page naming a different file than the one compiled is the one
    // failure this pipeline exists to prevent
    let index = project();

    // When
    let html = render_over(&index, &linking_flow());
    let (sources, errors) = recorded(&index, &linking_flow());

    // Then
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(sources.len(), 1);
    assert!(
        html.contains(&format!("{}.svg", sources[0].hash())),
        "{html}"
    );
}

#[test]
fn test_two_identical_flowcharts_on_one_page_share_one_compiled_picture() {
    // Given — the hash is the filename, so one source file is enough
    let index = project();

    // When
    let sources = with_ctx_for(
        &index,
        "index.rst",
        ResolvedLanguage::default(),
        &EmbeddedAssets::new(),
        |ctx| {
            let mut html = String::new();
            render_entity_flow(&mut html, &linking_flow(), ctx);
            render_entity_flow(&mut html, &linking_flow(), ctx);
            ctx.diagram_sources.clone()
        },
    );

    // Then
    assert_eq!(sources.len(), 1);
}

#[test]
fn test_the_presentation_options_reach_the_figure() {
    // Given
    let index = project();
    let flow = EntityFlow {
        caption: Some("How the tests reach the requirements".to_string()),
        align: Some(ImageAlign::Center),
        width: Some(LengthOrPercentage::new("400px").expect("a valid length")),
        scale: Some(50),
        classes: vec!["wide".to_string()],
        name: Some(TargetName::new("coverage-flow")),
        ..linking_flow()
    };

    // When
    let html = render_over(&index, &flow);

    // Then
    assert!(
        html.contains("class=\"plantuml-diagram align-center wide\""),
        "{html}"
    );
    assert!(html.contains("id=\"coverage-flow\""), "{html}");
    assert!(html.contains("style=\"width: 200px\""), "{html}");
    assert!(
        html.contains("<p class=\"caption\">How the tests reach the requirements</p>"),
        "{html}"
    );
}

#[test]
fn test_debug_shows_the_generated_plantuml_below_the_picture() {
    // Given — this text exists nowhere else, so the option earns its place
    // here more than on a diagram whose source the author wrote
    let index = project();
    let flow = EntityFlow {
        debug: true,
        ..linking_flow()
    };

    // When
    let html = render_over(&index, &flow);

    // Then
    assert!(html.contains("plantuml-debug"), "{html}");
    assert!(html.contains("TEST_001 --&gt; REQ_001"), "{html}");
    assert!(
        html.find("<img").unwrap() < html.find("plantuml-debug").unwrap(),
        "{html}"
    );
}

#[test]
fn test_a_flowchart_that_drew_nothing_renders_no_picture_and_is_reported() {
    // Given — over a project with no entities at all
    let index = ProjectIndex::default();
    let flow = linking_flow();

    // When
    let html = render_over(&index, &flow);
    let (sources, errors) = recorded(&index, &flow);

    // Then — a broken `<img>` on top of the warning would say the same thing
    // twice, once unhelpfully
    assert_eq!(html, "");
    assert!(sources.is_empty());
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].code(),
        rusty_sphinx_ast::DiagnosticCode::EntityFlowEmptyResult
    );
}

#[test]
fn test_a_failure_names_the_directive_the_author_wrote() {
    // Given — an author who wrote `needflow` should not be told about
    // `entity-flow`
    let index = ProjectIndex::default();
    let flow = EntityFlow::new(EntityFlowSource::NeedFlow);

    // When
    let (_, errors) = recorded(&index, &flow);

    // Then
    assert!(errors[0].message().starts_with("needflow:"), "{errors:?}");
}

#[test]
fn test_a_failure_carries_the_directives_own_position() {
    // Given — the renderer is the only phase that knows both the failure and
    // where the directive was written
    let index = ProjectIndex::default();
    let span = rusty_sphinx_ast::Span::new(
        rusty_sphinx_ast::Position { line: 7, column: 1 },
        rusty_sphinx_ast::Position {
            line: 7,
            column: 18,
        },
    );
    let flow = EntityFlow {
        span: Some(span),
        ..EntityFlow::new(EntityFlowSource::EntityFlow)
    };

    // When
    let (_, errors) = recorded(&index, &flow);

    // Then
    assert_eq!(errors[0].span, Some(span));
}
