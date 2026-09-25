use std::num::NonZeroU32;

use rinx_ast::{Diagnostic, DiagnosticCode, Directive, SectnumOptions};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::indent::unindent_body_lines;

use super::options::{OptionLine, parse_positive_depth, report_unknown_options, scan_option_lines};

const DIRECTIVE: &str = "sectnum";

/// The name(s) `.. sectnum::` is spelled under. Docutils' own alias,
/// `.. section-numbering::`, shares this directive's implementation
/// completely — unlike `.. code-block::`/`.. code::`, the two spellings agree
/// on every option, so there is no need to record which one an author wrote.
pub(super) fn is_sectnum(name: &str) -> bool {
    name == "sectnum" || name == "section-numbering"
}

/// Reads a `:start:` value: a positive integer, or the diagnosed default of
/// `None` (docutils' own default of `1`) for anything else, `0` included —
/// unlike `:depth:`, there is no sensible number to display at `0`.
fn parse_start(
    line: &OptionLine,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<NonZeroU32> {
    if let Ok(value) = line.value.trim().parse::<NonZeroU32>() {
        return Some(value);
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::SectnumStartInvalid,
        format!(
            "A {DIRECTIVE} :start: option needs a positive integer: {}",
            line.raw
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
    None
}

/// Reads a `:prefix:`/`:suffix:` value, diagnosing an empty one — docutils
/// requires both options to carry a value.
fn parse_required_text(
    line: &OptionLine,
    option: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> String {
    if line.value.is_empty() {
        diagnostics.push(Diagnostic::at(
            code,
            format!(
                "A {DIRECTIVE} :{option}: option needs a value: {}",
                line.raw
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
        return String::new();
    }
    line.value.clone()
}

/// Reads the four options `.. sectnum::` accepts, returning them with the
/// lines it did not recognize, in source order.
fn parse_sectnum_options<'a>(
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (SectnumOptions, Vec<&'a OptionLine>) {
    let mut options = SectnumOptions::default();
    let mut unrecognized = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "depth" => {
                options.depth = parse_positive_depth(
                    line,
                    DIRECTIVE,
                    "depth",
                    DiagnosticCode::SectnumDepthInvalid,
                    diagnostics,
                    ctx,
                );
            }
            "start" => {
                options.start = parse_start(line, diagnostics, ctx);
            }
            "prefix" => {
                options.prefix = parse_required_text(
                    line,
                    "prefix",
                    DiagnosticCode::SectnumEmptyPrefix,
                    diagnostics,
                    ctx,
                );
            }
            "suffix" => {
                options.suffix = parse_required_text(
                    line,
                    "suffix",
                    DiagnosticCode::SectnumEmptySuffix,
                    diagnostics,
                    ctx,
                );
            }
            _ => unrecognized.push(line),
        }
    }

    (options, unrecognized)
}

/// Parses a `.. sectnum::`/`.. section-numbering::` body into a
/// [`Directive::Sectnum`]. Docutils declares zero argument slots for this
/// directive, and really does reject one given anyway — as an inline
/// directive error, not silently — so a non-empty `argument` is diagnosed
/// here too, the same way [`super::admonitions::parse_version_change`]
/// diagnoses a *missing* required argument against the same directive-marker
/// span.
pub(super) fn parse_sectnum(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    if !argument.trim().is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::SectnumUnexpectedArgument,
            format!("A {DIRECTIVE} directive takes no argument: '{argument}'"),
            body_span(body_lines, ctx),
        ));
    }

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, _body_start) = scan_option_lines(&unindented_lines);
    let (options, unrecognized) = parse_sectnum_options(&option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveSectnumUnknownOption,
        diagnostics,
        ctx,
    );

    Directive::Sectnum(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Domain, Node};
    use std::num::NonZeroUsize;

    /// Parses a directive body directly, bypassing the block dispatcher, for
    /// tests about `parse_sectnum` itself rather than about recognition.
    /// Takes no argument — see [`parse_body_with_argument`] for that.
    fn parse_body(body_lines: &[&str]) -> (SectnumOptions, Diagnostics) {
        parse_body_with_argument("", body_lines)
    }

    fn parse_body_with_argument(
        argument: &str,
        body_lines: &[&str],
    ) -> (SectnumOptions, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let directive = parse_sectnum(
            argument,
            body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        match directive {
            Directive::Sectnum(options) => (options, diagnostics),
            other => panic!("Expected Sectnum, got {other:?}"),
        }
    }

    /// Parses a whole document and returns its single `.. sectnum::`, for
    /// tests that need the real span origin the dispatcher sets up.
    fn parse_document(input: &str) -> (SectnumOptions, Vec<Diagnostic>) {
        let doc = parse("test.rst", input);
        let options = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive(Directive::Sectnum(options)) => Some(options.clone()),
                _ => None,
            })
            .expect("document should contain a sectnum directive");
        (options, doc.diagnostics)
    }

    #[test]
    fn test_is_sectnum_recognizes_both_spellings() {
        // Given / When / Then
        assert!(is_sectnum("sectnum"));
        assert!(is_sectnum("section-numbering"));
        assert!(!is_sectnum("toctree"));
    }

    #[test]
    fn test_parse_sectnum_with_no_options_is_all_defaults() {
        // Given / When
        let (options, diagnostics) = parse_body(&[]);

        // Then
        assert_eq!(options, SectnumOptions::default());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_stray_argument() {
        // Given — docutils declares zero argument slots for this directive
        // and really does reject one, as a directive error, rather than
        // silently dropping it.
        let (options, diagnostics) = parse_body_with_argument("oops", &[]);

        // Then — the argument is diagnosed, not applied to anything.
        assert_eq!(options, SectnumOptions::default());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumUnexpectedArgument
        );
        assert!(diagnostics.entries()[0].message.contains("oops"));
    }

    #[test]
    fn test_parse_sectnum_reads_depth() {
        // Given
        let body = [":depth: 2"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.depth, NonZeroUsize::new(2));
    }

    #[test]
    fn test_parse_sectnum_treats_a_zero_depth_as_unlimited() {
        // Given
        let body = [":depth: 0"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.depth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_non_numeric_depth() {
        // Given
        let body = [":depth: deep"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.depth, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumDepthInvalid
        );
    }

    #[test]
    fn test_parse_sectnum_reads_start() {
        // Given
        let body = [":start: 5"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.start, NonZeroU32::new(5));
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_zero_start() {
        // Given — unlike `:depth:`, `0` has no sensible number to display.
        let body = [":start: 0"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.start, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumStartInvalid
        );
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_non_numeric_start() {
        // Given
        let body = [":start: five"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.start, None);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumStartInvalid
        );
    }

    #[test]
    fn test_parse_sectnum_reads_prefix_and_suffix() {
        // Given
        let body = [":prefix: Appendix ", ":suffix: ."];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.prefix, "Appendix");
        assert_eq!(options.suffix, ".");
    }

    #[test]
    fn test_parse_sectnum_diagnoses_an_empty_prefix() {
        // Given
        let body = [":prefix:"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.prefix, "");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumEmptyPrefix
        );
    }

    #[test]
    fn test_parse_sectnum_diagnoses_an_empty_suffix() {
        // Given
        let body = [":suffix:"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.suffix, "");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumEmptySuffix
        );
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_stray_argument_through_the_full_parser() {
        // Given
        let input = ".. sectnum:: oops\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code,
            DiagnosticCode::SectnumUnexpectedArgument
        );
        assert!(diagnostics[0].message.contains("oops"));
    }

    #[test]
    fn test_parse_sectnum_diagnoses_an_unknown_option_under_its_own_code() {
        // Given
        let input = ".. sectnum::\n   :invalid_opt:\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code,
            DiagnosticCode::DirectiveSectnumUnknownOption
        );
        assert!(diagnostics[0].message.contains(":invalid_opt:"));
    }

    #[test]
    fn test_parse_sectnum_reads_every_option_together() {
        // Given
        let body = [":depth: 2", ":start: 3", ":prefix: Sec ", ":suffix: ."];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.depth, NonZeroUsize::new(2));
        assert_eq!(options.start, NonZeroU32::new(3));
        assert_eq!(options.prefix, "Sec");
        assert_eq!(options.suffix, ".");
    }

    #[test]
    fn test_both_spellings_parse_to_the_same_directive() {
        // Given
        let sphinx_spelling = parse("test.rst", ".. sectnum::\n   :depth: 1\n");
        let docutils_spelling = parse("test.rst", ".. section-numbering::\n   :depth: 1\n");

        // When
        let find = |doc: &rinx_ast::Document| {
            doc.nodes
                .iter()
                .find_map(|node| match node {
                    Node::Directive(directive @ Directive::Sectnum(_)) => Some(directive.clone()),
                    _ => None,
                })
                .expect("should contain a sectnum directive")
        };

        // Then
        assert_eq!(find(&sphinx_spelling), find(&docutils_spelling));
    }
}
