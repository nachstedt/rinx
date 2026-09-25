//! Turning one document's flat run of headings into the nested
//! [`DocumentOutline`] a `.. toctree::` lists sections from.
//!
//! Kept as its own document-order pass rather than an arm inside
//! [`super::document_index::index_nodes`], for the same reasons
//! [`super::equation_numbering`] is: nesting is inherently sequential (a
//! heading's parent is decided by the levels that came before it), so folding
//! it in would mean threading a level stack through that recursive matcher for
//! this one node kind.
//!
//! Only *top-level* headings are sections. reStructuredText has no notion of a
//! section inside a directive body, so a heading nested in one is ordinary
//! content — it is neither given an id nor listed in an outline.

use rinx_ast::{Node, SectionId, allocate_section_ids, inline_plain_text};
use rinx_index::{DocumentOutline, OutlineSection};

/// Builds the outline of `nodes`, a document's top-level node list.
///
/// Section ids come from [`allocate_section_ids`] — the same call the renderer
/// makes when it emits the `id` attributes — so an outline entry and the
/// anchor it points at can never disagree, even when two headings share a
/// title.
///
/// Heading *levels* are used only for relative nesting, never as absolute
/// depths: a document that starts at `=====` and one that starts at `-----`
/// produce the same shape, and a document that skips from level 1 to level 3
/// nests the level 3 directly under the level 1 rather than inventing an empty
/// level 2. That mirrors how the parser already assigns levels from adornment
/// order rather than from a fixed character table.
///
/// When every section hangs off a single level-1 heading, that heading is the
/// document *title* rather than a section — a toctree already shows it as the
/// document's own entry — so its children are returned in its place.
#[must_use]
pub(super) fn build_document_outline(nodes: &[Node]) -> DocumentAnalysis {
    let ids = allocate_section_ids(nodes);

    // Each entry is one open ancestor: the heading's level plus the section
    // being accumulated for it. A heading closes every open ancestor at its
    // own level or deeper.
    let mut stack: Vec<(u8, OutlineSection)> = Vec::new();
    let mut roots: Vec<OutlineSection> = Vec::new();
    let mut toctree_sections: Vec<Option<SectionId>> = Vec::new();

    for (index, node) in nodes.iter().enumerate() {
        match node {
            Node::Heading { level, text } => {
                let Some(id) = ids.get(&index) else {
                    continue;
                };

                while stack.last().is_some_and(|(open, _)| *open >= *level) {
                    close_innermost(&mut stack, &mut roots);
                }

                stack.push((
                    *level,
                    OutlineSection {
                        title: inline_plain_text(text),
                        id: id.clone(),
                        children: Vec::new(),
                    },
                ));
            }
            // A toctree's entries belong where the directive was written —
            // under the section enclosing it — because that is where Sphinx
            // splices them into the document's own section tree. Recording the
            // enclosing section here is what lets the renderer reproduce that
            // without a second traversal.
            Node::Directive(rinx_ast::Directive::Toctree(_)) => {
                toctree_sections.push(stack.last().map(|(_, section)| section.id.clone()));
            }
            _ => {}
        }
    }

    while !stack.is_empty() {
        close_innermost(&mut stack, &mut roots);
    }

    let sections = unwrap_single_title(roots);
    // A toctree written under the lone title heading is written under the
    // *document*, since that heading is not itself a section here.
    let title_id = lone_title_id(&sections, &ids, nodes);
    if let Some(title_id) = title_id {
        for placement in &mut toctree_sections {
            if placement.as_ref() == Some(&title_id) {
                *placement = None;
            }
        }
    }

    DocumentAnalysis {
        outline: DocumentOutline { sections },
        toctree_sections,
    }
}

/// One document's structural analysis: its section outline, and where each of
/// its `.. toctree::` directives sits within it.
///
/// The two travel together because they come from one walk — a toctree's
/// placement is the section stack's state at the moment the directive is
/// reached, which only this pass knows.
pub(super) struct DocumentAnalysis {
    pub outline: DocumentOutline,
    /// For each toctree in the document, in document order, the section it was
    /// written inside — `None` when it was written before the first section,
    /// or directly under a lone title heading.
    pub toctree_sections: Vec<Option<SectionId>>,
}

/// The id of the document's title heading, when [`unwrap_single_title`]
/// removed one.
///
/// A toctree written under that heading is not "inside a section" from the
/// outline's point of view, since the heading is the document itself.
fn lone_title_id(
    sections: &[OutlineSection],
    ids: &std::collections::BTreeMap<usize, SectionId>,
    nodes: &[Node],
) -> Option<SectionId> {
    let first_heading = nodes
        .iter()
        .enumerate()
        .find(|(_, node)| matches!(node, Node::Heading { .. }))?;
    let first_id = ids.get(&first_heading.0)?;
    // The title was unwrapped exactly when it is no longer among the sections.
    if sections.iter().any(|section| section.id == *first_id) {
        None
    } else {
        Some(first_id.clone())
    }
}

/// Pops the innermost open section and attaches it to its parent, or to the
/// root list when it has none.
fn close_innermost(stack: &mut Vec<(u8, OutlineSection)>, roots: &mut Vec<OutlineSection>) {
    let Some((_, finished)) = stack.pop() else {
        return;
    };
    match stack.last_mut() {
        Some((_, parent)) => parent.children.push(finished),
        None => roots.push(finished),
    }
}

/// Replaces a lone root with its children.
///
/// A document whose headings all hang off one top heading has that heading as
/// its *title*, which a toctree already renders as the document's own entry.
/// Keeping it as a section too would show every such document twice, once
/// nested inside itself.
fn unwrap_single_title(roots: Vec<OutlineSection>) -> Vec<OutlineSection> {
    let mut roots = roots;
    if roots.len() == 1 {
        return roots.remove(0).children;
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::InlineNode;

    fn heading(level: u8, title: &str) -> Node {
        Node::Heading {
            level,
            text: vec![InlineNode::Text(title.to_string())],
        }
    }

    fn paragraph(text: &str) -> Node {
        Node::Paragraph(vec![InlineNode::Text(text.to_string())])
    }

    /// The titles of `sections`, with each one's children in parentheses, as a
    /// compact shape assertion.
    fn shape(sections: &[OutlineSection]) -> String {
        sections
            .iter()
            .map(|section| {
                if section.children.is_empty() {
                    section.title.clone()
                } else {
                    format!("{}({})", section.title, shape(&section.children))
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    fn toctree_node() -> Node {
        Node::Directive(rinx_ast::Directive::Toctree(rinx_ast::Toctree::default()))
    }

    #[test]
    fn test_toctree_before_any_section_is_placed_at_the_document_level() {
        // Given — the overwhelmingly common `index.rst` shape.
        let nodes = vec![heading(1, "Guide"), toctree_node(), heading(2, "Install")];

        // When
        let analysis = build_document_outline(&nodes);

        // Then
        assert_eq!(analysis.toctree_sections, vec![None]);
    }

    #[test]
    fn test_toctree_inside_a_section_is_placed_under_it() {
        // Given — Sphinx splices the entries in where the directive sits, so a
        // toctree under "Advanced" lists its documents under Advanced.
        let nodes = vec![heading(1, "Guide"), heading(2, "Advanced"), toctree_node()];

        // When
        let analysis = build_document_outline(&nodes);

        // Then
        assert_eq!(
            analysis.toctree_sections,
            vec![Some(rinx_ast::SectionId::from_title("Advanced"))]
        );
    }

    #[test]
    fn test_toctree_is_placed_under_the_innermost_enclosing_section() {
        // Given
        let nodes = vec![
            heading(1, "Guide"),
            heading(2, "Advanced"),
            heading(3, "Tuning"),
            toctree_node(),
        ];

        // When
        let analysis = build_document_outline(&nodes);

        // Then
        assert_eq!(
            analysis.toctree_sections,
            vec![Some(rinx_ast::SectionId::from_title("Tuning"))]
        );
    }

    #[test]
    fn test_toctree_under_a_lone_title_heading_is_placed_at_the_document_level() {
        // Given — the title heading is the document, not a section of it, so a
        // toctree beneath it belongs to the document.
        let nodes = vec![heading(1, "Guide"), toctree_node()];

        // When
        let analysis = build_document_outline(&nodes);

        // Then
        assert_eq!(analysis.toctree_sections, vec![None]);
    }

    #[test]
    fn test_several_toctrees_are_placed_in_document_order() {
        // Given
        let nodes = vec![
            heading(1, "Guide"),
            toctree_node(),
            heading(2, "Advanced"),
            toctree_node(),
        ];

        // When
        let analysis = build_document_outline(&nodes);

        // Then
        assert_eq!(
            analysis.toctree_sections,
            vec![None, Some(rinx_ast::SectionId::from_title("Advanced"))]
        );
    }

    #[test]
    fn test_build_document_outline_of_an_empty_document_is_empty() {
        // Given
        let nodes = vec![paragraph("Just prose.")];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then
        assert!(outline.is_empty());
    }

    #[test]
    fn test_build_document_outline_nests_a_subsection_under_its_section() {
        // Given — a title, then two sections, the second nested.
        let nodes = vec![
            heading(1, "Guide"),
            heading(2, "Install"),
            heading(3, "From Source"),
        ];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then — the level-1 title is unwrapped, leaving Install as the root.
        assert_eq!(shape(&outline.sections), "Install(From Source)");
    }

    #[test]
    fn test_build_document_outline_keeps_siblings_flat() {
        // Given
        let nodes = vec![
            heading(1, "Guide"),
            heading(2, "Install"),
            heading(2, "Configure"),
        ];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then
        assert_eq!(shape(&outline.sections), "Install,Configure");
    }

    #[test]
    fn test_build_document_outline_closes_a_deeper_section_on_return_to_a_shallower_level() {
        // Given
        let nodes = vec![
            heading(1, "Guide"),
            heading(2, "Install"),
            heading(3, "From Source"),
            heading(2, "Configure"),
        ];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then — Configure is a sibling of Install, not a child of From Source.
        assert_eq!(shape(&outline.sections), "Install(From Source),Configure");
    }

    #[test]
    fn test_build_document_outline_attaches_a_skipped_level_to_the_nearest_ancestor() {
        // Given — the document jumps from level 2 to level 4.
        let nodes = vec![
            heading(1, "Guide"),
            heading(2, "Install"),
            heading(4, "Deep"),
        ];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then — no empty level-3 section is invented.
        assert_eq!(shape(&outline.sections), "Install(Deep)");
    }

    #[test]
    fn test_build_document_outline_keeps_several_top_level_headings() {
        // Given — no single title heading, so nothing is unwrapped.
        let nodes = vec![heading(1, "Part One"), heading(1, "Part Two")];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then
        assert_eq!(shape(&outline.sections), "Part One,Part Two");
    }

    #[test]
    fn test_build_document_outline_unwraps_a_lone_title_heading() {
        // Given — the common shape: one H1 title with sections beneath it.
        let nodes = vec![heading(1, "Guide"), heading(2, "Install")];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then — the title is not repeated as a section of itself.
        assert_eq!(shape(&outline.sections), "Install");
    }

    #[test]
    fn test_build_document_outline_of_a_title_only_document_is_empty() {
        // Given
        let nodes = vec![heading(1, "Guide"), paragraph("Prose.")];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then
        assert!(outline.is_empty());
    }

    #[test]
    fn test_build_document_outline_uses_the_shared_section_ids() {
        // Given — two sections sharing a title, the case where an
        // independently derived id would silently collide.
        let nodes = vec![
            heading(1, "Guide"),
            heading(2, "Overview"),
            heading(2, "Overview"),
        ];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then — the ids match what `allocate_section_ids` (and so the
        // renderer's `id` attributes) produced.
        let expected = allocate_section_ids(&nodes);
        assert_eq!(outline.sections[0].id, expected[&1]);
        assert_eq!(outline.sections[1].id, expected[&2]);
        assert_ne!(outline.sections[0].id, outline.sections[1].id);
    }

    #[test]
    fn test_build_document_outline_flattens_inline_markup_in_a_title() {
        // Given
        let nodes = vec![
            heading(1, "Guide"),
            Node::Heading {
                level: 2,
                text: vec![
                    InlineNode::Text("The ".to_string()),
                    InlineNode::Emphasis("fast".to_string()),
                    InlineNode::Text(" path".to_string()),
                ],
            },
        ];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then
        assert_eq!(outline.sections[0].title, "The fast path");
    }

    #[test]
    fn test_build_document_outline_ignores_a_heading_inside_a_directive_body() {
        // Given — reStructuredText has no section inside a directive body, so
        // this heading is ordinary content.
        let nodes = vec![
            heading(1, "Guide"),
            heading(2, "Install"),
            Node::Directive(rinx_ast::Directive::SeeAlso {
                body: vec![heading(2, "Not A Section")],
            }),
        ];

        // When
        let outline = build_document_outline(&nodes).outline;

        // Then
        assert_eq!(shape(&outline.sections), "Install");
    }
}
