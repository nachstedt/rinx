use serde::{Deserialize, Serialize};

use crate::code_language::CodeLanguage;
use crate::definition_list_item::DefinitionListItem;
use crate::directive::Directive;
use crate::enumerator::Enumerator;
use crate::hashed_content::HashedContent;
use crate::inline_node::InlineNode;
use crate::line_block::LineBlockItem;
use crate::list_item::ListItem;
use crate::option_list_item::OptionListItem;
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
        items: Vec<ListItem>,
    },
    /// An enumerated (ordered) list.
    ///
    /// `start` is the *list's* enumerator, not the first item's content: it
    /// carries the enumeration sequence, the punctuation format, and the
    /// ordinal the list begins at — docutils' `enumtype`/`prefix`/`suffix`/
    /// `start` attributes bundled into one validated value. Item *n* is
    /// implicitly `start` advanced *n-1* times, because the parser only keeps
    /// consecutive enumerators in one list: any break in the sequence, format
    /// or ordering starts a new list instead of being recorded here.
    EnumeratedList {
        start: Enumerator,
        items: Vec<ListItem>,
    },
    DefinitionList {
        items: Vec<DefinitionListItem>,
    },
    /// An option list (`-h, --help  Show this help.`), documenting a
    /// program's command-line options. See [`OptionListItem`] for the
    /// option-marker grammar.
    OptionList {
        items: Vec<OptionListItem>,
    },
    /// A grid table (`+---+---+` / `|` / `=` ASCII-art syntax). `header_rows`
    /// is empty when the table has no `=`-separated header.
    Table {
        header_rows: Vec<TableRow>,
        body_rows: Vec<TableRow>,
    },
    /// A literal block introduced by a `::` paragraph ending or a standalone
    /// `::`. The option-bearing directive forms live in
    /// [`Directive::CodeBlock`](crate::Directive::CodeBlock) instead.
    LiteralBlock {
        /// Always [`CodeLanguage::Inherit`] as parsed — a `::` block has no
        /// syntax for naming a language. It is still highlighted, taking its
        /// language from the enclosing `.. highlight::` exactly as Sphinx
        /// does, which is why this is a language and not a `bool`.
        language: CodeLanguage,
        /// Verbatim content with common leading indentation stripped.
        content: String,
    },
    /// A docutils *doctest block*: a text block beginning with `>>> ` and
    /// ending at a blank line, with no directive introducing it.
    ///
    /// Deliberately its own variant rather than a flag on [`Self::LiteralBlock`].
    /// The distinction is not cosmetic: a `::`-introduced literal block is
    /// *never* executed, while a doctest block *is* — Sphinx tests these by
    /// default and places them in the `default` group, sharing one Python
    /// namespace with any `.. doctest::` directives in the same document.
    /// Expressing that as a boolean on one variant would make the two
    /// confusable; separate variants make a mix-up a type error.
    DoctestBlock(HashedContent),
    /// An RST comment (`.. text` or `..` followed by an indented body).
    /// Comments produce no output and are discarded during rendering.
    Comment,
    /// A transition (horizontal rule): 4+ repeated punctuation characters on their own
    /// line, blank-line-delimited. Renders as `<hr />`.
    Transition,
    /// A block quote: a text block indented relative to its surrounding
    /// context with no preceding directive/list marker — indentation is the
    /// sole markup indicator. May end with an attribution (`-- Author`),
    /// parsed as inline markup rather than plain text so it can itself carry
    /// emphasis or a cross-reference.
    BlockQuote {
        content: Vec<Node>,
        attribution: Option<Vec<InlineNode>>,
    },
    /// A line block (`| text`): a sequence of lines rendered without
    /// paragraph reflow, for poetry and addresses. Nesting is expressed
    /// purely through the indentation of the text after each `|` — an item
    /// indented further than its siblings becomes a [`LineBlockItem::Nested`]
    /// run rather than a sibling [`LineBlockItem::Line`], grouped once here
    /// at parse time (docutils' `nest_line_block_segment`) rather than on
    /// every render.
    LineBlock(Vec<LineBlockItem>),
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

    #[test]
    fn test_block_quote_node_serialization_roundtrip() {
        // Given
        let node = Node::BlockQuote {
            content: vec![Node::Paragraph(vec![InlineNode::Text(
                "Quoted.".to_string(),
            )])],
            attribution: Some(vec![InlineNode::Text("Sherlock Holmes".to_string())]),
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: Node = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_line_block_node_serialization_roundtrip() {
        // Given
        let node = Node::LineBlock(vec![
            LineBlockItem::Line(vec![InlineNode::Text("First line.".to_string())]),
            LineBlockItem::Nested(vec![LineBlockItem::Line(vec![InlineNode::Text(
                "Nested line.".to_string(),
            )])]),
            LineBlockItem::Line(vec![]),
        ]);

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: Node = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }
}
