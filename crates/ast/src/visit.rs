//! Pre-order traversal over a document's block-level nodes.
//!
//! # Scope
//!
//! This walker exists for the *collect matching nodes* use cases — the pipeline
//! phases that need to find every node of some kind anywhere in a document,
//! regardless of nesting, and that carry no state across the traversal:
//! [`crate::Directive::Uml`] extraction and validation being the original
//! two.
//!
//! [`for_each_child`] is the one-level step it is built from, and the step a
//! walk keeping state across the document builds on instead:
//! `rinx_scope`'s `DocumentScopes` brackets each domain object's body with
//! the scope it lends and reads the inline lists in between, which a
//! `FnMut(&Node)` visitor cannot express. `rinx_analyzer`'s `index_nodes` and
//! `rinx_renderer`'s `render_nodes` keep walks of their own, which decide how
//! each construct's children are indexed or drawn.
//!
//! # Exhaustiveness
//!
//! [`for_each_child`]'s `match` lists every [`Node`] variant explicitly, with
//! no `_` arm. That is the point: a future variant that carries child nodes
//! becomes a compile error here rather than a silently unvisited subtree. This
//! is the same protection `render_directive` has, and that
//! `rinx_analyzer`'s `index_nodes` conspicuously lacks.

use crate::directive::Directive;
use crate::inline_node::InlineNode;
use crate::line_block::LineBlockItem;
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
/// use rinx_ast::{walk_nodes, ListItem, Node};
///
/// let nodes = vec![Node::BulletList {
///     bullet: '-',
///     items: vec![ListItem {
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
/// use rinx_ast::{walk_nodes, Node};
///
/// let nodes = vec![Node::Transition, Node::Comment];
/// let mut collected: Vec<&Node> = Vec::new();
/// walk_nodes(&nodes, &mut |node| collected.push(node));
/// assert_eq!(collected.len(), 2);
/// ```
pub fn walk_nodes<'a>(nodes: &'a [Node], visit: &mut impl FnMut(&'a Node)) {
    walk_nodes_with_siblings(nodes, &mut |siblings, index| visit(&siblings[index]));
}

/// [`walk_nodes`], passing each node as its position in the list holding it
/// rather than as the node alone.
///
/// For the passes that need a node's *neighbours* as well as the node — which
/// `.. _label:` targets stand directly before an element, or whether a node is
/// at the document's top level (its list is the document's own `nodes`).
/// The order is exactly [`walk_nodes`]'s, since that is implemented on this.
pub fn walk_nodes_with_siblings<'a>(nodes: &'a [Node], visit: &mut impl FnMut(&'a [Node], usize)) {
    for (index, node) in nodes.iter().enumerate() {
        visit(nodes, index);
        for_each_child(node, &mut |child| {
            if let Child::Nodes(children) = child {
                walk_nodes_with_siblings(children, visit);
            }
        });
    }
}

/// One list a node holds directly — see [`for_each_child`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Child<'a> {
    /// Inline content: a paragraph, a heading's text, a definition term, one
    /// line of a line block, a caption.
    Inline(&'a [InlineNode]),
    /// Block-level content: a list item, a directive body, a table cell.
    Nodes(&'a [Node]),
}

/// Passes every list `node` holds *directly* to `visit`, inline and
/// block-level alike, in document order — without descending into them.
///
/// The one-level step the recursive walks are built from: [`walk_nodes`]
/// recurses into the [`Child::Nodes`], and a walk that keeps state across the
/// document (`rinx_scope`'s `DocumentScopes`) recurses into those and reads
/// the [`Child::Inline`] in between, so a scope change written in a block
/// quote's content applies to its attribution, as it is read.
pub fn for_each_child<'a>(node: &'a Node, visit: &mut impl FnMut(Child<'a>)) {
    match node {
        Node::Heading { text, .. } => visit(Child::Inline(text)),
        Node::Paragraph(inlines) => visit(Child::Inline(inlines)),
        Node::Directive(directive) => for_each_directive_child(directive, visit),
        Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
            for item in items {
                visit(Child::Nodes(&item.nodes));
            }
        }
        Node::DefinitionList { items } => {
            for item in items {
                visit(Child::Inline(&item.term));
                visit(Child::Nodes(&item.definition));
            }
        }
        Node::OptionList { items } => {
            for item in items {
                visit(Child::Nodes(&item.description));
            }
        }
        Node::Table {
            header_rows,
            body_rows,
        } => {
            for row in header_rows.iter().chain(body_rows) {
                for cell in &row.cells {
                    visit(Child::Nodes(&cell.content));
                }
            }
        }
        Node::BlockQuote {
            content,
            attribution,
        } => {
            visit(Child::Nodes(content));
            if let Some(attribution) = attribution {
                visit(Child::Inline(attribution));
            }
        }
        // A line block's nesting is expressed through `LineBlockItem`, not
        // `Node`: each line is one inline list.
        Node::LineBlock(items) => for_each_line(items, visit),
        // Leaf nodes: a target's URI, a literal/doctest block's verbatim
        // text, and a comment/transition hold no list at all.
        Node::Target { .. }
        | Node::AnonymousTarget { .. }
        | Node::LiteralBlock { .. }
        | Node::DoctestBlock(_)
        | Node::Comment
        | Node::Transition => {}
    }
}

/// Passes every line of a line block, nested ones included, to `visit`.
fn for_each_line<'a>(items: &'a [LineBlockItem], visit: &mut impl FnMut(Child<'a>)) {
    for item in items {
        match item {
            LineBlockItem::Line(inlines) => visit(Child::Inline(inlines)),
            LineBlockItem::Nested(nested) => for_each_line(nested, visit),
        }
    }
}

/// The lists a [`Directive`] holds directly.
///
/// Split out from [`for_each_child`] so both matches stay exhaustive and
/// readable; the directive arm of `Node` is by far the most branch-heavy.
fn for_each_directive_child<'a>(directive: &'a Directive, visit: &mut impl FnMut(Child<'a>)) {
    match directive {
        Directive::Admonition { body, .. }
        | Directive::VersionChange { body, .. }
        // A section that reached this walker was written outside any entity, so
        // it never got folded into one. Its body is still ordinary content.
        | Directive::EntitySection { body, .. }
        | Directive::SeeAlso { body } => visit(Child::Nodes(body)),
        // Both halves carry markup: the title is inline-parsed, unlike every
        // other directive caption in this build, and the body is ordinary
        // block content.
        Directive::Dropdown(dropdown) => {
            visit(Child::Inline(&dropdown.title));
            visit(Child::Nodes(&dropdown.body));
        }
        // Its body is ordinary content, exactly as a dropdown's is — a target
        // or entity written in the justification belongs to this document.
        Directive::EntityUpdate(update) => visit(Child::Nodes(&update.body)),
        Directive::Grid(grid) => visit(Child::Nodes(&grid.body)),
        Directive::GridItem(item) => visit(Child::Nodes(&item.body)),
        // Inline markup only: a button's content is its label, not a body.
        Directive::ButtonLink(button) => visit(Child::Inline(&button.label)),
        Directive::Glossary { entries, .. } => {
            for entry in entries {
                visit(Child::Nodes(&entry.definition));
            }
        }
        Directive::DataTable { rows, .. } => {
            for row in rows {
                for cell in &row.cells {
                    visit(Child::Nodes(&cell.content));
                }
            }
        }
        Directive::Table {
            header_rows,
            body_rows,
            ..
        } => {
            for row in header_rows.iter().chain(body_rows) {
                for cell in &row.cells {
                    visit(Child::Nodes(&cell.content));
                }
            }
        }
        Directive::DomainObject(body) => visit(Child::Nodes(body.body())),
        // Every section's prose is ordinary body content — a target, a nested
        // directive or even another entity inside a `.. verification-criteria::`
        // must be reached, or it would be invisible to indexing. Attribute
        // values are deliberately not passed: they are typed values validated
        // against the schema, not inline markup.
        Directive::Entity(entity) => {
            for section in &entity.sections {
                visit(Child::Nodes(&section.body));
            }
        }
        // A figure's caption is inline markup, and its legend is ordinary
        // body content that may hold anything, including another image.
        Directive::Figure(figure) => {
            if let Some(caption) = &figure.caption {
                visit(Child::Inline(caption));
            }
            visit(Child::Nodes(&figure.legend));
        }
        // Directives holding no list. A doctest block's body is verbatim
        // text, a math block's is verbatim LaTeX, and a code block's is
        // verbatim source. A `.. highlight::` has no body at all, and an
        // `.. image::`'s `:alt:` and a toctree entry's title are plain
        // strings, not inline-parsed. A substitution definition's `replace`
        // content was spliced into every reference by the parser, so it is
        // not read where it is defined. An entity table's rows are not
        // content of this document either: they are resolved from the project
        // index while rendering, and neither are a flowchart's nodes and
        // edges or a chart's wedges and bars, which are generated from that
        // same index.
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
    use crate::admonition_kind::AdmonitionKind;
    use crate::definition_list_item::DefinitionListItem;
    use crate::domain_object_body::DomainObjectBody;
    use crate::glossary_entry::GlossaryEntry;
    use crate::inline_node::InlineNode;
    use crate::list_item::ListItem;
    use crate::non_empty_vector::NonEmptyVector;
    use crate::table::{TableCell, TableRow};
    use crate::uml::{Uml, UmlSource};

    /// A `.. plantuml::` node whose body identifies it in assertions.
    fn diagram(body: &str) -> Node {
        Node::Directive(Directive::Uml(Box::new(Uml::new(
            UmlSource::PlantUml,
            body.to_string(),
        ))))
    }

    /// Collects the templates of every diagram directive the walker visits.
    fn walk_diagram_bodies(nodes: &[Node]) -> Vec<String> {
        let mut bodies = Vec::new();
        walk_nodes(nodes, &mut |node| {
            if let Node::Directive(Directive::Uml(uml)) = node {
                bodies.push(uml.template.clone());
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
            items: vec![ListItem {
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
                ListItem {
                    nodes: vec![diagram("item-one")],
                },
                ListItem {
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
    fn test_walk_nodes_descends_into_option_list_descriptions() {
        // Given
        use crate::option_list_item::{OptionListItem, OptionSpec};
        let nodes = vec![Node::OptionList {
            items: vec![OptionListItem {
                options: vec![OptionSpec {
                    flag: "-h".to_string(),
                    argument: None,
                }],
                description: vec![diagram("nested-in-option-description")],
            }],
        }];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-option-description"]);
    }

    #[test]
    fn test_walk_nodes_descends_into_block_quote_content() {
        // Given
        let nodes = vec![Node::BlockQuote {
            content: vec![diagram("nested-in-block-quote")],
            attribution: None,
        }];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["nested-in-block-quote"]);
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
    fn test_walk_nodes_descends_into_data_table_cells() {
        // Given
        let nodes = vec![Node::Directive(Directive::DataTable {
            source: crate::TableSource::List,
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
    fn test_walk_nodes_descends_into_table_directive_header_and_body_rows() {
        // Given
        let nodes = vec![Node::Directive(Directive::Table {
            title: None,
            widths: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: None,
            header_rows: vec![row_with(vec![diagram("in-header")])],
            body_rows: vec![row_with(vec![diagram("in-body")])],
        })];

        // When
        let bodies = walk_diagram_bodies(&nodes);

        // Then
        assert_eq!(bodies, vec!["in-header", "in-body"]);
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
                flags: crate::DescriptionFlags::default(),
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
                items: vec![ListItem {
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
            Node::Directive(Directive::StdProgram {
                name: Some("dis".to_string()),
            }),
            Node::Directive(Directive::Unknown {
                name: "doctest".to_string(),
                argument: String::new(),
                body: "unparsed body".to_string(),
            }),
        ];

        // When
        let mut visited = 0;
        walk_nodes(&nodes, &mut |_| visited += 1);

        // Then — the three directives themselves, and nothing more.
        assert_eq!(visited, 3);
    }

    #[test]
    fn test_walk_nodes_with_siblings_passes_each_node_with_its_list() {
        // Given a top-level comment, then an admonition holding a transition
        let nodes = vec![
            Node::Comment,
            Node::Directive(Directive::SeeAlso {
                body: vec![Node::Transition],
            }),
        ];

        // When
        let mut seen: Vec<(bool, usize)> = Vec::new();
        walk_nodes_with_siblings(&nodes, &mut |siblings, index| {
            seen.push((std::ptr::eq(siblings, nodes.as_slice()), index));
        });

        // Then — pre-order, and only the first two are in the top-level list
        assert_eq!(seen, vec![(true, 0), (true, 1), (false, 0)]);
    }

    /// What `for_each_child` passes for `node`: each inline list's text, and
    /// each node list's length.
    fn children_of(node: &Node) -> Vec<String> {
        let mut seen = Vec::new();
        for_each_child(node, &mut |child| match child {
            Child::Inline(inlines) => seen.push(crate::inline_plain_text(inlines)),
            Child::Nodes(nodes) => seen.push(format!("{} nodes", nodes.len())),
        });
        seen
    }

    fn text(value: &str) -> Vec<InlineNode> {
        vec![InlineNode::Text(value.to_string())]
    }

    #[test]
    fn test_for_each_child_passes_a_block_quote_content_before_its_attribution() {
        // Given
        let node = Node::BlockQuote {
            content: vec![Node::Transition, Node::Comment],
            attribution: Some(text("author")),
        };

        // When / Then — in the order a reader meets them
        assert_eq!(children_of(&node), ["2 nodes", "author"]);
    }

    #[test]
    fn test_for_each_child_interleaves_each_definition_term_with_its_definition() {
        // Given
        let node = Node::DefinitionList {
            items: vec![
                DefinitionListItem {
                    term: text("first"),
                    definition: vec![Node::Transition],
                },
                DefinitionListItem {
                    term: text("second"),
                    definition: vec![],
                },
            ],
        };

        // When / Then
        assert_eq!(
            children_of(&node),
            ["first", "1 nodes", "second", "0 nodes"]
        );
    }

    #[test]
    fn test_for_each_child_passes_every_line_of_a_nested_line_block() {
        // Given
        let node = Node::LineBlock(vec![
            LineBlockItem::Line(text("outer")),
            LineBlockItem::Nested(vec![LineBlockItem::Line(text("inner"))]),
        ]);

        // When / Then
        assert_eq!(children_of(&node), ["outer", "inner"]);
    }

    #[test]
    fn test_for_each_child_passes_a_paragraph_and_nothing_for_a_leaf() {
        // Given
        let paragraph = Node::Paragraph(text("prose"));

        // When / Then
        assert_eq!(children_of(&paragraph), ["prose"]);
        assert!(children_of(&Node::Transition).is_empty());
    }

    #[test]
    fn test_for_each_child_passes_a_domain_object_body() {
        // Given
        let node = Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            name: "pkg".to_string(),
            options: crate::module_options::ModuleOptions::default(),
            body: vec![Node::Transition],
        }));

        // When / Then
        assert_eq!(children_of(&node), ["1 nodes"]);
    }
}
