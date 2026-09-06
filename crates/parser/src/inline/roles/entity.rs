//! The inline roles a schema declares, plus the built-in `:entity:`.
//!
//! Unlike every other role in this crate, these have no fixed spelling: a
//! project names them. So one static regex matches the *shape* of any role and
//! this module decides, against the schema, whether the name it captured is
//! one — which keeps `regexes.rs` a static table rather than something
//! recompiled per project.
//!
//! A role is never required for an entity to be linkable: every entity
//! registers an ordinary target name, so `:ref:` reaches one regardless. What a
//! declared role adds is the type check on the link, and the spelling an
//! existing sphinx-needs project already writes.

use rusty_sphinx_ast::InlineNode;
use rusty_sphinx_entity::EntitySchema;

use crate::explicit_title::split_display_and_target;

use super::super::regexes::ENTITY_ROLE_REGEX;

/// Builds an entity reference, or plain text when the role is not declared.
///
/// Falling back to text rather than to a diagnostic is deliberate: this regex
/// matches any ``:word:`text` `` at all, including the many role spellings this
/// build does not implement and the occasional false positive in ordinary
/// prose. Reporting those would be reporting on text the author never meant as
/// markup — the diagnostics belong to roles that *are* declared and fail to
/// resolve, which is a render-time question.
pub(in crate::inline) fn handle_entity_role_match(
    m_str: &str,
    schema: &EntitySchema,
) -> InlineNode {
    let caps = ENTITY_ROLE_REGEX
        .captures(m_str)
        .expect("the caller matched this text with this regex");
    let role = caps["role"].to_string();

    if schema.role(&role).is_none() {
        return InlineNode::Text(m_str.to_string());
    }

    let (display, target) = split_display_and_target(&caps["target"]);
    InlineNode::EntityReference {
        role,
        target,
        display,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_entity::{NoReservedNames, load_schema};

    fn schema() -> EntitySchema {
        load_schema(
            r#"
            [[entity_type]]
            name = "req"

            [[role]]
            name = "req"
            types = ["req"]
            "#,
            &NoReservedNames,
        )
        .unwrap()
    }

    #[test]
    fn test_a_declared_role_becomes_an_entity_reference() {
        // Given
        let schema = schema();

        // When
        let node = handle_entity_role_match(":req:`REQ_001`", &schema);

        // Then
        assert_eq!(
            node,
            InlineNode::EntityReference {
                role: "req".to_string(),
                target: "REQ_001".to_string(),
                display: "REQ_001".to_string(),
                span: None,
            }
        );
    }

    #[test]
    fn test_the_builtin_entity_role_needs_no_declaration() {
        // Given
        let schema = schema();

        // When
        let node = handle_entity_role_match(":entity:`REQ_001`", &schema);

        // Then
        assert!(matches!(node, InlineNode::EntityReference { .. }));
    }

    #[test]
    fn test_an_explicit_title_splits_display_from_target() {
        // Given
        let schema = schema();

        // When
        let node = handle_entity_role_match(":req:`the boot rule <REQ_001>`", &schema);

        // Then
        let InlineNode::EntityReference {
            display, target, ..
        } = node
        else {
            panic!("expected an entity reference");
        };
        assert_eq!(display, "the boot rule");
        assert_eq!(target, "REQ_001");
    }

    #[test]
    fn test_an_undeclared_role_degrades_to_plain_text() {
        // Given — this regex matches any `:word:`text``, most of which is not markup
        let schema = schema();

        // When
        let node = handle_entity_role_match(":nonsense:`whatever`", &schema);

        // Then
        assert_eq!(node, InlineNode::Text(":nonsense:`whatever`".to_string()));
    }

    #[test]
    fn test_every_role_degrades_without_a_schema() {
        // Given
        let empty = EntitySchema::empty();

        // When
        let declared = handle_entity_role_match(":req:`REQ_001`", &empty);
        let builtin = handle_entity_role_match(":entity:`REQ_001`", &empty);

        // Then — the built-in still works; a project-specific spelling does not
        assert_eq!(declared, InlineNode::Text(":req:`REQ_001`".to_string()));
        assert!(matches!(builtin, InlineNode::EntityReference { .. }));
    }
}
