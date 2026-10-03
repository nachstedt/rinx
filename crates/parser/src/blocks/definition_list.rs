use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::{indent_width, strip_indent};
use crate::inline::{SourceMap, parse_inline_text_mapped};
use rinx_ast::{DefinitionListItem, Node};

/// Detects whether `lines[i]` is the term of a definition-list entry: a
/// non-blank line immediately followed (no blank line in between) by a
/// non-blank line that is *more indented* than it.
///
/// Returns `(term_indent, body_indent)` on a match.
pub(super) fn detect_definition_term(lines: &[&str], i: usize) -> Option<(usize, usize)> {
    let line = lines[i].trim_end();
    if line.trim().is_empty() {
        return None;
    }
    let term_indent = indent_width(line);

    let next_line = *lines.get(i + 1)?;
    let next_line = next_line.trim_end();
    if next_line.trim().is_empty() {
        return None;
    }
    let body_indent = indent_width(next_line);
    if body_indent <= term_indent {
        return None;
    }

    Some((term_indent, body_indent))
}

pub(super) fn try_parse_definition_list(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let mut items = Vec::new();
    let mut i = start_i;
    let mut list_indent = None;

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            break;
        }

        let Some((term_indent, body_indent)) = detect_definition_term(lines, i) else {
            break;
        };

        match list_indent {
            None => list_indent = Some(term_indent),
            Some(indent) if indent == term_indent => {}
            Some(_) => break,
        }

        // A term is always exactly one line, so its map needs no accumulation.
        let term = parse_inline_text_mapped(
            line.trim(),
            ctx.default_domain,
            &SourceMap::single_line(
                line.trim(),
                i,
                line.chars().take_while(|c| c.is_whitespace()).count(),
            ),
            ctx,
        );
        i += 1;
        // The definition's first line, before `i` walks past the body.
        let definition_start = i;

        let mut body_lines = Vec::new();
        while i < lines.len() {
            let next_line = lines[i].trim_end();
            if next_line.trim().is_empty() {
                body_lines.push(String::new());
                i += 1;
                continue;
            }

            let next_indent = indent_width(next_line);
            if next_indent >= body_indent {
                body_lines.push(strip_indent(next_line, body_indent).to_string());
                i += 1;
            } else {
                break;
            }
        }

        // Remove trailing empty lines to keep the definition body clean.
        while body_lines.last().is_some_and(String::is_empty) {
            body_lines.pop();
        }

        let body_refs: Vec<&str> = body_lines.iter().map(String::as_str).collect();
        let definition_ctx = ctx
            .nested(definition_start, body_indent)
            .without_section_titles();
        let definition = parse_blocks(&body_refs, adornment_order, diagnostics, &definition_ctx);

        items.push(DefinitionListItem { term, definition });
    }

    if items.is_empty() {
        None
    } else {
        Some((i - start_i, Node::DefinitionList { items }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::InlineNode;

    #[test]
    fn test_parse_definition_list_single_item() {
        // Given a single term followed by an indented definition
        let input = "Term\n   Definition text.";

        // When parsed
        let doc = parse("test.rst", input);

        // Then it produces a DefinitionList with one item
        assert_eq!(doc.nodes.len(), 1);
        if let Node::DefinitionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].term, vec![InlineNode::Text("Term".to_string())]);
            assert_eq!(items[0].definition.len(), 1);
            if let Node::Paragraph(inlines) = &items[0].definition[0] {
                assert_eq!(inlines[0], InlineNode::Text("Definition text.".to_string()));
            } else {
                panic!("Expected Paragraph, got {:?}", items[0].definition[0]);
            }
        } else {
            panic!("Expected DefinitionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_definition_list_multiple_items() {
        // Given two term/definition entries separated by a blank line
        let input = "Term1\n   Def1\n\nTerm2\n   Def2";

        // When parsed
        let doc = parse("test.rst", input);

        // Then both items are captured in one DefinitionList
        assert_eq!(doc.nodes.len(), 1);
        if let Node::DefinitionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].term, vec![InlineNode::Text("Term1".to_string())]);
            assert_eq!(items[1].term, vec![InlineNode::Text("Term2".to_string())]);
        } else {
            panic!("Expected DefinitionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_definition_list_term_with_inline_markup() {
        // Given the CPython benchmark's seealso-style definition list, with
        // :mod:/:ref: roles in the term text
        let input = "Module :mod:`curses.ascii`\n   Utilities for ASCII characters.\n\n:ref:`curses-howto`\n   Tutorial material.";

        // When parsed
        let doc = parse("test.rst", input);

        // Then the domain-object/ref roles in the term are resolved as inline nodes
        assert_eq!(doc.nodes.len(), 1);
        if let Node::DefinitionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            assert!(matches!(
                items[0].term[1],
                InlineNode::DomainObjectReference { .. }
            ));
            assert!(matches!(items[1].term[0], InlineNode::Reference { .. }));
        } else {
            panic!("Expected DefinitionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_definition_list_does_not_fire_on_plain_paragraph() {
        // Given two lines at the same indentation (no term/definition indent step)
        let input = "Line one\nLine two";

        // When parsed
        let doc = parse("test.rst", input);

        // Then it stays a single ordinary Paragraph, not a DefinitionList
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(
                inlines[0],
                InlineNode::Text("Line one\nLine two".to_string())
            );
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_definition_list_multi_paragraph_definition() {
        // Given a definition body containing two paragraphs
        let input = "Term\n   Para 1\n\n   Para 2";

        // When parsed
        let doc = parse("test.rst", input);

        // Then the single item's definition holds both paragraphs
        if let Node::DefinitionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].definition.len(), 2);
            assert!(matches!(items[0].definition[0], Node::Paragraph(_)));
            assert!(matches!(items[0].definition[1], Node::Paragraph(_)));
        } else {
            panic!("Expected DefinitionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_definition_list_ends_before_trailing_paragraph() {
        // Given a definition list followed by an unrelated, unindented paragraph
        let input = "Term\n   Def1\n\nNot part of the list";

        // When parsed
        let doc = parse("test.rst", input);

        // Then the list ends after its one item and the trailing text is a
        // separate Paragraph node
        assert_eq!(doc.nodes.len(), 2);
        if let Node::DefinitionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 1);
        } else {
            panic!("Expected DefinitionList, got {:?}", doc.nodes[0]);
        }
        assert!(matches!(doc.nodes[1], Node::Paragraph(_)));
    }
}
