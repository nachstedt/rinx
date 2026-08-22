//! Pre-order traversal over a document's block-level nodes.
//!
//! # Scope
//!
//! This walker exists for the *collect matching nodes* use cases — the pipeline
//! phases that need to find every node of some kind anywhere in a document,
//! regardless of nesting, and that carry no state across the traversal:
//! [`crate::Directive::PlantUml`] extraction and validation being the original
//! two.
//!
//! It is deliberately **not** a general refactoring target for
//! `rusty_sphinx_analyzer`'s `index_nodes` or `rusty_sphinx_renderer`'s
//! `render_nodes`. Those push and pop domain scope around a node's children and
//! interleave output with the walk, neither of which a `FnMut(&Node)` visitor
//! can express. Rewriting them on top of this would require a visitor that
//! brackets every descent, which is a different (and much larger) abstraction.
//!
//! # Exhaustiveness
//!
//! [`walk_nodes`]'s inner `match` lists every [`Node`] variant explicitly, with
//! no `_` arm. That is the point: a future variant that carries child nodes
//! becomes a compile error here rather than a silently unvisited subtree. This
//! is the same protection `render_directive` has, and that
//! `rusty_sphinx_analyzer`'s `index_nodes` conspicuously lacks.

use crate::directive::Directive;
use crate::node::Node;

/// Visits `nodes` and every block-level node nested within them, in document
/// (pre-order) order: a container is passed to `visit` before its children.
///
/// Inline content is not traversed — no [`crate::InlineNode`] contains a
/// [`Node`].
///
/// # Examples
///
/// ```
/// use rusty_sphinx_ast::{walk_nodes, BulletListItem, Node};
///
/// let nodes = vec![Node::BulletList {
///     bullet: '-',
///     items: vec![BulletListItem {
///         nodes: vec![Node::Comment],
///     }],
/// }];
///
/// let mut seen = 0;
/// walk_nodes(&nodes, &mut |_| seen += 1);
/// assert_eq!(seen, 2); // the list itself, then the nested comment
/// ```
///
/// The visited references share `nodes`' lifetime rather than being bound to
/// each call, so a visitor may collect them:
///
/// ```
/// use rusty_sphinx_ast::{walk_nodes, Node};
///
/// let nodes = vec![Node::Transition, Node::Comment];
/// let mut collected: Vec<&Node> = Vec::new();
/// walk_nodes(&nodes, &mut |node| collected.push(node));
/// assert_eq!(collected.len(), 2);
/// ```
pub fn walk_nodes<'a>(nodes: &'a [Node], visit: &mut impl FnMut(&'a Node)) {
    for node in nodes {
        visit(node);

        match node {
            Node::Directive(directive) => walk_directive(directive, visit),
            Node::BulletList { items, .. } => {
                for item in items {
                    walk_nodes(&item.nodes, visit);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    walk_nodes(&item.definition, visit);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter().chain(body_rows) {
                    for cell in &row.cells {
                        walk_nodes(&cell.content, visit);
                    }
                }
            }
            // Leaf nodes: no block-level children to descend into.
            Node::Heading { .. }
            | Node::Paragraph(_)
            | Node::Target { .. }
            | Node::AnonymousTarget { .. }
            | Node::LiteralBlock { .. }
            | Node::Comment
            | Node::Transition => {}
        }
    }
}

/// Descends into the block-level children a [`Directive`] carries.
///
/// Split out from [`walk_nodes`] so both matches stay exhaustive and readable;
/// the directive arm of `Node` is by far the most branch-heavy.
fn walk_directive<'a>(directive: &'a Directive, visit: &mut impl FnMut(&'a Node)) {
    match directive {
        Directive::Admonition { body, .. }
        | Directive::VersionChange { body, .. }
        | Directive::SeeAlso { body } => walk_nodes(body, visit),
        Directive::Glossary { entries, .. } => {
            for entry in entries {
                walk_nodes(&entry.definition, visit);
            }
        }
        Directive::ListTable { rows, .. } => {
            for row in rows {
                for cell in &row.cells {
                    walk_nodes(&cell.content, visit);
                }
            }
        }
        Directive::DomainObject(body) => walk_nodes(body.body(), visit),
        // Directives with no block-level children. A doctest block's body is
        // verbatim text, not nested nodes, so there is nothing to descend into.
        Directive::DocTest(_)
        | Directive::Toctree { .. }
        | Directive::PlantUml(_)
        | Directive::Index { .. }
        | Directive::PyCurrentModule { .. }
        | Directive::CNamespace { .. }
        | Directive::CNamespacePush { .. }
        | Directive::CNamespacePop
        | Directive::Unknown { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admonition_kind::AdmonitionKind;
    use crate::bullet_list_item::BulletListItem;
    use crate::definition_list_item::DefinitionListItem;
    use crate::domain_object_body::DomainObjectBody;
    use crate::glossary_entry::GlossaryEntry;
    use crate::hashed_content::HashedContent;
    use crate::inline_node::InlineNode;
    use crate::non_empty_vector::NonEmptyVector;
    use crate::table::{TableCell, TableRow};

    /// A `.. plantuml::` node whose body identifies it in assertions.
    fn diagram(body: &str) -> Node {
        Node::Directive(Directive::PlantUml(HashedContent::new(body.to_string())))
    }

    /// Collects the bodies of every `PlantUml` directive the walker visits.
    fn walk_diagram_bodies(nodes: &[Node]) -> Vec<String> {
        let mut bodies = Vec::new();
        walk_nodes(nodes, &mut |node| {
            if let Node::Directive(Directive::PlantUml(content)) = node {
                bodies.push(content.body().to_string());
            }
        });
        bodies
    }

    /// Wraps `inner` in a single-celled table row.
    fn row_with(inner: Vec<Node>) -> TableRow {
        TableRow {
            cells: vec![TableCell {
                colspan: 1,
                rowspan: 1,
                content: inner,
            }],
        }
    }

    #[test]
    fn test_walk_nodes_visits_top_level_nodes_in_document_order() {
        // Given
        let nodes = vec![diagram("first"), Node::Transition, diagram("second")];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["first", "second"]);
    }

    #[test]
    fn test_walk_nodes_visits_a_container_before_its_children() {
        // Given
        let nodes = vec![Node::BulletList {
            bullet: '-',
            items: vec![BulletListItem {
                nodes: vec![Node::Transition],
            }],
        }];

        // When
        let mut visited = Vec::new();
        walk_nodes(&nodes, &mut |node| visited.push(node.clone()));

        // Then — pre-order: the list, then its child.
        assert_eq!(visited.len(), 2);
        assert!(matches!(visited[0], Node::BulletList { .. }));
        assert_eq!(visited[1], Node::Transition);
    }

    #[test]
    fn test_walk_nodes_descends_into_admonition_bodies() {
        // Given — the case that the top-level-only scan used to miss.
        let nodes = vec![Node::Directive(Directive::Admonition {
            kind: AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![diagram("nested-in-note")],
        })];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-note"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_version_change_bodies() {
        // Given
        use crate::version_change_kind::VersionChangeKind;
        let nodes = vec![Node::Directive(Directive::VersionChange {
            kind: VersionChangeKind::Added,
            version: "1.0".to_string(),
            body: vec![diagram("nested-in-versionadded")],
        })];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-versionadded"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_seealso_bodies() {
        // Given
        let nodes = vec![Node::Directive(Directive::SeeAlso {
            body: vec![diagram("nested-in-seealso")],
        })];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-seealso"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_bullet_list_items() {
        // Given
        let nodes = vec![Node::BulletList {
            bullet: '*',
            items: vec![
                BulletListItem {
                    nodes: vec![diagram("item-one")],
                },
                BulletListItem {
                    nodes: vec![diagram("item-two")],
                },
            ],
        }];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["item-one", "item-two"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_definition_list_definitions() {
        // Given
        let nodes = vec![Node::DefinitionList {
            items: vec![DefinitionListItem {
                term: vec![InlineNode::Text("term".to_string())],
                definition: vec![diagram("nested-in-definition")],
            }],
        }];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-definition"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_grid_table_header_and_body_cells() {
        // Given
        let nodes = vec![Node::Table {
            header_rows: vec![row_with(vec![diagram("in-header")])],
            body_rows: vec![row_with(vec![diagram("in-body")])],
        }];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["in-header", "in-body"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_list_table_cells() {
        // Given
        let nodes = vec![Node::Directive(Directive::ListTable {
            title: None,
            header_rows: 0,
            stub_columns: 0,
            widths: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: None,
            rows: vec![row_with(vec![diagram("in-list-table")])],
        })];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["in-list-table"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_glossary_definitions() {
        // Given
        let nodes = vec![Node::Directive(Directive::Glossary {
            entries: vec![GlossaryEntry {
                terms: vec!["term".to_string()],
                definition: vec![diagram("nested-in-glossary")],
            }],
            sorted: false,
        })];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-glossary"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_domain_object_bodies() {
        // Given
        let nodes = vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CFunction {
                signatures: NonEmptyVector::single("int add(int a, int b)".into()),
                body: vec![diagram("nested-in-domain-object")],
            },
        ))];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-domain-object"]);
    }

    #[test]
    fn test_walk_nodes_descends_through_several_levels_of_nesting() {
        // Given — a diagram buried three containers deep.
        let nodes = vec![Node::Directive(Directive::SeeAlso {
            body: vec![Node::BulletList {
                bullet: '-',
                items: vec![BulletListItem {
                    nodes: vec![Node::Directive(Directive::Admonition {
                        kind: AdmonitionKind::Warning,
                        title: None,
                        collapsible: None,
                        body: vec![diagram("deeply-nested")],
                    })],
                }],
            }],
        })];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["deeply-nested"]);
    }

    #[test]
    fn test_walk_nodes_visits_nothing_for_an_empty_document() {
        // Given
        let nodes: Vec<Node> = Vec::new();

        // When
        let mut visited = 0;
        walk_nodes(&nodes, &mut |_| visited += 1);

        // Then
        assert_eq!(visited, 0);
    }

    #[test]
    fn test_walk_nodes_does_not_descend_into_leaf_directives() {
        // Given — a directive family that carries no block-level children.
        let nodes = vec![
            Node::Directive(Directive::CNamespacePop),
            Node::Directive(Directive::Unknown {
                name: "doctest".to_string(),
                argument: String::new(),
                body: "unparsed body".to_string(),
            }),
        ];

        // When
        let mut visited = 0;
        walk_nodes(&nodes, &mut |_| visited += 1);

        // Then — the two directives themselves, and nothing more.
        assert_eq!(visited, 2);
    }
}
