//! Line blocks (`| text`): a sequence of lines rendered without paragraph
//! reflow, for poetry and addresses — ported from docutils'
//! `Body.line_block`/`line_block_line`/`nest_line_block_lines`/
//! `nest_line_block_segment`.
//!
//! Nesting is expressed purely through the indentation of the text *after*
//! each `|`; the marker itself always sits at column 0 of whatever local
//! frame this runs over. An outer indentation (say, a line block quoted
//! inside a paragraph) is therefore already handled by
//! `try_parse_block_quote` running first in the dispatch chain and dedenting
//! to a zero baseline before recursing — this module never has to think
//! about it.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::{indent_width, strip_indent};
use crate::inline::{SourceMap, parse_inline_text_mapped};
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, InlineNode, LineBlockItem, Node};

/// One line-block item as first collected, before indent inheritance and
/// nesting are resolved.
struct RawItem {
    /// `None` for a bare `|` (a deliberately blank line), which inherits its
    /// predecessor's indent once the whole run is known.
    indent: Option<usize>,
    content: Vec<InlineNode>,
}

/// Matches docutils' `\|( +|$)`, anchored at column 0: a `|` immediately
/// followed by one or more spaces, or by nothing at all. Returns the column
/// the line's text starts at — one past the bar and its run of spaces — or
/// `None` when the line doesn't open a line-block item at all, e.g. `|foo`
/// (a bar directly followed by non-space text, which is not a marker).
fn match_line_block_marker(line: &str) -> Option<usize> {
    let rest = line.strip_prefix('|')?;
    let spaces = rest.chars().take_while(|c| *c == ' ').count();
    if spaces == 0 && !rest.is_empty() {
        return None;
    }
    Some(1 + spaces)
}

/// Tries to open a line block at `lines[start_i]`.
///
/// Declines (returns `None`) unless that line opens with the line-block
/// marker, leaving anything else — including a `|foo` with no space — for
/// the paragraph fallback, matching docutils.
pub(super) fn try_parse_line_block(
    lines: &[&str],
    start_i: usize,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    match_line_block_marker(lines[start_i].trim_end())?;

    let mut raw_items: Vec<RawItem> = Vec::new();
    let mut i = start_i;
    let mut blank_finish = false;

    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.trim().is_empty() {
            blank_finish = true;
            i += 1;
            break;
        }
        let Some(text_start_col) = match_line_block_marker(line) else {
            break; // A non-blank, non-`|` line ends the block, uncleanly.
        };
        let (consumed, item) = collect_line_block_item(lines, i, line, text_start_col, ctx);
        raw_items.push(item);
        i += consumed;
    }
    if i >= lines.len() {
        blank_finish = true;
    }

    if !blank_finish {
        let last = i - 1;
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::LineBlockEndsWithoutBlankLine,
            "Line block ends without a blank line; unexpected non-`|` line.",
            ctx.line_span(last, lines[last]),
        ));
    }

    let items = nest_line_block_items(resolve_indents(raw_items));
    Some((i - start_i, Node::LineBlock(items)))
}

/// Collects one logical item starting at the marker line `lines[i]`
/// (`line` is that same line, already right-trimmed), whose text begins at
/// `text_start_col`. Continuation lines — physical lines that aren't
/// themselves a marker, indented at least as far as `text_start_col` — join
/// the same item, dedented to that column and re-joined with `\n`, matching
/// docutils' `get_first_known_indented` call in `line_block_line`: a long
/// verse line wrapped onto several physical lines is still one logical line.
///
/// Returns the number of physical lines consumed and the item's own
/// (not-yet-inherited) indent alongside its already inline-parsed content.
fn collect_line_block_item(
    lines: &[&str],
    i: usize,
    line: &str,
    text_start_col: usize,
    ctx: &ParseCtx<'_>,
) -> (usize, RawItem) {
    // A bare `|`, however many trailing spaces it carries, has no indent of
    // its own — docutils checks the whole (rstripped) line here, not just
    // the space run the marker matched.
    let is_bare = line.trim_end() == "|";
    let indent = if is_bare {
        None
    } else {
        Some(text_start_col - 2)
    };

    let mut raw_lines: Vec<(usize, &str)> = vec![(i, strip_indent(line, text_start_col))];
    let mut j = i + 1;
    while j < lines.len() {
        let next = lines[j].trim_end();
        if next.trim().is_empty() || indent_width(next) < text_start_col {
            break;
        }
        raw_lines.push((j, strip_indent(next, text_start_col)));
        j += 1;
    }

    let mut text = String::new();
    let mut map = SourceMap::none();
    for (offset, (line_idx, stripped)) in raw_lines.iter().enumerate() {
        if offset > 0 {
            text.push('\n');
        }
        map.push(text.len(), stripped, *line_idx, text_start_col);
        text.push_str(stripped);
    }
    let content = if text.is_empty() {
        Vec::new()
    } else {
        parse_inline_text_mapped(&text, ctx.default_domain, &map, ctx)
    };

    (j - i, RawItem { indent, content })
}

/// Resolves every item's inherited indent: the first item's `None` (a bare
/// `|` opening the block) becomes `0`, and every later `None` inherits its
/// predecessor's already-resolved value — docutils' `nest_line_block_lines`.
fn resolve_indents(raw_items: Vec<RawItem>) -> Vec<(usize, Vec<InlineNode>)> {
    let mut previous = 0;
    raw_items
        .into_iter()
        .map(|item| {
            let indent = item.indent.unwrap_or(previous);
            previous = indent;
            (indent, item.content)
        })
        .collect()
}

/// Groups a flat, indent-resolved run into nested [`LineBlockItem`]s: items
/// at the run's own minimum indent stay direct siblings, and every
/// consecutive run of more-indented items between them becomes one
/// [`LineBlockItem::Nested`], itself grouped the same way — docutils'
/// `nest_line_block_segment`.
fn nest_line_block_items(items: Vec<(usize, Vec<InlineNode>)>) -> Vec<LineBlockItem> {
    let Some(least) = items.iter().map(|(indent, _)| *indent).min() else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut buffer = Vec::new();
    for (indent, content) in items {
        if indent > least {
            buffer.push((indent, content));
        } else {
            if !buffer.is_empty() {
                result.push(LineBlockItem::Nested(nest_line_block_items(
                    std::mem::take(&mut buffer),
                )));
            }
            result.push(LineBlockItem::Line(content));
        }
    }
    if !buffer.is_empty() {
        result.push(LineBlockItem::Nested(nest_line_block_items(buffer)));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Diagnostics;
    use rusty_sphinx_ast::Domain;

    fn ctx() -> ParseCtx<'static> {
        ParseCtx::with_domain(Domain::Py)
    }

    fn parse(lines: &[&str]) -> (usize, Node, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let (consumed, node) = try_parse_line_block(lines, 0, &mut diagnostics, &ctx())
            .expect("expected a line block");
        (consumed, node, diagnostics)
    }

    #[test]
    fn test_try_parse_line_block_declines_a_line_with_no_bar() {
        // Given
        let lines = ["Not a line block."];
        let mut diagnostics = Diagnostics::default();

        // When
        let result = try_parse_line_block(&lines, 0, &mut diagnostics, &ctx());

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_line_block_declines_a_bar_with_no_following_space() {
        // Given — `|foo` has no space after the bar, so docutils' own
        // pattern does not match it either.
        let lines = ["|foo"];
        let mut diagnostics = Diagnostics::default();

        // When
        let result = try_parse_line_block(&lines, 0, &mut diagnostics, &ctx());

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_line_block_parses_a_single_line() {
        // Given
        let lines = ["| One line."];

        // When
        let (consumed, node, diagnostics) = parse(&lines);

        // Then
        assert_eq!(consumed, 1);
        assert_eq!(
            node,
            Node::LineBlock(vec![LineBlockItem::Line(vec![InlineNode::Text(
                "One line.".to_string()
            )])])
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_try_parse_line_block_keeps_sibling_lines_separate() {
        // Given
        let lines = ["| Line one.", "| Line two."];

        // When
        let (consumed, node, _) = parse(&lines);

        // Then
        assert_eq!(consumed, 2);
        assert_eq!(
            node,
            Node::LineBlock(vec![
                LineBlockItem::Line(vec![InlineNode::Text("Line one.".to_string())]),
                LineBlockItem::Line(vec![InlineNode::Text("Line two.".to_string())]),
            ])
        );
    }

    #[test]
    fn test_try_parse_line_block_renders_a_bare_bar_as_an_empty_line() {
        // Given
        let lines = ["| Line one.", "|", "| Line three."];

        // When
        let (_, node, _) = parse(&lines);

        // Then
        assert_eq!(
            node,
            Node::LineBlock(vec![
                LineBlockItem::Line(vec![InlineNode::Text("Line one.".to_string())]),
                LineBlockItem::Line(vec![]),
                LineBlockItem::Line(vec![InlineNode::Text("Line three.".to_string())]),
            ])
        );
    }

    #[test]
    fn test_try_parse_line_block_nests_a_more_indented_line() {
        // Given — the middle line's text is indented two columns further
        // than its siblings, so it nests one level deeper.
        let lines = ["| Line one.", "|   Indented line two.", "| Line three."];

        // When
        let (_, node, _) = parse(&lines);

        // Then
        assert_eq!(
            node,
            Node::LineBlock(vec![
                LineBlockItem::Line(vec![InlineNode::Text("Line one.".to_string())]),
                LineBlockItem::Nested(vec![LineBlockItem::Line(vec![InlineNode::Text(
                    "Indented line two.".to_string()
                )])]),
                LineBlockItem::Line(vec![InlineNode::Text("Line three.".to_string())]),
            ])
        );
    }

    #[test]
    fn test_try_parse_line_block_nests_multiple_levels() {
        // Given — each line is indented one column further than the last,
        // so every one nests one level deeper than its predecessor.
        let lines = ["| Level 0.", "|  Level 1.", "|   Level 2."];

        // When
        let (_, node, _) = parse(&lines);

        // Then
        assert_eq!(
            node,
            Node::LineBlock(vec![
                LineBlockItem::Line(vec![InlineNode::Text("Level 0.".to_string())]),
                LineBlockItem::Nested(vec![
                    LineBlockItem::Line(vec![InlineNode::Text("Level 1.".to_string())]),
                    LineBlockItem::Nested(vec![LineBlockItem::Line(vec![InlineNode::Text(
                        "Level 2.".to_string()
                    )])]),
                ]),
            ])
        );
    }

    #[test]
    fn test_try_parse_line_block_joins_a_wrapped_continuation_line() {
        // Given — the second physical line has no bar of its own but is
        // indented to align under the first line's text, so it continues
        // the same logical line rather than starting a new one.
        let lines = ["| A long verse line", "  that wraps onto a second row."];

        // When
        let (consumed, node, _) = parse(&lines);

        // Then
        assert_eq!(consumed, 2);
        assert_eq!(
            node,
            Node::LineBlock(vec![LineBlockItem::Line(vec![InlineNode::Text(
                "A long verse line\nthat wraps onto a second row.".to_string()
            )])])
        );
    }

    #[test]
    fn test_try_parse_line_block_stops_at_a_blank_line() {
        // Given
        let lines = ["| Quoted.", "", "Not part of the block."];

        // When
        let (consumed, _, diagnostics) = parse(&lines);

        // Then — the separating blank line is consumed too, and no warning
        // is raised since the block ended cleanly.
        assert_eq!(consumed, 2);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_try_parse_line_block_treats_two_blocks_separated_by_a_blank_line_as_distinct() {
        // Given
        let lines = ["| First block.", "", "| Second block."];

        // When
        let (consumed, node, _) = parse(&lines);

        // Then — only the first block (plus its separating blank line) is
        // consumed here; a second call starting after it would find the
        // second block afresh.
        assert_eq!(consumed, 2);
        assert_eq!(
            node,
            Node::LineBlock(vec![LineBlockItem::Line(vec![InlineNode::Text(
                "First block.".to_string()
            )])])
        );
    }

    #[test]
    fn test_try_parse_line_block_warns_when_it_ends_without_a_blank_line() {
        // Given — no blank line separates the block from what follows.
        let lines = ["| Quoted.", "Back to normal."];

        // When
        let (consumed, _, diagnostics) = parse(&lines);

        // Then
        assert_eq!(consumed, 1);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::LineBlockEndsWithoutBlankLine
        );
    }

    #[test]
    fn test_try_parse_line_block_does_not_warn_at_end_of_input() {
        // Given
        let lines = ["| Quoted."];

        // When
        let (_, _, diagnostics) = parse(&lines);

        // Then
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_try_parse_line_block_parses_inline_markup_in_a_line() {
        // Given
        let lines = ["| *Emphasis* in a line."];

        // When
        let (_, node, _) = parse(&lines);

        // Then
        assert_eq!(
            node,
            Node::LineBlock(vec![LineBlockItem::Line(vec![
                InlineNode::Emphasis("Emphasis".to_string()),
                InlineNode::Text(" in a line.".to_string()),
            ])])
        );
    }

    #[test]
    fn test_match_line_block_marker_accepts_a_bare_bar() {
        // Given / When / Then
        assert_eq!(match_line_block_marker("|"), Some(1));
    }

    #[test]
    fn test_match_line_block_marker_accepts_a_bar_with_text() {
        // Given / When / Then
        assert_eq!(match_line_block_marker("| text"), Some(2));
        assert_eq!(match_line_block_marker("|   text"), Some(4));
    }

    #[test]
    fn test_match_line_block_marker_rejects_a_bar_directly_followed_by_text() {
        // Given / When / Then
        assert_eq!(match_line_block_marker("|foo"), None);
    }

    #[test]
    fn test_match_line_block_marker_rejects_a_line_with_no_bar() {
        // Given / When / Then
        assert_eq!(match_line_block_marker("no bar here"), None);
    }

    #[test]
    fn test_resolve_indents_uses_zero_for_a_leading_bare_bar() {
        // Given
        let raw = vec![RawItem {
            indent: None,
            content: vec![],
        }];

        // When
        let resolved = resolve_indents(raw);

        // Then
        assert_eq!(resolved, vec![(0, vec![])]);
    }

    #[test]
    fn test_resolve_indents_inherits_the_previous_items_indent() {
        // Given
        let raw = vec![
            RawItem {
                indent: Some(2),
                content: vec![],
            },
            RawItem {
                indent: None,
                content: vec![],
            },
        ];

        // When
        let resolved = resolve_indents(raw);

        // Then
        assert_eq!(resolved, vec![(2, vec![]), (2, vec![])]);
    }

    #[test]
    fn test_nest_line_block_items_returns_empty_for_no_items() {
        // Given / When / Then
        assert_eq!(nest_line_block_items(vec![]), vec![]);
    }

    #[test]
    fn test_nest_line_block_items_keeps_equal_indents_as_siblings() {
        // Given
        let items = vec![(0, vec![]), (0, vec![])];

        // When
        let nested = nest_line_block_items(items);

        // Then
        assert_eq!(
            nested,
            vec![LineBlockItem::Line(vec![]), LineBlockItem::Line(vec![])]
        );
    }
}
