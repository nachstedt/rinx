use rusty_sphinx_ast::{
    EntityUpdateSource, FieldMutation, FieldMutationMode, InlineNode, Node, UpdateTarget,
};

use crate::blocks::render_test_support::with_ctx;

use super::{EntityUpdateVisibility, render_entity_update};

#[test]
fn test_from_config_true_is_show() {
    // Given / When / Then
    assert_eq!(
        EntityUpdateVisibility::from_config(true),
        EntityUpdateVisibility::Show
    );
}

#[test]
fn test_from_config_false_is_hide() {
    // Given / When / Then
    assert_eq!(
        EntityUpdateVisibility::from_config(false),
        EntityUpdateVisibility::Hide
    );
}

#[test]
fn test_is_visible_matches_the_show_variant_only() {
    // Given / When / Then
    assert!(EntityUpdateVisibility::Show.is_visible());
    assert!(!EntityUpdateVisibility::Hide.is_visible());
}

fn target(raw: &str) -> UpdateTarget {
    UpdateTarget {
        candidate_id: rusty_sphinx_ast::EntityId::new(raw).ok(),
        filter: None,
        raw: raw.to_string(),
    }
}

#[test]
fn test_renders_the_source_and_the_target() {
    // Given
    let update =
        rusty_sphinx_ast::EntityUpdate::new(EntityUpdateSource::EntityUpdate, target("REQ_001"));

    // When
    let html = with_ctx(|ctx| {
        let mut html = String::new();
        render_entity_update(&mut html, &update, ctx);
        html
    });

    // Then
    assert!(html.contains("entity-update"));
    assert!(html.contains("REQ_001"));
}

#[test]
fn test_renders_a_set_mutation_with_no_prefix() {
    // Given
    let mut update =
        rusty_sphinx_ast::EntityUpdate::new(EntityUpdateSource::EntityUpdate, target("REQ_001"));
    update.fields.push(FieldMutation {
        field: "status".to_string(),
        mode: FieldMutationMode::Set("closed".to_string()),
        span: None,
    });

    // When
    let html = with_ctx(|ctx| {
        let mut html = String::new();
        render_entity_update(&mut html, &update, ctx);
        html
    });

    // Then
    assert!(html.contains("status"));
    assert!(html.contains("closed"));
}

#[test]
fn test_renders_a_cleared_field_with_no_value() {
    // Given
    let mut update =
        rusty_sphinx_ast::EntityUpdate::new(EntityUpdateSource::EntityUpdate, target("REQ_001"));
    update.fields.push(FieldMutation {
        field: "tags".to_string(),
        mode: FieldMutationMode::Clear,
        span: None,
    });

    // When
    let html = with_ctx(|ctx| {
        let mut html = String::new();
        render_entity_update(&mut html, &update, ctx);
        html
    });

    // Then
    assert!(html.contains("cleared"));
}

#[test]
fn test_renders_the_justification_body() {
    // Given
    let mut update =
        rusty_sphinx_ast::EntityUpdate::new(EntityUpdateSource::EntityUpdate, target("REQ_001"));
    update.body = vec![Node::Paragraph(vec![InlineNode::Text(
        "Closed after review.".to_string(),
    )])];

    // When
    let html = with_ctx(|ctx| {
        let mut html = String::new();
        render_entity_update(&mut html, &update, ctx);
        html
    });

    // Then
    assert!(html.contains("Closed after review."));
}
