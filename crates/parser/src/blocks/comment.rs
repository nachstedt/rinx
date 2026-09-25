use crate::directives::body::collect_directive_body;
use crate::indent::indent_width;
use rinx_ast::{DiagnosticCode, Node, SuppressionCodes};

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
    let body = collect_directive_body(lines, i + 1, indent_width(line));
    let consumed = 1 + body.consumed;

    Some((consumed, Node::Comment))
}

/// The `.. noqa:` prefix, written as an RST comment so that real Sphinx —
/// which knows nothing about it — ignores the line instead of erroring on an
/// unknown directive. A document stays buildable by both tools.
const NOQA: &str = "noqa";

/// Reads a `.. noqa` / `.. noqa: code, code` comment's payload.
///
/// Returns the codes it names, plus any id that is not a diagnostic code —
/// those are reported rather than dropped, because a typo'd suppression that
/// silently matches nothing is precisely the "silently degrades valid-looking
/// input" case this project diagnoses on principle.
///
/// Returns `None` when the line is an ordinary comment. Note the deliberate
/// narrowness: `.. noqa-ish` and `.. noqad` are *not* suppressions, so an
/// author's prose comment cannot become one by accident.
pub(super) fn parse_noqa_comment(line: &str) -> Option<NoqaComment> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix(".. ")?.trim_start();
    let payload = match rest.strip_prefix(NOQA)? {
        // `.. noqa` on its own.
        "" => "",
        // `.. noqa: a, b` — the colon is required before a list.
        rest => rest.strip_prefix(':')?,
    };

    let mut codes = Vec::new();
    let mut unknown = Vec::new();
    for token in payload
        .split([',', ' ', '\t'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        match token.parse::<DiagnosticCode>() {
            Ok(code) => codes.push(code),
            Err(_) => unknown.push(token.to_string()),
        }
    }

    Some(NoqaComment {
        // A bare `.. noqa`, or one whose every id was a typo, silences
        // everything in the next block — the author's evident intent, with the
        // typo reported alongside so it can be fixed.
        codes: if codes.is_empty() && unknown.is_empty() {
            SuppressionCodes::All
        } else {
            SuppressionCodes::Only(codes)
        },
        unknown,
    })
}

/// A parsed `.. noqa:` comment.
pub(super) struct NoqaComment {
    pub(super) codes: SuppressionCodes,
    /// Ids that name no diagnostic code, kept so each can be reported.
    pub(super) unknown: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

#[cfg(test)]
mod integration_tests {
    use crate::parse;
    use rinx_ast::{InlineNode, Node};

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
}

#[cfg(test)]
mod noqa_tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Suppression, SuppressionCodes};

    /// The suppressions `input` yields, for the scope-resolution tests.
    fn suppressions(input: &str) -> Vec<Suppression> {
        parse("test.rst", input).suppressions
    }

    // --- parse_noqa_comment ---

    #[test]
    fn test_parses_a_single_named_code() {
        // Given
        let noqa = parse_noqa_comment(".. noqa: link.broken-ref").expect("a noqa comment");

        // Then
        assert_eq!(
            noqa.codes,
            SuppressionCodes::Only(vec![DiagnosticCode::LinkBrokenRef])
        );
        assert!(noqa.unknown.is_empty());
    }

    #[test]
    fn test_parses_several_codes_separated_by_commas_or_spaces() {
        // Given both separators, which authors mix in practice
        for line in [
            ".. noqa: link.broken-ref, link.broken-term",
            ".. noqa: link.broken-ref link.broken-term",
            ".. noqa: link.broken-ref,link.broken-term",
        ] {
            // When
            let noqa = parse_noqa_comment(line).expect("a noqa comment");

            // Then
            assert_eq!(
                noqa.codes,
                SuppressionCodes::Only(vec![
                    DiagnosticCode::LinkBrokenRef,
                    DiagnosticCode::LinkBrokenTerm
                ]),
                "{line}"
            );
        }
    }

    #[test]
    fn test_a_bare_noqa_suppresses_everything() {
        // Given
        let noqa = parse_noqa_comment(".. noqa").expect("a noqa comment");

        // Then
        assert_eq!(noqa.codes, SuppressionCodes::All);
    }

    #[test]
    fn test_reports_an_unknown_code_and_keeps_the_known_ones() {
        // Given a list with one typo in it
        let noqa =
            parse_noqa_comment(".. noqa: link.broken-ref, link.brokenref").expect("a noqa comment");

        // Then — the good one still applies, and the typo is handed back to be
        // reported rather than silently ignored
        assert_eq!(
            noqa.codes,
            SuppressionCodes::Only(vec![DiagnosticCode::LinkBrokenRef])
        );
        assert_eq!(noqa.unknown, ["link.brokenref"]);
    }

    #[test]
    fn test_an_all_typo_list_suppresses_nothing_but_is_reported() {
        // Given a list whose every id is a typo
        let noqa = parse_noqa_comment(".. noqa: nope").expect("a noqa comment");

        // Then — an empty `Only`, so nothing is silenced; the author is told
        assert_eq!(noqa.codes, SuppressionCodes::Only(Vec::new()));
        assert_eq!(noqa.unknown, ["nope"]);
    }

    #[test]
    fn test_an_ordinary_comment_is_not_a_noqa() {
        // Given / When / Then
        for line in [
            ".. just a comment",
            ".. note about noqa",
            ".. noqad: x",
            ".. noqa-ish",
            "..",
            "not a comment at all",
        ] {
            assert!(parse_noqa_comment(line).is_none(), "{line}");
        }
    }

    #[test]
    fn test_an_indented_noqa_is_recognised() {
        // Given a suppression written inside an indented container
        let noqa = parse_noqa_comment("      .. noqa: link.broken-ref").expect("a noqa comment");

        // Then
        assert_eq!(
            noqa.codes,
            SuppressionCodes::Only(vec![DiagnosticCode::LinkBrokenRef])
        );
    }

    // --- scope resolution, through a whole parse ---

    #[test]
    fn test_a_suppression_covers_the_block_that_follows_it() {
        // Given a comment on line 1 and a paragraph on line 3
        let found = suppressions(".. noqa: link.broken-ref\n\nA paragraph.\n");

        // Then — the range is the *paragraph's*, not the comment's
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].start_line, 3);
        assert_eq!(found[0].end_line, 3);
    }

    #[test]
    fn test_a_suppression_covers_a_multi_line_block_entirely() {
        // Given a paragraph spanning three lines
        let found = suppressions(".. noqa\n\nline one\nline two\nline three\n");

        // Then
        assert_eq!(found[0].start_line, 3);
        assert_eq!(found[0].end_line, 5);
    }

    #[test]
    fn test_a_suppression_covers_everything_nested_in_the_block() {
        // Given a suppression before a directive whose body holds the role
        let found = suppressions(".. noqa\n\n.. note::\n\n   Nested text.\n");

        // Then — the whole directive, body included, is covered
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].start_line, 3);
        assert!(found[0].end_line >= 5, "{:?}", found[0]);
    }

    #[test]
    fn test_two_suppressions_in_a_row_both_apply_to_the_next_block() {
        // Given two comments before one paragraph
        let found =
            suppressions(".. noqa: link.broken-ref\n\n.. noqa: link.broken-term\n\nA paragraph.\n");

        // Then — neither is lost, and both name the paragraph
        assert_eq!(found.len(), 2);
        for suppression in &found {
            assert_eq!(suppression.start_line, 5);
        }
    }

    #[test]
    fn test_a_suppression_at_the_end_of_a_document_covers_nothing() {
        // Given a comment with no block after it
        let found = suppressions("A paragraph.\n\n.. noqa: link.broken-ref\n");

        // Then — nothing is suppressed rather than something arbitrary
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn test_a_suppression_inside_a_directive_body_resolves_against_that_body() {
        // Given a suppression written inside a note, before its second
        // paragraph
        let input = ".. note::\n\n   .. noqa: link.broken-ref\n\n   Suppressed.\n\n   Reported.\n";

        // When
        let found = suppressions(input);

        // Then — it names the nested paragraph's real document lines, so the
        // offsets composed through the directive rebase
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].start_line, 5);
        assert_eq!(found[0].end_line, 5);
    }

    #[test]
    fn test_a_suppression_inside_a_list_item_resolves_against_that_item() {
        // Given a suppression inside a bullet item
        let input = "* item one\n\n* .. noqa\n\n  Suppressed text.\n";

        // When
        let found = suppressions(input);

        // Then
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].start_line, 5);
    }

    #[test]
    fn test_a_document_without_noqa_has_no_suppressions() {
        // Given / When / Then
        assert!(suppressions("Just a paragraph.\n").is_empty());
    }

    #[test]
    fn test_an_unknown_code_is_reported_at_the_comments_own_line() {
        // Given a typo on the third line
        let doc = parse("test.rst", "A paragraph.\n\n.. noqa: nope\n\nAnother.\n");

        // When
        let reported: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code == DiagnosticCode::NoqaUnknownCode)
            .collect();

        // Then — the comment's line, since that is what needs correcting
        assert_eq!(reported.len(), 1);
        assert_eq!(
            reported[0]
                .span
                .expect("a document-rooted parse always yields a span")
                .start
                .line,
            3
        );
        assert!(reported[0].message.contains("nope"));
    }
}
