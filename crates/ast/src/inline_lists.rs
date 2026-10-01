//! Every `Vec<InlineNode>` a document holds — the inline-content counterpart
//! of [`crate::walk_nodes`], which deliberately descends into no inline
//! content.
//!
//! Two walkers with one reach: [`for_each_inline_list_mut`] for the parser's
//! whole-document passes that rewrite inline content after the parse
//! (resolving `|name|` substitutions, reporting refused roles, numbering
//! `:pep:`/`:rfc:`/`:cve:`/`:cwe:` anchors), and [`for_each_inline_list`]
//! for the analyzer, which must find exactly the inline nodes those passes
//! reached — such a role's general-index entry points at the anchor the
//! parser gave it. Both are generated from one body by
//! [`inline_list_walker`], so the two cannot disagree about which lists
//! exist.

use crate::directive::Directive;
use crate::inline_node::InlineNode;
use crate::line_block::LineBlockItem;
use crate::node::Node;

/// Declares one walker over every inline list, taking its nodes by `&` or,
/// with a trailing `mut`, by `&mut`. `$iter` and `$body` name the matching
/// iterator and domain-object-body accessor.
///
/// Exhaustive rather than a `_` catch-all, so a variant added later that
/// carries inline content is a compile error here rather than a silently
/// unvisited subtree — in both walkers at once.
macro_rules! inline_list_walker {
    (
        $(#[$meta:meta])*
        $walk:ident, $line_block:ident, $directive:ident, $iter:ident, $body:ident $(, $m:tt)?
    ) => {
        $(#[$meta])*
        pub fn $walk(
            nodes: & $($m)? [Node],
            visit: &mut impl FnMut(& $($m)? Vec<InlineNode>),
        ) {
            for node in nodes {
                match node {
                    Node::Heading { text, .. } => visit(text),
                    Node::Paragraph(inlines) => visit(inlines),
                    Node::BlockQuote {
                        content,
                        attribution,
                    } => {
                        $walk(content, visit);
                        if let Some(attribution) = attribution {
                            visit(attribution);
                        }
                    }
                    Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
                        for item in items {
                            $walk(& $($m)? item.nodes, visit);
                        }
                    }
                    Node::DefinitionList { items } => {
                        for item in items {
                            visit(& $($m)? item.term);
                            $walk(& $($m)? item.definition, visit);
                        }
                    }
                    Node::OptionList { items } => {
                        for item in items {
                            $walk(& $($m)? item.description, visit);
                        }
                    }
                    Node::Table {
                        header_rows,
                        body_rows,
                    } => {
                        for row in header_rows.$iter().chain(body_rows.$iter()) {
                            for cell in & $($m)? row.cells {
                                $walk(& $($m)? cell.content, visit);
                            }
                        }
                    }
                    Node::LineBlock(items) => {
                        $line_block(items, visit);
                    }
                    Node::Directive(directive) => {
                        $directive(directive, visit);
                    }
                    // Leaves with no `InlineNode`/`Node` content of their own:
                    // a target's URI, a literal/doctest block's verbatim text,
                    // and a comment/transition carry nothing to visit.
                    Node::Target { .. }
                    | Node::AnonymousTarget { .. }
                    | Node::LiteralBlock { .. }
                    | Node::DoctestBlock(_)
                    | Node::Comment
                    | Node::Transition => {}
                }
            }
        }

        fn $line_block(
            items: & $($m)? [LineBlockItem],
            visit: &mut impl FnMut(& $($m)? Vec<InlineNode>),
        ) {
            for item in items {
                match item {
                    LineBlockItem::Line(inlines) => visit(inlines),
                    LineBlockItem::Nested(nested) => $line_block(nested, visit),
                }
            }
        }

        /// Descends into the block-level/inline children a [`Directive`]
        /// carries. Split out of the node walker for the same reason
        /// [`crate::visit`]'s `walk_directive` is: it is by far the most
        /// branch-heavy arm.
        fn $directive(
            directive: & $($m)? Directive,
            visit: &mut impl FnMut(& $($m)? Vec<InlineNode>),
        ) {
            match directive {
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body } => $walk(body, visit),
                // Every section of an entity is ordinary body content.
                // Attribute values are deliberately *not* visited: they are
                // typed values validated against the schema, not inline markup.
                Directive::Entity(entity) => {
                    for section in & $($m)? entity.sections {
                        $walk(& $($m)? section.body, visit);
                    }
                }
                Directive::EntitySection { body, .. } => $walk(body, visit),
                Directive::Grid(grid) => $walk(& $($m)? grid.body, visit),
                Directive::GridItem(item) => $walk(& $($m)? item.body, visit),
                Directive::Glossary { entries, .. } => {
                    for entry in entries {
                        $walk(& $($m)? entry.definition, visit);
                    }
                }
                Directive::DataTable { rows, .. } => {
                    for row in rows {
                        for cell in & $($m)? row.cells {
                            $walk(& $($m)? cell.content, visit);
                        }
                    }
                }
                Directive::Table {
                    header_rows,
                    body_rows,
                    ..
                } => {
                    for row in header_rows.$iter().chain(body_rows.$iter()) {
                        for cell in & $($m)? row.cells {
                            $walk(& $($m)? cell.content, visit);
                        }
                    }
                }
                // Both halves carry markup: the title is inline-parsed, unlike
                // every other directive caption in this build, and the body is
                // ordinary block content.
                Directive::Dropdown(dropdown) => {
                    visit(& $($m)? dropdown.title);
                    $walk(& $($m)? dropdown.body, visit);
                }
                // Its body is ordinary block content, with no inline-markup
                // title of its own.
                Directive::EntityUpdate(update) => $walk(& $($m)? update.body, visit),
                // Inline markup only: a button's content is its label, not a
                // body.
                Directive::ButtonLink(button) => visit(& $($m)? button.label),
                Directive::DomainObject(body) => $walk(body.$body(), visit),
                Directive::Figure(figure) => {
                    if let Some(caption) = & $($m)? figure.caption {
                        visit(caption);
                    }
                    $walk(& $($m)? figure.legend, visit);
                }
                // No `InlineNode`/`Node` content: an image's `:alt:` and a
                // toctree entry's title are plain strings, not inline-parsed,
                // matching every other caption in this codebase; the rest
                // carry no text at all. An entity table's cells are not text
                // of this document either — they are resolved from the project
                // index while rendering — and neither is a flowchart's or a
                // chart's: their labels come from the same index, or from an
                // option line, which is not inline-parsed either.
                Directive::EntityTable(_)
                | Directive::EntityFlow(_)
                | Directive::EntitySequence(_)
                | Directive::EntityPie(_)
                | Directive::EntityBar(_)
                | Directive::Image(_)
                | Directive::DocTest(_)
                | Directive::CodeBlock(_)
                | Directive::Highlight { .. }
                | Directive::Math { .. }
                | Directive::Toctree { .. }
                | Directive::Contents { .. }
                | Directive::Sectnum(_)
                | Directive::Uml(_)
                | Directive::Index { .. }
                | Directive::PyCurrentModule { .. }
                | Directive::CNamespace { .. }
                | Directive::CNamespacePush { .. }
                | Directive::CNamespacePop
                | Directive::StdProgram { .. }
                | Directive::SubstitutionDefinition(_)
                | Directive::Unknown { .. }
                | Directive::Malformed { .. } => {}
            }
        }
    };
}

inline_list_walker!(
    /// Walks every block-level container the document tree can hold, passing
    /// every `Vec<InlineNode>` it finds to `visit`, in document order.
    for_each_inline_list,
    visit_line_block_items,
    visit_directive,
    iter,
    body
);

inline_list_walker!(
    /// [`for_each_inline_list`], passing each list mutably so a
    /// whole-document pass can rewrite inline content in place.
    for_each_inline_list_mut,
    visit_line_block_items_mut,
    visit_directive_mut,
    iter_mut,
    body_mut,
    mut
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetUri, DefinitionListItem, Figure, ImageOptions, inline_plain_text};

    fn text(value: &str) -> Vec<InlineNode> {
        vec![InlineNode::Text(value.to_string())]
    }

    fn nested_document() -> Vec<Node> {
        let mut inner = Figure::new(ImageOptions::new(AssetUri::new("b.png")));
        inner.caption = Some(text("inner caption"));
        let mut outer = Figure::new(ImageOptions::new(AssetUri::new("a.png")));
        outer.caption = Some(text("outer caption"));
        outer.legend = vec![Node::Directive(Directive::Figure(Box::new(inner)))];
        vec![
            Node::Heading {
                level: 1,
                text: text("title"),
            },
            Node::DefinitionList {
                items: vec![DefinitionListItem {
                    term: text("term"),
                    definition: vec![Node::Paragraph(text("definition"))],
                }],
            },
            Node::LineBlock(vec![LineBlockItem::Nested(vec![LineBlockItem::Line(
                text("line"),
            )])]),
            Node::Directive(Directive::Figure(Box::new(outer))),
        ]
    }

    const EXPECTED: [&str; 6] = [
        "title",
        "term",
        "definition",
        "line",
        "outer caption",
        "inner caption",
    ];

    #[test]
    fn test_for_each_inline_list_reaches_nested_and_directive_inline_content() {
        // Given a heading, a definition term, a nested line and a figure
        // caption in a legend
        let nodes = nested_document();

        // When
        let mut seen = Vec::new();
        for_each_inline_list(&nodes, &mut |list| seen.push(inline_plain_text(list)));

        // Then — every list, in document order
        assert_eq!(seen, EXPECTED);
    }

    #[test]
    fn test_for_each_inline_list_mut_reaches_the_same_lists_mutably() {
        // Given
        let mut nodes = nested_document();

        // When
        let mut seen = Vec::new();
        for_each_inline_list_mut(&mut nodes, &mut |list| {
            seen.push(inline_plain_text(list));
            list.push(InlineNode::Text("!".to_string()));
        });

        // Then — the same lists as the read-only walker, each one mutable
        assert_eq!(seen, EXPECTED);
        let Node::Heading { text: title, .. } = &nodes[0] else {
            unreachable!()
        };
        assert_eq!(inline_plain_text(title), "title!");
    }
}
