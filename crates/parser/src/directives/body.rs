//! Collection of the lines belonging to an explicit-markup block — a
//! directive's indented body, the further signature lines a domain-object
//! directive may declare, and the flattening of a body back to plain text for
//! the callers that want the body as *data* — `.. csv-table::`'s inline rows.
//! A body kept to be shown back as *source* goes through
//! `crate::indent::strip_common_indent` instead, which preserves the relative
//! indentation this does not.
//!
//! Also reached from `crate::blocks::comment`, since an RST comment is the
//! same `.. ` explicit-markup construct and its body is delimited by exactly
//! the same rule.

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::{indent_width, strip_indent, unindent_body_lines};
use rinx_ast::{Node, Span};

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
pub(crate) struct DirectiveBody<'a> {
    /// Source lines consumed, including the blank ones trimmed off either end.
    pub(crate) consumed: usize,
    /// The body proper, with leading and trailing blank lines removed.
    pub(crate) lines: Vec<&'a str>,
    /// How many lines below `start_index` `lines[0]` actually sits — the
    /// leading blank lines that were trimmed.
    ///
    /// Recorded rather than discarded so a caller can rebase a
    /// [`ParseCtx`](crate::ParseCtx) onto the body: without it, every
    /// diagnostic raised inside a directive whose body starts after a blank
    /// line would point one line too high.
    pub(crate) first_line_offset: usize,
}

impl<'a> DirectiveBody<'a> {
    /// The body as a directive parser receives it, and the offset of its first
    /// line below the directive's marker line.
    ///
    /// Unlike [`Self::lines`], a body that began after a blank line keeps one
    /// of those blank lines at its head. That blank line is the only sign that
    /// the body has no option block: docutils reads options solely from the
    /// lines directly below the directive, so in
    ///
    /// ```rst
    /// .. code-block:: rst
    ///
    ///    :ref:`label`
    /// ```
    ///
    /// the role is content, and a scanner handed the trimmed body could not
    /// tell it from `:ref:` written directly below the marker.
    pub(crate) fn for_directive(&self) -> (Vec<&'a str>, usize) {
        if self.first_line_offset == 0 {
            return (self.lines.clone(), 0);
        }
        let mut lines = Vec::with_capacity(self.lines.len() + 1);
        lines.push("");
        lines.extend_from_slice(&self.lines);
        (lines, self.first_line_offset - 1)
    }
}

pub(crate) fn collect_directive_body<'a>(
    lines: &[&'a str],
    start_index: usize,
    min_indent: usize,
) -> DirectiveBody<'a> {
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

    DirectiveBody {
        consumed,
        lines: body_lines[start..].to_vec(),
        first_line_offset: start,
    }
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

/// The span covering a directive body's first line — the anchor for a
/// diagnostic about the body *as a whole* rather than about one line within
/// it, which is what most content-bearing directives can offer once their
/// content has been reassembled.
///
/// The body's own indent is deliberately not counted. `ParseCtx` has already
/// been shifted by it (see `dispatch`'s `body_indent`), so the local column
/// frame starts at the first content character; measuring the raw line would
/// push the span's end past the line's real end by exactly the indent width.
/// Only the end moves, and the terminal prints only the start — but the range
/// is what a language server would highlight, so it has to be honest.
pub(in crate::directives) fn body_span(body_lines: &[&str], ctx: &ParseCtx<'_>) -> Option<Span> {
    let indent = body_lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .map_or(0, |line| indent_width(line));
    let first = body_lines.first().map_or("", |line| {
        if line.chars().count() >= indent {
            strip_indent(line, indent)
        } else {
            line.trim()
        }
    });
    ctx.line_span(0, first)
}

/// The indentation every directive-body parser strips before parsing, so a
/// `ParseCtx` can be shifted by the same amount.
///
/// Mirrors [`crate::indent::unindent_body_lines`]'s rule exactly — the first
/// non-blank line's indent — because that is the function whose effect this
/// compensates for. Without it every position inside a directive body would
/// be short by the body's indent.
pub(in crate::directives) fn body_indent(body_lines: &[&str]) -> usize {
    body_lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .map_or(0, |line| indent_width(line))
}

/// Text written on a directive's marker line, after its `::`, and the column
/// it starts at.
///
/// What that text *is* depends on the directive: an argument for most, but
/// the first line of the content for one that takes none — docutils reads
/// `.. seealso:: text` exactly as if `text` had been the body's first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::directives) struct MarkerText<'t> {
    /// The text, trimmed on both sides; empty when the marker carries none.
    pub(in crate::directives) text: &'t str,
    /// The character column, within the marker line, `text` starts at.
    pub(in crate::directives) column: usize,
}

impl<'t> MarkerText<'t> {
    /// No text at all — for a directive whose marker-line text is not
    /// content, such as the title `.. admonition::` takes.
    pub(in crate::directives) const NONE: Self = Self {
        text: "",
        column: 0,
    };

    /// The text after the first `::` of `line`, a directive's marker line.
    pub(in crate::directives) fn after_marker(line: &'t str) -> Self {
        let Some(end) = line.find("::").map(|at| at + 2) else {
            return Self::NONE;
        };
        let rest = &line[end..];
        let leading = rest.len() - rest.trim_start().len();
        Self {
            text: rest.trim(),
            column: line[..end + leading].chars().count(),
        }
    }

    /// Splits off the first whitespace-delimited word, returning it and the
    /// text after it — how a version change separates its version from the
    /// explanation Sphinx lets follow it on the same line.
    pub(in crate::directives) fn split_first_word(&self) -> (&'t str, Self) {
        let word_end = self
            .text
            .find(char::is_whitespace)
            .unwrap_or(self.text.len());
        let (word, rest) = self.text.split_at(word_end);
        let leading = rest.len() - rest.trim_start().len();
        let rest = Self {
            text: rest.trim_start(),
            column: self.column + self.text[..word_end + leading].chars().count(),
        };
        (word, rest)
    }
}

/// A directive's marker line, as seen by a parser whose content may begin on
/// it.
pub(in crate::directives) struct MarkerLine<'t> {
    /// The marker's index within the slice the directive was found in.
    pub(in crate::directives) index: usize,
    /// The whole marker line, for a diagnostic about the directive itself.
    pub(in crate::directives) span: Option<Span>,
    /// What follows its `::`.
    pub(in crate::directives) argument: MarkerText<'t>,
}

/// A directive's content, unindented, with the context positioned on its
/// first line.
pub(in crate::directives) struct DirectiveContent<'c> {
    /// One entry per source line, blank lines included, so that a line's index
    /// is its offset from the content's first line.
    pub(in crate::directives) lines: Vec<String>,
    /// The context `lines[0]` is positioned by.
    pub(in crate::directives) ctx: ParseCtx<'c>,
}

impl DirectiveContent<'_> {
    /// Parses the content as body elements, under its own context.
    pub(in crate::directives) fn parse(
        &self,
        adornment_order: &mut Vec<Adornment>,
        diagnostics: &mut Diagnostics,
    ) -> Vec<Node> {
        let lines: Vec<&str> = self.lines.iter().map(String::as_str).collect();
        parse_blocks(&lines, adornment_order, diagnostics, &self.ctx)
    }
}

/// The content of the directive whose marker is line `marker_index` of `ctx`'s
/// slice: `first_line`, when it is not empty, followed by the indented body.
///
/// This is docutils' rule for a directive taking no argument: text after the
/// `::` is the content's first line, and lines indented below it continue it
/// — `.. seealso:: a` over an indented `b` is the one paragraph "a b". The two
/// start at different columns, which is why the context *hangs* its first
/// line rather than shifting every line alike.
///
/// With no `first_line`, this is the body exactly as
/// [`DirectiveBody::for_directive`] hands it over, keeping the blank line that
/// tells an option scan there are no options.
pub(in crate::directives) fn directive_content<'c>(
    first_line: &MarkerText<'_>,
    marker_index: usize,
    body: &DirectiveBody<'_>,
    ctx: &ParseCtx<'c>,
) -> DirectiveContent<'c> {
    let indent = body_indent(&body.lines);
    if first_line.text.is_empty() {
        let (lines, offset) = body.for_directive();
        return DirectiveContent {
            lines: unindent_body_lines(&lines),
            ctx: ctx.nested(marker_index + 1 + offset, indent),
        };
    }
    let mut lines = vec![first_line.text.to_string()];
    lines.extend(std::iter::repeat_n(String::new(), body.first_line_offset));
    lines.extend(unindent_body_lines(&body.lines));
    DirectiveContent {
        lines,
        ctx: ctx.hanging(marker_index, first_line.column, indent),
    }
}

/// Flattens a directive body back to plain text, one line per source line
/// with each line's own leading whitespace dropped.
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_for_directive_hands_over_a_body_directly_below_the_marker_unchanged() {
        // Given
        let lines = vec![".. code-block:: rst", "   :linenos:", "", "   x"];
        let body = collect_directive_body(&lines, 1, 0);

        // When
        let (body_lines, offset) = body.for_directive();

        // Then
        assert_eq!(body_lines, ["   :linenos:", "", "   x"]);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_for_directive_keeps_one_blank_line_ahead_of_a_body_that_follows_one() {
        // Given — two blank lines between the marker and the content
        let lines = vec![".. code-block:: rst", "", "", "   :ref:`label`"];
        let body = collect_directive_body(&lines, 1, 0);

        // When
        let (body_lines, offset) = body.for_directive();

        // Then — one blank line kept, and the offset now points at it, so
        // `offset + index` still names each line's place below the marker
        assert_eq!(body_lines, ["", "   :ref:`label`"]);
        assert_eq!(offset, 1);
    }

    #[test]
    fn test_collect_directive_body_collects_indented_lines() {
        // Given
        let lines = vec![".. note::", "   body1", "   body2"];
        // When
        let body_result = collect_directive_body(&lines, 1, 0);
        let (consumed, body) = (body_result.consumed, body_result.lines);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body1", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_stops_at_unindented_line() {
        // Given
        let lines = vec![".. note::", "   body1", "unindented", "   body2"];
        // When
        let body_result = collect_directive_body(&lines, 1, 0);
        let (consumed, body) = (body_result.consumed, body_result.lines);
        // Then
        assert_eq!(consumed, 1);
        assert_eq!(body, vec!["   body1"]);
    }

    #[test]
    fn test_collect_directive_body_strips_leading_and_trailing_blank_lines() {
        // Given
        let lines = vec![".. note::", "  ", "   body1", "  ", "   body2", "   ", ""];
        // When
        let body_result = collect_directive_body(&lines, 1, 0);
        let (consumed, body) = (body_result.consumed, body_result.lines);
        // Then
        assert_eq!(consumed, 6);
        assert_eq!(body, vec!["   body1", "", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_returns_empty_when_no_body() {
        // Given
        let lines = vec![".. note::", "unindented"];
        // When
        let body_result = collect_directive_body(&lines, 1, 0);
        let (consumed, body) = (body_result.consumed, body_result.lines);
        // Then
        assert_eq!(consumed, 0);
        assert!(body.is_empty());
    }

    #[test]
    fn test_collect_directive_body_returns_correct_consumed_count() {
        // Given
        let lines = vec![".. note::", "   body", "  "];
        // When
        let body_result = collect_directive_body(&lines, 1, 0);
        let (consumed, body) = (body_result.consumed, body_result.lines);
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
        let body_result = collect_directive_body(&lines, 1, 3);
        let (consumed, body) = (body_result.consumed, body_result.lines);
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
        let body_result = collect_directive_body(&lines, 1, 3);
        let (consumed, body) = (body_result.consumed, body_result.lines);
        // Then — only the deeper-indented line is included
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["      Actual body."]);
    }

    #[test]
    fn test_body_span_measures_the_line_without_its_indent() {
        // Given a body indented by three, in a context already shifted by that
        // indent — the shape every directive body parser is called with
        let body_lines = vec!["   Apple, Red"];
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py).nested(0, 3);

        // When
        let span = body_span(&body_lines, &ctx).expect("a placed context yields a span");

        // Then the span covers columns 4..14 — the ten characters of
        // "Apple, Red" itself. Measuring the raw line would count its three
        // spaces too, putting the end three columns past the line's end.
        assert_eq!(span.start.column, 4);
        assert_eq!(span.end.column, 14);
    }

    #[test]
    fn test_body_span_takes_the_indent_from_the_first_non_blank_line() {
        // Given a body opening with a blank line
        let body_lines = vec!["", "   a = b"];
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py).nested(0, 3);

        // When
        let span = body_span(&body_lines, &ctx).expect("a placed context yields a span");

        // Then the blank first line contributes no width, and the indent still
        // came from the line below it
        assert_eq!(span.start.column, 4);
        assert_eq!(span.end.column, 4);
    }

    #[test]
    fn test_body_span_handles_an_empty_body() {
        // Given no body at all, as in a directive whose content is its argument
        let body_lines: Vec<&str> = Vec::new();
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py).nested(0, 0);

        // When
        let span = body_span(&body_lines, &ctx).expect("a placed context yields a span");

        // Then it degenerates to a zero-width span rather than panicking
        assert_eq!(span.start, span.end);
    }

    // --- marker text and directive content ---

    #[test]
    fn test_after_marker_finds_the_text_and_its_column() {
        // Given
        let line = "   .. seealso::  Plain text  ";

        // When
        let marker = MarkerText::after_marker(line);

        // Then
        assert_eq!(marker.text, "Plain text");
        assert_eq!(marker.column, 17);
    }

    #[test]
    fn test_after_marker_counts_characters_not_bytes() {
        // Given a marker line with a multi-byte character before the text
        let line = ".. é:: text";

        // When
        let marker = MarkerText::after_marker(line);

        // Then
        assert_eq!(marker.column, 7);
    }

    #[test]
    fn test_after_marker_without_text_is_empty() {
        // Given / When
        let marker = MarkerText::after_marker(".. seealso::");

        // Then
        assert_eq!(marker.text, "");
    }

    #[test]
    fn test_split_first_word_moves_the_column_past_the_word() {
        // Given
        let marker = MarkerText::after_marker(".. versionchanged:: 3.1   Some text");

        // When
        let (word, rest) = marker.split_first_word();

        // Then
        assert_eq!(word, "3.1");
        assert_eq!(rest.text, "Some text");
        assert_eq!(rest.column, 26);
    }

    #[test]
    fn test_split_first_word_of_a_single_word_leaves_nothing() {
        // Given
        let marker = MarkerText::after_marker(".. versionchanged:: 3.1");

        // When
        let (word, rest) = marker.split_first_word();

        // Then
        assert_eq!(word, "3.1");
        assert_eq!(rest.text, "");
    }

    #[test]
    fn test_directive_content_of_marker_text_alone() {
        // Given
        let lines = vec![".. seealso:: Plain text"];
        let body = collect_directive_body(&lines, 1, 0);
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py);

        // When
        let content = directive_content(&MarkerText::after_marker(lines[0]), 0, &body, &ctx);

        // Then
        assert_eq!(content.lines, ["Plain text"]);
        let point = content.ctx.position(0, 0).expect("positioned");
        assert_eq!(point.position, rinx_ast::Position::new(1, 14));
    }

    #[test]
    fn test_directive_content_continues_marker_text_with_the_body() {
        // Given
        let lines = vec![".. seealso:: First", "   second"];
        let body = collect_directive_body(&lines, 1, 0);
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py);

        // When
        let content = directive_content(&MarkerText::after_marker(lines[0]), 0, &body, &ctx);

        // Then
        assert_eq!(content.lines, ["First", "second"]);
        let point = content.ctx.position(1, 0).expect("positioned");
        assert_eq!(point.position, rinx_ast::Position::new(2, 4));
    }

    #[test]
    fn test_directive_content_keeps_the_blank_lines_before_the_body() {
        // Given
        let lines = vec!["Intro", "", ".. note:: First", "", "", "   Body"];
        let body = collect_directive_body(&lines, 3, 0);
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py);

        // When
        let content = directive_content(&MarkerText::after_marker(lines[2]), 2, &body, &ctx);

        // Then
        assert_eq!(content.lines, ["First", "", "", "Body"]);
        let point = content.ctx.position(3, 0).expect("positioned");
        assert_eq!(point.position, rinx_ast::Position::new(6, 4));
    }

    #[test]
    fn test_directive_content_without_marker_text_is_the_body() {
        // Given
        let lines = vec![".. seealso::", "", "   Body"];
        let body = collect_directive_body(&lines, 1, 0);
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py);

        // When
        let content = directive_content(&MarkerText::NONE, 0, &body, &ctx);

        // Then the blank line marking "no options" is kept
        assert_eq!(content.lines, ["", "Body"]);
        let point = content.ctx.position(1, 0).expect("positioned");
        assert_eq!(point.position, rinx_ast::Position::new(3, 4));
    }

    #[test]
    fn test_directive_content_does_not_panic_on_a_short_multi_byte_line() {
        // Given a body whose second line is less indented than the first and
        // holds a multi-byte character where byte slicing would have panicked
        let lines = vec![".. seealso::", "   First line normal indent.", "  éfoo"];
        let body = collect_directive_body(&lines, 1, 0);
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py);

        // When
        let content = directive_content(&MarkerText::NONE, 0, &body, &ctx);

        // Then it does not panic, and keeps both lines
        assert_eq!(content.lines.len(), 2);
    }

    #[test]
    fn test_body_indent_is_the_first_non_blank_lines_indent() {
        // Given / When / Then
        assert_eq!(body_indent(&["", "    x", "  y"]), 4);
        assert_eq!(body_indent(&[]), 0);
    }

    #[test]
    fn test_directive_content_parse_joins_marker_text_and_continuation() {
        // Given
        let lines = vec![".. seealso:: First", "   second"];
        let body = collect_directive_body(&lines, 1, 0);
        let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py);
        let content = directive_content(&MarkerText::after_marker(lines[0]), 0, &body, &ctx);

        // When
        let nodes = content.parse(&mut Vec::new(), &mut Diagnostics::default());

        // Then
        assert_eq!(nodes.len(), 1);
        let Node::Paragraph(inlines) = &nodes[0] else {
            panic!("Expected a paragraph, got {:?}", nodes[0]);
        };
        assert_eq!(
            inlines,
            &[rinx_ast::InlineNode::Text("First\nsecond".to_string())]
        );
    }
}
