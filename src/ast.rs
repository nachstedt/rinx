//! Abstract Syntax Tree representations for the Rusty-Sphinx Document.

#[derive(Debug, PartialEq, Eq)]
pub enum Node {
    Heading(String),
    Paragraph(String),
}

#[derive(Debug, PartialEq, Eq)]
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
    fn test_document_new() {
        let nodes = vec![
            Node::Heading("Title".to_string()),
            Node::Paragraph("Body".to_string()),
        ];
        let doc = Document::new(nodes);
        assert_eq!(doc.nodes.len(), 2);
    }
}
