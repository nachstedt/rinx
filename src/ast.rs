//! Abstract Syntax Tree representations for the Rusty-Sphinx Document.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Directive {
    Toctree {
        paths: Vec<String>,
    },
    Unknown {
        name: String,
        argument: String,
        body: String,
    },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineNode {
    Text(String),
    Reference(String),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    Heading { level: u8, text: String },
    Paragraph(Vec<InlineNode>),
    Directive(Directive),
    Target(String),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub path: String,
    pub nodes: Vec<Node>,
}

impl Document {
    #[must_use]
    pub fn new(path: String, nodes: Vec<Node>) -> Self {
        Self { path, nodes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_document_with_given_nodes() {
        // Given
        let nodes = vec![
            Node::Heading {
                level: 1,
                text: "Title".to_string(),
            },
            Node::Paragraph(vec![InlineNode::Text("Body".to_string())]),
        ];

        // When
        let doc = Document::new("test.rst".to_string(), nodes);

        // Then
        assert_eq!(doc.nodes.len(), 2);
    }
}
