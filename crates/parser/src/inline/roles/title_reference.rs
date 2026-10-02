//! `:title-reference:`, also spelled `:title:` and `:t:` — docutils' generic
//! role for the title of a work, and the role interpreted text written without
//! one gets unless a `.. default-role::` says otherwise.
//!
//! Flat under `roles/` for the reason `:sub:` is: it belongs to no domain. Its
//! content is plain text, as docutils gives a generic role, with smart
//! typography applied since it is prose. Escape markers are still present
//! here; `unescape_node` drops them later.

use rinx_ast::InlineNode;

use crate::inline::regexes::TITLE_REFERENCE_ROLE_REGEX;
use crate::inline::typography::apply_smart_typography;

/// Builds the `InlineNode` for a matched `:title-reference:`, `:title:` or
/// `:t:` role.
pub(crate) fn handle_title_reference_match(m_str: &str) -> InlineNode {
    let caps = TITLE_REFERENCE_ROLE_REGEX.captures(m_str).unwrap();
    title_reference_node(&caps["text"])
}

/// The node a title — written with its role or as bare interpreted text —
/// shows `text` as.
pub(crate) fn title_reference_node(text: &str) -> InlineNode {
    InlineNode::TitleReference(apply_smart_typography(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_title_reference_match_reads_every_spelling() {
        // Given / When / Then
        for role in [":title-reference:`Dune`", ":title:`Dune`", ":t:`Dune`"] {
            assert_eq!(
                handle_title_reference_match(role),
                InlineNode::TitleReference("Dune".to_string()),
                "{role}"
            );
        }
    }

    #[test]
    fn test_title_reference_node_applies_smart_typography() {
        // Given / When
        let node = title_reference_node("Pride -- Prejudice");

        // Then
        assert_eq!(
            node,
            InlineNode::TitleReference("Pride \u{2013} Prejudice".to_string())
        );
    }
}
