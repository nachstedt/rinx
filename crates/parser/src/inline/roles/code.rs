//! `:code:`, and the roles a document derives from it with
//! `.. role:: name(code)`.
//!
//! Flat under `roles/` for the reason `:math:` is: it belongs to no domain.
//! Both handlers keep every character between the backticks — no trimming and
//! no explicit-title split, since `` :code:`a <b>` `` is code, not a title.
//! Escape markers are still present here; `unescape_node` drops them later,
//! taking the display form because Sphinx's `code_role` receives interpreted
//! text rather than a literal's verbatim one.

use rinx_ast::{InlineNode, ResolvedLanguage};

use crate::custom_roles::{CodeRole, CustomRoles};
use crate::inline::regexes::{CODE_ROLE_REGEX, NAMED_ROLE_REGEX};

/// Builds the `InlineNode` for a matched `:code:` role: unhighlighted, with
/// no classes of its own.
pub(crate) fn handle_code_match(m_str: &str) -> InlineNode {
    let caps = CODE_ROLE_REGEX.captures(m_str).unwrap();
    InlineNode::Code {
        text: caps["code"].to_string(),
        language: ResolvedLanguage::None,
        classes: Vec::new(),
        span: None,
    }
}

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
    let CodeRole { language, classes } = roles?.lookup(&caps["role"])?;
    Some(InlineNode::Code {
        text: caps["target"].to_string(),
        language,
        classes,
        span: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> InlineNode {
        InlineNode::Code {
            text: text.to_string(),
            language: ResolvedLanguage::None,
            classes: Vec::new(),
            span: None,
        }
    }

    fn python_role() -> CodeRole {
        CodeRole {
            language: ResolvedLanguage::parse("python").unwrap(),
            classes: vec!["python".to_string()],
        }
    }

    #[test]
    fn test_handle_code_match_reads_the_text() {
        // Given / When
        let result = handle_code_match(":code:`x = 1`");

        // Then
        assert_eq!(result, code("x = 1"));
    }

    #[test]
    fn test_handle_code_match_keeps_surrounding_whitespace() {
        // Given code padded inside the backticks
        let result = handle_code_match(":code:` x `");

        // Then every character between the backticks is content
        assert_eq!(result, code(" x "));
    }

    #[test]
    fn test_handle_code_match_does_not_split_an_angle_bracket_as_a_title() {
        // Given code that looks like an explicit-title override
        let result = handle_code_match(":code:`a <b>`");

        // Then the whole thing is code
        assert_eq!(result, code("a <b>"));
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
