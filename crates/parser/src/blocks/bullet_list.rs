use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::strip_indent;
use rinx_ast::{Diagnostic, DiagnosticCode, Node};

pub(super) fn detect_bullet_item(line: &str) -> Option<(char, usize, usize)> {
    let mut chars = line.chars();
    let mut indent = 0;
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            indent += 1;
        } else {
            let bullet = c;
            if matches!(bullet, '*' | '+' | '-' | '•' | '‣' | '⁃') {
                if let Some(next_c) = chars.next() {
                    if next_c.is_whitespace() {
                        let mut body_indent = indent + 1 + 1;
                        for ws_c in chars.by_ref() {
                            if ws_c.is_whitespace() {
                                body_indent += 1;
                            } else {
                                break;
                            }
                        }
                        return Some((bullet, indent, body_indent));
                    }
                } else {
                    // bullet is the last character on the line
                    return Some((bullet, indent, indent + 2)); // Default +2 body indent
                }
            }
            break;
        }
    }
    None
}

pub(super) fn try_parse_bullet_list(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let mut items = Vec::new();
    let mut i = start_i;
    let mut current_bullet = None;
    let mut list_indent = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            break;
        }

        if let Some((bullet, item_indent, body_indent)) = detect_bullet_item(line) {
            match current_bullet {
                None => {
                    current_bullet = Some(bullet);
                    list_indent = item_indent;
                }
                Some(cur_bullet) => {
                    if item_indent != list_indent {
                        break;
                    } else if bullet != cur_bullet {
                        if i > 0 && !lines[i - 1].trim().is_empty() {
                            diagnostics.push(Diagnostic::at(
                                DiagnosticCode::ListBulletChanged,
                                format!(
                                    "Different bullet characters in adjacent lists ('{cur_bullet}' and '{bullet}'). A blank line is required to separate lists.",
                                ),
                                ctx.line_span(i, line),
                            ));
                        }
                        break;
                    }
                }
            }

            // Where this item's body begins in the document. Recorded before
            // `i` advances past it, since every body line is dedented by
            // `body_indent` and so needs both offsets to map back.
            let item_start = i;
            let mut body_lines = Vec::new();
            let first_line_body = if line.len() > body_indent {
                strip_indent(line, body_indent)
            } else {
                ""
            };

            body_lines.push(first_line_body.to_string());

            i += 1;

            while i < lines.len() {
                let next_line = lines[i].trim_end();
                if next_line.trim().is_empty() {
                    body_lines.push(String::new());
                    i += 1;
                    continue;
                }

                let next_line_indent = next_line.chars().take_while(|c| c.is_whitespace()).count();

                if next_line_indent >= body_indent {
                    body_lines.push(strip_indent(next_line, body_indent).to_string());
                    i += 1;
                } else {
                    break;
                }
            }

            // Remove trailing empty lines to keep block clean
            while body_lines.last().is_some_and(String::is_empty) {
                body_lines.pop();
            }

            let body_refs: Vec<&str> = body_lines.iter().map(String::as_str).collect();
            let item_ctx = ctx.nested(item_start, body_indent).without_section_titles();
            let body_nodes = parse_blocks(&body_refs, adornment_order, diagnostics, &item_ctx);

            items.push(rinx_ast::ListItem { nodes: body_nodes });
        } else {
            break;
        }
    }

    if items.is_empty() {
        None
    } else {
        Some((
            i - start_i,
            Node::BulletList {
                bullet: current_bullet.unwrap(),
                items,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::InlineNode;

    #[test]
    fn test_parse_bullet_list_simple() {
        let input = "* Item 1\n* Item 2";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::BulletList { bullet, items } = &doc.nodes[0] {
            assert_eq!(*bullet, '*');
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].nodes.len(), 1);
            if let Node::Paragraph(inlines) = &items[0].nodes[0] {
                assert_eq!(inlines[0], InlineNode::Text("Item 1".to_string()));
            }
        } else {
            panic!("Expected BulletList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bullet_list_nested() {
        let input = "* Item 1\n\n  * Subitem 1\n\n* Item 2";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::BulletList { bullet, items } = &doc.nodes[0] {
            assert_eq!(*bullet, '*');
            assert_eq!(items.len(), 2);
            // Item 1 should have 2 nodes: Paragraph "Item 1" and BulletList
            assert_eq!(items[0].nodes.len(), 2);
            assert!(matches!(items[0].nodes[1], Node::BulletList { .. }));
        } else {
            panic!("Expected BulletList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bullet_list_paragraph_continuation() {
        let input = "* Item 1\n  * Subitem 1\n* Item 2";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::BulletList { bullet, items } = &doc.nodes[0] {
            assert_eq!(*bullet, '*');
            assert_eq!(items.len(), 2);
            // Item 1 should have 1 node: Paragraph "Item 1\n* Subitem 1"
            assert_eq!(items[0].nodes.len(), 1);
            if let Node::Paragraph(inlines) = &items[0].nodes[0] {
                assert_eq!(
                    inlines[0],
                    InlineNode::Text("Item 1\n* Subitem 1".to_string())
                );
            } else {
                panic!("Expected Paragraph, got {:?}", items[0].nodes[0]);
            }
        } else {
            panic!("Expected BulletList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bullet_list_diagnostic_adjacent_different_bullets() {
        let input = "* Item 1\n- Item 2";
        let doc = parse("test.rst", input);
        // Should be 2 lists because of different bullets
        assert_eq!(doc.nodes.len(), 2);
        assert!(!doc.diagnostics.is_empty());
        assert!(
            doc.diagnostics[0]
                .message
                .contains("Different bullet characters in adjacent lists")
        );
    }

    #[test]
    fn test_parse_bullet_list_multi_line_item() {
        let input = "* Item 1 line 1\n  Item 1 line 2\n* Item 2";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::BulletList { items, .. } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            if let Node::Paragraph(inlines) = &items[0].nodes[0] {
                assert_eq!(
                    inlines[0],
                    InlineNode::Text("Item 1 line 1\nItem 1 line 2".to_string())
                );
            }
        }
    }

    #[test]
    fn test_parse_bullet_list_multi_paragraph_item() {
        let input = "* Item 1 Para 1\n\n  Item 1 Para 2\n* Item 2";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::BulletList { items, .. } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].nodes.len(), 2);
            assert!(matches!(items[0].nodes[0], Node::Paragraph(_)));
            assert!(matches!(items[0].nodes[1], Node::Paragraph(_)));
        }
    }

    #[test]
    fn test_parse_bullet_list_different_bullets_same_indent() {
        let input = "* Item 1\n+ Item 2\n- Item 3";
        let doc = parse("test.rst", input);
        // Different bullets should result in separate lists
        assert_eq!(doc.nodes.len(), 3);
        for node in &doc.nodes {
            assert!(matches!(node, Node::BulletList { .. }));
        }
    }

    #[test]
    fn test_parse_bullet_list_empty_item() {
        let input = "* \n* Item 2";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::BulletList { items, .. } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            assert!(items[0].nodes.is_empty());
        }
    }

    #[test]
    fn test_parse_bullet_list_complex_nesting() {
        let input =
            "* Level 1\n\n  * Level 2\n\n    * Level 3\n\n  * Level 2 again\n\n* Level 1 again";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::BulletList { items, .. } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            // First item Level 1 has a Paragraph and a BulletList
            assert_eq!(items[0].nodes.len(), 2);
            if let Node::BulletList {
                items: l2_items, ..
            } = &items[0].nodes[1]
            {
                assert_eq!(l2_items.len(), 2);
                assert_eq!(l2_items[0].nodes.len(), 2); // Paragraph and Level 3 BulletList
            }
        }
    }
}
