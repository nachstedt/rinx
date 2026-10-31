use serde::{Deserialize, Serialize};

use crate::block_id::BlockId;
use crate::node::Node;

/// A single entry in a `.. glossary::` directive.
///
/// Each entry groups one or more terms (all sharing the same definition)
/// together with the parsed definition body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryEntry {
    /// One or more terms that share this definition.
    pub terms: Vec<GlossaryTerm>,
    /// The definition body, parsed as block-level RST nodes.
    pub definition: Vec<Node>,
}

/// One term of a [`GlossaryEntry`], with the line it was written on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryTerm {
    /// The term as written, classifier and index key already stripped.
    pub text: String,
    /// The term's line, which a `:term:` reference's definition is, as the
    /// parse's source map records it (`docs/decisions/045-definition-spans.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<BlockId>,
}

impl GlossaryTerm {
    /// A term written nowhere in particular — a fixture's, or one whose
    /// position is not known.
    #[must_use]
    pub fn unplaced(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            block: None,
        }
    }
}

/// Generates the HTML anchor `id` for a glossary term.
///
/// Lowercases the term and replaces runs of whitespace with hyphens,
/// then prepends `"term-"`. This is consistent with Sphinx's HTML output.
///
/// # Examples
///
/// ```
/// use rinx_ast::term_id;
/// assert_eq!(term_id("Environment Variable"), "term-environment-variable");
/// assert_eq!(term_id("python"), "term-python");
/// ```
#[must_use]
pub fn term_id(term: &str) -> String {
    let normalized = term
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    format!("term-{normalized}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    #[test]
    fn test_term_id_single_word() {
        // Given
        let term = "python";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-python");
    }

    #[test]
    fn test_term_id_multi_word_joins_with_hyphens() {
        // Given
        let term = "Environment Variable";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-environment-variable");
    }

    #[test]
    fn test_term_id_normalizes_to_lowercase() {
        // Given
        let term = "MY TERM";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-my-term");
    }

    #[test]
    fn test_term_id_collapses_extra_whitespace() {
        // Given
        let term = "  term   with   spaces  ";

        // When
        let id = term_id(term);

        // Then
        assert_eq!(id, "term-term-with-spaces");
    }

    #[test]
    fn test_glossary_entry_serialization_roundtrip() {
        // Given
        let entry = GlossaryEntry {
            terms: vec![GlossaryTerm::unplaced("foo"), GlossaryTerm::unplaced("bar")],
            definition: vec![Node::Paragraph(vec![InlineNode::Text(
                "A definition.".to_string(),
            )])],
        };

        // When
        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let deserialized: GlossaryEntry =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(entry, deserialized);
    }
}
