//! Reading a reStructuredText directive's block into a
//! [`DirectiveInvocation`], by docutils' own rule (`RSTState.
//! parse_directive_block` and `parse_directive_arguments`) over the
//! directive's [`DirectiveShape`].
//!
//! The block is the text after the marker's `::` and the indented lines below
//! it. When the directive takes arguments or options, its first lines up to a
//! blank one are the *argument block*, and the option lines — from the first
//! field marker in it — are taken off its end; a directive taking no argument
//! reads whatever text is left there as the start of its content. The
//! argument block then splits on whitespace into the arguments.
//!
//! Where docutils refuses the whole directive — a missing argument, surplus
//! words, content it takes none of — this build reports the mistake under one
//! shared code and reads the directive anyway, from what was written.
//!
//! One leniency is this build's own: option lines followed, before any blank
//! line, by text that is no option end the options there, and the text starts
//! the content. docutils refuses such a block as an invalid option block.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::{indent_width, strip_indent};
use rinx_ast::{Diagnostic, DiagnosticCode};

use super::body::{DirectiveBody, MarkerLine};
use super::invocation::DirectiveInvocation;
use super::options::{is_field_marker, scan_option_lines};
use super::spec::{Arguments, DirectiveShape};

/// The directive written as `name` on `marker`, with `body` below it, read by
/// `shape`.
///
/// `source_body` is the body as written, for a node quoting it back; `ctx` is
/// the context the marker line was read in.
pub(in crate::directives) fn read_directive_block<'a, 'c>(
    name: &'a str,
    shape: DirectiveShape,
    marker: &MarkerLine<'_>,
    body: &DirectiveBody<'_>,
    source_body: &'a [&'a str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'c>,
) -> DirectiveInvocation<'a, 'c> {
    let indent = shared_indent(&body.lines);
    let lines = block_lines(marker, body, indent);
    let block_ctx = ctx
        .hanging(marker.index, marker.argument.column, indent)
        .without_section_titles();
    let split = split_block(&lines, shape);
    let arguments = read_arguments(
        &lines[split.arguments.clone()].join("\n"),
        shape.arguments,
        name,
        marker,
        diagnostics,
    );
    if !shape.takes_content {
        let refused = RefusedContent {
            directive: name,
            hint: shape.content_hint,
        };
        report_content(
            &lines,
            split.content_start,
            &refused,
            &block_ctx,
            diagnostics,
        );
    }
    DirectiveInvocation {
        name,
        span: marker.span,
        arguments,
        argument_lines: split.arguments,
        options: split.options,
        lines,
        ctx: block_ctx,
        content_start: split.content_start,
        body_line: body.first_line_offset.max(1),
        option_lines: split.option_lines,
        enclosing_titles: ctx.section_titles(),
        source_body,
    }
}

/// The block's lines in its own index space: the text after the `::` first,
/// then one line per source line below the marker, with `indent` characters
/// stripped.
fn block_lines(marker: &MarkerLine<'_>, body: &DirectiveBody<'_>, indent: usize) -> Vec<String> {
    let mut lines = vec![marker.argument.text.to_string()];
    lines.extend(std::iter::repeat_n(String::new(), body.first_line_offset));
    lines.extend(body.lines.iter().map(|line| {
        if line.trim().is_empty() {
            String::new()
        } else {
            strip_indent(line, indent).to_string()
        }
    }));
    lines
}

/// The indentation every non-blank line of `lines` shares — docutils strips
/// the smallest one from a directive's block, so lines indented deeper, such
/// as further signatures aligned under the first, keep the difference.
fn shared_indent(lines: &[&str]) -> usize {
    lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| indent_width(line))
        .min()
        .unwrap_or(0)
}

/// Where a block's arguments, options and content lie.
struct BlockSplit {
    /// The lines holding the arguments.
    arguments: std::ops::Range<usize>,
    /// The options, in block index space.
    options: Vec<super::options::OptionLine>,
    /// The lines the options were read from.
    option_lines: std::ops::Range<usize>,
    /// The content's first line.
    content_start: usize,
}

/// Splits `lines` by `shape`, as `parse_directive_block` does.
fn split_block(lines: &[String], shape: DirectiveShape) -> BlockSplit {
    // An empty marker line is no part of the block.
    let start = usize::from(lines.first().is_some_and(|line| line.trim().is_empty()));
    let takes_arguments = shape.arguments.takes_any();
    let argument_block_end = if takes_arguments || shape.takes_options {
        lines[start..]
            .iter()
            .position(|line| line.trim().is_empty())
            .map_or(lines.len(), |offset| start + offset)
    } else {
        start
    };

    let (arguments_end, options, option_lines) = if shape.takes_options {
        read_option_lines(lines, start, argument_block_end)
    } else {
        (
            argument_block_end,
            Vec::new(),
            argument_block_end..argument_block_end,
        )
    };

    let content_start = if !takes_arguments && arguments_end > start {
        // Text left in the argument block of a directive taking no argument
        // is the start of its content.
        start
    } else if option_lines.end < argument_block_end {
        // Text directly below the options, before any blank line.
        option_lines.end
    } else {
        argument_block_end
    };
    let content_start = skip_blank_lines(lines, content_start);
    let arguments = if takes_arguments {
        start..arguments_end
    } else {
        start..start
    };
    BlockSplit {
        arguments,
        options,
        option_lines,
        content_start,
    }
}

/// The options in `lines[start..end]`, from its first field marker on: where
/// the arguments end, the options, and the lines they were read from.
fn read_option_lines(
    lines: &[String],
    start: usize,
    end: usize,
) -> (
    usize,
    Vec<super::options::OptionLine>,
    std::ops::Range<usize>,
) {
    let Some(first) = (start..end).find(|&index| is_field_marker(&lines[index])) else {
        return (end, Vec::new(), end..end);
    };
    let (mut options, consumed) = scan_option_lines(&lines[first..end]);
    for option in &mut options {
        option.line_index += first;
    }
    (first, options, first..first + consumed)
}

/// The first line at or after `index` that is not blank.
fn skip_blank_lines(lines: &[String], index: usize) -> usize {
    lines[index.min(lines.len())..]
        .iter()
        .position(|line| !line.trim().is_empty())
        .map_or(lines.len(), |offset| index + offset)
}

/// Splits `text` into `shape`'s arguments, as `parse_directive_arguments`
/// does, reporting too few or too many.
fn read_arguments(
    text: &str,
    shape: Arguments,
    name: &str,
    marker: &MarkerLine<'_>,
    diagnostics: &mut Diagnostics,
) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let accepted = shape.required + shape.optional;
    if words.len() < shape.required {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveMissingArgument,
            format!(
                "{name}: {} argument(s) required, {} supplied",
                shape.required,
                words.len()
            ),
            marker.span,
        ));
    }
    if words.len() <= accepted {
        return words.into_iter().map(str::to_string).collect();
    }
    if shape.final_whitespace {
        return split_keeping_rest(text, accepted);
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveTooManyArguments,
        format!(
            "{name}: maximum {accepted} argument(s) allowed, {} supplied; ignoring '{}'",
            words.len(),
            words[accepted..].join(" ")
        ),
        marker.span,
    ));
    words[..accepted]
        .iter()
        .map(|word| (*word).to_string())
        .collect()
}

/// `text` split into `count` arguments, the last keeping the rest — Python's
/// `text.split(None, count - 1)`.
fn split_keeping_rest(text: &str, count: usize) -> Vec<String> {
    let mut arguments = Vec::with_capacity(count);
    let mut rest = text.trim_start();
    while arguments.len() + 1 < count {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        arguments.push(rest[..end].to_string());
        rest = rest[end..].trim_start();
    }
    arguments.push(rest.trim_end().to_string());
    arguments
}

/// A directive refusing content, and what it suggests instead.
struct RefusedContent<'a> {
    /// The name, as written.
    directive: &'a str,
    /// What to write instead.
    hint: Option<&'a str>,
}

/// Reports the content of a directive taking none, from `start` on.
fn report_content(
    lines: &[String],
    start: usize,
    refused: &RefusedContent<'_>,
    ctx: &ParseCtx<'_>,
    diagnostics: &mut Diagnostics,
) {
    let Some(line) = lines.get(start).filter(|line| !line.trim().is_empty()) else {
        return;
    };
    let name = refused.directive;
    let refusal = if start == 0 {
        format!("{name}: takes no argument or content, but '{line}' follows the '::'")
    } else {
        format!("{name}: takes no content")
    };
    let message = match refused.hint {
        Some(hint) => format!("{refusal}; {hint}"),
        None => refusal,
    };
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveContentNotPermitted,
        message,
        ctx.line_span(start, line),
    ));
}

/// A test's invocation of `name` with `argument` on its marker line and
/// `body_lines` below it, read by `shape` as the dispatcher would, under
/// `ctx` positioned at the marker line.
#[cfg(test)]
pub(in crate::directives) fn test_invocation<'a, 'c>(
    name: &'a str,
    shape: DirectiveShape,
    argument: &'a str,
    body_lines: &'a [&'a str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'c>,
) -> DirectiveInvocation<'a, 'c> {
    let first = body_lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(body_lines.len());
    let last = body_lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(first, |index| index + 1);
    let body = DirectiveBody {
        consumed: body_lines.len(),
        lines: body_lines[first..last].to_vec(),
        first_line_offset: first,
    };
    let marker = MarkerLine {
        index: 0,
        span: None,
        argument: super::body::MarkerText {
            text: argument.trim(),
            column: 0,
        },
    };
    read_directive_block(name, shape, &marker, &body, body_lines, diagnostics, ctx)
}

/// `body` as a document writes a directive's content below its marker: after
/// a blank line, unless it opens with an option line — so a test about a
/// directive's content need not restate the blank line docutils requires
/// before it.
#[cfg(test)]
pub(in crate::directives) fn written_after_blank<'a>(body: &[&'a str]) -> Vec<&'a str> {
    let opens_with_content = body
        .iter()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| !is_field_marker(line.trim()));
    if opens_with_content && body.first().is_some_and(|line| !line.trim().is_empty()) {
        std::iter::once("").chain(body.iter().copied()).collect()
    } else {
        body.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Domain, Position};

    /// The shape of a directive taking one argument keeping the rest of its
    /// line, options and content — an image with a legend, say.
    const TEXT_ARGUMENT: DirectiveShape = DirectiveShape {
        arguments: Arguments::ONE_TEXT,
        takes_options: true,
        takes_content: true,
        content_hint: None,
    };

    /// The shape of an admonition: no argument, options and content.
    const NO_ARGUMENT: DirectiveShape = DirectiveShape {
        arguments: Arguments::NONE,
        ..TEXT_ARGUMENT
    };

    /// What `.. x:: argument` over `body` reads as under `shape`: the
    /// arguments, option names, content lines and diagnostic codes.
    fn read(
        shape: DirectiveShape,
        argument: &str,
        body: &[&str],
    ) -> (Vec<String>, Vec<String>, Vec<String>, Vec<DiagnosticCode>) {
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = test_invocation("x", shape, argument, body, &mut diagnostics, &ctx);
        let options = invocation.options.iter().map(|o| o.name.clone()).collect();
        let content = invocation.content().lines;
        let codes = diagnostics.iter().map(|d| d.code).collect();
        (invocation.arguments, options, content, codes)
    }

    #[test]
    fn test_text_after_the_marker_starts_the_content_of_a_directive_taking_no_argument() {
        // Given / When — docutils' `.. note:: text`
        let (arguments, _, content, codes) = read(NO_ARGUMENT, "Mind the gap", &["   below it"]);

        // Then
        assert!(arguments.is_empty());
        assert_eq!(content, ["Mind the gap", "below it"]);
        assert!(codes.is_empty());
    }

    #[test]
    fn test_the_argument_block_ends_at_the_first_blank_line() {
        // Given / When
        let (arguments, _, content, _) =
            read(TEXT_ARGUMENT, "a long", &["   title", "", "   Content."]);

        // Then — the line below the marker continues the argument
        assert_eq!(arguments, ["a long\ntitle"]);
        assert_eq!(content, ["Content."]);
    }

    #[test]
    fn test_options_are_taken_off_the_end_of_the_argument_block() {
        // Given / When — a further signature, then options, then content
        let (arguments, options, content, _) = read(
            TEXT_ARGUMENT,
            "AF_UNIX",
            &["   AF_INET", "   :no-index:", "", "   Content."],
        );

        // Then
        assert_eq!(arguments, ["AF_UNIX\nAF_INET"]);
        assert_eq!(options, ["no-index"]);
        assert_eq!(content, ["Content."]);
    }

    #[test]
    fn test_text_directly_below_the_options_starts_the_content() {
        // Given / When — docutils refuses this as an invalid option block;
        // this build reads it as content
        let (_, options, content, codes) =
            read(TEXT_ARGUMENT, "python", &["   :linenos:", "   print(1)"]);

        // Then
        assert_eq!(options, ["linenos"]);
        assert_eq!(content, ["print(1)"]);
        assert!(codes.is_empty());
    }

    #[test]
    fn test_options_among_the_content_of_a_directive_taking_no_argument_are_blanked() {
        // Given / When — `.. note:: text` with an option below the text
        let (_, options, content, _) =
            read(NO_ARGUMENT, "Text", &["   :class: wide", "", "   More."]);

        // Then — every line keeps its index
        assert_eq!(options, ["class"]);
        assert_eq!(content, ["Text", "", "", "More."]);
    }

    #[test]
    fn test_a_role_opening_a_line_is_no_option() {
        // Given / When — the backtick makes `:pep:` a role, not a field
        let (_, options, content, _) = read(NO_ARGUMENT, "", &["   :pep:`8` matters"]);

        // Then
        assert!(options.is_empty());
        assert_eq!(content, [":pep:`8` matters"]);
    }

    #[test]
    fn test_a_missing_required_argument_is_reported() {
        // Given / When
        let (arguments, _, _, codes) = read(TEXT_ARGUMENT, "", &[]);

        // Then
        assert!(arguments.is_empty());
        assert_eq!(codes, [DiagnosticCode::DirectiveMissingArgument]);
    }

    #[test]
    fn test_surplus_words_are_reported_and_ignored_without_final_whitespace() {
        // Given
        let shape = DirectiveShape {
            arguments: Arguments::OPTIONAL_WORD,
            ..TEXT_ARGUMENT
        };

        // When
        let (arguments, _, _, codes) = read(shape, "python extra", &[]);

        // Then
        assert_eq!(arguments, ["python"]);
        assert_eq!(codes, [DiagnosticCode::DirectiveTooManyArguments]);
    }

    #[test]
    fn test_the_last_argument_keeps_the_rest_with_final_whitespace() {
        // Given
        let shape = DirectiveShape {
            arguments: Arguments {
                required: 1,
                optional: 1,
                final_whitespace: true,
            },
            ..TEXT_ARGUMENT
        };

        // When — a version change's version and explanation
        let (arguments, _, _, codes) = read(shape, "3.1 Some  text", &["   continued"]);

        // Then
        assert_eq!(arguments, ["3.1", "Some  text\ncontinued"]);
        assert!(codes.is_empty());
    }

    #[test]
    fn test_split_keeping_rest_splits_off_all_but_the_last_argument() {
        // Given / When / Then — Python's `str.split(None, 1)`
        assert_eq!(split_keeping_rest("  a b  c ", 2), ["a", "b  c"]);
        assert_eq!(split_keeping_rest("a", 1), ["a"]);
    }

    #[test]
    fn test_content_is_reported_for_a_directive_taking_none() {
        // Given
        let shape = DirectiveShape {
            takes_content: false,
            content_hint: Some("use figure for a caption"),
            ..TEXT_ARGUMENT
        };
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let body = ["", "   A caption."];

        // When
        test_invocation("image", shape, "logo.png", &body, &mut diagnostics, &ctx);

        // Then
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, DiagnosticCode::DirectiveContentNotPermitted);
        assert_eq!(
            found[0].message,
            "image: takes no content; use figure for a caption"
        );
    }

    #[test]
    fn test_text_after_the_marker_is_named_when_a_directive_takes_neither() {
        // Given
        let shape = DirectiveShape {
            arguments: Arguments::NONE,
            takes_content: false,
            ..TEXT_ARGUMENT
        };
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);

        // When
        test_invocation("sectnum", shape, "oops", &[], &mut diagnostics, &ctx);

        // Then
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(
            found[0].message,
            "sectnum: takes no argument or content, but 'oops' follows the '::'"
        );
    }

    #[test]
    fn test_a_short_multi_byte_line_does_not_panic() {
        // Given / When — a line shorter, in bytes, than the body's indent
        let (_, _, content, _) = read(NO_ARGUMENT, "", &["", "   First.", "  éfoo"]);

        // Then
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn test_options_and_content_are_positioned_where_they_were_written() {
        // Given — the marker on line 1, an option on line 2, content on 4
        let input = ".. note:: Text\n   :class: wide\n\n   .. bogus::\n";

        // When
        let document = crate::parse("index.rst", input);

        // Then — the unsupported option on its line, the unknown directive
        // on its
        let starts: Vec<Position> = document
            .diagnostics
            .iter()
            .filter_map(|d| d.span.map(|span| span.start))
            .collect();
        assert_eq!(starts, [Position::new(2, 4), Position::new(4, 4)]);
    }

    #[test]
    fn test_shared_indent_is_the_smallest_indentation_of_a_non_blank_line() {
        // Given / When / Then — further signatures aligned deeper than options
        assert_eq!(
            shared_indent(&["           AF_INET", "", "   :no-index:"]),
            3
        );
        assert_eq!(shared_indent(&["", ""]), 0);
    }

    #[test]
    fn test_options_below_deeper_signatures_are_read() {
        // Given / When
        let (arguments, options, _, _) = read(
            TEXT_ARGUMENT,
            "AF_UNIX",
            &["          AF_INET", "   :no-index:", "", "   Content."],
        );

        // Then — the signature keeps its relative indentation until trimmed
        assert_eq!(arguments, ["AF_UNIX\n       AF_INET"]);
        assert_eq!(options, ["no-index"]);
    }

    #[test]
    fn test_written_after_blank_adds_the_blank_line_content_needs() {
        // Given / When / Then
        assert_eq!(written_after_blank(&["   text"]), ["", "   text"]);
        assert_eq!(written_after_blank(&["   :opt:"]), ["   :opt:"]);
        assert_eq!(written_after_blank(&["", "   text"]), ["", "   text"]);
    }
}
