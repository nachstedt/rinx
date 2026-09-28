//! Every `Vec<InlineNode>` a document holds, reached mutably — the
//! inline-content counterpart of [`rinx_ast::walk_nodes`], which deliberately
//! descends into neither inline content nor anything mutable.
//!
//! Shared by the whole-document passes that rewrite inline content after the
//! parse: resolving `|name|` substitutions, and reporting the `:numref:` roles
//! the inline scan refused. The two need the same reach, so they walk with the
//! same function rather than two copies of one exhaustive match.

use rinx_ast::{Directive, InlineNode, Node};

/// Walks every block-level container the document tree can hold, passing
/// every `Vec<InlineNode>` it finds to `visit` — the mutable, inline-content-reaching
/// counterpart of [`rinx_ast::walk_nodes`], which deliberately does
/// neither. Exhaustive rather than a `_` catch-all, so a variant added later
/// that carries inline content is a compile error here rather than a
/// silently unresolved subtree.
pub(crate) fn for_each_inline_list_mut(
    nodes: &mut [Node],
    visit: &mut impl FnMut(&mut Vec<InlineNode>),
) {
    for node in nodes {
        match node {
            Node::Heading { text, .. } => visit(text),
            Node::Paragraph(inlines) => visit(inlines),
            Node::BlockQuote {
                content,
                attribution,
            } => {
                for_each_inline_list_mut(content, visit);
                if let Some(attribution) = attribution {
                    visit(attribution);
                }
            }
            Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
                for item in items {
                    for_each_inline_list_mut(&mut item.nodes, visit);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    visit(&mut item.term);
                    for_each_inline_list_mut(&mut item.definition, visit);
                }
            }
            Node::OptionList { items } => {
                for item in items {
                    for_each_inline_list_mut(&mut item.description, visit);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter_mut().chain(body_rows.iter_mut()) {
                    for cell in &mut row.cells {
                        for_each_inline_list_mut(&mut cell.content, visit);
                    }
                }
            }
            Node::LineBlock(items) => {
                visit_line_block_items(items, visit);
            }
            Node::Directive(directive) => {
                visit_directive(directive, visit);
            }
            // Leaves with no `InlineNode`/`Node` content of their own: a
            // target's URI, a literal/doctest block's verbatim text, and a
            // comment/transition carry nothing this pass can resolve.
            Node::Target { .. }
            | Node::AnonymousTarget { .. }
            | Node::LiteralBlock { .. }
            | Node::DoctestBlock(_)
            | Node::Comment
            | Node::Transition => {}
        }
    }
}

fn visit_line_block_items(
    items: &mut [rinx_ast::LineBlockItem],
    visit: &mut impl FnMut(&mut Vec<InlineNode>),
) {
    for item in items {
        match item {
            rinx_ast::LineBlockItem::Line(inlines) => {
                visit(inlines);
            }
            rinx_ast::LineBlockItem::Nested(nested) => {
                visit_line_block_items(nested, visit);
            }
        }
    }
}

/// Descends into the block-level/inline children a [`Directive`] carries.
/// Split out of [`for_each_inline_list_mut`] for the same reason
/// [`rinx_ast::visit::walk_directive`] is: it is by far the most
/// branch-heavy arm.
fn visit_directive(directive: &mut Directive, visit: &mut impl FnMut(&mut Vec<InlineNode>)) {
    match directive {
        Directive::Admonition { body, .. }
        | Directive::VersionChange { body, .. }
        | Directive::SeeAlso { body } => for_each_inline_list_mut(body, visit),
        // Every section of an entity is ordinary body content, so a `|name|`
        // written inside a `.. verification-criteria::` resolves like any other.
        // Attribute values are deliberately *not* touched: they are typed
        // values validated against the schema, not inline markup.
        Directive::Entity(entity) => {
            for section in &mut entity.sections {
                for_each_inline_list_mut(&mut section.body, visit);
            }
        }
        Directive::EntitySection { body, .. } => {
            for_each_inline_list_mut(body, visit);
        }
        Directive::Grid(grid) => for_each_inline_list_mut(&mut grid.body, visit),
        Directive::GridItem(item) => {
            for_each_inline_list_mut(&mut item.body, visit);
        }
        Directive::Glossary { entries, .. } => {
            for entry in entries {
                for_each_inline_list_mut(&mut entry.definition, visit);
            }
        }
        Directive::DataTable { rows, .. } => {
            for row in rows {
                for cell in &mut row.cells {
                    for_each_inline_list_mut(&mut cell.content, visit);
                }
            }
        }
        Directive::Table {
            header_rows,
            body_rows,
            ..
        } => {
            for row in header_rows.iter_mut().chain(body_rows.iter_mut()) {
                for cell in &mut row.cells {
                    for_each_inline_list_mut(&mut cell.content, visit);
                }
            }
        }
        // Both halves carry markup: the title is inline-parsed, unlike every
        // other directive caption in this build, and the body is ordinary
        // block content.
        Directive::Dropdown(dropdown) => {
            visit(&mut dropdown.title);
            for_each_inline_list_mut(&mut dropdown.body, visit);
        }
        // Its body is ordinary block content, with no inline-markup title of
        // its own to resolve.
        Directive::EntityUpdate(update) => {
            for_each_inline_list_mut(&mut update.body, visit);
        }
        // Inline markup only: a button's content is its label, not a body.
        Directive::ButtonLink(button) => {
            visit(&mut button.label);
        }
        Directive::DomainObject(body) => {
            for_each_inline_list_mut(body.body_mut(), visit);
        }
        Directive::Figure(figure) => {
            if let Some(caption) = &mut figure.caption {
                visit(caption);
            }
            for_each_inline_list_mut(&mut figure.legend, visit);
        }
        // No `InlineNode`/`Node` content: an image's `:alt:` and a toctree
        // entry's title are plain strings, not inline-parsed, matching every
        // other caption in this codebase; the rest carry no text at all. An
        // entity table's cells are not text of this document either — they are
        // resolved from the project index while rendering, so a `|name|` in
        // one would have to be substituted there, where the definition no
        // longer exists, and neither is a flowchart's or a pie chart's — their
        // labels come from the same index, or from an option line, which is
        // not inline-parsed either.
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

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{AssetUri, DefinitionListItem, Figure, ImageOptions};

    fn text(value: &str) -> Vec<InlineNode> {
        vec![InlineNode::Text(value.to_string())]
    }

    #[test]
    fn test_for_each_inline_list_mut_reaches_nested_and_directive_inline_content() {
        // Given a heading, a definition term, and a figure caption in a legend
        let mut inner = Figure::new(ImageOptions::new(AssetUri::new("b.png")));
        inner.caption = Some(text("inner caption"));
        let mut outer = Figure::new(ImageOptions::new(AssetUri::new("a.png")));
        outer.caption = Some(text("outer caption"));
        outer.legend = vec![Node::Directive(Directive::Figure(Box::new(inner)))];
        let mut nodes = vec![
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
            Node::Directive(Directive::Figure(Box::new(outer))),
        ];

        // When
        let mut seen = Vec::new();
        for_each_inline_list_mut(&mut nodes, &mut |list| {
            seen.push(rinx_ast::inline_plain_text(list));
            list.push(InlineNode::Text("!".to_string()));
        });

        // Then — every list, in document order, and each one mutable
        assert_eq!(
            seen,
            vec![
                "title",
                "term",
                "definition",
                "outer caption",
                "inner caption"
            ]
        );
        let Node::Heading { text: title, .. } = &nodes[0] else {
            unreachable!()
        };
        assert_eq!(rinx_ast::inline_plain_text(title), "title!");
    }
}
