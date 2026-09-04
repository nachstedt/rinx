//! `.. math::` — display equations written in LaTeX.
//!
//! Parsing is deliberately shallow: the LaTeX is never inspected, only
//! collected. What the directive *does* decide is the structure RST expresses
//! around it — which options were set, and where one equation ends and the
//! next begins (a blank line, in RST, separates two equations in a single
//! directive). Whether that LaTeX is valid is the renderer's question, since
//! only it knows the math backend; see `rusty_sphinx_renderer`'s `math`.
//!
//! `:nowrap:` is what makes the split worth recording in the AST rather than
//! redoing downstream: under it the body is one opaque chunk the author has
//! already wrapped in their own environment, blank lines and all, so the
//! blank-line rule must *not* apply — a distinction that is invisible once the
//! parts have been rejoined.

use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Directive, Span, TargetName};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::indent::unindent_body_lines;

const DIRECTIVE: &str = "math";

/// The options `.. math::` recognizes.
struct MathOptions {
    label: Option<TargetName>,
    nowrap: bool,
    classes: Vec<String>,
}

impl MathOptions {
    fn empty() -> Self {
        Self {
            label: None,
            nowrap: false,
            classes: Vec::new(),
        }
    }
}

/// Consumes the options `.. math::` knows, returning them with the lines it
/// did not recognize, in source order.
///
/// `:name:` is accepted as a spelling of `:label:` — real Sphinx's
/// `MathDirective` reads whichever is present and stores one label — so the two
/// collapse here rather than surviving as separate AST fields. When both are
/// given the last one wins, matching docutils' general option behaviour.
///
/// `:nowrap:` is a flag, so any value at all counts as setting it; `:no-wrap:`
/// is accepted alongside it because docutils normalizes option names that way
/// and authors write both.
fn parse_math_options<'a>(
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (MathOptions, Vec<&'a OptionLine>) {
    let mut options = MathOptions::empty();
    let mut unrecognized = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "label" | "name" => {
                if line.value.is_empty() {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::MathEmptyLabel,
                        format!(
                            "{DIRECTIVE}: :{}: needs a label, so the equation is left unnumbered",
                            line.name
                        ),
                        ctx.line_span(line.line_index, &line.raw),
                    ));
                } else {
                    options.label = Some(TargetName::new(&line.value));
                }
            }
            "nowrap" | "no-wrap" => options.nowrap = true,
            "class" => {
                options.classes = line.value.split_whitespace().map(str::to_string).collect();
            }
            _ => unrecognized.push(line),
        }
    }

    (options, unrecognized)
}

/// Splits a math body into one string per equation.
///
/// A blank line separates two equations, and runs of them count once; leading
/// and trailing blank lines produce no empty equation. Each part keeps its own
/// internal line breaks, because LaTeX is whitespace-sensitive inside an
/// environment and `\\` row breaks span lines.
fn split_equations(body: &[String]) -> Vec<String> {
    body.split(|line| line.trim().is_empty())
        .map(|part| part.join("\n"))
        .filter(|part| !part.trim().is_empty())
        .collect()
}

/// Parses a `.. math::` directive body into a [`Directive::Math`].
///
/// `argument` is the equation written on the directive line itself
/// (`.. math:: e^{i\pi} + 1 = 0`), which RST allows as a shorthand for a
/// one-line body. It is prepended to the body rather than special-cased, so
/// the argument and body forms produce identical ASTs — and so the rare
/// combination of both stays meaningful.
///
/// `body_lines` is the raw, still-indented body collected by
/// `collect_directive_body`, exactly as every other content-bearing directive
/// parser receives it.
pub(in crate::directives) fn parse_math_directive(
    argument: String,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);
    let (options, unrecognized) = parse_math_options(&option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    let mut content: Vec<String> = Vec::new();
    if !argument.trim().is_empty() {
        content.push(argument);
    }
    content.extend_from_slice(&unindented_lines[body_start..]);

    // Under `:nowrap:` the author owns the whole body, blank lines included —
    // splitting it would break an environment that spans one.
    let parts = if options.nowrap {
        let joined = content.join("\n");
        if joined.trim().is_empty() {
            Vec::new()
        } else {
            vec![joined]
        }
    } else {
        split_equations(&content)
    };

    Directive::Math {
        parts,
        label: options.label,
        nowrap: options.nowrap,
        classes: options.classes,
        span: equation_span(&unindented_lines, body_start, ctx),
    }
}

/// Where to report a diagnostic raised about this directive's LaTeX.
///
/// The first line of the *equation*, not of the directive body: with options
/// present the body opens with a `:label:`, and underlining that when the
/// LaTeX two lines below it is what failed points the author at the wrong
/// thing. (`.. csv-table::` anchors at the body's first line instead, but it
/// has no better option — its data may not be in the document at all.)
///
/// Still only a line, not a column: by the time the renderer rejects the
/// LaTeX, the offending part has been rejoined from several lines, so no
/// finer position survives.
///
/// The lines are the *unindented* body, because `ParseCtx` has already been
/// shifted by the body's indent — passing the raw line would overshoot the
/// span's end column by that width.
fn equation_span(
    unindented_lines: &[String],
    body_start: usize,
    ctx: &ParseCtx<'_>,
) -> Option<Span> {
    unindented_lines.get(body_start).map_or_else(
        // No body content: the argument form, whose equation is on the
        // directive line itself, and the degenerate options-only body.
        || ctx.line_span(0, ""),
        |line| ctx.line_span(body_start, line),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::Domain;

    fn parse(argument: &str, body: &[&str]) -> (Directive, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let directive = parse_math_directive(
            argument.to_string(),
            body,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        (directive, diagnostics)
    }

    fn parts_of(directive: &Directive) -> Vec<String> {
        match directive {
            Directive::Math { parts, .. } => parts.clone(),
            other => panic!("expected a math directive, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_math_directive_reads_the_equation_from_the_argument() {
        // Given the one-line form, with the equation on the directive line
        let (directive, diagnostics) = parse(r"e^{i\pi} + 1 = 0", &[]);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(parts_of(&directive), vec![r"e^{i\pi} + 1 = 0".to_string()]);
    }

    #[test]
    fn test_parse_math_directive_reads_the_equation_from_the_body() {
        // Given the body form
        let (directive, diagnostics) = parse("", &["   a^2 + b^2 = c^2"]);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(parts_of(&directive), vec!["a^2 + b^2 = c^2".to_string()]);
    }

    #[test]
    fn test_parse_math_directive_splits_equations_on_a_blank_line() {
        // Given two equations separated by a blank line
        let (directive, _) = parse("", &["   a = b", "", "   c = d"]);

        // Then each becomes its own part
        assert_eq!(
            parts_of(&directive),
            vec!["a = b".to_string(), "c = d".to_string()]
        );
    }

    #[test]
    fn test_parse_math_directive_keeps_a_multiline_equation_as_one_part() {
        // Given one equation spanning two lines with no blank between them
        let (directive, _) = parse("", &[r"   a &= b \\", r"   c &= d"]);

        // Then it stays a single part, line break included
        assert_eq!(
            parts_of(&directive),
            vec!["a &= b \\\\\nc &= d".to_string()]
        );
    }

    #[test]
    fn test_parse_math_directive_ignores_leading_and_trailing_blank_lines() {
        // Given a body padded with blank lines
        let (directive, _) = parse("", &["", "   a = b", "", ""]);

        // Then no empty parts are produced
        assert_eq!(parts_of(&directive), vec!["a = b".to_string()]);
    }

    #[test]
    fn test_parse_math_directive_reads_the_label_option() {
        // Given
        let (directive, diagnostics) = parse("", &["   :label: euler", "", "   a = b"]);

        // Then
        assert!(diagnostics.is_empty());
        let Directive::Math { label, parts, .. } = &directive else {
            panic!("expected a math directive");
        };
        assert_eq!(label.as_ref(), Some(&TargetName::new("euler")));
        assert_eq!(parts, &vec!["a = b".to_string()]);
    }

    #[test]
    fn test_parse_math_directive_accepts_name_as_a_label_alias() {
        // Given `:name:`, which Sphinx treats as a spelling of `:label:`
        let (directive, diagnostics) = parse("", &["   :name: euler", "", "   a = b"]);

        // Then
        assert!(diagnostics.is_empty());
        let Directive::Math { label, .. } = &directive else {
            panic!("expected a math directive");
        };
        assert_eq!(label.as_ref(), Some(&TargetName::new("euler")));
    }

    #[test]
    fn test_parse_math_directive_reports_an_empty_label() {
        // Given a `:label:` with no value
        let (directive, diagnostics) = parse("", &["   :label:", "", "   a = b"]);

        // Then the equation stays unlabeled and the author is told
        let Directive::Math { label, .. } = &directive else {
            panic!("expected a math directive");
        };
        assert_eq!(label.as_ref(), None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::MathEmptyLabel);
    }

    #[test]
    fn test_parse_math_directive_reads_the_nowrap_flag() {
        // Given `:nowrap:`, a flag option with no value
        let (directive, diagnostics) =
            parse("", &["   :nowrap:", "", r"   \begin{align}a&=b\end{align}"]);

        // Then
        assert!(diagnostics.is_empty());
        let Directive::Math { nowrap, .. } = &directive else {
            panic!("expected a math directive");
        };
        assert!(*nowrap);
    }

    #[test]
    fn test_parse_math_directive_accepts_the_no_wrap_spelling() {
        // Given the hyphenated spelling
        let (directive, diagnostics) = parse("", &["   :no-wrap:", "", "   a = b"]);

        // Then
        assert!(diagnostics.is_empty());
        let Directive::Math { nowrap, .. } = &directive else {
            panic!("expected a math directive");
        };
        assert!(*nowrap);
    }

    #[test]
    fn test_parse_math_directive_keeps_a_nowrap_body_whole_across_blank_lines() {
        // Given a `:nowrap:` body whose environment spans a blank line
        let (directive, _) = parse(
            "",
            &[
                "   :nowrap:",
                "",
                r"   \begin{align}",
                "",
                r"   a &= b\end{align}",
            ],
        );

        // Then the blank line does not split it into two equations
        assert_eq!(
            parts_of(&directive),
            vec!["\\begin{align}\n\na &= b\\end{align}".to_string()]
        );
    }

    #[test]
    fn test_parse_math_directive_reads_the_class_option() {
        // Given
        let (directive, diagnostics) = parse("", &["   :class: big boxed", "", "   a = b"]);

        // Then
        assert!(diagnostics.is_empty());
        let Directive::Math { classes, .. } = &directive else {
            panic!("expected a math directive");
        };
        assert_eq!(classes, &vec!["big".to_string(), "boxed".to_string()]);
    }

    #[test]
    fn test_parse_math_directive_reports_an_unknown_option() {
        // Given an option `.. math::` does not have
        let (_, diagnostics) = parse("", &["   :widths: 30 70", "", "   a = b"]);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::DirectiveUnknownOption);
        assert!(
            diagnostics[0].message.contains("math"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_math_directive_produces_no_parts_for_an_empty_body() {
        // Given nothing at all
        let (directive, _) = parse("", &[]);

        // Then
        assert!(parts_of(&directive).is_empty());
    }

    #[test]
    fn test_split_equations_drops_a_whitespace_only_part() {
        // Given a run of blank lines between two equations
        let body = vec![
            "a = b".to_string(),
            String::new(),
            "   ".to_string(),
            String::new(),
            "c = d".to_string(),
        ];

        // When
        let parts = split_equations(&body);

        // Then the run counts once
        assert_eq!(parts, vec!["a = b".to_string(), "c = d".to_string()]);
    }
}
