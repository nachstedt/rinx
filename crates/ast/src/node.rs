use serde::{Deserialize, Serialize};

use crate::bullet_list_item::BulletListItem;
use crate::definition_list_item::DefinitionListItem;
use crate::directive::Directive;
use crate::inline_node::InlineNode;
use crate::table::TableRow;
use crate::target_name::TargetName;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    Heading {
        level: u8,
        text: Vec<InlineNode>,
    },
    Paragraph(Vec<InlineNode>),
    Directive(Directive),
    Target {
        name: TargetName,
        uri: Option<String>,
    },
    AnonymousTarget {
        uri: String,
    },
    BulletList {
        bullet: char,
        items: Vec<BulletListItem>,
    },
    DefinitionList {
        items: Vec<DefinitionListItem>,
    },
    /// A grid table (`+---+---+` / `|` / `=` ASCII-art syntax). `header_rows`
    /// is empty when the table has no `=`-separated header.
    Table {
        header_rows: Vec<TableRow>,
        body_rows: Vec<TableRow>,
    },
    LiteralBlock {
        /// The language hint (e.g. `"python"`), if specified via `.. code-block:: lang`.
        /// `None` for plain `::` paragraph-introduced blocks.
        language: Option<String>,
        /// Verbatim content with common leading indentation stripped.
        content: String,
    },
    /// An RST comment (`.. text` or `..` followed by an indented body).
    /// Comments produce no output and are discarded during rendering.
    Comment,
    /// A transition (horizontal rule): 4+ repeated punctuation characters on their own
    /// line, blank-line-delimited. Renders as `<hr />`.
    Transition,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::TableCell;

    #[test]
    fn test_table_node_serialization_roundtrip() {
        // Given
        let node = Node::Table {
            header_rows: vec![TableRow {
                cells: vec![TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Header".to_string(),
                    )])],
                }],
            }],
            body_rows: vec![TableRow {
                cells: vec![TableCell {
                    colspan: 2,
                    rowspan: 3,
                    content: vec![Node::Paragraph(vec![InlineNode::Text("Body".to_string())])],
                }],
            }],
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: Node = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }
}
