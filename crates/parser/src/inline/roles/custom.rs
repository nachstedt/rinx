//! The roles a document defines for itself with `.. role:: name(base)`.
//!
//! Each derived role is built as the node its base builds — a `code` role as
//! [`InlineNode::Code`], a `sub`/`sup` role as [`InlineNode::Script`] — with
//! the language and classes its definition recorded. Like their bases, they
//! keep every character between the backticks and split no title.

use rinx_ast::InlineNode;

use super::script::script_node;
use crate::custom_roles::{CodeRole, CustomRole, CustomRoles};
use crate::inline::regexes::NAMED_ROLE_REGEX;

/// Builds the `InlineNode` for a role the document defined with `.. role::`,
/// or `None` when the matched name is not one — by now, since a role applies
/// only after its definition.
pub(crate) fn handle_custom_role_match(
    m_str: &str,
    roles: Option<&CustomRoles>,
) -> Option<InlineNode> {
    let caps = NAMED_ROLE_REGEX
        .captures(m_str)
        .expect("the caller matched this text with this regex");
    let text = &caps["target"];
    Some(match roles?.lookup(&caps["role"])? {
        CustomRole::Code(CodeRole { language, classes }) => InlineNode::Code {
            text: text.to_string(),
            language,
            classes,
            span: None,
        },
        CustomRole::Script { position, classes } => script_node(position, text, classes),
    })
}

#[cfg(test)]
mod tests {
    use rinx_ast::{ResolvedLanguage, ScriptPosition};

    use super::*;

    fn python_role() -> CustomRole {
        CustomRole::Code(CodeRole {
            language: ResolvedLanguage::parse("python").unwrap(),
            classes: vec!["python".to_string()],
        })
    }

    #[test]
    fn test_handle_custom_role_match_builds_code_from_the_definition() {
        // Given
        let roles = CustomRoles::default();
        roles.define("python", python_role());

        // When
        let result = handle_custom_role_match(":python:`print(1)`", Some(&roles));

        // Then
        assert_eq!(
            result,
            Some(InlineNode::Code {
                text: "print(1)".to_string(),
                language: ResolvedLanguage::parse("python").unwrap(),
                classes: vec!["python".to_string()],
                span: None,
            })
        );
    }

    #[test]
    fn test_handle_custom_role_match_builds_a_script_from_the_definition() {
        // Given
        let roles = CustomRoles::default();
        roles.define(
            "chem",
            CustomRole::Script {
                position: ScriptPosition::Subscript,
                classes: vec!["chem".to_string()],
            },
        );

        // When
        let result = handle_custom_role_match(":chem:`2`", Some(&roles));

        // Then
        assert_eq!(
            result,
            Some(InlineNode::Script {
                position: ScriptPosition::Subscript,
                text: "2".to_string(),
                classes: vec!["chem".to_string()],
            })
        );
    }

    #[test]
    fn test_handle_custom_role_match_misses_an_undefined_role() {
        // Given
        let roles = CustomRoles::default();

        // When / Then
        assert_eq!(handle_custom_role_match(":python:`x`", Some(&roles)), None);
    }

    #[test]
    fn test_handle_custom_role_match_misses_without_a_table() {
        // Given / When / Then
        assert_eq!(handle_custom_role_match(":python:`x`", None), None);
    }
}
