//! Assigns anchor ids to `.. index::` directives in document order, and then
//! to the inline roles whose general-index entries need an anchor too: the
//! registry roles (`:pep:`, `:rfc:`, `:cve:`, `:cwe:`) and `:index:`.
//!
//! A `.. index::` directive marks a bare location with no content-derived
//! identity (unlike a glossary term or a domain object's name), so an id is
//! minted here, once, right after parsing — rather than having the analyzer
//! and renderer each independently re-derive the same positional counter by
//! walking the tree in lockstep, which would silently desync if either one's
//! recursion shape ever drifted from the other's.

use rinx_ast::{Directive, InlineNode, Node, for_each_inline_list_mut};

/// Walks `nodes` mutably, in document order, assigning `format!("index-{n}")`
/// to every `Directive::Index`'s `id` field. Recurses into exactly the
/// container shapes `rinx_analyzer::index_nodes` also recurses into
/// (bullet/enumerated/definition lists, tables, admonition/version-change/seealso
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
            // The three sphinx-design containers. Their bodies are ordinary
            // content, so an `.. index::` written in one needs an id like any
            // other — and without these arms it would silently get none.
            Node::Directive(Directive::Dropdown(dropdown)) => {
                assign_index_ids(&mut dropdown.body, counter);
            }
            Node::Directive(Directive::Grid(grid)) => {
                assign_index_ids(&mut grid.body, counter);
            }
            Node::Directive(Directive::GridItem(item)) => {
                assign_index_ids(&mut item.body, counter);
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                assign_index_ids(obj.body_mut(), counter);
            }
            Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
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

/// Gives every registry role and `:index:` role in `nodes` its anchor id, in
/// document order among themselves, continuing the numbering
/// [`assign_index_ids`] left in `counter`.
///
/// Sphinx numbers both kinds from one per-document counter, so the two
/// share it here and no id can repeat. A second walk rather than one
/// interleaved with the first, because an inline role can sit in content
/// [`assign_index_ids`] does not reach — a glossary definition, a dropdown's
/// title — and the analyzer finds these with the inline walker whose reach
/// this one shares. The order differs from Sphinx's when both kinds appear;
/// an anchor's name is not something a reader links to.
///
/// Runs after substitutions are resolved, so each use of a `replace`
/// definition holding a `:pep:` or an `:index:` gets an anchor of its own,
/// and after refused roles are lowered, so an `:index:` whose entry was
/// refused still gets the anchor Sphinx gives it.
pub(super) fn assign_inline_index_ids(nodes: &mut [Node], counter: &mut usize) {
    for_each_inline_list_mut(nodes, &mut |list| {
        for node in list.iter_mut() {
            if let InlineNode::RegistryReference { index_id, .. }
            | InlineNode::IndexReference { index_id, .. } = node
            {
                *index_id = format!("index-{counter}");
                *counter += 1;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{
        AdmonitionKind, DomainObjectBody, IndexEntry, ListItem, NonEmptyVector, TableCell, TableRow,
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
            items: vec![ListItem {
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
                is_async: false,
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::single("foo()".to_string()),
                is_decorator: false,
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
                items: vec![ListItem {
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

    fn pep_paragraph() -> Node {
        Node::Paragraph(vec![InlineNode::RegistryReference {
            target: rinx_ast::RegistryTarget::parse(rinx_ast::Registry::Pep, "8").unwrap(),
            display: None,
            index_id: String::new(),
            span: None,
        }])
    }

    fn pep_id_of(node: &Node) -> &str {
        let Node::Paragraph(inlines) = node else {
            panic!("Expected Paragraph")
        };
        let InlineNode::RegistryReference { index_id, .. } = &inlines[0] else {
            panic!("Expected RegistryReference")
        };
        index_id
    }

    #[test]
    fn test_assign_inline_index_ids_continues_the_directive_counter() {
        // Given a directive and two roles
        let mut nodes = vec![pep_paragraph(), index_directive(), pep_paragraph()];
        let mut counter = 0;

        // When
        assign_index_ids(&mut nodes, &mut counter);
        assign_inline_index_ids(&mut nodes, &mut counter);

        // Then — no id repeats
        assert_eq!(id_of(&nodes[1]), "index-0");
        assert_eq!(pep_id_of(&nodes[0]), "index-1");
        assert_eq!(pep_id_of(&nodes[2]), "index-2");
    }

    #[test]
    fn test_assign_inline_index_ids_reaches_a_glossary_definition() {
        // Given a role where `assign_index_ids` does not look
        let mut nodes = vec![Node::Directive(Directive::Glossary {
            entries: vec![rinx_ast::GlossaryEntry {
                terms: vec!["term".to_string()],
                definition: vec![pep_paragraph()],
            }],
            sorted: false,
        })];
        let mut counter = 0;

        // When
        assign_inline_index_ids(&mut nodes, &mut counter);

        // Then
        let Node::Directive(Directive::Glossary { entries, .. }) = &nodes[0] else {
            unreachable!()
        };
        assert_eq!(pep_id_of(&entries[0].definition[0]), "index-0");
    }

    #[test]
    fn test_assign_inline_index_ids_numbers_index_roles_among_registry_roles() {
        // Given an `:index:` between two `:pep:`s in one paragraph
        let mut inlines = vec![InlineNode::IndexReference {
            title: "x".to_string(),
            entries: Vec::new(),
            index_id: String::new(),
            span: None,
        }];
        let Node::Paragraph(pep) = pep_paragraph() else {
            unreachable!()
        };
        inlines.insert(0, pep[0].clone());
        inlines.push(pep[0].clone());
        let mut nodes = vec![Node::Paragraph(inlines)];
        let mut counter = 3;

        // When
        assign_inline_index_ids(&mut nodes, &mut counter);

        // Then — document order, continuing the counter
        let Node::Paragraph(inlines) = &nodes[0] else {
            unreachable!()
        };
        let ids: Vec<&str> = inlines
            .iter()
            .map(|node| match node {
                InlineNode::RegistryReference { index_id, .. }
                | InlineNode::IndexReference { index_id, .. } => index_id.as_str(),
                _ => panic!("unexpected {node:?}"),
            })
            .collect();
        assert_eq!(ids, vec!["index-3", "index-4", "index-5"]);
        assert_eq!(counter, 6);
    }
}
