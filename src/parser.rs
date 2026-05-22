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
    let mut adornment_order: Vec<Adornment> = Vec::new();
    let mut diagnostics = Vec::new();

    let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics);

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

fn parse_blocks(
    lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if let Some((consumed, node)) = try_parse_directive(lines, i, adornment_order, diagnostics)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_target(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_heading(lines, i, adornment_order) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_bullet_list(lines, i, adornment_order, diagnostics)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        let (consumed, node) = parse_paragraph(lines, i);
        nodes.push(node);
        i += consumed;
    }
    nodes
}

fn try_parse_directive(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
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
    } else if let Ok(kind) = name.parse::<crate::ast::AdmonitionKind>() {
        parse_admonition(
            kind,
            argument,
            &body_lines[start..],
            adornment_order,
            diagnostics,
        )
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

    let level = adornment_order
        .iter()
        .position(|&a| a == adornment)
        .map_or_else(
            || {
                adornment_order.push(adornment);
                adornment_order.len()
            },
            |pos| pos + 1,
        );

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
        let inline_markup_match = find_inline_markup(&paragraph_text, last_match_end);

        // Find the one that starts earliest. If multiple start at the same pos, pick the longest.
        let mut all_matches = Vec::new();
        if let Some(m) = ref_match {
            all_matches.push((m.start(), m.end(), "ref", None));
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
            // Push text before the match
            if start > 0 {
                inlines.push(InlineNode::Text(remaining[..start].to_string()));
            }

            let m_str = &remaining[start..end];
            inlines.push(handle_inline_match(kind, m_str, node_opt));
            last_match_end += end;
        } else {
            // No more matches
            inlines.push(InlineNode::Text(remaining.to_string()));
            break;
        }
    }

    (current_pos_line - i, Node::Paragraph(inlines))
}

fn handle_inline_match(kind: &str, m_str: &str, node_opt: Option<InlineNode>) -> InlineNode {
    match kind {
        "inline" => node_opt.expect("inline node should be present"),
        "ref" => {
            let caps = REF_REGEX.captures(m_str).unwrap();
            InlineNode::Reference(caps["target"].to_string())
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

fn find_inline_markup(full_text: &str, start_offset: usize) -> Option<(usize, usize, InlineNode)> {
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

        // Try Strong Emphasis first (**)
        if text[i..].starts_with("**")
            && let Some((end_pos, content)) = try_match_inline(full_text, abs_i, 2)
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
            && let Some((end_pos, content)) = try_match_inline(full_text, abs_i, 1)
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

fn try_match_inline(
    full_text: &str,
    start_pos: usize,
    marker_len: usize,
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

        // Check for escaping of end marker
        if full_text.as_bytes()[abs_end_pos - 1] == b'\\' {
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

fn parse_admonition(
    kind: crate::ast::AdmonitionKind,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
) -> Directive {
    let title = if kind == crate::ast::AdmonitionKind::Admonition {
        if argument.is_empty() {
            diagnostics
                .push("Generic 'admonition' directive requires a title argument.".to_string());
            Some("Admonition".to_string())
        } else {
            Some(argument)
        }
    } else {
        None
    };

    let mut collapsible = None;

    // Strip common indentation and parse options
    if let Some(first) = body_lines.iter().find(|l| !l.trim().is_empty()) {
        let indent = first.chars().take_while(|c| c.is_whitespace()).count();
        let unindented_lines: Vec<String> = body_lines
            .iter()
            .map(|l| {
                if l.len() >= indent {
                    l[indent..].to_string()
                } else {
                    l.trim().to_string()
                }
            })
            .collect();

        // Parse options (specifically :collapsible:)
        let mut opt_idx = 0;
        while opt_idx < unindented_lines.len() {
            let line = unindented_lines[opt_idx].trim();
            if line.is_empty() {
                opt_idx += 1;
                continue;
            }
            if line.starts_with(':') && line.contains(':') {
                if line.starts_with(":collapsible:") {
                    let arg = line.strip_prefix(":collapsible:").unwrap().trim();
                    if arg == "open" {
                        collapsible = Some(true);
                    } else {
                        // Default to closed if ":collapsible:" or ":collapsible: close"
                        collapsible = Some(false);
                    }
                }
                opt_idx += 1;
            } else {
                break;
            }
        }

        // The rest is the body
        let body_content: Vec<&str> = unindented_lines[opt_idx..]
            .iter()
            .map(String::as_str)
            .collect();
        let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics);

        Directive::Admonition {
            kind,
            title,
            collapsible,
            body: body_nodes,
        }
    } else {
        Directive::Admonition {
            kind,
            title,
            collapsible: None,
            body: vec![],
        }
    }
}

fn strip_indent(s: &str, indent_chars: usize) -> &str {
    let mut indices = s.char_indices();
    if let Some((idx, _)) = indices.nth(indent_chars) {
        &s[idx..]
    } else {
        ""
    }
}

fn detect_bullet_item(line: &str) -> Option<(char, usize, usize)> {
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

fn try_parse_bullet_list(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
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
                            diagnostics.push(format!(
                                "Different bullet characters in adjacent lists ('{cur_bullet}' and '{bullet}'). A blank line is required to separate lists.",
                            ));
                        }
                        break;
                    }
                }
            }

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
            let body_nodes = parse_blocks(&body_refs, adornment_order, diagnostics);

            items.push(crate::ast::BulletListItem { nodes: body_nodes });
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
    fn test_parse_creates_admonition() {
        // Given
        let input = ".. note::\n\n   This is a note.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition {
            kind, title, body, ..
        }) = &doc.nodes[0]
        {
            assert_eq!(kind, &crate::ast::AdmonitionKind::Note);
            assert_eq!(title, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_admonition_with_title() {
        // Given
        let input = ".. admonition:: My Title\n\n   Custom content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { kind, title, .. }) = &doc.nodes[0] {
            assert_eq!(kind, &crate::ast::AdmonitionKind::Admonition);
            assert_eq!(title, &Some("My Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_admonition() {
        // Given
        let input = ".. note::\n   :collapsible:\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(false));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_open_admonition() {
        // Given
        let input = ".. note::\n   :collapsible: open\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(true));
        } else {
            panic!("Expected Admonition directive");
        }
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

    #[test]
    fn test_parse_blocks_empty_input() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let nodes = parse_blocks(&[], &mut adornment_order, &mut diagnostics);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_blocks_simple_paragraph() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let lines = vec!["Hello world"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics);
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::Paragraph(inlines) => {
                assert_eq!(inlines.len(), 1);
                assert_eq!(inlines[0], InlineNode::Text("Hello world".to_string()));
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn test_parse_blocks_maintains_adornment_order() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        let lines1 = vec!["Title 1", "======="];
        let nodes1 = parse_blocks(&lines1, &mut adornment_order, &mut diagnostics);
        assert_eq!(nodes1.len(), 1);

        let lines2 = vec!["Title 2", "-------"];
        let nodes2 = parse_blocks(&lines2, &mut adornment_order, &mut diagnostics);
        assert_eq!(nodes2.len(), 1);

        assert_eq!(adornment_order.len(), 2);
    }

    #[test]
    fn test_parse_blocks_collects_diagnostics() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // This will trigger a diagnostic because of the unknown option
        let lines = vec![".. toctree::", "   :unknown_option: value"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics);

        assert_eq!(nodes.len(), 1);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
    }

    #[test]
    fn test_parse_admonition_basic() {
        // Given
        let kind = crate::ast::AdmonitionKind::Note;
        let argument = String::new();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        if let Directive::Admonition {
            kind, title, body, ..
        } = directive
        {
            assert_eq!(kind, crate::ast::AdmonitionKind::Note);
            assert_eq!(title, None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_generic_with_title() {
        // Given
        let kind = crate::ast::AdmonitionKind::Admonition;
        let argument = "Custom Title".to_string();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        if let Directive::Admonition { kind, title, .. } = directive {
            assert_eq!(kind, crate::ast::AdmonitionKind::Admonition);
            assert_eq!(title, Some("Custom Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_collapsible() {
        // Given
        let kind = crate::ast::AdmonitionKind::Warning;
        let argument = String::new();
        let body_lines = vec!["   :collapsible: open", "", "   Content"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        if let Directive::Admonition { collapsible, .. } = directive {
            assert_eq!(collapsible, Some(true));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_admonition_generic_requires_title_diagnostic() {
        // Given
        let kind = crate::ast::AdmonitionKind::Admonition;
        let argument = String::new();
        let body_lines = vec!["   Body line"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let _ = parse_admonition(
            kind,
            argument,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
        );

        // Then
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].contains("requires a title argument"));
    }

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
    fn test_parse_paragraph_with_emphasis() {
        // Given
        let input = "*emphasized* text";

        // When
        let doc = parse("test.rst", input);

        // Then
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
    fn test_parse_paragraph_with_strong_emphasis() {
        // Given
        let input = "some **strong** text";

        // When
        let doc = parse("test.rst", input);

        // Then
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
    fn test_parse_paragraph_with_mixed_markup() {
        // Given
        let input = "Go to *emphasis* or **strong** link.";

        // When
        let doc = parse("test.rst", input);

        // Then
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
    fn test_parse_bullet_list_diagnostic_adjacent_different_bullets() {
        let input = "* Item 1\n- Item 2";
        let doc = parse("test.rst", input);
        // Should be 2 lists because of different bullets
        assert_eq!(doc.nodes.len(), 2);
        assert!(!doc.diagnostics.is_empty());
        assert!(doc.diagnostics[0].contains("Different bullet characters in adjacent lists"));
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

    #[test]
    fn test_parse_paragraph_with_escaped_markup() {
        // Given
        let input = r"Keep \*stars\* as is and **strong** text.";

        // When
        let doc = parse("test.rst", input);

        // Then
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
        // Given - markup requires specific boundaries
        let input = "a*text* *text*b";

        // When
        let doc = parse("test.rst", input);

        // Then
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
        // Given
        let input = "(*emphasis*), [**strong**];";

        // When
        let doc = parse("test.rst", input);

        // Then
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
    fn test_handle_inline_match() {
        use crate::ast::InlineNode;

        // Test "inline" variant
        let node = InlineNode::Emphasis("text".to_string());
        let result = handle_inline_match("inline", "", Some(node.clone()));
        assert_eq!(result, node);

        // Test "ref" variant
        let result = handle_inline_match("ref", ":ref:`target`", None);
        assert_eq!(result, InlineNode::Reference("target".to_string()));

        // Test "phrased" variant
        let result = handle_inline_match("phrased", "`text <http://uri>`_", None);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );

        let result = handle_inline_match("phrased", "`just text`_", None);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "just text".to_string(),
                target: "just text".to_string(),
            }
        );

        // Test "simple" variant
        let result = handle_inline_match("simple", "name_", None);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "name".to_string(),
                target: "name".to_string(),
            }
        );

        // Test "anon_phrased" variant
        let result = handle_inline_match("anon_phrased", "`text <http://uri>`__", None);
        assert_eq!(
            result,
            InlineNode::AnonymousHyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );

        let result = handle_inline_match("anon_phrased", "`anon text`__", None);
        assert_eq!(
            result,
            InlineNode::AnonymousReference("anon text".to_string())
        );

        // Test "anon_simple" variant
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
        let res = try_match_inline(input, 0, 1);

        // Then
        assert_eq!(res, Some((4, "π".to_string())));
    }

    #[test]
    fn test_parse_paragraph_with_emphasis_multibyte() {
        // Given
        let input = "*π* text";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(inlines[0], InlineNode::Emphasis("π".to_string()));
            assert_eq!(inlines[1], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }
}
