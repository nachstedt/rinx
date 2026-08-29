//! Literal block bodies — the indented run introduced by a paragraph ending
//! in `::`, kept verbatim.

use crate::indent::strip_common_indent;

/// Collects a literal block body starting at `start_index`.
///
/// - Skips a leading blank line (mandatory after `::`).
/// - Collects contiguous lines until indentation drops to (or below) the base level.
/// - Strips the *minimum* common indentation from all non-blank lines, preserving relative
///   indentation within the block (RST spec behaviour).
///
/// Consuming the whole indented run here is also what keeps a `>>>` inside a
/// literal block non-executable: [`super::doctest_block`] never re-examines
/// these lines.
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
