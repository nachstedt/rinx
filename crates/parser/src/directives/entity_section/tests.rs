use rusty_sphinx_ast::{Directive, Node};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"

          [[entity_type.section]]
          name = "rationale"
        "#,
        &NoReservedNames,
    )
    .unwrap()
}

fn parse(rst: &str) -> rusty_sphinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rusty_sphinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("d", rst, &ctx)
}

#[test]
fn test_a_declared_section_is_recognised_inside_its_entity() {
    // Given
    let rst = ".. req::\n\n   .. rationale::\n\n      Because.\n";

    // When
    let doc = parse(rst);

    // Then
    let Some(Node::Directive(Directive::Entity(entity))) = doc.nodes.first() else {
        panic!("expected an entity");
    };
    assert_eq!(entity.named_sections("rationale").len(), 1);
}

#[test]
fn test_a_section_outside_an_entity_is_reported_and_kept() {
    // Given — losing its body would turn one mistake into several
    let rst = ".. rationale::\n\n   Because.\n";

    // When
    let doc = parse(rst);

    // Then
    assert!(matches!(
        doc.nodes.first(),
        Some(Node::Directive(Directive::EntitySection { .. }))
    ));
    assert_eq!(
        doc.diagnostics[0].code,
        rusty_sphinx_ast::DiagnosticCode::EntitySectionOutsideEntity
    );
}

#[test]
fn test_a_name_no_type_declares_is_left_to_the_ordinary_chain() {
    // Given — not this module's business
    let rst = ".. not-a-section::\n\n   Body.\n";

    // When
    let doc = parse(rst);

    // Then
    assert!(matches!(
        doc.nodes.first(),
        Some(Node::Directive(Directive::Unknown { .. }))
    ));
    assert!(doc.diagnostics.is_empty());
}

#[test]
fn test_an_argument_on_a_section_is_diagnosed() {
    // Given — text on the marker line would otherwise vanish
    let rst = ".. req::\n\n   .. rationale:: stray text\n\n      Because.\n";

    // When
    let doc = parse(rst);

    // Then
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == rusty_sphinx_ast::DiagnosticCode::EntityMalformedArgument)
    );
}

#[test]
fn test_a_section_name_is_inert_without_a_schema() {
    // Given
    let ctx = ParseCtx::with_domain(rusty_sphinx_ast::Domain::Py);

    // When
    let doc = parse_with_ctx("d", ".. rationale::\n\n   Because.\n", &ctx);

    // Then
    assert!(matches!(
        doc.nodes.first(),
        Some(Node::Directive(Directive::Unknown { .. }))
    ));
    assert!(doc.diagnostics.is_empty());
}
