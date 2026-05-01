//! The parser module converts RST text into an Abstract Syntax Tree (Document).

use crate::ast::{Directive, Document, HashedContent, Node};

/// Parses an RST-formatted string into a Document.
///
/// Current logic:
/// - A heading is defined as a line of text followed by a line composed purely
///   of punctuation character(s) (e.g. `===` or `---`) that is at least as long
///   as the text line above it.
/// - Heading levels are determined by the order in which each underline character
///   is first encountered in the document: the first character seen becomes level
///   1, the second distinct character level 2, etc.
/// - Otherwise, consecutive non-blank lines are grouped into a Paragraph.
///
/// # Panics
///
/// The internal implementation uses `expect()` on an iterator that is guaranteed
/// to be non-empty by preceding checks.
#[must_use]
pub fn parse(path: &str, input: &str) -> Document {
    let lines: Vec<&str> = input.lines().collect();
    let mut nodes = Vec::new();
    let mut adornment_order: Vec<char> = Vec::new();

    let mut i = 0;
    let mut diagnostics = Vec::new();

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if let Some((consumed, node)) = try_parse_directive(&lines, i, &mut diagnostics) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_target(&lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_heading(&lines, i, &mut adornment_order) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        let (consumed, node) = parse_paragraph(&lines, i);
        nodes.push(node);
        i += consumed;
    }

    if !diagnostics.is_empty() {
        eprintln!("Diagnostics for '{path}':");
        for diag in &diagnostics {
            eprintln!("  - {diag}");
        }
    }

    let mut doc = Document::new(path.to_string(), nodes);
    doc.diagnostics = diagnostics;
    doc
}

fn try_parse_target(lines: &[&str], i: usize) -> Option<(usize, Node)> {
    let line = lines[i].trim();
    if line.starts_with(".. _") && line.ends_with(':') {
        let name = &line[4..line.len() - 1];
        if !name.is_empty() {
            return Some((1, Node::Target(name.trim().to_string())));
        }
    }
    None
}

fn try_parse_directive(
    lines: &[&str],
    i: usize,
    diagnostics: &mut Vec<String>,
) -> Option<(usize, Node)> {
    let line = lines[i].trim_end();
    if !(line.trim().starts_with(".. ") && line.contains("::")) {
        return None;
    }

    let trimmed = line.trim();
    let (name_part, arg_part) = trimmed.split_once("::")?;
    let name_inner = name_part.strip_prefix(".. ")?;

    let name = name_inner.trim().to_string();
    let argument = arg_part.trim().to_string();

    let mut body_lines = Vec::new();
    let mut current = i + 1;
    while current < lines.len() {
        let next_line = lines[current].trim_end();
        if next_line.trim().is_empty() || next_line.starts_with(' ') || next_line.starts_with('\t')
        {
            body_lines.push(next_line);
        } else {
            break;
        }
        current += 1;
    }

    // Remove trailing and leading empty lines
    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }
    let mut start = 0;
    while start < body_lines.len() && body_lines[start].trim().is_empty() {
        start += 1;
    }

    let directive = if name == "toctree" {
        let mut paths = Vec::new();
        let mut maxdepth = None;
        let mut ignored_options = Vec::new();

        for l in &body_lines[start..] {
            let line = l.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with(':') {
                let opt_name = line.split(':').nth(1).unwrap_or("");
                match opt_name {
                    "maxdepth" => {
                        if let Some(rest) = line.strip_prefix(":maxdepth:")
                            && let Ok(depth) = rest.trim().parse::<usize>()
                        {
                            maxdepth = Some(depth);
                        }
                    }
                    "numbered" | "caption" | "name" | "titlesonly" | "glob" | "reversed"
                    | "hidden" | "includehidden" => {
                        ignored_options.push(line.to_string());
                    }
                    _ => {
                        diagnostics.push(format!(
                            "Invalid or non-standard Sphinx toctree option encountered: {line}"
                        ));
                    }
                }
                continue;
            }
            paths.push(line.to_string());
        }
        Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        }
    } else if name == "plantuml" {
        let mut body = String::new();
        for l in &body_lines[start..] {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(l.trim_start());
        }
        Directive::PlantUml(HashedContent::new(body))
    } else {
        let mut body = String::new();
        for l in &body_lines[start..] {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(l.trim_start());
        }
        Directive::Unknown {
            name,
            argument,
            body,
        }
    };

    Some((current - i, Node::Directive(directive)))
}

fn try_parse_heading(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<char>,
) -> Option<(usize, Node)> {
    if i + 1 >= lines.len() {
        return None;
    }

    let line = lines[i].trim_end();
    let next_line = lines[i + 1].trim();

    if !next_line.is_empty()
        && next_line.chars().all(|c| c.is_ascii_punctuation())
        && next_line.len() >= line.trim().len()
    {
        let adornment_char = next_line.chars().next().expect("non-empty underline");
        let level = if let Some(pos) = adornment_order.iter().position(|&c| c == adornment_char) {
            pos + 1
        } else {
            adornment_order.push(adornment_char);
            adornment_order.len()
        };

        #[allow(clippy::cast_possible_truncation)]
        let level = level as u8;

        return Some((
            2,
            Node::Heading {
                level,
                text: line.trim().to_string(),
            },
        ));
    }

    None
}

fn parse_paragraph(lines: &[&str], i: usize) -> (usize, Node) {
    let mut paragraph_text = String::new();
    let mut current_pos_line = i;

    while current_pos_line < lines.len() {
        let line = lines[current_pos_line].trim_end();
        if line.trim().is_empty() {
            break;
        }

        // Peek at next line to ensure we don't consume a heading's text line
        if current_pos_line + 1 < lines.len() {
            let peek_next = lines[current_pos_line + 1].trim();
            let is_heading = !peek_next.is_empty()
                && peek_next.chars().all(|c| c.is_ascii_punctuation())
                && peek_next.len() >= line.trim().len();
            if is_heading && !paragraph_text.is_empty() {
                break;
            }
        }

        if !paragraph_text.is_empty() {
            paragraph_text.push('\n');
        }
        paragraph_text.push_str(line.trim());
        current_pos_line += 1;
    }

    // split paragraph text by `:ref:\`target\``
    let mut inlines = Vec::new();
    let mut current_pos = 0;
    while let Some(start) = paragraph_text[current_pos..].find(":ref:`") {
        let absolute_start = current_pos + start;
        let search_start = absolute_start + 6; // length of ":ref:`"
        if let Some(end) = paragraph_text[search_start..].find('`') {
            let absolute_end = search_start + end;
            // push text before
            if absolute_start > current_pos {
                inlines.push(crate::ast::InlineNode::Text(
                    paragraph_text[current_pos..absolute_start].to_string(),
                ));
            }
            // push reference
            let target = paragraph_text[search_start..absolute_end].to_string();
            inlines.push(crate::ast::InlineNode::Reference(target));
            current_pos = absolute_end + 1;
        } else {
            break;
        }
    }
    if current_pos < paragraph_text.len() {
        inlines.push(crate::ast::InlineNode::Text(
            paragraph_text[current_pos..].to_string(),
        ));
    }

    (current_pos_line - i, Node::Paragraph(inlines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_returns_empty_document_for_empty_input() {
        // Given
        let input = "";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 0);
    }

    #[test]
    fn test_parse_creates_heading_node() {
        // Given
        let input = "Heading\n=======";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Heading".to_string()
            }
        );
    }

    #[test]
    fn test_parse_creates_paragraph_node() {
        // Given
        let input = "Just some\ntext";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![crate::ast::InlineNode::Text(
                "Just some\ntext".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_mixed_nodes_for_heading_and_paragraph() {
        // Given
        let input = "Title\n=====\n\nText.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Title".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Text.".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_paragraph_for_shorter_underline() {
        // Given
        let input = "Long Heading\n===";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![crate::ast::InlineNode::Text(
                "Long Heading\n===".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_heading_for_alternate_punctuation() {
        // Given
        let input = "Sub Title\n---------";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Sub Title".to_string()
            }
        );
    }

    #[test]
    fn test_parse_ignores_surrounding_whitespace_for_heading() {
        // Given
        let input = "Heading  \n  =======  \n\nNext";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Heading".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Next".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_multiple_paragraphs_ignoring_blank_lines() {
        // Given
        let input = "Para 1\n\n\nPara 2\n\nPara 3";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Para 1".to_string())])
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Para 2".to_string())])
        );
        assert_eq!(
            doc.nodes[2],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Para 3".to_string())])
        );
    }

    #[test]
    fn test_parse_handles_carriage_returns_gracefully() {
        // Given
        let input = "Heading\r\n=======\r\n\r\nPara\r\nline 2";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Heading".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text(
                "Para\nline 2".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_heading_from_punctuation_lines() {
        // Given
        let input = "===\n---";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "===".to_string()
            }
        );
    }

    #[test]
    fn test_parse_creates_h1_for_first_adornment_char() {
        // Given — a single heading using `=`
        let input = "Title\n=====";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Title".to_string()
            }
        );
    }

    #[test]
    fn test_parse_creates_h2_for_second_adornment_char() {
        // Given — first heading with `=`, second with `-`
        let input = "H1\n==\n\nH2\n--";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "H1".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 2,
                text: "H2".to_string()
            }
        );
    }

    #[test]
    fn test_parse_reuses_level_for_same_adornment_char() {
        // Given — both headings use the same `=` adornment
        let input = "First\n=====\n\nSecond\n======";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "First".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 1,
                text: "Second".to_string()
            }
        );
    }

    #[test]
    fn test_parse_assigns_levels_by_encounter_order() {
        // Given — three headings using `=`, `-`, and `~` in that order
        let input = "H1\n==\n\nH2\n--\n\nH3\n~~";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "H1".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 2,
                text: "H2".to_string()
            }
        );
        assert_eq!(
            doc.nodes[2],
            Node::Heading {
                level: 3,
                text: "H3".to_string()
            }
        );
    }

    #[test]
    fn test_parse_creates_directive() {
        // Given
        let input = ".. toctree::\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Next Para".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_directive_with_maxdepth() {
        // Given
        let input =
            ".. toctree::\n   :maxdepth: 2\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: Some(2),
                ignored_options: vec![],
            })
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Next Para".to_string())])
        );
    }

    #[test]
    fn test_parse_directive_with_argument_and_trailing_indents() {
        // Given
        let input = ".. code-block:: rust\n\n   let x = 1;\n   \n   let y = 2;\n\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Unknown {
                name: "code-block".to_string(),
                argument: "rust".to_string(),
                body: "let x = 1;\n\nlet y = 2;".to_string(),
            })
        );
    }

    #[test]
    fn test_parse_creates_plantuml_directive_with_hash() {
        // Given
        let input = ".. plantuml::\n\n   A -> B\n   B -> C\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);

        let expected = HashedContent::new("A -> B\nB -> C".to_string());
        assert_eq!(doc.nodes[0], Node::Directive(Directive::PlantUml(expected)));
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Next Para".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_target_node_for_explicit_target() {
        // Given
        let input = ".. _my-target:\n\nSome text.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0], Node::Target("my-target".to_string()));
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Some text.".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_inline_text_and_reference_nodes_for_paragraph() {
        // Given
        let input = "Here is a :ref:`my-target` link.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                crate::ast::InlineNode::Text("Here is a ".to_string()),
                crate::ast::InlineNode::Reference("my-target".to_string()),
                crate::ast::InlineNode::Text(" link.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_toctree_collects_diagnostic_for_invalid_option() {
        // Given
        let input = ".. toctree::\n   :invalid_opt:\n\n   foo";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.diagnostics.len(), 1);
        assert!(doc.diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
        assert!(doc.diagnostics[0].contains(":invalid_opt:"));

        if let Node::Directive(Directive::Toctree {
            paths,
            ignored_options,
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0], "foo");
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree directive");
        }
    }

    #[test]
    fn test_parse_toctree_whitelists_standard_options() {
        let standard_options = vec![
            "numbered",
            "caption: My Caption",
            "name: myname",
            "titlesonly",
            "glob",
            "reversed",
            "hidden",
            "includehidden",
        ];

        for opt in standard_options {
            // Given
            let input = format!(".. toctree::\n   :{opt}:\n\n   foo");

            // When
            let doc = parse("test.rst", &input);

            // Then
            assert!(
                doc.diagnostics.is_empty(),
                "Option :{opt} generated a diagnostic!"
            );

            if let Node::Directive(Directive::Toctree {
                ignored_options, ..
            }) = &doc.nodes[0]
            {
                assert_eq!(ignored_options.len(), 1);
                assert_eq!(ignored_options[0], format!(":{opt}:"));
            } else {
                panic!("Expected Toctree directive");
            }
        }
    }
}
