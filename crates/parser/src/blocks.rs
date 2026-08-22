use super::bullet_list::try_parse_bullet_list;
use super::definition_list::try_parse_definition_list;
use super::directives::try_parse_directive;
use super::headings::{Adornment, detect_adornment, try_parse_heading};
use super::inline::parse_inline_text;
use super::table::try_parse_grid_table;
use rusty_sphinx_ast::{Document, Domain, Node, TargetName};

/// Tries to parse an RST comment starting at line `i`.
///
/// A comment is any RST explicit markup block that is not a directive, target, or
/// substitution definition. The **explicit markup start** is strictly either:
///
/// - Exactly `..` (bare, nothing after), or
/// - `.. ` (two dots followed by at least one space, then optional inline text)
///
/// This means `...` or `....` or `... text` are **not** explicit markup starts and
/// must not be matched — they are ordinary text (e.g. heading text or paragraph text).
///
/// Three comment forms are recognised:
///
/// - `.. inline text` — optional indented continuation body may follow
/// - `..` (bare) followed by an indented body
/// - `..` (bare) with no body at all
///
/// The comment body is consumed but discarded; only [`Node::Comment`] is returned.
pub(super) fn try_parse_comment(lines: &[&str], i: usize) -> Option<(usize, Node)> {
    let line = lines[i].trim_end();
    let trimmed = line.trim();

    // RST explicit markup start: exactly ".." or ".. " (dot dot space).
    // Three or more dots (e.g. "...") are NOT an explicit markup start.
    if trimmed != ".." && !trimmed.starts_with(".. ") {
        return None;
    }
    // Directives require "::" somewhere on the intro line.
    if line.contains("::") {
        return None;
    }
    // Targets start with ".. _".
    if trimmed.starts_with(".. _") {
        return None;
    }

    // Consume the intro line and any indented body that follows.
    let (body_consumed, _body) = collect_directive_body(lines, i + 1, indent_width(line));
    let consumed = 1 + body_consumed;

    Some((consumed, Node::Comment))
}

/// Tries to parse an RST transition (horizontal rule) starting at line `i`.
///
/// A transition is a single line of 4 or more repeated identical ASCII punctuation
/// characters (e.g. `----` or `====`), with a blank line (or document boundary)
/// immediately before and after it.
///
/// The RST spec also says a transition should not begin or end the document, nor
/// immediately follow another transition; violations are reported as diagnostics
/// rather than rejected, since rusty-sphinx's parser is error-resilient.
pub(super) fn try_parse_transition(
    lines: &[&str],
    i: usize,
    nodes: &[Node],
    diagnostics: &mut Vec<String>,
) -> Option<(usize, Node)> {
    let line = lines[i].trim();
    let mut chars = line.chars();
    let first = chars.next()?;
    if line.len() < 4 || !first.is_ascii_punctuation() || !chars.all(|c| c == first) {
        return None;
    }

    let preceded_by_blank = i == 0 || lines[i - 1].trim().is_empty();
    let followed_by_blank = i + 1 >= lines.len() || lines[i + 1].trim().is_empty();
    if !preceded_by_blank || !followed_by_blank {
        return None;
    }

    if nodes.is_empty() {
        diagnostics.push("transition (horizontal rule) may not begin the document".to_string());
    } else if matches!(nodes.last(), Some(Node::Transition)) {
        diagnostics.push(
            "transition (horizontal rule) may not immediately follow another transition"
                .to_string(),
        );
    }
    if lines[i + 1..].iter().all(|l| l.trim().is_empty()) {
        diagnostics.push("transition (horizontal rule) may not end the document".to_string());
    }

    Some((1, Node::Transition))
}

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
    parse_with_domain(path, input, Domain::Py)
}

/// Parses an RST-formatted string into a `Document`, resolving any bare
/// (unprefixed) domain directive or role — e.g. `.. function::` or `:func:` —
/// via `default_domain`. `parse` is a thin wrapper defaulting to `Domain::Py`.
///
/// # Panics
///
/// The internal implementation uses `expect()` on an iterator that is guaranteed
/// to be non-empty by preceding checks.
#[must_use]
pub fn parse_with_domain(path: &str, input: &str, default_domain: Domain) -> Document {
    let lines: Vec<&str> = input.lines().collect();
    let mut adornment_order: Vec<Adornment> = Vec::new();
    let mut diagnostics = Vec::new();

    let mut nodes = parse_blocks(
        &lines,
        &mut adornment_order,
        &mut diagnostics,
        default_domain,
    );

    let mut index_id_counter = 0;
    super::index_ids::assign_index_ids(&mut nodes, &mut index_id_counter);

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

pub(super) fn try_parse_target(lines: &[&str], i: usize) -> Option<(usize, Node)> {
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

pub(super) fn parse_blocks(
    lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_directive(lines, i, adornment_order, diagnostics, default_domain)
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

        if let Some((consumed, node)) = try_parse_comment(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_transition(lines, i, &nodes, diagnostics) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_heading(lines, i, adornment_order, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_grid_table(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_bullet_list(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_definition_list(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        let (consumed, new_nodes) = parse_paragraph(lines, i, default_domain);
        nodes.extend(new_nodes);
        i += consumed;
    }
    nodes
}

/// Counts the leading whitespace characters on a line, char-based (not
/// byte-based) so it doesn't panic on multi-byte characters near the
/// indentation boundary.
pub(super) fn indent_width(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// Collects the indented body belonging to a directive/comment whose own
/// intro line has `min_indent` leading whitespace characters.
///
/// Only lines indented *more* than `min_indent` (plus blank lines) are part
/// of the body; a line indented at or below `min_indent` ends it. This
/// distinguishes true nested body content from a sibling block at the same
/// indentation — e.g. a bodyless `.. index:: single: x` immediately followed
/// by a paragraph at the same indent level (common inside glossary entries,
/// list items, and other indented containers) must not swallow that
/// paragraph as if it were the directive's own body.
pub(super) fn collect_directive_body<'a>(
    lines: &[&'a str],
    start_index: usize,
    min_indent: usize,
) -> (usize, Vec<&'a str>) {
    let mut body_lines = Vec::new();
    let mut current = start_index;
    while current < lines.len() {
        let next_line = lines[current].trim_end();
        if next_line.trim().is_empty() || indent_width(next_line) > min_indent {
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

/// Collects a domain object directive's *argument continuation lines*: the
/// further signatures a single definition directive may declare, each on its
/// own line immediately below the directive marker, e.g.
///
/// ```rst
/// .. data:: AF_UNIX
///           AF_INET
///           AF_INET6
/// ```
///
/// Unlike a directive body, continuation lines need no blank line to separate
/// them from the marker — so scanning stops at the first blank line, at any
/// line indented no more than `min_indent` (the marker's own indentation), or
/// at an option field such as `:type: int`, whichever comes first. Everything
/// after that belongs to [`collect_directive_body`] instead, which cannot
/// make this distinction itself: to it a continuation line and a docstring
/// line are both just "indented more than the marker".
///
/// Returns the number of lines consumed and each one's trimmed text.
pub(super) fn collect_argument_continuation_lines(
    lines: &[&str],
    start_index: usize,
    min_indent: usize,
) -> (usize, Vec<String>) {
    let mut continuations = Vec::new();
    let mut current = start_index;
    while current < lines.len() {
        let line = lines[current].trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || indent_width(line) <= min_indent || trimmed.starts_with(':') {
            break;
        }
        continuations.push(trimmed.to_string());
        current += 1;
    }

    (current - start_index, continuations)
}

pub(super) fn join_body_lines(body_lines: &[&str]) -> String {
    let mut body = String::new();
    for l in body_lines {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(l.trim_start());
    }
    body
}

/// Strips the *minimum* common leading indentation from `body_lines` and joins
/// them with newlines, preserving relative indentation within the block (RST
/// spec behaviour) and normalising blank lines to empty strings.
///
/// This is what verbatim block content needs, and the opposite of what
/// [`join_body_lines`] does — that one `trim_start`s every line, which is fine
/// for prose but destroys the meaning of indentation-sensitive content such as
/// Python source.
pub(super) fn strip_common_indent(body_lines: &[&str]) -> String {
    let min_indent = body_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    body_lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                l.chars().skip(min_indent).collect()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collects a literal block body starting at `start_index`.
///
/// - Skips a leading blank line (mandatory after `::`).
/// - Collects contiguous lines until indentation drops to (or below) the base level.
/// - Strips the *minimum* common indentation from all non-blank lines, preserving relative
///   indentation within the block (RST spec behaviour).
///
/// Returns `(lines_consumed, verbatim_content)`.
pub(super) fn collect_literal_block_body(lines: &[&str], start_index: usize) -> (usize, String) {
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

    let content = strip_common_indent(&body_lines);

    (current - start_index, content)
}

fn parse_paragraph(lines: &[&str], i: usize, default_domain: Domain) -> (usize, Vec<Node>) {
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

    let inlines = parse_inline_text(&paragraph_text, default_domain);

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
        let inlines = parse_inline_text(&stripped, default_domain);
        return (total_consumed, vec![Node::Paragraph(inlines), literal_node]);
    }

    (current_pos_line - i, vec![Node::Paragraph(inlines)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::InlineNode;

    #[test]
    fn test_strip_common_indent_removes_the_shared_leading_whitespace() {
        // Given
        let lines = vec!["    first", "    second"];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "first\nsecond");
    }

    #[test]
    fn test_strip_common_indent_preserves_relative_indentation() {
        // Given — the property that makes this usable for Python source.
        let lines = vec!["    def f():", "        return 1"];

        // When
        let content = strip_common_indent(&lines);

        // Then — only the shared four spaces go; the inner four remain.
        assert_eq!(content, "def f():\n    return 1");
    }

    #[test]
    fn test_strip_common_indent_normalizes_blank_lines_to_empty_strings() {
        // Given — a blank line that is shorter than the common indent.
        let lines = vec!["    first", "", "    second"];

        // When
        let content = strip_common_indent(&lines);

        // Then — the blank line must not influence the minimum, and must not
        // become a run of stray spaces.
        assert_eq!(content, "first\n\nsecond");
    }

    #[test]
    fn test_strip_common_indent_uses_the_least_indented_line_as_the_baseline() {
        // Given — the first line is deeper than a later one.
        let lines = vec!["        deep", "    shallow"];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "    deep\nshallow");
    }

    #[test]
    fn test_strip_common_indent_leaves_unindented_lines_untouched() {
        // Given
        let lines = vec!["no indent", "still none"];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "no indent\nstill none");
    }

    #[test]
    fn test_strip_common_indent_returns_empty_string_for_no_lines() {
        // Given
        let lines: Vec<&str> = Vec::new();

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "");
    }

    #[test]
    fn test_strip_common_indent_handles_only_blank_lines() {
        // Given — no non-blank line to derive a minimum indent from.
        let lines = vec!["", "   "];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "\n");
    }

    #[test]
    fn test_collect_argument_continuation_lines_collects_every_further_signature() {
        // Given — the shape `library/socket.rst` declares its address
        // families in: three aliases for one object, no blank line between.
        let lines = vec![
            ".. data:: AF_UNIX",
            "          AF_INET",
            "          AF_INET6",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 2);
        assert_eq!(continuations, ["AF_INET", "AF_INET6"]);
    }

    #[test]
    fn test_collect_argument_continuation_lines_stops_at_a_blank_line() {
        // Given — everything past the blank line is docstring body.
        let lines = vec![
            ".. data:: AF_UNIX",
            "          AF_INET",
            "",
            "   The address families.",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 1);
        assert_eq!(continuations, ["AF_INET"]);
    }

    #[test]
    fn test_collect_argument_continuation_lines_stops_at_an_option_field() {
        // Given — `:type:` opens the directive's option block, which ends the
        // argument text even though it is indented like a continuation.
        let lines = vec![
            ".. data:: DEFAULT_TIMEOUT",
            "   :type: int",
            "   :value: 30",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 0);
        assert!(continuations.is_empty());
    }

    #[test]
    fn test_collect_argument_continuation_lines_stops_at_a_dedent() {
        // Given — a line indented no further than the marker is a sibling
        // block, not part of this directive at all.
        let lines = vec![".. data:: AF_UNIX", "Next paragraph."];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 0);
        assert!(continuations.is_empty());
    }

    #[test]
    fn test_collect_argument_continuation_lines_respects_a_nested_markers_indentation() {
        // Given — a directive nested inside another block: its continuation
        // lines are indented past *its* marker, not past column zero.
        let lines = vec![
            "   .. data:: AF_UNIX",
            "             AF_INET",
            "   Sibling text.",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 3);

        // Then
        assert_eq!(consumed, 1);
        assert_eq!(continuations, ["AF_INET"]);
    }

    #[test]
    fn test_collect_argument_continuation_lines_returns_nothing_at_end_of_input() {
        // Given — a bodyless directive on the document's last line.
        let lines = vec![".. data:: AF_UNIX"];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 0);
        assert!(continuations.is_empty());
    }

    #[test]
    fn test_parse_blocks_empty_input() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let nodes = parse_blocks(&[], &mut adornment_order, &mut diagnostics, Domain::Py);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_blocks_simple_paragraph() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let lines = vec!["Hello world"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics, Domain::Py);
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
        let nodes1 = parse_blocks(&lines1, &mut adornment_order, &mut diagnostics, Domain::Py);
        assert_eq!(nodes1.len(), 1);

        let lines2 = vec!["Title 2", "-------"];
        let nodes2 = parse_blocks(&lines2, &mut adornment_order, &mut diagnostics, Domain::Py);
        assert_eq!(nodes2.len(), 1);

        assert_eq!(adornment_order.len(), 2);
    }

    #[test]
    fn test_parse_blocks_collects_diagnostics() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // This will trigger a diagnostic because of the unknown option
        let lines = vec![".. toctree::", "   :unknown_option: value"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics, Domain::Py);

        assert_eq!(nodes.len(), 1);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
    }

    // --- try_parse_comment unit tests ---

    #[test]
    fn test_try_parse_comment_returns_none_for_plain_text() {
        // Given
        let lines = vec!["Hello world"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_single_line() {
        // Given
        let lines = vec![".. a comment"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((1, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_bare_dots() {
        // Given
        let lines = vec![".."];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((1, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_with_inline_text_and_body() {
        // Given
        let lines = vec![".. comment text", "   continuation line"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((2, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_bare_dots_with_body() {
        // Given
        let lines = vec!["..", "   indented body"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((2, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_returns_none_for_three_dots() {
        // Given — "..." is not an RST explicit markup start
        let lines = vec!["..."];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_returns_none_for_ellipsis_text() {
        // Given — "... some text" starts with two dots but the third char is not a space
        let lines = vec!["... some text"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_returns_none_for_ellipsis_heading_text() {
        // Given — the exact reported regression: heading text beginning with "..."
        let lines = vec!["... install scientific Python packages?"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_does_not_match_directive() {
        // Given — directive lines contain "::" and are handled by try_parse_directive
        let lines = vec![".. note::"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_does_not_match_named_target() {
        // Given — named targets start with ".. _"
        let lines = vec![".. _label:"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_does_not_match_anonymous_target() {
        // Given — anonymous targets start with ".. __:"
        let lines = vec![".. __: https://example.com"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_multiline_body_consumes_all_indented_lines() {
        // Given
        let lines = vec!["..", "   line one", "   line two", "Not part of comment"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((3, Node::Comment)));
    }

    // --- try_parse_transition unit tests ---

    #[test]
    fn test_try_parse_transition_matches_hyphens() {
        // Given
        let lines = vec!["", "----", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_matches_other_punctuation_characters() {
        // Given
        let lines = vec!["", "====", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_returns_none_for_fewer_than_four_characters() {
        // Given
        let lines = vec!["", "---", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_returns_none_for_mixed_characters() {
        // Given
        let lines = vec!["", "--==", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_returns_none_without_preceding_blank_line() {
        // Given
        let lines = vec!["Some text", "----", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_returns_none_without_following_blank_line() {
        // Given
        let lines = vec!["", "----", "Some text"];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_matches_at_start_of_document() {
        // Given
        let lines = vec!["----", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 0, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_matches_at_end_of_document() {
        // Given
        let lines = vec!["", "----"];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_emits_diagnostic_when_beginning_document() {
        // Given — no nodes parsed yet
        let lines = vec!["----", "", "More text"];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 0, &nodes, &mut diagnostics);
        // Then
        assert!(diagnostics.iter().any(|d| d.contains("begin the document")));
    }

    #[test]
    fn test_try_parse_transition_emits_diagnostic_when_ending_document() {
        // Given — only blank lines remain afterwards
        let lines = vec!["Some text", "", "----", "", "  "];
        let nodes = vec![Node::Paragraph(vec![])];
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 2, &nodes, &mut diagnostics);
        // Then
        assert!(diagnostics.iter().any(|d| d.contains("end the document")));
    }

    #[test]
    fn test_try_parse_transition_emits_diagnostic_when_immediately_adjacent() {
        // Given — the previously parsed node is also a transition
        let lines = vec!["", "----", "", "Some text"];
        let nodes = vec![Node::Transition];
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(
            diagnostics
                .iter()
                .any(|d| d.contains("immediately follow another transition"))
        );
    }

    #[test]
    fn test_try_parse_transition_no_diagnostic_for_well_formed_transition() {
        // Given — a transition with content both before and after
        let lines = vec!["Some text", "", "----", "", "More text"];
        let nodes = vec![Node::Paragraph(vec![])];
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 2, &nodes, &mut diagnostics);
        // Then
        assert!(diagnostics.is_empty());
    }

    // --- join_body_lines unit tests ---

    #[test]
    fn test_join_body_lines_with_empty_input() {
        // Given
        let input: &[&str] = &[];
        // When
        let result = join_body_lines(input);
        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_join_body_lines_with_single_line() {
        // Given
        let input = vec!["   hello"];
        // When
        let result = join_body_lines(&input);
        // Then
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_join_body_lines_with_multiple_lines() {
        // Given
        let input = vec!["   line1", "  line2", "line3"];
        // When
        let result = join_body_lines(&input);
        // Then
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_collect_directive_body_collects_indented_lines() {
        // Given
        let lines = vec![".. note::", "   body1", "   body2"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body1", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_stops_at_unindented_line() {
        // Given
        let lines = vec![".. note::", "   body1", "unindented", "   body2"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 1);
        assert_eq!(body, vec!["   body1"]);
    }

    #[test]
    fn test_collect_directive_body_strips_leading_and_trailing_blank_lines() {
        // Given
        let lines = vec![".. note::", "  ", "   body1", "  ", "   body2", "   ", ""];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 6);
        assert_eq!(body, vec!["   body1", "", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_returns_empty_when_no_body() {
        // Given
        let lines = vec![".. note::", "unindented"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 0);
        assert!(body.is_empty());
    }

    #[test]
    fn test_collect_directive_body_returns_correct_consumed_count() {
        // Given
        let lines = vec![".. note::", "   body", "  "];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body"]);
    }

    #[test]
    fn test_collect_directive_body_stops_at_sibling_indented_at_same_level_as_directive() {
        // Given — a bodyless directive nested at 3-space indent (e.g. inside
        // a glossary entry), immediately followed by a sibling paragraph at
        // the SAME 3-space indent, not a deeper one. Real Sphinx docs do
        // this constantly (CPython's glossary.rst: `.. index:: pair: magic;
        // method` followed by a plain paragraph at the same indent).
        let lines = vec![
            "   .. index:: pair: magic; method",
            "",
            "   An informal synonym for something.",
        ];
        // When — min_indent is the directive's own 3-space indentation
        let (consumed, body) = collect_directive_body(&lines, 1, 3);
        // Then — the body is empty and only the blank line is consumed;
        // critically, the sibling paragraph itself is NOT swallowed, so the
        // caller will parse it as its own paragraph node afterwards.
        assert_eq!(consumed, 1);
        assert!(body.is_empty());
    }

    #[test]
    fn test_collect_directive_body_includes_lines_indented_deeper_than_directive() {
        // Given — true nested body content, indented deeper than the
        // directive's own 3-space indent
        let lines = vec!["   .. note::", "", "      Actual body.", "   Sibling."];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 3);
        // Then — only the deeper-indented line is included
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["      Actual body."]);
    }

    #[test]
    fn test_indent_width_counts_leading_whitespace() {
        // Given / When / Then
        assert_eq!(indent_width("   text"), 3);
        assert_eq!(indent_width("text"), 0);
        assert_eq!(indent_width("  "), 2);
    }

    #[test]
    fn test_indent_width_does_not_panic_on_multibyte_char_after_indent() {
        // Given — a multi-byte character immediately after the indentation,
        // exercising the char-based (not byte-based) counting
        let line = "  éfoo";
        // When
        let width = indent_width(line);
        // Then
        assert_eq!(width, 2);
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::parse;
    use rusty_sphinx_ast::TargetName;
    use rusty_sphinx_ast::{InlineNode, Node};

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
    fn test_parse_creates_paragraph_node() {
        // Given
        let input = "Just some\ntext";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("Just some\ntext".to_string())])
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
                text: vec![InlineNode::Text("Title".to_string())]
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Text.".to_string())])
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
            Node::Paragraph(vec![InlineNode::Text("Long Heading\n===".to_string())])
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
                text: vec![InlineNode::Text("Heading".to_string())]
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Next".to_string())])
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
            Node::Paragraph(vec![InlineNode::Text("Para 1".to_string())])
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Para 2".to_string())])
        );
        assert_eq!(
            doc.nodes[2],
            Node::Paragraph(vec![InlineNode::Text("Para 3".to_string())])
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
                text: vec![InlineNode::Text("Heading".to_string())]
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Para\nline 2".to_string())])
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
    fn test_parse_paragraph_breaks_at_overline() {
        // Given
        let input = "Para text.\n#######\nHeading\n#######";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("Para text.".to_string())])
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Heading".to_string())]
            }
        );
    }

    #[test]
    fn test_parse_double_colon_paragraph_emits_literal_block() {
        // Given: a paragraph ending with :: followed by an indented block
        let input = "Here is some code::\n\n    def hello():\n        pass\n";
        // When
        let doc = parse("test.rst", input);
        // Then: two nodes — paragraph (with :: reduced to :) and LiteralBlock
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let text = match &inlines[0] {
                InlineNode::Text(t) => t.as_str(),
                other => panic!("Expected Text inline, got {other:?}"),
            };
            assert_eq!(text, "Here is some code:");
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
        if let Node::LiteralBlock { language, content } = &doc.nodes[1] {
            assert!(language.is_none());
            assert_eq!(content, "def hello():\n    pass");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    #[test]
    fn test_parse_standalone_double_colon_suppresses_paragraph() {
        // Given: a line of only "::" introduces a literal block with no visible paragraph
        let input = "::\n\n    verbatim content\n";
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
        let input = "Example::\n\n    content\n";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let text = match &inlines[0] {
                InlineNode::Text(t) => t.as_str(),
                other => panic!("Expected Text, got {other:?}"),
            };
            assert_eq!(text, "Example:");
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_literal_block_preserves_internal_blank_lines() {
        // Given: blank lines inside the block must be kept
        let input = "Example::\n\n    line one\n\n    line three\n";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
            assert_eq!(content, "line one\n\nline three");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    #[test]
    fn test_parse_literal_block_strips_common_indentation() {
        // Given: all lines indented 4 spaces; inner block adds 4 more
        let input = "Example::\n\n    outer\n        inner\n    outer again\n";
        // When
        let doc = parse("test.rst", input);
        // Then: 4 spaces stripped from all lines; inner keeps its extra 4
        assert_eq!(doc.nodes.len(), 2);
        if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
            assert_eq!(content, "outer\n    inner\nouter again");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    // --- Comment integration tests ---

    #[test]
    fn test_parse_comment_produces_single_comment_node() {
        // Given
        let input = ".. This is a comment";
        // When
        let doc = parse("test.rst", input);
        // Then — the document contains exactly one Comment node
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_bare_comment_marker_produces_comment_node() {
        // Given — bare `..` with no following text or body
        let input = "..";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_multiline_comment_produces_single_comment_node() {
        // Given — bare `..` followed by indented body
        let input = "..\n\n   This is a multi-line\n   comment body.";
        // When
        let doc = parse("test.rst", input);
        // Then — still just one Comment node; body is discarded
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_comment_before_paragraph_yields_only_paragraph() {
        // Given
        let input = ".. A comment\n\nA paragraph.";
        // When
        let doc = parse("test.rst", input);
        // Then — comment is discarded; only the paragraph survives
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0], Node::Comment);
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("A paragraph.".to_string())])
        );
    }

    #[test]
    fn test_parse_comment_between_heading_and_paragraph() {
        // Given
        let input = "Title\n=====\n\n.. A comment\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Title".to_string())]
            }
        );
        assert_eq!(doc.nodes[1], Node::Comment);
        assert_eq!(
            doc.nodes[2],
            Node::Paragraph(vec![InlineNode::Text("Some text.".to_string())])
        );
    }

    #[test]
    fn test_parse_comment_with_double_colon_in_body_is_not_a_directive() {
        // Given — the `::` is only in the indented body, not on the `..` intro line
        let input = ".. some text\n   contains:: stuff";
        // When
        let doc = parse("test.rst", input);
        // Then — still parsed as a comment (body discarded)
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_heading_starting_with_ellipsis_is_not_a_comment() {
        // Given — heading text that begins with "..." (three dots)
        let input =
            "... install scientific Python packages?\n---------------------------------------";
        // When
        let doc = parse("test.rst", input);
        // Then — must be a Heading, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text(
                    "\u{2026} install scientific Python packages?".to_string()
                )]
            }
        );
    }

    #[test]
    fn test_parse_paragraph_starting_with_ellipsis_is_not_a_comment() {
        // Given — a plain paragraph whose text begins with "..."
        let input = "...continued from above.";
        // When
        let doc = parse("test.rst", input);
        // Then — must be a Paragraph, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "\u{2026}continued from above.".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_directive_is_not_misidentified_as_comment() {
        // Given — a real directive must not be swallowed by try_parse_comment
        let input = ".. note::\n\n   Body text.";
        // When
        let doc = parse("test.rst", input);
        // Then — directive is parsed, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(_)),
            "Expected Directive, got {:?}",
            doc.nodes[0]
        );
    }

    #[test]
    fn test_parse_target_is_not_misidentified_as_comment() {
        // Given — a named target must not be swallowed by try_parse_comment
        let input = ".. _my-target:";
        // When
        let doc = parse("test.rst", input);
        // Then — target is parsed, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Target { .. }),
            "Expected Target, got {:?}",
            doc.nodes[0]
        );
    }

    // --- Transition integration tests ---

    #[test]
    fn test_parse_transition_between_paragraphs_produces_no_diagnostics() {
        // Given
        let input = "First paragraph.\n\n----\n\nSecond paragraph.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(
            doc.nodes,
            vec![
                Node::Paragraph(vec![InlineNode::Text("First paragraph.".to_string())]),
                Node::Transition,
                Node::Paragraph(vec![InlineNode::Text("Second paragraph.".to_string())]),
            ]
        );
        assert!(doc.diagnostics.is_empty());
    }

    #[test]
    fn test_parse_real_heading_is_unaffected_by_transition_dispatch() {
        // Given — a normal underline-only heading, adjacent to the transition dispatch
        let input = "Heading\n=======\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(
            doc.nodes,
            vec![
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Heading".to_string())]
                },
                Node::Paragraph(vec![InlineNode::Text("Some text.".to_string())]),
            ]
        );
    }

    #[test]
    fn test_parse_transition_at_document_start_emits_diagnostic() {
        // Given
        let input = "----\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes[0], Node::Transition);
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.contains("begin the document"))
        );
    }

    #[test]
    fn test_parse_bodyless_index_directive_inside_glossary_does_not_swallow_following_paragraph() {
        // Given — mirrors real CPython usage (Doc/glossary.rst): a bodyless
        // `.. index::` directive nested inside a glossary term's definition,
        // immediately followed by a plain paragraph at the SAME indentation
        // (not a deeper one). Before the indentation-depth fix, the
        // paragraph was swallowed into the directive's body and mis-parsed
        // as bogus index entries.
        let input = "\
.. glossary::

   magic method
      .. index:: pair: magic; method

      An informal synonym for something.
";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(rusty_sphinx_ast::Directive::Glossary { entries, .. }) = &doc.nodes[0]
        else {
            panic!("Expected Glossary directive, got {:?}", doc.nodes[0]);
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].terms, vec!["magic method".to_string()]);
        // The definition must contain the Index directive AND the paragraph
        // as two separate sibling nodes — not one node with the paragraph's
        // text corrupted into bogus index entries.
        assert_eq!(entries[0].definition.len(), 2);
        assert!(matches!(
            entries[0].definition[0],
            Node::Directive(rusty_sphinx_ast::Directive::Index { .. })
        ));
        assert_eq!(
            entries[0].definition[1],
            Node::Paragraph(vec![InlineNode::Text(
                "An informal synonym for something.".to_string()
            )])
        );
        // No diagnostics about invalid/unknown index entries should be emitted.
        assert!(
            !doc.diagnostics
                .iter()
                .any(|d| d.contains(".. index::") || d.contains("index:"))
        );
    }
}
