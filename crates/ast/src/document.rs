use serde::{Deserialize, Serialize};

use crate::diagnostic::Diagnostic;
use crate::node::Node;
use crate::suppression::Suppression;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub path: String,
    pub nodes: Vec<Node>,
    /// What went wrong while parsing, recorded rather than raised — the
    /// parser degrades bad input and lets the build step decide whether any
    /// of this is fatal.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    /// The `.. noqa:` comments this document carries, already resolved to the
    /// line ranges they cover.
    ///
    /// Serialized with the AST because the diagnostics they silence are not
    /// all raised in the same process: a broken link is found at *render*
    /// time, long after the comment that excuses it was parsed.
    #[serde(default)]
    pub suppressions: Vec<Suppression>,
}

impl Document {
    #[must_use]
    pub const fn new(path: String, nodes: Vec<Node>) -> Self {
        Self {
            path,
            nodes,
            diagnostics: Vec::new(),
            suppressions: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    #[test]
    fn test_new_creates_document_with_given_nodes() {
        // Given
        let nodes = vec![
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Title".to_string())],
            },
            Node::Paragraph(vec![InlineNode::Text("Body".to_string())]),
        ];

        // When
        let doc = Document::new("test.rst".to_string(), nodes);

        // Then
        assert_eq!(doc.nodes.len(), 2);
    }
}
