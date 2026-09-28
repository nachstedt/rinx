//! Recording what one document contributes to `numfig` numbering: the
//! [`NumberingStep`]s the project-wide walk takes through it, and the labels a
//! `:numref:` may name in it.
//!
//! A per-document pass inside `analyze()`, beside
//! [`super::equation_numbering`], because both halves depend only on this
//! document. The numbers themselves cannot be assigned here — Sphinx counts
//! across the whole toctree — which is [`super::element_numbering`]'s job.
//!
//! Elements are found by [`rinx_ast::enumerable_elements`], the same function
//! the renderer calls to find the captions it writes numbers into; an
//! element's position in its result is the ordinal both sides key a number on.

use std::collections::BTreeMap;

use rinx_ast::{
    Directive, Document, Node, SectionId, TargetName, allocate_section_ids, enumerable_elements,
    preceding_labels,
};
use rinx_index::{NumberingStep, NumrefSubject, NumrefTarget};

/// What one document contributes: its steps, in document order, and the
/// targets its labels name.
pub(super) struct DocumentNumbering {
    pub steps: Vec<NumberingStep>,
    pub targets: BTreeMap<TargetName, NumrefTarget>,
}

/// Records `doc`'s numbering steps and the labels a `:numref:` may name.
pub(super) fn collect_numbering(doc: &Document) -> DocumentNumbering {
    let sections = enclosing_sections(&doc.nodes);
    let elements = enumerable_elements(&doc.nodes);
    let mut targets = BTreeMap::new();

    for (ordinal, element) in elements.iter().enumerate() {
        for label in &element.labels {
            targets.insert(
                (*label).clone(),
                NumrefTarget {
                    doc_path: doc.path.clone(),
                    subject: NumrefSubject::Element {
                        ordinal,
                        kind: element.kind,
                    },
                },
            );
        }
    }
    for (index, id) in allocate_section_ids(&doc.nodes) {
        for label in preceding_labels(&doc.nodes, index) {
            targets.insert(
                label.clone(),
                NumrefTarget {
                    doc_path: doc.path.clone(),
                    subject: NumrefSubject::Section(id.clone()),
                },
            );
        }
    }

    // Toctrees are only ever top-level (see `analyze`), so one pass over the
    // top-level nodes interleaves them with the elements by position.
    let mut steps = Vec::new();
    let mut pending = elements.iter().peekable();
    let mut toctree_position = 0;
    for (index, node) in doc.nodes.iter().enumerate() {
        while let Some(element) = pending.next_if(|element| element.top_level_index == index) {
            steps.push(NumberingStep::Element {
                kind: element.kind,
                sections: sections[index].clone(),
            });
        }
        if let Node::Directive(Directive::Toctree(_)) = node {
            steps.push(NumberingStep::Toctree {
                position: toctree_position,
                sections: sections[index].clone(),
            });
            toctree_position += 1;
        }
    }

    DocumentNumbering { steps, targets }
}

/// For each top-level node, the sections it is inside, outermost first.
///
/// Nesting follows heading levels exactly as the outline's does: a heading
/// closes every open section at its own level or deeper. A heading is inside
/// its own section — it opens it.
fn enclosing_sections(nodes: &[Node]) -> Vec<Vec<SectionId>> {
    let ids = allocate_section_ids(nodes);
    let mut open: Vec<(u8, SectionId)> = Vec::new();
    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            if let (Node::Heading { level, .. }, Some(id)) = (node, ids.get(&index)) {
                while open
                    .last()
                    .is_some_and(|(open_level, _)| open_level >= level)
                {
                    open.pop();
                }
                open.push((*level, id.clone()));
            }
            open.iter().map(|(_, id)| id.clone()).collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{AssetUri, EnumerableKind, Figure, ImageOptions, InlineNode, Toctree};

    fn heading(level: u8, title: &str) -> Node {
        Node::Heading {
            level,
            text: vec![InlineNode::Text(title.to_string())],
        }
    }

    fn figure(caption: &str) -> Node {
        let mut figure = Figure::new(ImageOptions::new(AssetUri::new("a.png")));
        figure.caption = Some(vec![InlineNode::Text(caption.to_string())]);
        Node::Directive(Directive::Figure(Box::new(figure)))
    }

    fn target(name: &str) -> Node {
        Node::Target {
            name: TargetName::new(name),
            uri: None,
        }
    }

    fn toctree() -> Node {
        Node::Directive(Directive::Toctree(Toctree::default()))
    }

    fn id(title: &str) -> SectionId {
        SectionId::from_title(title)
    }

    #[test]
    fn test_steps_interleave_elements_and_toctrees_in_document_order() {
        // Given
        let doc = Document::new(
            "index.rst".to_string(),
            vec![
                heading(1, "Root"),
                figure("Before"),
                toctree(),
                figure("After"),
            ],
        );

        // When
        let numbering = collect_numbering(&doc);

        // Then
        let root = vec![id("Root")];
        assert_eq!(
            numbering.steps,
            vec![
                NumberingStep::Element {
                    kind: EnumerableKind::Figure,
                    sections: root.clone(),
                },
                NumberingStep::Toctree {
                    position: 0,
                    sections: root.clone(),
                },
                NumberingStep::Element {
                    kind: EnumerableKind::Figure,
                    sections: root,
                },
            ]
        );
    }

    #[test]
    fn test_an_element_nested_in_a_container_is_placed_by_its_ancestor() {
        // Given a figure inside an admonition under a subsection
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![
                heading(1, "Guide"),
                heading(2, "Usage"),
                Node::Directive(Directive::SeeAlso {
                    body: vec![figure("Nested")],
                }),
            ],
        );

        // When
        let numbering = collect_numbering(&doc);

        // Then
        assert_eq!(
            numbering.steps,
            vec![NumberingStep::Element {
                kind: EnumerableKind::Figure,
                sections: vec![id("Guide"), id("Usage")],
            }]
        );
    }

    #[test]
    fn test_labels_name_elements_by_ordinal_and_headings_by_section() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![
                target("guide-label"),
                heading(1, "Guide"),
                figure("First"),
                target("second-fig"),
                figure("Second"),
            ],
        );

        // When
        let numbering = collect_numbering(&doc);

        // Then
        assert_eq!(
            numbering.targets[&TargetName::new("second-fig")].subject,
            NumrefSubject::Element {
                ordinal: 1,
                kind: EnumerableKind::Figure,
            }
        );
        assert_eq!(
            numbering.targets[&TargetName::new("guide-label")],
            NumrefTarget {
                doc_path: "guide.rst".to_string(),
                subject: NumrefSubject::Section(id("Guide")),
            }
        );
    }

    #[test]
    fn test_enclosing_sections_close_at_the_same_or_a_shallower_level() {
        // Given
        let nodes = vec![
            heading(1, "A"),
            heading(2, "B"),
            Node::Comment,
            heading(2, "C"),
            heading(1, "D"),
        ];

        // When
        let sections = enclosing_sections(&nodes);

        // Then
        assert_eq!(sections[2], vec![id("A"), id("B")]);
        assert_eq!(sections[3], vec![id("A"), id("C")]);
        assert_eq!(sections[4], vec![id("D")]);
    }
}
