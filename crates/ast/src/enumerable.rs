//! The elements Sphinx's `numfig` numbers — figures, tables and code blocks
//! with a caption — and the one walk that finds them.
//!
//! Two phases in two processes must agree on which element is which: the
//! analyzer numbers them for the whole project, and the renderer writes each
//! one's number into its caption. They share [`enumerable_elements`] rather
//! than each counting on their own, and the position an element has in its
//! result is the identifier the project index stores numbers under — so a
//! container one phase descends into and the other does not cannot shift every
//! number after it.
//!
//! Only a *captioned* element is numbered, labelled or not — Sphinx skips an
//! uncaptioned one before it reaches its counter, and gives every captioned
//! one a number whether or not anything can refer to it. A grid or simple
//! table is never captioned: it gets a title only by being wrapped in
//! `.. table::`.

use serde::{Deserialize, Serialize};

use crate::directive::Directive;
use crate::inline_node::inline_plain_text;
use crate::node::Node;
use crate::target_name::TargetName;
use crate::visit::walk_nodes_with_siblings;

/// What kind of thing a `:numref:` points at, which picks the format its
/// number is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnumerableKind {
    Figure,
    Table,
    CodeBlock,
    /// A numbered section — numbered by a `:numbered:` toctree or a
    /// `.. sectnum::`, never by `numfig`, so [`enumerable_elements`] never
    /// yields one.
    Section,
}

impl EnumerableKind {
    /// The key this kind's format has in `rinx.toml`'s `[numfig_format]`, as
    /// in Sphinx's `numfig_format` dictionary.
    #[must_use]
    pub const fn format_key(self) -> &'static str {
        match self {
            Self::Figure => "figure",
            Self::Table => "table",
            Self::CodeBlock => "code-block",
            Self::Section => "section",
        }
    }
}

/// One numbered element, as found by [`enumerable_elements`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumerableElement<'a> {
    /// The directive itself — what the renderer recognizes it by.
    pub directive: &'a Directive,
    pub kind: EnumerableKind,
    /// Every name a `:numref:` can reach it by: the `.. _label:` targets
    /// written directly above it, then its own `:name:`.
    pub labels: Vec<&'a TargetName>,
    /// The caption as plain text — what a `{name}` in a format shows.
    pub caption: String,
    /// The index, in the document's top-level node list, of the node holding
    /// this element (the element itself, when it is written at the top level).
    /// Sections are top-level only, so this is what places it in one.
    pub top_level_index: usize,
}

/// Every numbered element of the document whose top-level nodes are `nodes`,
/// in document order.
#[must_use]
pub fn enumerable_elements(nodes: &[Node]) -> Vec<EnumerableElement<'_>> {
    let mut found = Vec::new();
    let mut top_level_index = 0;
    walk_nodes_with_siblings(nodes, &mut |siblings, index| {
        if std::ptr::eq(siblings, nodes) {
            top_level_index = index;
        }
        let Node::Directive(directive) = &siblings[index] else {
            return;
        };
        let Some((kind, own_name, caption)) = captioned(directive) else {
            return;
        };
        let mut labels = preceding_labels(siblings, index);
        labels.extend(own_name);
        found.push(EnumerableElement {
            directive,
            kind,
            labels,
            caption,
            top_level_index,
        });
    });
    found
}

/// The `.. _label:` targets written directly above `siblings[index]`, in the
/// order they were written.
///
/// A chain of internal targets all name the element after them; anything else
/// — a target with a URI, a comment, a paragraph — ends the chain, as it ends
/// docutils' propagation of a target to the next element.
#[must_use]
pub fn preceding_labels(siblings: &[Node], index: usize) -> Vec<&TargetName> {
    let mut labels: Vec<&TargetName> = siblings[..index]
        .iter()
        .rev()
        .map_while(|node| match node {
            Node::Target { name, uri: None } => Some(name),
            _ => None,
        })
        .collect();
    labels.reverse();
    labels
}

/// The kind, own `:name:` and caption text of a directive `numfig` numbers,
/// or `None` for any other directive — including a figure, table or code
/// block written without a caption.
fn captioned(directive: &Directive) -> Option<(EnumerableKind, Option<&TargetName>, String)> {
    match directive {
        Directive::Figure(figure) => figure.caption.as_ref().map(|caption| {
            (
                EnumerableKind::Figure,
                figure.image.name.as_ref(),
                inline_plain_text(caption),
            )
        }),
        Directive::Table { title, name, .. } | Directive::DataTable { title, name, .. } => title
            .as_ref()
            .map(|title| (EnumerableKind::Table, name.as_ref(), title.clone())),
        Directive::CodeBlock(block) => block.caption.as_ref().map(|caption| {
            (
                EnumerableKind::CodeBlock,
                block.name.as_ref(),
                caption.clone(),
            )
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetUri, CodeBlock, CodeBlockSource, CodeLanguage, Figure, ImageOptions};
    use crate::{InlineNode, TableSource};

    fn figure(caption: Option<&str>, name: Option<&str>) -> Node {
        let mut image = ImageOptions::new(AssetUri::new("a.png"));
        image.name = name.map(TargetName::new);
        let mut figure = Figure::new(image);
        figure.caption = caption.map(|text| vec![InlineNode::Text(text.to_string())]);
        Node::Directive(Directive::Figure(Box::new(figure)))
    }

    fn table(title: Option<&str>, name: Option<&str>) -> Node {
        Node::Directive(Directive::DataTable {
            source: TableSource::List,
            title: title.map(str::to_string),
            header_rows: 0,
            stub_columns: 0,
            widths: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: name.map(TargetName::new),
            rows: Vec::new(),
        })
    }

    fn code(caption: Option<&str>) -> Node {
        Node::Directive(Directive::CodeBlock(CodeBlock {
            source: CodeBlockSource::CodeBlock,
            language: CodeLanguage::Inherit,
            content: "x".to_string(),
            caption: caption.map(str::to_string),
            name: None,
            classes: Vec::new(),
            linenos: false,
            lineno_start: None,
            emphasize_lines: Vec::new(),
            force: false,
            span: None,
        }))
    }

    fn target(name: &str) -> Node {
        Node::Target {
            name: TargetName::new(name),
            uri: None,
        }
    }

    fn kinds(nodes: &[Node]) -> Vec<(EnumerableKind, String, usize)> {
        enumerable_elements(nodes)
            .into_iter()
            .map(|element| (element.kind, element.caption, element.top_level_index))
            .collect()
    }

    #[test]
    fn test_finds_every_captioned_kind_in_document_order() {
        // Given
        let nodes = vec![
            figure(Some("A figure"), None),
            table(Some("A table"), None),
            code(Some("A listing")),
        ];

        // When / Then
        assert_eq!(
            kinds(&nodes),
            vec![
                (EnumerableKind::Figure, "A figure".to_string(), 0),
                (EnumerableKind::Table, "A table".to_string(), 1),
                (EnumerableKind::CodeBlock, "A listing".to_string(), 2),
            ]
        );
    }

    #[test]
    fn test_skips_an_uncaptioned_element_even_when_labelled() {
        // Given
        let nodes = vec![
            target("bare"),
            figure(None, Some("also-bare")),
            table(None, Some("untitled")),
            code(None),
        ];

        // When / Then
        assert!(kinds(&nodes).is_empty());
    }

    #[test]
    fn test_labels_are_the_targets_above_then_the_own_name() {
        // Given
        let nodes = vec![
            target("first"),
            target("second"),
            figure(Some("Caption"), Some("own")),
        ];

        // When
        let elements = enumerable_elements(&nodes);

        // Then
        let labels: Vec<&str> = elements[0].labels.iter().map(|l| l.as_str()).collect();
        assert_eq!(labels, vec!["first", "second", "own"]);
    }

    #[test]
    fn test_a_nested_element_records_its_top_level_ancestor() {
        // Given a figure inside an admonition at top-level index 1
        let nodes = vec![
            Node::Comment,
            Node::Directive(Directive::SeeAlso {
                body: vec![figure(Some("Nested"), None)],
            }),
            figure(Some("After"), None),
        ];

        // When / Then
        assert_eq!(
            kinds(&nodes),
            vec![
                (EnumerableKind::Figure, "Nested".to_string(), 1),
                (EnumerableKind::Figure, "After".to_string(), 2),
            ]
        );
    }

    #[test]
    fn test_the_directive_is_the_node_in_the_document() {
        // Given
        let nodes = vec![figure(Some("Caption"), None)];

        // When
        let elements = enumerable_elements(&nodes);

        // Then — the renderer recognizes an element by this address
        let Node::Directive(directive) = &nodes[0] else {
            unreachable!()
        };
        assert!(std::ptr::eq(elements[0].directive, directive));
    }

    #[test]
    fn test_preceding_labels_stop_at_anything_but_an_internal_target() {
        // Given
        let nodes = vec![
            target("too-far"),
            Node::Comment,
            Node::Target {
                name: TargetName::new("external"),
                uri: Some("https://example.org".to_string()),
            },
            target("near"),
            Node::Transition,
        ];

        // When
        let labels = preceding_labels(&nodes, 4);

        // Then
        assert_eq!(labels, vec![&TargetName::new("near")]);
    }

    #[test]
    fn test_format_key_is_sphinx_numfig_format_key() {
        // Given / When / Then
        assert_eq!(EnumerableKind::Figure.format_key(), "figure");
        assert_eq!(EnumerableKind::Table.format_key(), "table");
        assert_eq!(EnumerableKind::CodeBlock.format_key(), "code-block");
        assert_eq!(EnumerableKind::Section.format_key(), "section");
    }
}
