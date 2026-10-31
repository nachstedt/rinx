use serde::{Deserialize, Serialize};

use crate::block_id::BlockId;
use crate::code_language::CodeLanguage;
use crate::definition_list_item::DefinitionListItem;
use crate::directive::Directive;
use crate::enumerator::Enumerator;
use crate::hashed_content::HashedContent;
use crate::inline_node::InlineNode;
use crate::line_block::LineBlockItem;
use crate::link_destination::LinkDestination;
use crate::list_item::ListItem;
use crate::named::Named;
use crate::option_list_item::OptionListItem;
use crate::table::TableRow;
use crate::target_name::TargetName;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    Heading {
        level: u8,
        text: Vec<InlineNode>,
        /// The title's line, where an editor's outline and a definition
        /// lead, as the parse's source map records it
        /// (`docs/decisions/045-definition-spans.md`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
    Paragraph {
        content: Vec<InlineNode>,
        /// The lines the block was written over, as the parse's source map
        /// records them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
    Directive {
        directive: Directive,
        /// The marker's lines, where an editor's outline and a definition
        /// lead, as the parse's source map records them
        /// (`docs/decisions/045-definition-spans.md`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
    /// `.. _name:` — an internal target, labelling what follows — or
    /// `.. _name: destination`, which is a URI or another target's name.
    Target {
        /// The label, with the span of its characters.
        name: Named<TargetName>,
        destination: Option<LinkDestination>,
        /// Where the target was written, which the label's definition is, as
        /// the parse's source map records it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
    /// `.. __: destination`, consumed in order by anonymous references.
    AnonymousTarget {
        destination: LinkDestination,
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
        /// The lines the block was written over, as the parse's source map
        /// records them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
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
        /// The lines from the `::` to the block's last, as the parse's
        /// source map records them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
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
    DoctestBlock {
        content: HashedContent,
        /// The lines the block was written over, as the parse's source map
        /// records them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
    /// An RST comment (`.. text` or `..` followed by an indented body).
    /// Comments produce no output and are discarded during rendering.
    Comment,
    /// A transition (horizontal rule): 4+ repeated punctuation characters on their own
    /// line, blank-line-delimited. Renders as `<hr />`.
    Transition {
        /// The lines the block was written over, as the parse's source map
        /// records them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
    /// A block quote: a text block indented relative to its surrounding
    /// context with no preceding directive/list marker — indentation is the
    /// sole markup indicator. May end with an attribution (`-- Author`),
    /// parsed as inline markup rather than plain text so it can itself carry
    /// emphasis or a cross-reference.
    BlockQuote {
        content: Vec<Node>,
        attribution: Option<Vec<InlineNode>>,
        /// The lines the block was written over, as the parse's source map
        /// records them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
    /// A line block (`| text`): a sequence of lines rendered without
    /// paragraph reflow, for poetry and addresses. Nesting is expressed
    /// purely through the indentation of the text after each `|` — an item
    /// indented further than its siblings becomes a [`LineBlockItem::Nested`]
    /// run rather than a sibling [`LineBlockItem::Line`], grouped once here
    /// at parse time (docutils' `nest_line_block_segment`) rather than on
    /// every render.
    LineBlock {
        items: Vec<LineBlockItem>,
        /// The lines the block was written over, as the parse's source map
        /// records them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block: Option<BlockId>,
    },
}

impl Node {
    /// A paragraph of `content`, recorded at no lines yet — the parser's
    /// block loop records them once the paragraph's extent is known.
    #[must_use]
    pub fn paragraph(content: Vec<InlineNode>) -> Self {
        Self::Paragraph {
            content,
            block: None,
        }
    }

    /// A node of `directive`, recorded at no lines yet — the directive
    /// parser records its marker's once the directive is read.
    #[must_use]
    pub fn directive(directive: Directive) -> Self {
        Self::Directive {
            directive,
            block: None,
        }
    }

    /// The lines this node was written over, when the parse recorded them:
    /// its handle into the [`SourceMap`](crate::SourceMap).
    #[must_use]
    pub fn block(&self) -> Option<BlockId> {
        match self {
            Self::Heading { block, .. }
            | Self::Paragraph { block, .. }
            | Self::Directive { block, .. }
            | Self::Target { block, .. }
            | Self::Table { block, .. }
            | Self::LiteralBlock { block, .. }
            | Self::DoctestBlock { block, .. }
            | Self::Transition { block }
            | Self::BlockQuote { block, .. }
            | Self::LineBlock { block, .. } => *block,
            Self::AnonymousTarget { .. }
            | Self::BulletList { .. }
            | Self::EnumeratedList { .. }
            | Self::DefinitionList { .. }
            | Self::OptionList { .. }
            | Self::Comment => None,
        }
    }

    /// Where this node's [`Self::block`] is kept, for the parser to record
    /// it — `None` for a node that is never placed: a list, whose items are,
    /// and the nodes that render nothing.
    pub fn block_mut(&mut self) -> Option<&mut Option<BlockId>> {
        match self {
            Self::Heading { block, .. }
            | Self::Paragraph { block, .. }
            | Self::Directive { block, .. }
            | Self::Target { block, .. }
            | Self::Table { block, .. }
            | Self::LiteralBlock { block, .. }
            | Self::DoctestBlock { block, .. }
            | Self::Transition { block }
            | Self::BlockQuote { block, .. }
            | Self::LineBlock { block, .. } => Some(block),
            Self::AnonymousTarget { .. }
            | Self::BulletList { .. }
            | Self::EnumeratedList { .. }
            | Self::DefinitionList { .. }
            | Self::OptionList { .. }
            | Self::Comment => None,
        }
    }
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
                    content: vec![Node::paragraph(vec![InlineNode::Text(
                        "Header".to_string(),
                    )])],
                }],
            }],
            body_rows: vec![TableRow {
                cells: vec![TableCell {
                    colspan: 2,
                    rowspan: 3,
                    content: vec![Node::paragraph(vec![InlineNode::Text("Body".to_string())])],
                }],
            }],
            block: None,
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
            content: vec![Node::paragraph(vec![InlineNode::Text(
                "Quoted.".to_string(),
            )])],
            attribution: Some(vec![InlineNode::Text("Sherlock Holmes".to_string())]),
            block: None,
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
        let node = Node::LineBlock {
            items: vec![
                LineBlockItem::Line(vec![InlineNode::Text("First line.".to_string())]),
                LineBlockItem::Nested(vec![LineBlockItem::Line(vec![InlineNode::Text(
                    "Nested line.".to_string(),
                )])]),
                LineBlockItem::Line(vec![]),
            ],
            block: None,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: Node = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_paragraph_and_directive_build_unplaced_nodes() {
        // Given / When
        let paragraph = Node::paragraph(vec![InlineNode::Text("A.".to_string())]);
        let directive = Node::directive(Directive::CNamespace { namespace: None });

        // Then
        assert_eq!(
            paragraph,
            Node::Paragraph {
                content: vec![InlineNode::Text("A.".to_string())],
                block: None,
            }
        );
        assert_eq!(
            directive,
            Node::Directive {
                directive: Directive::CNamespace { namespace: None },
                block: None,
            }
        );
    }

    #[test]
    fn test_block_reads_back_what_block_mut_records() {
        // Given
        let mut map = crate::SourceMap::default();
        let id = map.record_block_lines(crate::Span::new(
            crate::Position::new(1, 1),
            crate::Position::new(1, 5),
        ));
        let mut node = Node::paragraph(Vec::new());

        // When
        *node.block_mut().expect("a paragraph is placed") = Some(id);

        // Then
        assert_eq!(node.block(), Some(id));
    }

    #[test]
    fn test_a_list_and_a_comment_take_no_lines() {
        // Given
        let mut list = Node::BulletList {
            bullet: '-',
            items: vec![],
        };

        // When / Then — a list's items are placed, not the list
        assert_eq!(list.block_mut(), None);
        assert_eq!(list.block(), None);
        assert_eq!(Node::Comment.block(), None);
    }

    #[test]
    fn test_an_unplaced_block_is_left_out_of_the_ast_file() {
        // Given
        let node = Node::Transition { block: None };

        // When
        let json = serde_json::to_string(&node).expect("serializes");

        // Then
        assert_eq!(json, r#"{"Transition":{}}"#);
    }
}
