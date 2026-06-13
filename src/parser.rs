//! The parser module converts RST text into an Abstract Syntax Tree (Document).

use crate::ast::{Directive, Document, HashedContent, InlineNode, Node, TargetName};
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

        let (consumed, new_nodes) = parse_paragraph(lines, i);
        nodes.extend(new_nodes);
        i += consumed;
    }
    nodes
}

fn collect_directive_body<'a>(lines: &[&'a str], start_index: usize) -> (usize, Vec<&'a str>) {
    let mut body_lines = Vec::new();
    let mut current = start_index;
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

    let consumed = current - start_index;

    // Remove trailing empty lines
    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }

    let mut start = 0;
    while start < body_lines.len() && body_lines[start].trim().is_empty() {
        start += 1;
    }

    (consumed, body_lines[start..].to_vec())
}

fn join_body_lines(body_lines: &[&str]) -> String {
    let mut body = String::new();
    for l in body_lines {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(l.trim_start());
    }
    body
}

/// Collects a literal block body starting at `start_index`.
///
/// - Skips a leading blank line (mandatory after `::`).
/// - Collects contiguous lines until indentation drops to (or below) the base level.
/// - Strips the *minimum* common indentation from all non-blank lines, preserving relative
///   indentation within the block (RST spec behaviour).
///
/// Returns `(lines_consumed, verbatim_content)`.
fn collect_literal_block_body(lines: &[&str], start_index: usize) -> (usize, String) {
    let mut current = start_index;

    // Skip the mandatory blank line after `::`
    if current < lines.len() && lines[current].trim().is_empty() {
        current += 1;
    }

    // Find the first non-blank line to establish the base indentation level
    let Some(first_non_blank) = lines[current..].iter().find(|l| !l.trim().is_empty()) else {
        return (current - start_index, String::new());
    };
    let base_indent = first_non_blank
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();

    if base_indent == 0 {
        // No indented block follows
        return (current - start_index, String::new());
    }

    // Collect lines that belong to the block; blank lines are kept as separators
    let mut body_lines: Vec<&str> = Vec::new();
    while current < lines.len() {
        let line = lines[current];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body_lines.push("");
            current += 1;
            continue;
        }
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent < base_indent {
            break;
        }
        body_lines.push(line);
        current += 1;
    }

    // Remove trailing blank lines
    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }

    // Compute the minimum indentation of all non-blank lines
    let min_indent = body_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    // Strip the common indent; leave blank lines as empty strings
    let content = body_lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                l.chars().skip(min_indent).collect()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    (current - start_index, content)
}

fn parse_toctree(body_lines: &[&str], diagnostics: &mut Vec<String>) -> Directive {
    let mut paths = Vec::new();
    let mut maxdepth = None;
    let mut ignored_options = Vec::new();

    for l in body_lines {
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
                "numbered" | "caption" | "name" | "titlesonly" | "glob" | "reversed" | "hidden"
                | "includehidden" => {
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
    let name = name_part.strip_prefix(".. ")?.trim().to_string();
    let argument = arg_part.trim().to_string();

    let (consumed_lines, body_lines) = collect_directive_body(lines, i + 1);

    if name == "toctree" {
        let directive = parse_toctree(&body_lines, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "plantuml" {
        let directive = Directive::PlantUml(HashedContent::new(join_body_lines(&body_lines)));
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "code-block" {
        let language = if argument.is_empty() {
            None
        } else {
            Some(argument)
        };
        // body_lines was collected by collect_directive_body; strip common indentation
        // to preserve relative indentation within the block (RST spec behaviour).
        let min_indent = body_lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
            .min()
            .unwrap_or(0);
        let content = body_lines
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    String::new()
                } else {
                    l.chars().skip(min_indent).collect()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Some((1 + consumed_lines, Node::LiteralBlock { language, content }));
    }
    if let Ok(kind) = name.parse::<crate::ast::VersionChangeKind>() {
        let directive =
            parse_version_change(kind, argument, &body_lines, adornment_order, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "seealso" {
        let directive = parse_seealso(&body_lines, adornment_order, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if let Ok(kind) = name.parse::<crate::ast::AdmonitionKind>() {
        let directive = parse_admonition(kind, argument, &body_lines, adornment_order, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    if name == "glossary" {
        let directive = parse_glossary(&body_lines, adornment_order, diagnostics);
        return Some((1 + consumed_lines, Node::Directive(directive)));
    }
    let directive = Directive::Unknown {
        name,
        argument,
        body: join_body_lines(&body_lines),
    };
    Some((1 + consumed_lines, Node::Directive(directive)))
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

/// Parses a plain text string into a list of [`InlineNode`]s.
fn parse_inline_text(paragraph_text: &str) -> Vec<crate::ast::InlineNode> {
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

fn parse_paragraph(lines: &[&str], i: usize) -> (usize, Vec<Node>) {
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

    let inlines = parse_inline_text(&paragraph_text);

    // Detect trailing "::" to introduce a literal block
    let trailing_double_colon = paragraph_text.trim_end().ends_with("::");
    if trailing_double_colon {
        let trimmed = paragraph_text.trim_end();
        // Determine whether the whole paragraph is just "::" (standalone introducer)
        let only_colon = trimmed.trim() == "::";
        let lit_start = current_pos_line;
        let (lit_consumed, content) = collect_literal_block_body(lines, lit_start);
        let total_consumed = current_pos_line - i + lit_consumed;
        let literal_node = Node::LiteralBlock {
            language: None,
            content,
        };
        if only_colon {
            // The "::" line itself is suppressed; emit only the literal block
            return (total_consumed, vec![literal_node]);
        }
        // Strip trailing "::" → ":" and emit paragraph + literal block
        let stripped = trimmed[..trimmed.len() - 1].trim_end().to_string();
        let inlines = parse_inline_text(&stripped);
        return (total_consumed, vec![Node::Paragraph(inlines), literal_node]);
    }

    (current_pos_line - i, vec![Node::Paragraph(inlines)])
}

fn handle_inline_match(kind: &str, m_str: &str, node_opt: Option<InlineNode>) -> InlineNode {
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
            if let Some(angle_start) = content.rfind('<') {
                if let Some(angle_end) = content[angle_start..].find('>') {
                    let display = content[..angle_start].trim().to_string();
                    let term = content[angle_start + 1..angle_start + angle_end]
                        .trim()
                        .to_string();
                    return InlineNode::TermReference { display, term };
                }
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

fn try_match_inline(
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

fn parse_version_change(
    kind: crate::ast::VersionChangeKind,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
) -> Directive {
    let version = if argument.is_empty() {
        diagnostics.push(format!("'{}' requires a version argument.", kind.as_str()));
        "unknown".to_string()
    } else {
        argument
    };

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

        let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
        let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics);

        Directive::VersionChange {
            kind,
            version,
            body: body_nodes,
        }
    } else {
        Directive::VersionChange {
            kind,
            version,
            body: vec![],
        }
    }
}

fn parse_seealso(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
) -> Directive {
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

        let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
        let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics);

        Directive::SeeAlso { body: body_nodes }
    } else {
        Directive::SeeAlso { body: vec![] }
    }
}

/// Parses the body of a `.. glossary::` directive into a [`Directive::Glossary`].
///
/// The body follows definition-list markup:
/// - Non-blank lines at the base indentation level are **terms**.
/// - Consecutive terms (all at base indentation) before an indented block share one definition.
/// - Indented lines form the **definition** body, parsed recursively.
/// - Blank lines separate entries.
/// - The `:sorted:` option causes entries to be sorted alphabetically by first term.
fn parse_glossary(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
) -> Directive {
    // Determine the base indentation of the body (first non-blank line).
    let Some(first_non_blank) = body_lines.iter().find(|l| !l.trim().is_empty()) else {
        return Directive::Glossary {
            entries: vec![],
            sorted: false,
        };
    };
    let base_indent = first_non_blank
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();

    // Strip the base indentation from all lines.
    let unindented: Vec<String> = body_lines
        .iter()
        .map(|l| {
            if l.len() >= base_indent {
                l[base_indent..].to_string()
            } else {
                l.trim().to_string()
            }
        })
        .collect();

    // Parse the :sorted: option from the leading option lines.
    let mut sorted = false;
    let mut body_start = 0;
    for (idx, line) in unindented.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body_start = idx + 1;
            continue;
        }
        if trimmed == ":sorted:" {
            sorted = true;
            body_start = idx + 1;
            continue;
        }
        // First non-option, non-blank line: definition body starts here.
        body_start = idx;
        break;
    }

    // Parse definition-list entries from the remaining lines.
    // A line with NO leading whitespace (after base-indent stripping) is a term.
    // A line WITH leading whitespace is part of the definition.
    let mut entries: Vec<crate::ast::GlossaryEntry> = Vec::new();
    let mut current_terms: Vec<String> = Vec::new();
    let mut definition_lines: Vec<String> = Vec::new();
    let mut in_definition = false;

    let body_slice = &unindented[body_start..];

    for line in body_slice {
        let is_blank = line.trim().is_empty();
        let is_indented = line.starts_with(' ') || line.starts_with('\t');

        if is_blank {
            if in_definition {
                // Blank line may end the current definition or be part of it.
                // We flush on the next term; accumulate for now.
                definition_lines.push(String::new());
            }
            // Between entries: do nothing
            continue;
        }

        if is_indented {
            // Part of the current definition body.
            in_definition = true;
            definition_lines.push(line.clone());
        } else {
            // Non-indented: this is a term.
            if in_definition {
                // Flush the completed entry.
                let def_strs: Vec<&str> = definition_lines.iter().map(String::as_str).collect();
                let mut dummy_adorn = adornment_order.clone();
                let def_nodes = parse_blocks(&def_strs, &mut dummy_adorn, diagnostics);
                entries.push(crate::ast::GlossaryEntry {
                    terms: current_terms.drain(..).collect(),
                    definition: def_nodes,
                });
                definition_lines.clear();
                in_definition = false;
            }
            current_terms.push(line.trim().to_string());
        }
    }

    // Flush any remaining entry.
    if !current_terms.is_empty() {
        let def_strs: Vec<&str> = definition_lines.iter().map(String::as_str).collect();
        let def_nodes = parse_blocks(&def_strs, adornment_order, diagnostics);
        entries.push(crate::ast::GlossaryEntry {
            terms: current_terms,
            definition: def_nodes,
        });
    }

    // Sort alphabetically by the first term (case-insensitive) if :sorted: was set.
    if sorted {
        entries.sort_by(|a, b| {
            let a_key = a
                .terms
                .first()
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            let b_key = b
                .terms
                .first()
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            a_key.cmp(&b_key)
        });
    }

    Directive::Glossary { entries, sorted }
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
            Node::LiteralBlock {
                language: Some("rust".to_string()),
                content: "let x = 1;\n\nlet y = 2;".to_string(),
            }
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
    fn test_parse_paragraph_with_inline_literal() {
        // Given
        let input = "some ``venv`` text";

        // When
        let doc = parse("test.rst", input);

        // Then
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
        // Given
        let input = "``some\\path``";

        // When
        let doc = parse("test.rst", input);

        // Then
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
        // Given
        let input = "``**bold**``";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 1);
            assert_eq!(inlines[0], InlineNode::Literal("**bold**".to_string()));
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

        // Test "program" variant
        let result = handle_inline_match("program", ":program:`curl`", None);
        assert_eq!(result, InlineNode::Program("curl".to_string()));

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
        let res = try_match_inline(input, 0, 1, false);

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

    #[test]
    fn test_parse_paragraph_with_program_role() {
        // Given
        let input = "Run :program:`curl` to download files.";

        // When
        let doc = parse("test.rst", input);

        // Then
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
        // Given
        let input = "Use :program:`git` or :program:`hg` to manage code.";

        // When
        let doc = parse("test.rst", input);

        // Then
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
    fn test_parse_versionchanged_creates_directive() {
        // Given
        let input = ".. versionchanged:: 2.3\n\n   Added async support.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, crate::ast::VersionChangeKind::Changed);
            assert_eq!(version, "2.3");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }

    #[test]
    fn test_parse_versionadded_creates_directive() {
        // Given
        let input = ".. versionadded:: 1.0\n\n   Initial release.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, crate::ast::VersionChangeKind::Added);
            assert_eq!(version, "1.0");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }

    #[test]
    fn test_parse_deprecated_creates_directive() {
        // Given
        let input = ".. deprecated:: 3.0\n\n   Use new API.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, crate::ast::VersionChangeKind::Deprecated);
            assert_eq!(version, "3.0");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }

    #[test]
    fn test_parse_version_change_emits_diagnostic_for_missing_version() {
        // Given
        let input = ".. versionchanged::\n\n   Missing version.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.diagnostics.len(), 1);
        assert_eq!(
            doc.diagnostics[0],
            "'versionchanged' requires a version argument."
        );
        if let Node::Directive(Directive::VersionChange {
            kind,
            version,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(*kind, crate::ast::VersionChangeKind::Changed);
            assert_eq!(version, "unknown");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected VersionChange directive");
        }
    }
    #[test]
    fn test_join_body_lines_with_empty_input() {
        // Given
        let input: &[&str] = &[];
        // When
        let result = super::join_body_lines(input);
        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_join_body_lines_with_single_line() {
        // Given
        let input = vec!["   hello"];
        // When
        let result = super::join_body_lines(&input);
        // Then
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_join_body_lines_with_multiple_lines() {
        // Given
        let input = vec!["   line1", "  line2", "line3"];
        // When
        let result = super::join_body_lines(&input);
        // Then
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_collect_directive_body_collects_indented_lines() {
        // Given
        let lines = vec![".. note::", "   body1", "   body2"];
        // When
        let (consumed, body) = super::collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body1", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_stops_at_unindented_line() {
        // Given
        let lines = vec![".. note::", "   body1", "unindented", "   body2"];
        // When
        let (consumed, body) = super::collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 1);
        assert_eq!(body, vec!["   body1"]);
    }

    #[test]
    fn test_collect_directive_body_strips_leading_and_trailing_blank_lines() {
        // Given
        let lines = vec![".. note::", "  ", "   body1", "  ", "   body2", "   ", ""];
        // When
        let (consumed, body) = super::collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 6);
        assert_eq!(body, vec!["   body1", "", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_returns_empty_when_no_body() {
        // Given
        let lines = vec![".. note::", "unindented"];
        // When
        let (consumed, body) = super::collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 0);
        assert!(body.is_empty());
    }

    #[test]
    fn test_collect_directive_body_returns_correct_consumed_count() {
        // Given
        let lines = vec![".. note::", "   body", "  "];
        // When
        let (consumed, body) = super::collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body"]);
    }

    #[test]
    fn test_parse_toctree_collects_paths() {
        // Given
        let body_lines = vec!["path1", "path2/index"];
        let mut diagnostics = vec![];
        // When
        let directive = super::parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1", "path2/index"]);
            assert_eq!(maxdepth, None);
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_parses_maxdepth_option() {
        // Given
        let body_lines = vec![":maxdepth: 2", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = super::parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1"]);
            assert_eq!(maxdepth, Some(2));
            assert!(ignored_options.is_empty());
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_ignores_known_options() {
        // Given
        let body_lines = vec![":hidden:", ":caption: Some text", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = super::parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree {
            paths,
            maxdepth,
            ignored_options,
        } = directive
        {
            assert_eq!(paths, vec!["path1"]);
            assert_eq!(maxdepth, None);
            assert_eq!(ignored_options.len(), 2);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_emits_diagnostic_for_unknown_option() {
        // Given
        let body_lines = vec![":unknown_opt:", "path1"];
        let mut diagnostics = vec![];
        // When
        let directive = super::parse_toctree(&body_lines, &mut diagnostics);
        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
        if let Directive::Toctree { paths, .. } = directive {
            assert_eq!(paths, vec!["path1"]);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_toctree_skips_blank_lines() {
        // Given
        let body_lines = vec!["path1", "  ", "", "path2"];
        let mut diagnostics = vec![];
        // When
        let directive = super::parse_toctree(&body_lines, &mut diagnostics);
        // Then
        if let Directive::Toctree { paths, .. } = directive {
            assert_eq!(paths, vec!["path1", "path2"]);
        } else {
            panic!("Expected Toctree");
        }
    }

    #[test]
    fn test_parse_creates_seealso_with_paragraph_body() {
        // Given
        let input = ".. seealso::\n\n   Related information here.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::SeeAlso { body }) = &doc.nodes[0] {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected SeeAlso directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_seealso_with_empty_body() {
        // Given
        let input = ".. seealso::";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::SeeAlso { body }) = &doc.nodes[0] {
            assert!(body.is_empty());
        } else {
            panic!("Expected SeeAlso directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_seealso_with_bullet_list_body() {
        // Given
        let input = ".. seealso::\n\n   * Item A\n   * Item B";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::SeeAlso { body }) = &doc.nodes[0] {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::BulletList { .. }));
        } else {
            panic!("Expected SeeAlso directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_seealso_basic() {
        // Given
        let body_lines = vec!["   See the other page."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_seealso(&body_lines, &mut adornment_order, &mut diagnostics);

        // Then
        if let Directive::SeeAlso { body } = directive {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected SeeAlso directive");
        }
        assert!(diagnostics.is_empty());
    }

    // --- Literal block tests ---

    #[test]
    fn test_parse_double_colon_paragraph_emits_literal_block() {
        // Given: a paragraph ending with :: followed by an indented block
        let input = "Here is some code::

    def hello():
        pass
";

        // When
        let doc = parse("test.rst", input);

        // Then: two nodes — paragraph (with :: reduced to :) and LiteralBlock
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let text = match &inlines[0] {
                crate::ast::InlineNode::Text(t) => t.as_str(),
                other => panic!("Expected Text inline, got {other:?}"),
            };
            assert_eq!(text, "Here is some code:");
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
        if let Node::LiteralBlock { language, content } = &doc.nodes[1] {
            assert!(language.is_none());
            assert_eq!(
                content,
                "def hello():
    pass"
            );
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    #[test]
    fn test_parse_standalone_double_colon_suppresses_paragraph() {
        // Given: a line of only "::" introduces a literal block with no visible paragraph
        let input = "::

    verbatim content
";

        // When
        let doc = parse("test.rst", input);

        // Then: only the LiteralBlock is emitted (no paragraph)
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert!(language.is_none());
            assert_eq!(content, "verbatim content");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_double_colon_strips_to_single_colon() {
        // Given: text followed by "::" — the "::" becomes ":"
        let input = "Example::

    content
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let text = match &inlines[0] {
                crate::ast::InlineNode::Text(t) => t.as_str(),
                other => panic!("Expected Text, got {other:?}"),
            };
            assert_eq!(text, "Example:");
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_code_block_directive_with_language() {
        // Given
        let input = ".. code-block:: python

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert_eq!(language.as_deref(), Some("python"));
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_code_block_directive_without_language() {
        // Given
        let input = ".. code-block::

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert!(language.is_none());
            assert_eq!(content, "x = 1");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_literal_block_preserves_internal_blank_lines() {
        // Given: blank lines inside the block must be kept
        let input = "Example::

    line one

    line three
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
            assert_eq!(
                content,
                "line one

line three"
            );
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    #[test]
    fn test_parse_literal_block_strips_common_indentation() {
        // Given: all lines indented 4 spaces; inner block adds 4 more
        let input = "Example::

    outer
        inner
    outer again
";

        // When
        let doc = parse("test.rst", input);

        // Then: 4 spaces stripped from all lines; inner keeps its extra 4
        assert_eq!(doc.nodes.len(), 2);
        if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
            assert_eq!(
                content,
                "outer
    inner
outer again"
            );
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    // ── Glossary directive tests ──────────────────────────────────────────────

    #[test]
    fn test_parse_glossary_single_entry() {
        // Given
        let input =
            ".. glossary::\n\n   environment\n      A structure where information is saved.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Glossary { entries, sorted }) = &doc.nodes[0] {
            assert!(!sorted);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].terms, vec!["environment"]);
            assert!(!entries[0].definition.is_empty());
        } else {
            panic!("Expected Glossary directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_glossary_multiple_entries() {
        // Given
        let input = ".. glossary::\n\n   builder\n      Produces output.\n\n   environment\n      Stores information.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::Glossary { entries, .. }) = &doc.nodes[0] {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].terms, vec!["builder"]);
            assert_eq!(entries[1].terms, vec!["environment"]);
        } else {
            panic!("Expected Glossary, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_glossary_multi_term_entry() {
        // Given
        let input = ".. glossary::\n\n   term 1\n   term 2\n      Shared definition.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::Glossary { entries, .. }) = &doc.nodes[0] {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].terms, vec!["term 1", "term 2"]);
        } else {
            panic!("Expected Glossary, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_glossary_sorted_option_orders_entries_alphabetically() {
        // Given
        let input = ".. glossary::\n   :sorted:\n\n   zebra\n      Z entry.\n\n   apple\n      A entry.\n\n   mango\n      M entry.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::Glossary { entries, sorted }) = &doc.nodes[0] {
            assert!(sorted);
            assert_eq!(entries.len(), 3);
            assert_eq!(entries[0].terms[0], "apple");
            assert_eq!(entries[1].terms[0], "mango");
            assert_eq!(entries[2].terms[0], "zebra");
        } else {
            panic!("Expected Glossary, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_glossary_sorted_is_case_insensitive() {
        // Given — "Banana" should sort between "apple" and "cherry"
        let input = ".. glossary::\n   :sorted:\n\n   cherry\n      C.\n\n   Banana\n      B.\n\n   apple\n      A.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::Glossary { entries, .. }) = &doc.nodes[0] {
            assert_eq!(entries[0].terms[0], "apple");
            assert_eq!(entries[1].terms[0], "Banana");
            assert_eq!(entries[2].terms[0], "cherry");
        } else {
            panic!("Expected Glossary, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_glossary_empty_body_returns_empty_entries() {
        // Given
        let input = ".. glossary::\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::Glossary { entries, sorted }) = &doc.nodes[0] {
            assert!(!sorted);
            assert!(entries.is_empty());
        } else {
            panic!("Expected Glossary, got {:?}", doc.nodes[0]);
        }
    }

    // ── :term: role tests ─────────────────────────────────────────────────────

    #[test]
    fn test_parse_term_role_basic() {
        // Given
        let input = "See :term:`environment` for details.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
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
        // Given
        let input = "See :term:`the env <environment>` here.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
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
        // Given
        let input = "Before :term:`foo` after.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.len() >= 3, "Expected text + term + text");
            assert!(matches!(&inlines[0], InlineNode::Text(t) if t == "Before "));
            assert!(matches!(&inlines[1], InlineNode::TermReference { term, .. } if term == "foo"));
        } else {
            panic!("Expected Paragraph");
        }
    }
}
