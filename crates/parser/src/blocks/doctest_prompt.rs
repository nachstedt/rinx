use super::directive_body::strip_common_indent;
use rusty_sphinx_ast::{Node, TargetName};

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

/// Whether `line` opens a doctest block: `>>>` followed by a space or nothing,
/// ignoring indentation.
///
/// Requiring the space (or end of line) is what keeps this disjoint from a
/// transition, which needs four or more *identical* punctuation characters —
/// `>>>` is too short to be one and `>>>>` has no space, so neither construct
/// can be mistaken for the other.
fn opens_doctest_block(line: &str) -> bool {
    let trimmed = line.trim_start();
    match trimmed.strip_prefix(">>>") {
        Some(rest) => rest.is_empty() || rest.starts_with(' '),
        None => false,
    }
}

/// Parses a docutils *doctest block*: a text block beginning with `>>> ` and
/// running to the next blank line.
///
/// Everything up to that blank line belongs to the block — `...` continuation
/// lines and the expected output, which is why a doctest that wants to expect
/// an empty line has to write `<BLANKLINE>` rather than a real one.
///
/// This is reached only from the block-level dispatch chain, and only in
/// block-*start* position, so a `>>>` appearing partway through a paragraph
/// stays prose (as it does in docutils). Content introduced by `::` never
/// arrives here at all: `parse_paragraph` detects the trailing `::` and
/// `collect_literal_block_body` consumes the whole indented run, so a `>>>`
/// inside a literal block is never re-examined — which is what keeps those
/// blocks non-executable.
pub(super) fn try_parse_doctest_block(lines: &[&str], i: usize) -> Option<(usize, Node)> {
    if !opens_doctest_block(lines[i]) {
        return None;
    }

    let end = lines[i..]
        .iter()
        .position(|line| line.trim().is_empty())
        .map_or(lines.len(), |offset| i + offset);

    let content = strip_common_indent(&lines[i..end]);
    Some((
        end - i,
        Node::DoctestBlock(rusty_sphinx_ast::HashedContent::new(content)),
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opens_doctest_block_accepts_a_prompt_with_code() {
        // Given / When / Then
        assert!(opens_doctest_block(">>> 1 + 1"));
    }

    #[test]
    fn test_opens_doctest_block_accepts_an_indented_prompt() {
        // Given — essentially every real-world doctest block is indented,
        // sitting inside a block quote after a paragraph ending in `:`.
        assert!(opens_doctest_block("      >>> import re"));
    }

    #[test]
    fn test_opens_doctest_block_accepts_a_bare_prompt() {
        // Given / When / Then
        assert!(opens_doctest_block(">>>"));
    }

    #[test]
    fn test_opens_doctest_block_rejects_a_transition() {
        // Given — four or more repeated punctuation characters are a
        // transition; the two constructs are disjoint by spec, so neither
        // detector needs loosening to accommodate the other.
        assert!(!opens_doctest_block(">>>>"));
    }

    #[test]
    fn test_opens_doctest_block_rejects_a_prompt_without_separation() {
        // Given — `>>>x` is not the interactive prompt.
        assert!(!opens_doctest_block(">>>x = 1"));
    }

    #[test]
    fn test_opens_doctest_block_rejects_ordinary_prose() {
        // Given / When / Then
        assert!(!opens_doctest_block("Some prose about >>> prompts."));
    }

    #[test]
    fn test_try_parse_doctest_block_collects_until_a_blank_line() {
        // Given — continuation and expected-output lines belong to the block.
        let lines = vec![">>> f()", "... more", "result", "", "After."];

        // When
        let (consumed, node) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        assert_eq!(consumed, 3);
        match node {
            Node::DoctestBlock(content) => {
                assert_eq!(content.body(), ">>> f()\n... more\nresult");
            }
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_try_parse_doctest_block_strips_the_common_indent() {
        // Given
        let lines = vec!["      >>> import re", "      >>> re.escape('x')", ""];

        // When
        let (_, node) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        match node {
            Node::DoctestBlock(content) => {
                assert_eq!(content.body(), ">>> import re\n>>> re.escape('x')");
            }
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_try_parse_doctest_block_preserves_relative_indentation() {
        // Given — a continuation line indented under the prompt.
        let lines = vec!["   >>> def f():", "   ...     return 1", ""];

        // When
        let (_, node) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        match node {
            Node::DoctestBlock(content) => {
                assert_eq!(content.body(), ">>> def f():\n...     return 1");
            }
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_try_parse_doctest_block_runs_to_end_of_input() {
        // Given — no trailing blank line.
        let lines = vec![">>> 1 + 1", "2"];

        // When
        let (consumed, _) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_try_parse_doctest_block_declines_a_non_prompt_line() {
        // Given
        let lines = vec!["Just prose."];

        // When
        let result = try_parse_doctest_block(&lines, 0);

        // Then
        assert!(result.is_none());
    }
}
