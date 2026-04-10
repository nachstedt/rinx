//! Abstract Syntax Tree representations for the Rusty-Sphinx Document.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    Heading(String),
    Paragraph(String),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub nodes: Vec<Node>,
}

impl Document {
    #[must_use]
    pub fn new(nodes: Vec<Node>) -> Self {
        Self { nodes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_document_with_given_nodes() {
        // Given
        let nodes = vec![
            Node::Heading("Title".to_string()),
            Node::Paragraph("Body".to_string()),
        ];

        // When
        let doc = Document::new(nodes);

        // Then
        assert_eq!(doc.nodes.len(), 2);
    }
}
