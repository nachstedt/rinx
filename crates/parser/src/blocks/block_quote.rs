//! Block quotes: indented text with no preceding directive/list marker,
//! ported from docutils' `Body.indent`/`block_quote`/`split_attribution`/
//! `check_attribution`. Indentation is the sole markup indicator, and it is
//! checked *before* every marker-based construct — an indented bullet list,
//! table, or directive nests inside a `BlockQuote` rather than being matched
//! in place, exactly as it would be one recursion level deeper (inside a
//! list item's or directive's own body, which is already dedented to a zero
//! baseline before reaching `parse_blocks`).

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::{indent_width, strip_indent, unindent_body_lines};
use crate::inline::{SourceMap, parse_inline_text_mapped};
use rinx_ast::{Diagnostic, DiagnosticCode, InlineNode, Node};

/// Where an attribution paragraph sits within one indented run, and how much
/// of each of its lines is markup rather than display text.
struct AttributionMatch {
    /// Index into the run's dedented lines where the attribution starts.
    start: usize,
    /// Index one past the attribution's last line.
    end: usize,
    /// Characters to strip off the first line: the dash/em-dash marker plus
    /// its trailing spaces.
    marker_len: usize,
    /// Characters to strip off every subsequent line: their shared indent.
    continuation_indent: usize,
}

/// Tries to open a block quote at `lines[i]`.
///
/// Declines (returns `None`) whenever `lines[i]` isn't indented at all, so
/// the dispatch chain falls through to every other construct unchanged for
/// the zero-indent case they all assume.
pub(super) fn try_parse_block_quote(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Vec<Node>)> {
    let (reference, consumed, dedented, blank_finish) = collect_indented_block(lines, i)?;

    if !blank_finish {
        let last = i + consumed - 1;
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::BlockQuoteUnindentNoBlankLine,
            "Block quote ends without a blank line; unexpected unindent.",
            ctx.line_span(last, lines[last]),
        ));
    }

    let block_ctx = ctx.nested(i, reference).without_section_titles();
    let dedented_refs: Vec<&str> = dedented.iter().map(String::as_str).collect();
    let nodes = split_into_block_quotes(&dedented_refs, adornment_order, diagnostics, &block_ctx);
    Some((consumed, nodes))
}

/// Collects the run of lines starting at `lines[start_i]` that make up one
/// indented block: the trigger line's own indentation is the *reference*
/// indent, and every following blank line or line indented at least that
/// much extends the run.
///
/// Returns `None` when `lines[start_i]` has no indentation to speak of.
/// Otherwise returns the reference indent, the number of source lines
/// consumed (including trailing blank lines), the dedented content, and
/// whether the run ended on a blank line/end of input rather than an abrupt
/// dedent (docutils' `blank_finish`).
fn collect_indented_block(
    lines: &[&str],
    start_i: usize,
) -> Option<(usize, usize, Vec<String>, bool)> {
    let first = lines[start_i].trim_end();
    let reference = indent_width(first);
    if reference == 0 {
        return None;
    }

    let mut raw_lines = vec![first];
    let mut i = start_i + 1;
    let mut blank_finish = false;
    while i < lines.len() {
        let next_line = lines[i].trim_end();
        if next_line.trim().is_empty() {
            raw_lines.push("");
            blank_finish = true;
            i += 1;
            continue;
        }
        if indent_width(next_line) < reference {
            break;
        }
        raw_lines.push(next_line);
        blank_finish = false;
        i += 1;
    }
    // Running out of input ends the block as cleanly as a blank line does.
    if i >= lines.len() {
        blank_finish = true;
    }

    while raw_lines.last().is_some_and(|l| l.trim().is_empty()) {
        raw_lines.pop();
    }

    let consumed = i - start_i;
    let content = unindent_body_lines(&raw_lines);
    Some((reference, consumed, content, blank_finish))
}

/// Splits one collected, already-dedented indented run into one or more
/// sibling [`Node::BlockQuote`]s — more than one only when an attribution is
/// itself followed by further indented content, docutils' `while indented:`
/// loop in `block_quote()`.
///
/// `ctx`'s local line 0 is `dedented[0]`; every line's absolute index into
/// `dedented` doubles as its local line number under `ctx`.
fn split_into_block_quotes(
    dedented: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut start = 0;

    while start < dedented.len() {
        while start < dedented.len() && dedented[start].trim().is_empty() {
            start += 1;
        }
        if start >= dedented.len() {
            break;
        }

        let (content_end, attribution_match) = find_attribution_split(dedented, start);

        let content_slice = &dedented[start..content_end];
        let content_ctx = ctx.nested(start, 0);
        let content = parse_blocks(content_slice, adornment_order, diagnostics, &content_ctx);

        let attribution = attribution_match
            .as_ref()
            .map(|m| build_attribution(dedented, m, ctx));

        nodes.push(Node::BlockQuote {
            content,
            attribution,
        });

        start = attribution_match.map_or(dedented.len(), |m| m.end);
    }

    nodes
}

/// Scans `dedented[start..]` for a blank-line-preceded paragraph that opens
/// with an attribution marker and has consistently indented continuation
/// lines — docutils' `split_attribution`. A candidate whose marker matches
/// but whose continuation lines don't share one indent is not an
/// attribution; the scan continues past it, matching upstream's own
/// leniency here.
///
/// Returns the index the quote's own content ends at (exclusive) and the
/// attribution's extent, or `(dedented.len(), None)` when none is found.
fn find_attribution_split(dedented: &[&str], start: usize) -> (usize, Option<AttributionMatch>) {
    let mut last_blank: Option<usize> = None;
    let mut nonblank_seen = false;
    let mut i = start;

    while i < dedented.len() {
        let line = dedented[i];
        if line.trim().is_empty() {
            last_blank = Some(i);
            i += 1;
            continue;
        }

        if nonblank_seen
            && i > 0
            && last_blank == Some(i - 1)
            && let Some(marker_len) = match_attribution_marker(line)
            && let Some((end, continuation_indent)) = check_attribution_shape(dedented, i + 1)
        {
            return (
                i,
                Some(AttributionMatch {
                    start: i,
                    end,
                    marker_len,
                    continuation_indent,
                }),
            );
        }
        nonblank_seen = true;
        i += 1;
    }

    (dedented.len(), None)
}

/// Verifies the shape of an attribution's continuation lines, starting right
/// after its marker line: every non-blank line up to the next blank line or
/// end of input must share exactly one indent. Returns the index one past
/// the attribution's last line and that shared indent (`0` when there are no
/// continuation lines), or `None` when the indentation isn't consistent —
/// docutils' `check_attribution`.
fn check_attribution_shape(dedented: &[&str], start: usize) -> Option<(usize, usize)> {
    let mut indent: Option<usize> = None;
    let mut i = start;
    while i < dedented.len() {
        let line = dedented[i];
        if line.trim().is_empty() {
            break;
        }
        let width = indent_width(line);
        match indent {
            None => indent = Some(width),
            Some(existing) if existing != width => return None,
            Some(_) => {}
        }
        i += 1;
    }
    Some((i, indent.unwrap_or(0)))
}

/// Matches docutils' `attribution_pattern`: `"--"`, `"---"` (but not four or
/// more dashes), or a true em-dash, followed by zero or more spaces and then
/// a non-space character — anchored at the very start of `line`, since an
/// attribution must be flush left.
///
/// Returns how many *characters* the marker and its trailing spaces occupy,
/// so the caller can strip exactly that much off the display text.
fn match_attribution_marker(line: &str) -> Option<usize> {
    let mut chars = line.chars().peekable();
    let marker_len = match chars.next()? {
        '\u{2014}' => 1,
        '-' => {
            if chars.peek() != Some(&'-') {
                return None;
            }
            chars.next();
            if chars.peek() == Some(&'-') {
                chars.next();
                if chars.peek() == Some(&'-') {
                    return None;
                }
                3
            } else {
                2
            }
        }
        _ => return None,
    };

    let mut spaces = 0;
    while chars.peek() == Some(&' ') {
        chars.next();
        spaces += 1;
    }
    chars.peek()?;
    Some(marker_len + spaces)
}

/// Builds the attribution's inline content from its raw (dedented) lines,
/// stripping the marker off the first line and the shared indent off every
/// subsequent one, and recording each line's true source position so a
/// broken `:ref:`/hyperlink inside an attribution still reports correctly.
fn build_attribution(
    dedented: &[&str],
    m: &AttributionMatch,
    ctx: &ParseCtx<'_>,
) -> Vec<InlineNode> {
    let mut text = String::new();
    let mut map = SourceMap::none();

    for (offset, idx) in (m.start..m.end).enumerate() {
        let raw = dedented[idx];
        let column = if offset == 0 {
            m.marker_len
        } else {
            m.continuation_indent
        };
        let stripped = strip_indent(raw, column);

        if offset > 0 {
            text.push('\n');
        }
        map.push(text.len(), stripped, idx, column);
        text.push_str(stripped);
    }

    parse_inline_text_mapped(&text, ctx.default_domain, &map, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Diagnostics;
    use crate::headings::Adornment;
    use rinx_ast::Domain;

    fn ctx() -> ParseCtx<'static> {
        ParseCtx::with_domain(Domain::Py)
    }

    fn parse(lines: &[&str]) -> (usize, Vec<Node>, Diagnostics) {
        let mut adornment_order: Vec<Adornment> = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let ctx = ctx();
        let (consumed, nodes) =
            try_parse_block_quote(lines, 0, &mut adornment_order, &mut diagnostics, &ctx)
                .expect("expected a block quote");
        (consumed, nodes, diagnostics)
    }

    #[test]
    fn test_try_parse_block_quote_declines_a_zero_indent_line() {
        // Given
        let lines = ["Not indented."];
        let mut adornment_order: Vec<Adornment> = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let result =
            try_parse_block_quote(&lines, 0, &mut adornment_order, &mut diagnostics, &ctx());

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_block_quote_wraps_a_bare_indented_paragraph() {
        // Given
        let lines = ["    Quoted text.", "", "Back to normal."];

        // When
        let (consumed, nodes, diagnostics) = parse(&lines);

        // Then
        assert_eq!(consumed, 2); // the paragraph line plus the separating blank line
        assert_eq!(
            nodes,
            vec![Node::BlockQuote {
                content: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Quoted text.".to_string()
                )])],
                attribution: None,
            }]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_try_parse_block_quote_nests_a_deeper_indented_quote() {
        // Given
        let lines = ["    Outer.", "", "        Inner."];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        let Node::BlockQuote { content, .. } = &nodes[0] else {
            panic!("expected a block quote");
        };
        assert_eq!(
            content,
            &vec![
                Node::Paragraph(vec![InlineNode::Text("Outer.".to_string())]),
                Node::BlockQuote {
                    content: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Inner.".to_string()
                    )])],
                    attribution: None,
                },
            ]
        );
    }

    #[test]
    fn test_try_parse_block_quote_nests_a_bullet_list() {
        // Given — an indented bullet list has no block-quote marker of its
        // own, so per the full grammar it nests inside a block quote rather
        // than being matched in place.
        let lines = ["    - one", "    - two"];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        let Node::BlockQuote { content, .. } = &nodes[0] else {
            panic!("expected a block quote");
        };
        assert_eq!(content.len(), 1);
        assert!(matches!(content[0], Node::BulletList { .. }));
    }

    #[test]
    fn test_try_parse_block_quote_nests_a_line_block() {
        // Given — an indented line block has no block-quote marker of its
        // own either, so it nests inside a block quote the same way an
        // indented bullet list does above.
        let lines = ["    | One line.", "    | Another line."];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        let Node::BlockQuote { content, .. } = &nodes[0] else {
            panic!("expected a block quote");
        };
        assert_eq!(content.len(), 1);
        assert!(matches!(content[0], Node::LineBlock(_)));
    }

    #[test]
    fn test_try_parse_block_quote_splits_off_a_single_line_attribution() {
        // Given
        let lines = ["    Quoted text.", "", "    -- Sherlock Holmes"];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        assert_eq!(
            nodes,
            vec![Node::BlockQuote {
                content: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Quoted text.".to_string()
                )])],
                attribution: Some(vec![InlineNode::Text("Sherlock Holmes".to_string())]),
            }]
        );
    }

    #[test]
    fn test_try_parse_block_quote_accepts_an_em_dash_attribution_marker() {
        // Given
        let lines = ["    Quoted text.", "", "    \u{2014}Sherlock Holmes"];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        let Node::BlockQuote { attribution, .. } = &nodes[0] else {
            panic!("expected a block quote");
        };
        assert_eq!(
            attribution,
            &Some(vec![InlineNode::Text("Sherlock Holmes".to_string())])
        );
    }

    #[test]
    fn test_try_parse_block_quote_joins_a_multi_line_attribution() {
        // Given — the second line's indent must match the first's own body
        // indent (both flush at the same column relative to the quote).
        let lines = [
            "    Quoted text.",
            "",
            "    -- Sherlock Holmes,",
            "       A Study in Scarlet",
        ];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        let Node::BlockQuote { attribution, .. } = &nodes[0] else {
            panic!("expected a block quote");
        };
        assert_eq!(
            attribution,
            &Some(vec![InlineNode::Text(
                "Sherlock Holmes,\nA Study in Scarlet".to_string()
            )])
        );
    }

    #[test]
    fn test_try_parse_block_quote_treats_inconsistent_attribution_lines_as_plain_content() {
        // Given — the two continuation lines' indents (0, then 3) don't
        // match each other, so this isn't a valid attribution shape and
        // every line stays ordinary quote content, re-joined by the
        // recursive parse into one paragraph exactly as it would be without
        // any attribution-like marker at all.
        let lines = [
            "    Quoted text.",
            "",
            "    -- Sherlock Holmes,",
            "    A Study in Scarlet,",
            "       by Doyle",
        ];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        let Node::BlockQuote {
            content,
            attribution,
        } = &nodes[0]
        else {
            panic!("expected a block quote");
        };
        assert_eq!(attribution, &None);
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn test_try_parse_block_quote_parses_inline_markup_in_an_attribution() {
        // Given
        let lines = ["    Quoted text.", "", "    -- *Sherlock Holmes*"];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        let Node::BlockQuote { attribution, .. } = &nodes[0] else {
            panic!("expected a block quote");
        };
        assert_eq!(
            attribution,
            &Some(vec![InlineNode::Emphasis("Sherlock Holmes".to_string())])
        );
    }

    #[test]
    fn test_try_parse_block_quote_requires_content_before_an_attribution() {
        // Given — a dash-led paragraph with nothing quoted before it is not
        // an attribution at all, just a paragraph (whose "--" smart
        // typography still turns into an en dash, same as anywhere else).
        let lines = ["    -- Not an attribution."];

        // When
        let (_, nodes, _) = parse(&lines);

        // Then
        assert_eq!(
            nodes,
            vec![Node::BlockQuote {
                content: vec![Node::Paragraph(vec![InlineNode::Text(
                    "\u{2013} Not an attribution.".to_string()
                )])],
                attribution: None,
            }]
        );
    }

    #[test]
    fn test_try_parse_block_quote_warns_when_dedent_has_no_blank_line() {
        // Given — no blank line separates the quote from what follows.
        let lines = ["    Quoted text.", "Back to normal."];

        // When
        let (consumed, _, diagnostics) = parse(&lines);

        // Then
        assert_eq!(consumed, 1);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::BlockQuoteUnindentNoBlankLine
        );
    }

    #[test]
    fn test_try_parse_block_quote_does_not_warn_at_end_of_input() {
        // Given
        let lines = ["    Quoted text."];

        // When
        let (_, _, diagnostics) = parse(&lines);

        // Then
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_collect_indented_block_declines_zero_indent() {
        // Given / When / Then
        assert!(collect_indented_block(&["no indent"], 0).is_none());
    }

    #[test]
    fn test_collect_indented_block_stops_at_a_less_indented_line() {
        // Given
        let lines = ["    one", "    two", "back"];

        // When
        let (reference, consumed, content, blank_finish) =
            collect_indented_block(&lines, 0).expect("expected an indented block");

        // Then
        assert_eq!(reference, 4);
        assert_eq!(consumed, 2);
        assert_eq!(content, vec!["one".to_string(), "two".to_string()]);
        assert!(!blank_finish);
    }

    #[test]
    fn test_match_attribution_marker_accepts_double_and_triple_dash() {
        // Given / When / Then
        assert_eq!(match_attribution_marker("-- Author"), Some(3));
        assert_eq!(match_attribution_marker("--- Author"), Some(4));
    }

    #[test]
    fn test_match_attribution_marker_rejects_four_or_more_dashes() {
        // Given / When / Then — a run of four+ dashes reads as a transition,
        // not an attribution.
        assert_eq!(match_attribution_marker("---- Author"), None);
    }

    #[test]
    fn test_match_attribution_marker_rejects_a_single_dash() {
        // Given / When / Then
        assert_eq!(match_attribution_marker("- Author"), None);
    }

    #[test]
    fn test_match_attribution_marker_rejects_a_marker_with_nothing_after_it() {
        // Given / When / Then
        assert_eq!(match_attribution_marker("--"), None);
        assert_eq!(match_attribution_marker("--   "), None);
    }

    #[test]
    fn test_match_attribution_marker_requires_flush_left() {
        // Given / When / Then — leading whitespace means it isn't anchored
        // at column 0, so it doesn't match at all.
        assert_eq!(match_attribution_marker(" -- Author"), None);
    }

    #[test]
    fn test_check_attribution_shape_returns_zero_indent_for_a_single_line() {
        // Given
        let lines = ["-- Author"];

        // When
        let (end, indent) = check_attribution_shape(&lines, 1).expect("expected a valid shape");

        // Then
        assert_eq!(end, 1);
        assert_eq!(indent, 0);
    }

    #[test]
    fn test_check_attribution_shape_rejects_inconsistent_continuation_indent() {
        // Given
        let lines = ["-- Author,", "   line two", " line three"];

        // When / Then
        assert!(check_attribution_shape(&lines, 1).is_none());
    }
}
