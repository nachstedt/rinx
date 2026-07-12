//! Assigns anchor ids to `.. index::` directives in document order.
//!
//! A `.. index::` directive marks a bare location with no content-derived
//! identity (unlike a glossary term or a domain object's name), so an id is
//! minted here, once, right after parsing — rather than having the analyzer
//! and renderer each independently re-derive the same positional counter by
//! walking the tree in lockstep, which would silently desync if either one's
//! recursion shape ever drifted from the other's.

use rusty_sphinx_ast::{Directive, Node};

/// Walks `nodes` mutably, in document order, assigning `format!("index-{n}")`
/// to every `Directive::Index`'s `id` field. Recurses into exactly the
/// container shapes `rusty_sphinx_analyzer::index_nodes` also recurses into
/// (bullet/definition lists, tables, admonition/version-change/seealso
/// bodies, domain-object bodies) — anything outside that set (e.g. nested
/// inside a glossary entry's definition) is a placement the analyzer
/// wouldn't index either, so no id is needed there.
///
/// The assigned id is unique **within this document only** — pair it with
/// `doc_path` wherever it's used downstream.
pub(super) fn assign_index_ids(nodes: &mut [Node], counter: &mut usize) {
    for node in nodes {
        match node {
            Node::Directive(Directive::Index { id, .. }) => {
                *id = format!("index-{counter}");
                *counter += 1;
            }
            Node::Directive(
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                assign_index_ids(body, counter);
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                assign_index_ids(obj.body_mut(), counter);
            }
            Node::BulletList { items, .. } => {
                for item in items {
                    assign_index_ids(&mut item.nodes, counter);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    assign_index_ids(&mut item.definition, counter);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter_mut().chain(body_rows.iter_mut()) {
                    for cell in &mut row.cells {
                        assign_index_ids(&mut cell.content, counter);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{
        AdmonitionKind, BulletListItem, DomainObjectBody, IndexEntry, TableCell, TableRow,
    };

    fn index_directive() -> Node {
        Node::Directive(Directive::Index {
            entries: vec![IndexEntry::Term {
                primary: "x".to_string(),
                subentry: None,
                main: false,
            }],
            id: String::new(),
        })
    }

    fn id_of(node: &Node) -> &str {
        if let Node::Directive(Directive::Index { id, .. }) = node {
            id
        } else {
            panic!("Expected Index directive");
        }
    }

    #[test]
    fn test_assign_index_ids_assigns_sequential_ids_at_top_level() {
        // Given
        let mut nodes = vec![index_directive(), index_directive()];
        let mut counter = 0;

        // When
        assign_index_ids(&mut nodes, &mut counter);

        // Then
        assert_eq!(id_of(&nodes[0]), "index-0");
        assert_eq!(id_of(&nodes[1]), "index-1");
        assert_eq!(counter, 2);
    }

    #[test]
    fn test_assign_index_ids_recurses_into_bullet_list() {
        // Given
        let mut nodes = vec![Node::BulletList {
            bullet: '-',
            items: vec![BulletListItem {
                nodes: vec![index_directive()],
            }],
        }];
        let mut counter = 0;

        // When
        assign_index_ids(&mut nodes, &mut counter);

        // Then
        if let Node::BulletList { items, .. } = &nodes[0] {
            assert_eq!(id_of(&items[0].nodes[0]), "index-0");
        } else {
            panic!("Expected BulletList");
        }
    }

    #[test]
    fn test_assign_index_ids_recurses_into_admonition_body() {
        // Given
        let mut nodes = vec![Node::Directive(Directive::Admonition {
            kind: AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![index_directive()],
        })];
        let mut counter = 0;

        // When
        assign_index_ids(&mut nodes, &mut counter);

        // Then
        if let Node::Directive(Directive::Admonition { body, .. }) = &nodes[0] {
            assert_eq!(id_of(&body[0]), "index-0");
        } else {
            panic!("Expected Admonition");
        }
    }

    #[test]
    fn test_assign_index_ids_recurses_into_domain_object_body() {
        // Given
        let mut nodes = vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::PyFunction {
                signature: "foo()".to_string(),
                body: vec![index_directive()],
            },
        ))];
        let mut counter = 0;

        // When
        assign_index_ids(&mut nodes, &mut counter);

        // Then
        if let Node::Directive(Directive::DomainObject(obj)) = &nodes[0] {
            assert_eq!(id_of(&obj.body()[0]), "index-0");
        } else {
            panic!("Expected DomainObject");
        }
    }

    #[test]
    fn test_assign_index_ids_recurses_into_table_cells() {
        // Given
        let mut nodes = vec![Node::Table {
            header_rows: vec![],
            body_rows: vec![TableRow {
                cells: vec![TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![index_directive()],
                }],
            }],
        }];
        let mut counter = 0;

        // When
        assign_index_ids(&mut nodes, &mut counter);

        // Then
        if let Node::Table { body_rows, .. } = &nodes[0] {
            assert_eq!(id_of(&body_rows[0].cells[0].content[0]), "index-0");
        } else {
            panic!("Expected Table");
        }
    }

    #[test]
    fn test_assign_index_ids_orders_ids_by_document_order_across_containers() {
        // Given — a top-level Index directive followed by one nested in a bullet list
        let mut nodes = vec![
            index_directive(),
            Node::BulletList {
                bullet: '-',
                items: vec![BulletListItem {
                    nodes: vec![index_directive()],
                }],
            },
        ];
        let mut counter = 0;

        // When
        assign_index_ids(&mut nodes, &mut counter);

        // Then
        assert_eq!(id_of(&nodes[0]), "index-0");
        if let Node::BulletList { items, .. } = &nodes[1] {
            assert_eq!(id_of(&items[0].nodes[0]), "index-1");
        } else {
            panic!("Expected BulletList");
        }
    }
}
