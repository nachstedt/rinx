use std::collections::BTreeMap;
use std::num::NonZeroU32;

use rusty_sphinx_ast::{
    DiagnosticCode, EntityId, EntitySequence, EntitySequenceSource, HashedContent, ImageAlign,
    LengthOrPercentage, NonEmptyVector, ResolvedLanguage, TargetName,
};
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use crate::EmbeddedAssets;
use crate::blocks::render_test_support::with_ctx_for;

use super::*;

fn record(title: &str, sends: &[&str]) -> EntityRecord {
    EntityRecord {
        type_name: "component".to_string(),
        doc_path: "arch".to_string(),
        title: Some(title.to_string()),
        attributes: BTreeMap::new(),
        outgoing: BTreeMap::from([(
            "sends".to_string(),
            sends
                .iter()
                .map(|id| EntityId::new(id).expect("a valid id"))
                .collect::<Vec<_>>(),
        )]),
        uml: BTreeMap::new(),
    }
}

/// A ping and its reply between two components.
fn project() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for (id, record) in [
        ("COMP_A", record("A", &["PING"])),
        ("COMP_B", record("B", &["PONG"])),
        ("PING", record("ping", &["COMP_B"])),
        ("PONG", record("pong", &["COMP_A"])),
    ] {
        index
            .entities
            .insert(EntityId::new(id).expect("a valid id"), record);
    }
    index
}

fn sequence(source: EntitySequenceSource, start: &str) -> EntitySequence {
    EntitySequence::new(
        source,
        NonEmptyVector::single(EntityId::new(start).expect("a valid id")),
        NonEmptyVector::single("sends".to_string()),
    )
}

fn ping() -> EntitySequence {
    sequence(EntitySequenceSource::EntitySequence, "COMP_A")
}

/// Renders `sequence` over `index`, returning the HTML and what the render
/// recorded for the build.
fn render_over(
    index: &ProjectIndex,
    sequence: &EntitySequence,
) -> (String, Vec<HashedContent>, Vec<DiagramError>) {
    with_ctx_for(
        index,
        "index.rst",
        ResolvedLanguage::default(),
        &EmbeddedAssets::new(),
        |ctx| {
            let mut html = String::new();
            render_entity_sequence(&mut html, sequence, ctx);
            (
                html,
                ctx.diagram_sources.clone(),
                ctx.diagram_errors.clone(),
            )
        },
    )
}

#[test]
fn test_a_sequence_diagram_renders_the_figure_every_diagram_does() {
    // Given
    let index = project();

    // When
    let (html, sources, errors) = render_over(&index, &ping());

    // Then — the page names the very file the build is handed
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(sources.len(), 1);
    assert!(html.contains("<div class=\"plantuml-diagram\">"), "{html}");
    assert!(
        html.contains(&format!("{}.svg", sources[0].hash())),
        "{html}"
    );
}

#[test]
fn test_the_presentation_options_reach_the_figure() {
    // Given
    let index = project();
    let sequence = EntitySequence {
        caption: Some("Ping".to_string()),
        align: Some(ImageAlign::Center),
        width: Some(LengthOrPercentage::new("400px").expect("a valid length")),
        scale: Some(50),
        classes: vec!["wide".to_string()],
        name: Some(TargetName::new("ping-sequence")),
        ..ping()
    };

    // When
    let (html, _, _) = render_over(&index, &sequence);

    // Then
    assert!(
        html.contains("class=\"plantuml-diagram align-center wide\""),
        "{html}"
    );
    assert!(html.contains("id=\"ping-sequence\""), "{html}");
    assert!(html.contains("style=\"width: 200px\""), "{html}");
    assert!(html.contains("<p class=\"caption\">Ping</p>"), "{html}");
}

#[test]
fn test_debug_shows_the_generated_plantuml() {
    // Given
    let index = project();
    let sequence = EntitySequence {
        debug: true,
        ..ping()
    };

    // When
    let (html, _, _) = render_over(&index, &sequence);

    // Then
    assert!(html.contains("plantuml-debug"), "{html}");
    assert!(html.contains("COMP_A -&gt; COMP_B : ping"), "{html}");
}

#[test]
fn test_a_truncated_walk_draws_and_says_so_on_the_page_and_in_the_log() {
    // Given
    let index = project();
    let sequence = EntitySequence {
        max_items: NonZeroU32::new(1),
        ..ping()
    };

    // When
    let (html, sources, errors) = render_over(&index, &sequence);

    // Then
    assert_eq!(sources.len(), 1);
    assert!(
        html.contains("<p class=\"entity-sequence-truncated\">Showing the first 1 of 2 messages"),
        "{html}"
    );
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), DiagnosticCode::EntitySequenceTruncated);
}

#[test]
fn test_an_untruncated_walk_carries_no_notice() {
    // Given
    let index = project();

    // When
    let (html, _, _) = render_over(&index, &ping());

    // Then
    assert!(!html.contains("entity-sequence-truncated"), "{html}");
}

#[test]
fn test_a_walk_that_drew_nothing_renders_no_picture_and_is_reported() {
    // Given — a start no document declares
    let index = project();
    let sequence = sequence(EntitySequenceSource::EntitySequence, "COMP_404");

    // When
    let (html, sources, errors) = render_over(&index, &sequence);

    // Then
    assert_eq!(html, "");
    assert!(sources.is_empty());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), DiagnosticCode::EntitySequenceUnknownStart);
}

#[test]
fn test_a_failure_names_the_directive_the_author_wrote_at_its_position() {
    // Given
    let index = ProjectIndex::default();
    let span = rusty_sphinx_ast::Span::new(
        rusty_sphinx_ast::Position { line: 7, column: 1 },
        rusty_sphinx_ast::Position {
            line: 7,
            column: 18,
        },
    );
    let sequence = EntitySequence {
        span: Some(span),
        ..sequence(EntitySequenceSource::NeedSequence, "COMP_A")
    };

    // When
    let (_, _, errors) = render_over(&index, &sequence);

    // Then
    assert!(
        errors[0].message().starts_with("needsequence:"),
        "{errors:?}"
    );
    assert_eq!(errors[0].span, Some(span));
}
