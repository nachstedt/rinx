//! The parser module converts RST text into an Abstract Syntax Tree (Document).

use crate::ast::{Directive, Document, HashedContent, InlineNode, Node, TargetName};
use regex::Regex;
use std::sync::LazyLock;

static REF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":ref:`(?P<target>[^`]+)`").unwrap());
static PHRASED_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(?P<text>[^`]+)`_").unwrap());
static SIMPLE_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<name>[a-zA-Z0-9_.-]+)_").unwrap());
static EMBEDDED_URI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<text>.*)\s+<(?P<uri>[^>]+)>$").unwrap());
static ANONYMOUS_PHRASED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(?P<text>[^`]+)`__").unwrap());
static ANONYMOUS_SIMPLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<name>[a-zA-Z0-9_.-]+)__").unwrap());

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
    let mut adornment_order: Vec<Adornment> = Vec::new();

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
    if let Some(rest) = line.strip_prefix(".. __:") {
        let mut uri = rest.trim().to_string();
        let mut consumed = 1;

        if uri.is_empty() && i + 1 < lines.len() {
            let next_line = lines[i + 1];
            if next_line.starts_with(' ') || next_line.starts_with('\t') {
                uri = next_line.trim().to_string();
                consumed = 2;
            }
        }

        if !uri.is_empty() {
            return Some((consumed, Node::AnonymousTarget { uri }));
        }
    }

    if !line.starts_with(".. _") {
        return None;
    }

    // Try to find the colon that ends the target name
    if let Some(colon_pos) = line[4..].find(':') {
        let absolute_colon_pos = 4 + colon_pos;
        let name_str = &line[4..absolute_colon_pos].trim();
        if name_str.is_empty() {
            return None;
        }

        let mut uri = line[absolute_colon_pos + 1..].trim().to_string();
        let mut consumed = 1;

        // If URI is empty on the same line, check the next line for an indented block
        if uri.is_empty() && i + 1 < lines.len() {
            let next_line = lines[i + 1];
            if next_line.starts_with(' ') || next_line.starts_with('\t') {
                uri = next_line.trim().to_string();
                consumed = 2;
            }
        }

        let uri_opt = if uri.is_empty() { None } else { Some(uri) };

        return Some((
            consumed,
            Node::Target {
                name: TargetName::new(name_str),
                uri: uri_opt,
            },
        ));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdornmentStyle {
    Underline,
    Overline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Adornment {
    character: char,
    style: AdornmentStyle,
}

fn detect_adornment(lines: &[&str], i: usize) -> Option<(usize, Adornment, String)> {
    // 1. Try 3-line pattern (Overline + Text + Underline)
    if i + 2 < lines.len() {
        let overline = lines[i].trim();
        let text = lines[i + 1].trim();
        let underline = lines[i + 2].trim();

        if !overline.is_empty()
            && overline.chars().all(|c| c.is_ascii_punctuation())
            && overline == underline
            && overline.len() >= text.len()
        {
            let adornment_char = overline.chars().next().expect("non-empty adornment");
            return Some((
                3,
                Adornment {
                    character: adornment_char,
                    style: AdornmentStyle::Overline,
                },
                text.to_string(),
            ));
        }
    }

    // 2. Try 2-line pattern (Text + Underline)
    if i + 1 < lines.len() {
        let text = lines[i].trim();
        let underline = lines[i + 1].trim();

        if !underline.is_empty()
            && underline.chars().all(|c| c.is_ascii_punctuation())
            && underline.len() >= text.len()
        {
            let adornment_char = underline.chars().next().expect("non-empty underline");
            return Some((
                2,
                Adornment {
                    character: adornment_char,
                    style: AdornmentStyle::Underline,
                },
                text.to_string(),
            ));
        }
    }

    None
}

fn try_parse_heading(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
) -> Option<(usize, Node)> {
    let (consumed, adornment, text) = detect_adornment(lines, i)?;

    let level = if let Some(pos) = adornment_order.iter().position(|&a| a == adornment) {
        pos + 1
    } else {
        adornment_order.push(adornment);
        adornment_order.len()
    };

    #[allow(clippy::cast_possible_truncation)]
    let level = level as u8;

    Some((consumed, Node::Heading { level, text }))
}

fn parse_paragraph(lines: &[&str], i: usize) -> (usize, Node) {
    let mut paragraph_text = String::new();
    let mut current_pos_line = i;

    while current_pos_line < lines.len() {
        let line = lines[current_pos_line].trim_end();
        if line.trim().is_empty() {
            break;
        }

        if !paragraph_text.is_empty() {
            paragraph_text.push('\n');
        }
        paragraph_text.push_str(line.trim());
        current_pos_line += 1;

        // Peek at next line to ensure we don't consume a heading's text line or overline
        if current_pos_line < lines.len() && detect_adornment(lines, current_pos_line).is_some() {
            break;
        }
    }

    let mut inlines = Vec::new();
    let mut last_match_end = 0;

    // We need to find the earliest match among all regexes
    while last_match_end < paragraph_text.len() {
        let remaining = &paragraph_text[last_match_end..];

        let ref_match = REF_REGEX.find(remaining);
        let phrased_match = PHRASED_LINK_REGEX.find(remaining);
        let simple_match = SIMPLE_LINK_REGEX.find(remaining);
        let anon_phrased_match = ANONYMOUS_PHRASED_REGEX.find(remaining);
        let anon_simple_match = ANONYMOUS_SIMPLE_REGEX.find(remaining);

        // Find the one that starts earliest. If multiple start at the same pos, pick the longest.
        let matches = vec![
            ref_match.map(|m| (m, "ref")),
            anon_phrased_match.map(|m| (m, "anon_phrased")),
            phrased_match.map(|m| (m, "phrased")),
            anon_simple_match.map(|m| (m, "anon_simple")),
            simple_match.map(|m| (m, "simple")),
        ];

        let earliest = matches
            .into_iter()
            .flatten()
            .min_by_key(|(m, _)| (m.start(), std::cmp::Reverse(m.end())));

        if let Some((m, kind)) = earliest {
            // Push text before the match
            if m.start() > 0 {
                inlines.push(InlineNode::Text(remaining[..m.start()].to_string()));
            }

            match kind {
                "ref" => {
                    let caps = REF_REGEX.captures(m.as_str()).unwrap();
                    inlines.push(InlineNode::Reference(caps["target"].to_string()));
                }
                "phrased" => {
                    let caps = PHRASED_LINK_REGEX.captures(m.as_str()).unwrap();
                    let text_full = &caps["text"];
                    if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                        inlines.push(InlineNode::Hyperlink {
                            text: embedded["text"].trim().to_string(),
                            target: embedded["uri"].to_string(),
                        });
                    } else {
                        inlines.push(InlineNode::Hyperlink {
                            text: text_full.to_string(),
                            target: text_full.to_string(),
                        });
                    }
                }
                "simple" => {
                    let caps = SIMPLE_LINK_REGEX.captures(m.as_str()).unwrap();
                    let name = &caps["name"];
                    inlines.push(InlineNode::Hyperlink {
                        text: name.to_string(),
                        target: name.to_string(),
                    });
                }
                "anon_phrased" => {
                    let caps = ANONYMOUS_PHRASED_REGEX.captures(m.as_str()).unwrap();
                    let text_full = &caps["text"];
                    if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                        inlines.push(InlineNode::AnonymousHyperlink {
                            text: embedded["text"].trim().to_string(),
                            target: embedded["uri"].to_string(),
                        });
                    } else {
                        inlines.push(InlineNode::AnonymousReference(text_full.to_string()));
                    }
                }
                "anon_simple" => {
                    let caps = ANONYMOUS_SIMPLE_REGEX.captures(m.as_str()).unwrap();
                    let name = &caps["name"];
                    inlines.push(InlineNode::AnonymousReference(name.to_string()));
                }
                _ => unreachable!(),
            }
            last_match_end += m.end();
        } else {
            // No more matches
            inlines.push(InlineNode::Text(remaining.to_string()));
            break;
        }
    }

    (current_pos_line - i, Node::Paragraph(inlines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::InlineNode;

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
        assert_eq!(
            doc.nodes[0],
            Node::Target {
                name: TargetName::new("my-target"),
                uri: None
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Some text.".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_external_target_node() {
        // Given
        let input = ".. _my-link: https://example.com\n\nSome text.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Target {
                name: TargetName::new("my-link"),
                uri: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn test_parse_creates_indented_external_target_node() {
        // Given
        let input = ".. _my-link:\n   https://example.com\n\nSome text.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Target {
                name: TargetName::new("my-link"),
                uri: Some("https://example.com".to_string())
            }
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
                InlineNode::Text("Here is a ".to_string()),
                InlineNode::Reference("my-target".to_string()),
                InlineNode::Text(" link.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_phrased_hyperlink_node() {
        // Given
        let input = "Check the `Python Guide`_ for more.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Check the ".to_string()),
                InlineNode::Hyperlink {
                    text: "Python Guide".to_string(),
                    target: "Python Guide".to_string(),
                },
                InlineNode::Text(" for more.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_embedded_uri_hyperlink_node() {
        // Given
        let input = "Check `Google <https://google.com>`_ now.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Check ".to_string()),
                InlineNode::Hyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string(),
                },
                InlineNode::Text(" now.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_simple_link_node() {
        // Given
        let input = "Refer to target_ for details.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Refer to ".to_string()),
                InlineNode::Hyperlink {
                    text: "target".to_string(),
                    target: "target".to_string(),
                },
                InlineNode::Text(" for details.".to_string()),
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

    #[test]
    fn test_parse_creates_heading_with_overline() {
        // Given
        let input = "#######\nHeading\n#######";

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
    fn test_parse_overline_and_underline_distinct_levels() {
        // Given — same char '#' but different styles
        let input = "##########\nOverline\n##########\n\nUnderline\n#########";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Overline".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 2,
                text: "Underline".to_string()
            }
        );
    }

    #[test]
    fn test_parse_overline_requires_exact_match_with_underline() {
        // Given — mismatched overline/underline length
        let input = "#######\nHeading\n######";

        // When
        let doc = parse("test.rst", input);

        // Then — should not be a heading
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Paragraph(_) => {}
            _ => panic!("Expected paragraph for mismatched overline/underline"),
        }
    }

    #[test]
    fn test_parse_paragraph_breaks_at_overline() {
        // Given
        let input = "Para text.\n#######\nHeading\n#######";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![crate::ast::InlineNode::Text("Para text.".to_string())])
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 1,
                text: "Heading".to_string()
            }
        );
    }

    #[test]
    fn test_detect_adornment_overline() {
        let lines = vec!["#######", "Heading", "#######"];
        let result = detect_adornment(&lines, 0);
        assert!(result.is_some());
        let (consumed, adornment, text) = result.unwrap();
        assert_eq!(consumed, 3);
        assert_eq!(adornment.character, '#');
        assert_eq!(adornment.style, AdornmentStyle::Overline);
        assert_eq!(text, "Heading");
    }

    #[test]
    fn test_detect_adornment_underline() {
        let lines = vec!["Heading", "#######"];
        let result = detect_adornment(&lines, 0);
        assert!(result.is_some());
        let (consumed, adornment, text) = result.unwrap();
        assert_eq!(consumed, 2);
        assert_eq!(adornment.character, '#');
        assert_eq!(adornment.style, AdornmentStyle::Underline);
        assert_eq!(text, "Heading");
    }

    #[test]
    fn test_parse_creates_anonymous_target_node() {
        // Given
        let input = ".. __: https://example.com\n\nText";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::AnonymousTarget {
                uri: "https://example.com".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_creates_anonymous_reference() {
        // Given
        let input = "See `Example`__ and link__";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 4);
            assert_eq!(
                inlines[1],
                InlineNode::AnonymousReference("Example".to_string())
            );
            assert_eq!(
                inlines[3],
                InlineNode::AnonymousReference("link".to_string())
            );
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_parse_creates_anonymous_hyperlink_with_embedded_uri() {
        // Given
        let input = "See `Google <https://google.com>`__";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(
                inlines[1],
                InlineNode::AnonymousHyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string(),
                }
            );
        } else {
            panic!("Expected paragraph");
        }
    }
}
