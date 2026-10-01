//! The `:name: value` option lines any directive may open its body with, and
//! the diagnostic for the ones nobody claimed.
//!
//! Nothing here knows what any particular option *means*: [`scan_option_lines`]
//! splits the leading run of `:name: value` lines off a body without judging
//! them, and [`report_unknown_options`] turns whatever the directive's own
//! parser handed back into diagnostics. Interpretation belongs to the
//! directive — [`super::table_options`] layers the five presentation options
//! the table directives share on top of this, and `super::math` reads
//! `:label:`/`:nowrap:`/`:class:` off the same scan.
//!
//! Split out of `table_options` once `.. math::` needed the scanning without
//! any of the table options, so the generic half stops being named after one
//! caller's construct.

use std::num::NonZeroUsize;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rinx_ast::{Diagnostic, DiagnosticCode};

/// One `:name: value` line, with the source text kept so a diagnostic can
/// quote the line exactly as the author wrote it.
pub(in crate::directives) struct OptionLine {
    pub name: String,
    pub value: String,
    pub raw: String,
    /// This line's index within the directive body, so a diagnostic about the
    /// option can point at the line the author wrote rather than at the
    /// directive as a whole.
    pub line_index: usize,
}

/// Splits the leading `:option:` lines off an already-unindented directive
/// body, returning them plus the index of the first line that isn't an option
/// (where the real body content starts). The option block is the run of lines
/// directly below the directive and ends at the first blank line.
///
/// A line whose closing colon is missing (`:oops`) is still returned, with the
/// whole remainder as its `name`, so that it reaches the caller's
/// unknown-option diagnostic rather than being silently swallowed as body.
///
/// An option's value may run over several lines: docutils parses a directive's
/// option block as a *field list*, and a field body continues on any following
/// line indented past the field marker. Since these lines arrive already
/// unindented, an option starts at column 0 and a continuation is simply a
/// line that is indented at all. Continuations are joined with a single space,
/// which is what a field body's text does when it is normalized — `CPython`'s
/// `pathlib.rst` wraps one `:alt:` across five lines, and without this the
/// last four would be read as directive content.
pub(in crate::directives) fn scan_option_lines(
    unindented_lines: &[String],
) -> (Vec<OptionLine>, usize) {
    let mut options: Vec<OptionLine> = Vec::new();
    let mut index = 0;
    while index < unindented_lines.len() {
        let raw_line = &unindented_lines[index];
        let line = raw_line.trim();
        // A blank line ends the option block for good: docutils reads options
        // only from the lines directly below the directive, so everything
        // after one is content — indented text (how a `.. code-block::
        // :dedent:` writes its first line) and `:role:`-looking lines (how a
        // code block showing roles writes its) alike.
        if line.is_empty() {
            while unindented_lines
                .get(index)
                .is_some_and(|blank| blank.trim().is_empty())
            {
                index += 1;
            }
            break;
        }
        // An indented line continues the option above it; with no option
        // above it, it is body content and the option block is over.
        if raw_line.starts_with(char::is_whitespace) {
            let Some(previous) = options.last_mut() else {
                break;
            };
            if !previous.value.is_empty() {
                previous.value.push(' ');
            }
            previous.value.push_str(line);
            previous.raw.push(' ');
            previous.raw.push_str(line);
            index += 1;
            continue;
        }
        let Some(rest) = line.strip_prefix(':') else {
            break;
        };
        let (name, value) = match rest.split_once(':') {
            Some((name, value)) => (name, value.trim()),
            None => (rest, ""),
        };
        options.push(OptionLine {
            name: name.to_string(),
            value: value.to_string(),
            raw: line.to_string(),
            line_index: index,
        });
        index += 1;
    }
    (options, index)
}

/// Whether `line` opens with a field marker — `:name:` followed by whitespace
/// or the end of the line — by docutils' own `field_marker` pattern.
///
/// Stricter than [`scan_option_lines`]' leading-colon test, and needed where
/// content and options can share a block: `:pep:`634` -- text` starts with a
/// colon, but the backtick after the second one makes it a role, not a field.
pub(in crate::directives) fn is_field_marker(line: &str) -> bool {
    let Some(rest) = line.strip_prefix(':') else {
        return false;
    };
    let mut chars = rest.chars().peekable();
    if matches!(chars.peek(), None | Some(':' | ' ')) {
        return false;
    }
    let mut previous = ' ';
    while let Some(current) = chars.next() {
        match current {
            '\\' => {
                // An escaped character is part of the name, whatever it is.
                chars.next();
            }
            ':' => match chars.peek() {
                None | Some(' ' | '\t') => return previous != ' ',
                Some('`') => return false,
                Some(_) => {}
            },
            _ => {}
        }
        previous = current;
    }
    false
}

/// Takes the options out of a directive's unindented `content`, as docutils
/// does for a directive whose content may begin on its marker line.
///
/// The options are read from the content's first block — the lines before
/// its first blank one — starting at the first [field marker](is_field_marker)
/// in it, so that `.. note:: text` over an indented `:collapsible:` keeps the
/// text and still reads the option. The option lines are blanked rather than
/// removed: every line keeps its index, so every position below them stays
/// right, and a blank line where the options stood ends a paragraph exactly
/// where docutils' spliced content would.
pub(in crate::directives) fn take_option_block(content: &mut [String]) -> Vec<OptionLine> {
    let block_end = content
        .iter()
        .position(|line| line.trim().is_empty())
        .unwrap_or(content.len());
    let Some(start) = content[..block_end]
        .iter()
        .position(|line| is_field_marker(line))
    else {
        return Vec::new();
    };
    let (mut options, consumed) = scan_option_lines(&content[start..]);
    for option in &mut options {
        option.line_index += start;
    }
    for line in &mut content[start..start + consumed] {
        line.clear();
    }
    options
}

/// Reports every option line neither a shared parser nor the directive's own
/// parser claimed.
///
/// `code` is the caller's, rather than always [`DiagnosticCode::DirectiveUnknownOption`],
/// because `.. toctree::` reported its unknown options under
/// `directive.toctree-unknown-option` long before this helper existed. A code
/// is a stable, author-facing name a `.. noqa:` can spell, so keeping that one
/// is not a style preference — renaming it would silently break every document
/// already suppressing it.
pub(in crate::directives) fn report_unknown_options(
    unrecognized: &[&OptionLine],
    directive: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for line in unrecognized {
        diagnostics.push(Diagnostic::at(
            code,
            format!(
                "Invalid or non-standard Sphinx {directive} option encountered: {}",
                line.raw
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
    }
}

/// Reads an option whose value is a depth limit: a positive integer limits
/// the depth, and Sphinx's documented "unlimited" spelling — zero or a
/// negative number — clears it rather than being treated as a real limit.
///
/// Shared by `.. toctree::`'s `:maxdepth:` and `.. contents::`'s `:depth:`,
/// which agree on this exact ambiguity; `directive` and `option` name the
/// caller's own directive and option spelling for the diagnostic message, and
/// `code` lets each caller keep its own stable, `.. noqa:`-suppressible code.
pub(in crate::directives) fn parse_positive_depth(
    line: &OptionLine,
    directive: &str,
    option: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<NonZeroUsize> {
    match line.value.trim().parse::<isize>() {
        Ok(value) if value <= 0 => None,
        Ok(value) => NonZeroUsize::new(usize::try_from(value).unwrap_or(0)),
        Err(_) => {
            diagnostics.push(Diagnostic::at(
                code,
                format!(
                    "A {directive} :{option}: option needs a whole number: {}",
                    line.raw
                ),
                ctx.line_span(line.line_index, &line.raw),
            ));
            None
        }
    }
}

/// Reads an option whose value is a percentage — `.. image::`'s `:scale:`
/// and a diagram's alike.
///
/// docutils' `directives.percentage` strips one trailing `%` and then demands
/// a non-negative integer, so `50` and `50%` are the same option and `-50` is
/// no option at all. Shared for the reason [`parse_positive_depth`] is: two
/// directives agreeing on one ambiguity should not each own a copy of the
/// answer.
pub(in crate::directives) fn parse_percentage(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    let number = trimmed.strip_suffix('%').unwrap_or(trimmed).trim();
    number.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Domain;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_scan_option_lines_splits_name_and_value() {
        // Given
        let body = lines(&[":header-rows: 1", ":widths: 30 70", "", "* - Cell"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].name, "header-rows");
        assert_eq!(options[0].value, "1");
        assert_eq!(options[1].name, "widths");
        assert_eq!(options[1].value, "30 70");
        assert_eq!(body_start, 3);
    }

    #[test]
    fn test_scan_option_lines_accepts_a_value_with_no_space_after_the_colon() {
        // Given
        let body = lines(&[":header-rows:1"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].name, "header-rows");
        assert_eq!(options[0].value, "1");
    }

    #[test]
    fn test_scan_option_lines_treats_a_flag_option_as_an_empty_value() {
        // Given
        let body = lines(&[":keepspace:"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].name, "keepspace");
        assert_eq!(options[0].value, "");
    }

    #[test]
    fn test_scan_option_lines_keeps_an_unterminated_option_line() {
        // Given
        let body = lines(&[":oops"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].name, "oops");
        assert_eq!(options[0].raw, ":oops");
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_stops_at_the_first_body_line() {
        // Given
        let body = lines(&[":widths: auto", "Apple, Red", ":not-an-option: x"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_joins_a_wrapped_value() {
        // Given — CPython's `pathlib.rst` writes an `:alt:` this way
        let body = lines(&[
            ":alt: Inheritance diagram showing",
            "      the pathlib classes",
        ]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(
            options[0].value,
            "Inheritance diagram showing the pathlib classes"
        );
        assert_eq!(body_start, 2);
    }

    #[test]
    fn test_scan_option_lines_joins_a_value_wrapped_over_several_lines() {
        // Given
        let body = lines(&[":alt: one", "      two", "      three"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].value, "one two three");
    }

    #[test]
    fn test_scan_option_lines_continues_a_value_that_started_empty() {
        // Given — the whole value written on the wrapped line
        let body = lines(&[":alt:", "      the only text"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then — no leading space from joining onto nothing
        assert_eq!(options[0].value, "the only text");
    }

    #[test]
    fn test_scan_option_lines_ends_the_option_block_at_a_blank_line() {
        // Given — a blank line ends the field body, so the indented line below
        // it is directive content (a `.. code-block:: :dedent:` writes this)
        let body = lines(&[":dedent: 2", "", "     x = 1"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].value, "2");
        assert_eq!(body_start, 2);
    }

    #[test]
    fn test_scan_option_lines_reads_no_option_after_the_blank_line_opening_the_content() {
        // Given — docutils reads options only from the lines directly below
        // the directive, so after a blank line a `:role:`-looking line is
        // content: a code block showing roles writes exactly this
        let body = lines(&["", ":ref:`label`", ":term:`word`"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert!(options.is_empty());
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_reads_no_option_after_the_blank_line_ending_the_options() {
        // Given
        let body = lines(&[":caption: Roles", "", ":ref:`label`"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].name, "caption");
        assert_eq!(body_start, 2);
    }

    #[test]
    fn test_scan_option_lines_stops_at_an_indented_line_with_no_option_above_it() {
        // Given
        let body = lines(&["   indented body content"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert!(options.is_empty());
        assert_eq!(body_start, 0);
    }

    #[test]
    fn test_scan_option_lines_quotes_the_whole_wrapped_line_in_raw() {
        // Given — `raw` is what an unknown-option diagnostic echoes back
        let body = lines(&[":bogus: one", "      two"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].raw, ":bogus: one two");
    }

    #[test]
    fn test_scan_option_lines_returns_nothing_for_an_empty_body() {
        // Given
        let body: Vec<String> = Vec::new();

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert!(options.is_empty());
        assert_eq!(body_start, 0);
    }

    #[test]
    fn test_report_unknown_options_quotes_the_source_line() {
        // Given
        let body = lines(&[":bogus: value"]);
        let (option_lines, _) = scan_option_lines(&body);
        let unrecognized: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        report_unknown_options(
            &unrecognized,
            "list-table",
            DiagnosticCode::DirectiveUnknownOption,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains(
                "Invalid or non-standard Sphinx list-table option encountered: :bogus: value"
            ),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_report_unknown_options_uses_the_code_the_caller_passed() {
        // Given — a caller that does not use the generic directive code.
        let body = lines(&[":bogus:"]);
        let (option_lines, _) = scan_option_lines(&body);
        let unrecognized: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        report_unknown_options(
            &unrecognized,
            "toctree",
            DiagnosticCode::DirectiveToctreeUnknownOption,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code,
            DiagnosticCode::DirectiveToctreeUnknownOption
        );
    }

    fn option_line(name: &str, value: &str) -> OptionLine {
        OptionLine {
            name: name.to_string(),
            value: value.to_string(),
            raw: format!(":{name}: {value}"),
            line_index: 0,
        }
    }

    #[test]
    fn test_parse_positive_depth_reads_a_positive_value() {
        // Given
        let line = option_line("depth", "2");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "contents",
            "depth",
            DiagnosticCode::ContentsDepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, NonZeroUsize::new(2));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_positive_depth_treats_zero_as_unlimited() {
        // Given
        let line = option_line("depth", "0");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "contents",
            "depth",
            DiagnosticCode::ContentsDepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_positive_depth_treats_a_negative_value_as_unlimited() {
        // Given — Sphinx's documented spelling of "no limit".
        let line = option_line("maxdepth", "-1");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "toctree",
            "maxdepth",
            DiagnosticCode::ToctreeMaxdepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_positive_depth_diagnoses_a_non_numeric_value_under_the_callers_code() {
        // Given
        let line = option_line("depth", "deep");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "contents",
            "depth",
            DiagnosticCode::ContentsDepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::ContentsDepthInvalid
        );
        assert!(diagnostics.entries()[0].message.contains(":depth:"));
    }

    #[test]
    fn test_parse_percentage_reads_a_bare_integer() {
        // Given / When / Then
        assert_eq!(parse_percentage("50"), Some(50));
    }

    #[test]
    fn test_parse_percentage_strips_one_trailing_percent_sign() {
        // Given — docutils' `directives.percentage` accepts both spellings of
        // the same option
        assert_eq!(parse_percentage("50%"), parse_percentage("50"));
    }

    #[test]
    fn test_parse_percentage_ignores_surrounding_whitespace() {
        // Given / When / Then
        assert_eq!(parse_percentage("  75 % "), Some(75));
    }

    #[test]
    fn test_parse_percentage_accepts_zero() {
        // Given — zero is a real percentage, unlike a depth limit's zero
        assert_eq!(parse_percentage("0"), Some(0));
    }

    #[test]
    fn test_parse_percentage_refuses_a_negative_number() {
        // Given — docutils demands a non-negative integer
        assert_eq!(parse_percentage("-50"), None);
    }

    #[test]
    fn test_parse_percentage_refuses_a_fraction() {
        // Given / When / Then
        assert_eq!(parse_percentage("50.5"), None);
    }

    #[test]
    fn test_parse_percentage_refuses_text() {
        // Given / When / Then
        assert_eq!(parse_percentage("half"), None);
        assert_eq!(parse_percentage(""), None);
        assert_eq!(parse_percentage("%"), None);
    }

    #[test]
    fn test_is_field_marker_accepts_an_option() {
        // Given / When / Then
        assert!(is_field_marker(":collapsible:"));
        assert!(is_field_marker(":collapsible: open"));
        assert!(is_field_marker(":class: a b"));
    }

    #[test]
    fn test_is_field_marker_refuses_a_role() {
        // Given / When / Then
        assert!(!is_field_marker(
            ":pep:`634` -- Structural Pattern Matching"
        ));
        assert!(!is_field_marker(":ref:`label`"));
    }

    #[test]
    fn test_is_field_marker_refuses_text_and_malformed_markers() {
        // Given / When / Then
        assert!(!is_field_marker("Plain text"));
        assert!(!is_field_marker(":: x"));
        assert!(!is_field_marker(": x:"));
        assert!(!is_field_marker(":name :"));
        assert!(!is_field_marker(":unclosed"));
    }

    #[test]
    fn test_is_field_marker_accepts_an_escaped_colon_in_the_name() {
        // Given / When / Then
        assert!(is_field_marker(r":a\:b: value"));
    }

    #[test]
    fn test_take_option_block_reads_options_below_leading_text() {
        // Given
        let mut content = vec![
            "Some text".to_string(),
            ":collapsible: open".to_string(),
            String::new(),
            "Body".to_string(),
        ];

        // When
        let options = take_option_block(&mut content);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].name, "collapsible");
        assert_eq!(options[0].line_index, 1);
        assert_eq!(content, ["Some text", "", "", "Body"]);
    }

    #[test]
    fn test_take_option_block_keeps_a_role_as_content() {
        // Given
        let mut content = vec![":pep:`634` -- Specification".to_string()];

        // When
        let options = take_option_block(&mut content);

        // Then
        assert!(options.is_empty());
        assert_eq!(content, [":pep:`634` -- Specification"]);
    }

    #[test]
    fn test_take_option_block_ignores_fields_after_the_first_blank_line() {
        // Given
        let mut content = vec![String::new(), ":collapsible:".to_string()];

        // When
        let options = take_option_block(&mut content);

        // Then
        assert!(options.is_empty());
        assert_eq!(content, ["", ":collapsible:"]);
    }
}
