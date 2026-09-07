use serde::{Deserialize, Serialize};

use crate::node::Node;
use crate::span::Span;

/// Which section of an entity's body this is.
///
/// The unnamed leading prose gets its own variant rather than being spelled as
/// `Named("content")`, so a schema that declares a section genuinely called
/// `content` cannot be mistaken for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionKind {
    /// The unnamed prose before the first named sub-directive.
    Content,
    /// A sub-directive the entity's type declares, named as it was written.
    Named(String),
}

impl SectionKind {
    /// The declared name, or `None` for the unnamed content.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Content => None,
            Self::Named(name) => Some(name),
        }
    }
}

/// One run of parsed prose inside an entity.
///
/// A section is the *document* half of the model: its body is fully-parsed
/// RST — nested directives, cross-references, code blocks, further entities —
/// as opposed to an attribute, which is a typed value. That is why this holds
/// `Vec<Node>` and never a string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySection {
    pub kind: SectionKind,
    pub body: Vec<Node>,
    /// Where the sub-directive was written, for the diagnostics that must name
    /// it. `None` for the content section, which has no marker of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl EntitySection {
    /// Builds the unnamed content section.
    #[must_use]
    pub const fn content(body: Vec<Node>) -> Self {
        Self {
            kind: SectionKind::Content,
            body,
            span: None,
        }
    }

    /// Builds a named section.
    #[must_use]
    pub fn named(name: String, body: Vec<Node>, span: Option<Span>) -> Self {
        Self {
            kind: SectionKind::Named(name),
            body,
            span,
        }
    }

    /// The declared name, or `None` for the content section.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.kind.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    fn paragraph(text: &str) -> Vec<Node> {
        vec![Node::Paragraph(vec![InlineNode::Text(text.to_string())])]
    }

    #[test]
    fn test_content_section_has_no_name() {
        // Given
        let section = EntitySection::content(paragraph("prose"));

        // When
        let name = section.name();

        // Then
        assert_eq!(name, None);
        assert_eq!(section.kind, SectionKind::Content);
    }

    #[test]
    fn test_named_section_reports_the_name_as_written() {
        // Given
        let section = EntitySection::named(
            "verification-criteria".to_string(),
            paragraph("measured"),
            None,
        );

        // When
        let name = section.name();

        // Then
        assert_eq!(name, Some("verification-criteria"));
    }

    #[test]
    fn test_a_section_named_content_is_not_the_content_section() {
        // Given — the reason Content is its own variant rather than a name
        let declared = EntitySection::named("content".to_string(), paragraph("x"), None);
        let unnamed = EntitySection::content(paragraph("x"));

        // When / Then
        assert_eq!(declared.name(), Some("content"));
        assert_eq!(unnamed.name(), None);
        assert_ne!(declared.kind, unnamed.kind);
    }

    #[test]
    fn test_section_keeps_its_parsed_body_rather_than_text() {
        // Given
        let section = EntitySection::content(paragraph("prose"));

        // When
        let body = &section.body;

        // Then
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0], Node::Paragraph(_)));
    }

    #[test]
    fn test_section_survives_a_serialization_round_trip() {
        // Given
        let original =
            EntitySection::named("safety-comment".to_string(), paragraph("careful"), None);

        // When
        let json = serde_json::to_string(&original).unwrap();
        let restored: EntitySection = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(original, restored);
    }
}
