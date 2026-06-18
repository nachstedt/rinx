use crate::ast::InlineNode;
use regex::Regex;
use std::sync::LazyLock;

static REF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":ref:`(?P<target>[^`]+)`").unwrap());
static PROGRAM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":program:`(?P<name>[^`]+)`").unwrap());
static TERM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":term:`(?P<content>[^`]+)`").unwrap());
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

/// Parses a plain text string into a list of [`InlineNode`]s.
pub(super) fn parse_inline_text(paragraph_text: &str) -> Vec<InlineNode> {
    let mut inlines = Vec::new();
    let mut last_match_end = 0;

    while last_match_end < paragraph_text.len() {
        let remaining = &paragraph_text[last_match_end..];

        let ref_match = REF_REGEX.find(remaining);
        let program_match = PROGRAM_ROLE_REGEX.find(remaining);
        let term_match = TERM_ROLE_REGEX.find(remaining);
        let phrased_match = PHRASED_LINK_REGEX.find(remaining);
        let simple_match = SIMPLE_LINK_REGEX.find(remaining);
        let anon_phrased_match = ANONYMOUS_PHRASED_REGEX.find(remaining);
        let anon_simple_match = ANONYMOUS_SIMPLE_REGEX.find(remaining);
        let inline_markup_match = find_inline_markup(paragraph_text, last_match_end);

        let mut all_matches = Vec::new();
        if let Some(m) = ref_match {
            all_matches.push((m.start(), m.end(), "ref", None));
        }
        if let Some(m) = program_match {
            all_matches.push((m.start(), m.end(), "program", None));
        }
        if let Some(m) = term_match {
            all_matches.push((m.start(), m.end(), "term", None));
        }
        if let Some(m) = anon_phrased_match {
            all_matches.push((m.start(), m.end(), "anon_phrased", None));
        }
        if let Some(m) = phrased_match {
            all_matches.push((m.start(), m.end(), "phrased", None));
        }
        if let Some(m) = anon_simple_match {
            all_matches.push((m.start(), m.end(), "anon_simple", None));
        }
        if let Some(m) = simple_match {
            all_matches.push((m.start(), m.end(), "simple", None));
        }
        if let Some((start, end, node)) = inline_markup_match {
            all_matches.push((start, end, "inline", Some(node)));
        }

        let earliest = all_matches
            .into_iter()
            .min_by_key(|(start, end, _, _)| (*start, std::cmp::Reverse(*end)));

        if let Some((start, end, kind, node_opt)) = earliest {
            if start > 0 {
                inlines.push(InlineNode::Text(remaining[..start].to_string()));
            }
            let m_str = &remaining[start..end];
            inlines.push(handle_inline_match(kind, m_str, node_opt));
            last_match_end += end;
        } else {
            inlines.push(InlineNode::Text(remaining.to_string()));
            break;
        }
    }
    inlines
}

pub(super) fn handle_inline_match(
    kind: &str,
    m_str: &str,
    node_opt: Option<InlineNode>,
) -> InlineNode {
    match kind {
        "inline" => node_opt.expect("inline node should be present"),
        "ref" => {
            let caps = REF_REGEX.captures(m_str).unwrap();
            InlineNode::Reference(caps["target"].to_string())
        }
        "program" => {
            let caps = PROGRAM_ROLE_REGEX.captures(m_str).unwrap();
            InlineNode::Program(caps["name"].to_string())
        }
        "term" => {
            let caps = TERM_ROLE_REGEX.captures(m_str).unwrap();
            let content = &caps["content"];
            // Support :term:`display text <actual term>` syntax
            if let Some(angle_start) = content.rfind('<')
                && let Some(angle_end) = content[angle_start..].find('>')
            {
                let display = content[..angle_start].trim().to_string();
                let term = content[angle_start + 1..angle_start + angle_end]
                    .trim()
                    .to_string();
                return InlineNode::TermReference { display, term };
            }
            InlineNode::TermReference {
                display: content.to_string(),
                term: content.to_string(),
            }
        }
        "phrased" => {
            let caps = PHRASED_LINK_REGEX.captures(m_str).unwrap();
            let text_full = &caps["text"];
            if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                InlineNode::Hyperlink {
                    text: embedded["text"].trim().to_string(),
                    target: embedded["uri"].to_string(),
                }
            } else {
                InlineNode::Hyperlink {
                    text: text_full.to_string(),
                    target: text_full.to_string(),
                }
            }
        }
        "simple" => {
            let caps = SIMPLE_LINK_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::Hyperlink {
                text: name.to_string(),
                target: name.to_string(),
            }
        }
        "anon_phrased" => {
            let caps = ANONYMOUS_PHRASED_REGEX.captures(m_str).unwrap();
            let text_full = &caps["text"];
            if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                InlineNode::AnonymousHyperlink {
                    text: embedded["text"].trim().to_string(),
                    target: embedded["uri"].to_string(),
                }
            } else {
                InlineNode::AnonymousReference(text_full.to_string())
            }
        }
        "anon_simple" => {
            let caps = ANONYMOUS_SIMPLE_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::AnonymousReference(name.to_string())
        }
        _ => unreachable!(),
    }
}

pub(super) fn find_inline_markup(
    full_text: &str,
    start_offset: usize,
) -> Option<(usize, usize, InlineNode)> {
    let text = &full_text[start_offset..];
    let mut best_match: Option<(usize, usize, InlineNode)> = None;

    for (i, _) in text.char_indices() {
        let abs_i = start_offset + i;

        // Check for escaping
        if abs_i > 0 && full_text.as_bytes()[abs_i - 1] == b'\\' {
            // Count backslashes to see if it's escaped or the backslash itself is escaped
            let mut bs_count = 0;
            let mut j = abs_i - 1;
            while full_text.as_bytes()[j] == b'\\' {
                bs_count += 1;
                if j == 0 {
                    break;
                }
                j -= 1;
            }
            if bs_count % 2 != 0 {
                continue;
            }
        }

        // Try Inline Literal first (``)
        if text[i..].starts_with("``")
            && let Some((end_pos, content)) = try_match_inline(full_text, abs_i, 2, true)
        {
            let m = (i, i + (end_pos - abs_i), InlineNode::Literal(content));
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }

        // Try Strong Emphasis next (**)
        if text[i..].starts_with("**")
            && let Some((end_pos, content)) = try_match_inline(full_text, abs_i, 2, false)
        {
            let m = (i, i + (end_pos - abs_i), InlineNode::Strong(content));
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }

        // Try Emphasis (*)
        if text[i..].starts_with('*')
            && !text[i..].starts_with("**")
            && let Some((end_pos, content)) = try_match_inline(full_text, abs_i, 1, false)
        {
            let m = (i, i + (end_pos - abs_i), InlineNode::Emphasis(content));
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }
    }

    best_match
}

pub(super) fn try_match_inline(
    full_text: &str,
    start_pos: usize,
    marker_len: usize,
    is_literal: bool,
) -> Option<(usize, String)> {
    // Start context check
    if start_pos > 0 {
        let prev_char = full_text[..start_pos].chars().next_back().unwrap();
        if !prev_char.is_whitespace() && !"-:/'\"<([{".contains(prev_char) {
            return None;
        }
    }

    let after_start = start_pos + marker_len;
    if after_start >= full_text.len() {
        return None;
    }

    let first_inner = full_text[after_start..].chars().next().unwrap();
    if first_inner.is_whitespace() {
        return None;
    }

    // Find end marker
    let marker = &full_text[start_pos..start_pos + marker_len];
    let mut search_pos = after_start + first_inner.len_utf8();

    while let Some(end_marker_pos) = full_text[search_pos..].find(marker) {
        let abs_end_pos = search_pos + end_marker_pos;

        // End context check
        let last_inner = full_text[..abs_end_pos].chars().next_back().unwrap();
        if last_inner.is_whitespace() {
            search_pos = abs_end_pos + 1;
            continue;
        }

        // Check character after end marker
        let after_end = abs_end_pos + marker_len;
        if after_end < full_text.len() {
            let next_char = full_text[after_end..].chars().next().unwrap();
            if !next_char.is_whitespace() && !"-.,:;!?\\/ '\" >)]}".contains(next_char) {
                search_pos = abs_end_pos + 1;
                continue;
            }
        }

        // Check for escaping of end marker (skip if is_literal)
        if !is_literal && full_text.as_bytes()[abs_end_pos - 1] == b'\\' {
            let mut bs_count = 0;
            let mut j = abs_end_pos - 1;
            while full_text.as_bytes()[j] == b'\\' {
                bs_count += 1;
                if j == 0 {
                    break;
                }
                j -= 1;
            }
            if bs_count % 2 != 0 {
                search_pos = abs_end_pos + 1;
                continue;
            }
        }

        // Success!
        let content = full_text[after_start..abs_end_pos].to_string();
        if content.is_empty() {
            search_pos = abs_end_pos + 1;
            continue;
        }

        return Some((abs_end_pos + marker_len, content));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_inline_match_inline_variant() {
        // Given
        let node = InlineNode::Emphasis("text".to_string());
        // When
        let result = handle_inline_match("inline", "", Some(node.clone()));
        // Then
        assert_eq!(result, node);
    }

    #[test]
    fn test_handle_inline_match_ref_variant() {
        let result = handle_inline_match("ref", ":ref:`target`", None);
        assert_eq!(result, InlineNode::Reference("target".to_string()));
    }

    #[test]
    fn test_handle_inline_match_program_variant() {
        let result = handle_inline_match("program", ":program:`curl`", None);
        assert_eq!(result, InlineNode::Program("curl".to_string()));
    }

    #[test]
    fn test_handle_inline_match_phrased_with_embedded_uri() {
        let result = handle_inline_match("phrased", "`text <http://uri>`_", None);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );
    }

    #[test]
    fn test_handle_inline_match_phrased_without_uri() {
        let result = handle_inline_match("phrased", "`just text`_", None);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "just text".to_string(),
                target: "just text".to_string(),
            }
        );
    }

    #[test]
    fn test_handle_inline_match_simple_variant() {
        let result = handle_inline_match("simple", "name_", None);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "name".to_string(),
                target: "name".to_string(),
            }
        );
    }

    #[test]
    fn test_handle_inline_match_anon_phrased_with_embedded_uri() {
        let result = handle_inline_match("anon_phrased", "`text <http://uri>`__", None);
        assert_eq!(
            result,
            InlineNode::AnonymousHyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );
    }

    #[test]
    fn test_handle_inline_match_anon_phrased_without_uri() {
        let result = handle_inline_match("anon_phrased", "`anon text`__", None);
        assert_eq!(
            result,
            InlineNode::AnonymousReference("anon text".to_string())
        );
    }

    #[test]
    fn test_handle_inline_match_anon_simple_variant() {
        let result = handle_inline_match("anon_simple", "anon_name__", None);
        assert_eq!(
            result,
            InlineNode::AnonymousReference("anon_name".to_string())
        );
    }

    #[test]
    fn test_try_match_inline_multibyte_first_inner() {
        // Given
        let input = "*π*";
        // When
        let res = try_match_inline(input, 0, 1, false);
        // Then
        assert_eq!(res, Some((4, "π".to_string())));
    }

    #[test]
    fn test_try_match_inline_rejects_space_after_open_marker() {
        // Given: space immediately after marker is not valid markup
        let input = "* not emphasis *";
        // When
        let res = try_match_inline(input, 0, 1, false);
        // Then
        assert_eq!(res, None);
    }

    #[test]
    fn test_try_match_inline_requires_valid_end_boundary() {
        // Given: no valid end boundary
        let input = "*nospace*x";
        // When
        let res = try_match_inline(input, 0, 1, false);
        // Then
        assert_eq!(res, None);
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::ast::InlineNode;
    use crate::ast::Node;
    use crate::parser::parse;

    #[test]
    fn test_parse_creates_inline_text_and_reference_nodes_for_paragraph() {
        let input = "Here is a :ref:`my-target` link.";
        let doc = parse("test.rst", input);
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
        let input = "Check the `Python Guide`_ for more.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Check the ".to_string()),
                InlineNode::Hyperlink {
                    text: "Python Guide".to_string(),
                    target: "Python Guide".to_string()
                },
                InlineNode::Text(" for more.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_embedded_uri_hyperlink_node() {
        let input = "Check `Google <https://google.com>`_ now.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Check ".to_string()),
                InlineNode::Hyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string()
                },
                InlineNode::Text(" now.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_simple_link_node() {
        let input = "Refer to target_ for details.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Refer to ".to_string()),
                InlineNode::Hyperlink {
                    text: "target".to_string(),
                    target: "target".to_string()
                },
                InlineNode::Text(" for details.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_anonymous_target_node() {
        let input = ".. __: https://example.com\n\nText";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::AnonymousTarget {
                uri: "https://example.com".to_string()
            }
        );
    }

    #[test]
    fn test_parse_creates_anonymous_reference() {
        let input = "See `Example`__ and link__";
        let doc = parse("test.rst", input);
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
        let input = "See `Google <https://google.com>`__";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(
                inlines[1],
                InlineNode::AnonymousHyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string()
                }
            );
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_parse_paragraph_with_emphasis() {
        let input = "*emphasized* text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(inlines[0], InlineNode::Emphasis("emphasized".to_string()));
            assert_eq!(inlines[1], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_strong_emphasis() {
        let input = "some **strong** text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(inlines[0], InlineNode::Text("some ".to_string()));
            assert_eq!(inlines[1], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_inline_literal() {
        let input = "some ``venv`` text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(inlines[0], InlineNode::Text("some ".to_string()));
            assert_eq!(inlines[1], InlineNode::Literal("venv".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_inline_literal_with_backslash() {
        let input = "``some\\path``";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 1);
            assert_eq!(inlines[0], InlineNode::Literal("some\\path".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_inline_literal_ignoring_inner_markup() {
        let input = "``**bold**``";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 1);
            assert_eq!(inlines[0], InlineNode::Literal("**bold**".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_mixed_markup() {
        let input = "Go to *emphasis* or **strong** link.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 5);
            assert_eq!(inlines[0], InlineNode::Text("Go to ".to_string()));
            assert_eq!(inlines[1], InlineNode::Emphasis("emphasis".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" or ".to_string()));
            assert_eq!(inlines[3], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[4], InlineNode::Text(" link.".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_escaped_markup() {
        let input = r"Keep \*stars\* as is and **strong** text.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(
                inlines[0],
                InlineNode::Text(r"Keep \*stars\* as is and ".to_string())
            );
            assert_eq!(inlines[1], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" text.".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_rejects_invalid_boundary_markup() {
        let input = "a*text* *text*b";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 1);
            assert_eq!(inlines[0], InlineNode::Text("a*text* *text*b".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_punctuation_boundaries() {
        let input = "(*emphasis*), [**strong**];";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 5);
            assert_eq!(inlines[0], InlineNode::Text("(".to_string()));
            assert_eq!(inlines[1], InlineNode::Emphasis("emphasis".to_string()));
            assert_eq!(inlines[2], InlineNode::Text("), [".to_string()));
            assert_eq!(inlines[3], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[4], InlineNode::Text("];".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_emphasis_multibyte() {
        let input = "*\u{03c0}* text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(inlines[0], InlineNode::Emphasis("\u{03c0}".to_string()));
            assert_eq!(inlines[1], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_program_role() {
        let input = "Run :program:`curl` to download files.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(inlines[0], InlineNode::Text("Run ".to_string()));
            assert_eq!(inlines[1], InlineNode::Program("curl".to_string()));
            assert_eq!(
                inlines[2],
                InlineNode::Text(" to download files.".to_string())
            );
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_multiple_program_roles_and_punctuation() {
        let input = "Use :program:`git` or :program:`hg` to manage code.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 5);
            assert_eq!(inlines[0], InlineNode::Text("Use ".to_string()));
            assert_eq!(inlines[1], InlineNode::Program("git".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" or ".to_string()));
            assert_eq!(inlines[3], InlineNode::Program("hg".to_string()));
            assert_eq!(inlines[4], InlineNode::Text(" to manage code.".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_term_role_basic() {
        let input = "See :term:`environment` for details.\n";
        let doc = parse("test.rst", input);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let term_ref = inlines
                .iter()
                .find(|n| matches!(n, InlineNode::TermReference { .. }));
            assert!(term_ref.is_some(), "Expected TermReference in paragraph");
            if let Some(InlineNode::TermReference { display, term }) = term_ref {
                assert_eq!(display, "environment");
                assert_eq!(term, "environment");
            }
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_term_role_with_display_text() {
        let input = "See :term:`the env <environment>` here.\n";
        let doc = parse("test.rst", input);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let term_ref = inlines
                .iter()
                .find(|n| matches!(n, InlineNode::TermReference { .. }));
            assert!(term_ref.is_some());
            if let Some(InlineNode::TermReference { display, term }) = term_ref {
                assert_eq!(display, "the env");
                assert_eq!(term, "environment");
            }
        } else {
            panic!("Expected Paragraph");
        }
    }

    #[test]
    fn test_parse_term_role_mixed_with_surrounding_text() {
        let input = "Before :term:`foo` after.\n";
        let doc = parse("test.rst", input);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.len() >= 3, "Expected text + term + text");
            assert!(matches!(&inlines[0], InlineNode::Text(t) if t == "Before "));
            assert!(matches!(&inlines[1], InlineNode::TermReference { term, .. } if term == "foo"));
        } else {
            panic!("Expected Paragraph");
        }
    }
}
