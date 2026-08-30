//! Turning one `sphinx.ext.doctest` directive into its [`DocTestBlock`]:
//! scanning the options off the body, splitting the group argument, and
//! assembling the variant for the directive's kind.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, Directive, DocTestBlock, DocTestGroupSelector, HashedContent,
    NonEmptyVector, Span,
};

use crate::indent::strip_common_indent;

use super::kind::DocTestDirectiveKind;
use super::options::{DocTestOptions, scan_options};

/// Parses one directive of the family into a [`Directive`].
///
/// Returns [`Directive::Unknown`] only when the body is empty — a doctest block
/// with no code is unusable, and Sphinx warns about it too.
pub(crate) fn parse_doctest_directive(
    kind: DocTestDirectiveKind,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let (options, content_start) = scan_options(kind, body_lines, diagnostics, ctx);
    // The directive's body as a whole, for the diagnostics that are about the
    // block rather than about one option line.
    let block_span = ctx.line_span(0, body_lines.first().unwrap_or(&""));
    let content_lines = skip_leading_blank_lines(&body_lines[content_start..]);
    let content_text = strip_common_indent(content_lines);

    if content_text.trim().is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DoctestNoCode,
            format!("{}: no code in block", kind.as_name()),
            block_span,
        ));
        return Directive::Unknown {
            name: kind.as_name().to_string(),
            argument: argument.to_string(),
            body: content_text,
        };
    }

    let groups = parse_group_argument(kind, argument, diagnostics, block_span);
    let content = HashedContent::new(content_text);

    Directive::DocTest(build_block(kind, groups, content, options))
}

/// Assembles the variant for `kind`, handing each one only the options it
/// accepts.
fn build_block(
    kind: DocTestDirectiveKind,
    groups: NonEmptyVector<DocTestGroupSelector>,
    content: HashedContent,
    options: DocTestOptions,
) -> DocTestBlock {
    let DocTestOptions {
        hide,
        flags,
        pyversion,
        skipif,
        trim,
    } = options;

    match kind {
        DocTestDirectiveKind::Doctest => DocTestBlock::Interactive {
            groups,
            content,
            hide,
            flags,
            pyversion,
            skipif,
            trim,
        },
        DocTestDirectiveKind::TestCode => DocTestBlock::Code {
            groups,
            content,
            hide,
            pyversion,
            skipif,
            trim,
        },
        DocTestDirectiveKind::TestOutput => DocTestBlock::Output {
            groups,
            content,
            hide,
            flags,
            pyversion,
            skipif,
            trim,
        },
        DocTestDirectiveKind::TestSetup => DocTestBlock::Setup {
            groups,
            content,
            skipif,
        },
        DocTestDirectiveKind::TestCleanup => DocTestBlock::Cleanup {
            groups,
            content,
            skipif,
        },
    }
}

/// Splits the directive argument into group selectors.
///
/// The argument is a *comma-separated list*, so `.. testcode:: g1, g2` joins
/// both groups. An absent argument means the `default` group.
fn parse_group_argument(
    kind: DocTestDirectiveKind,
    argument: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> NonEmptyVector<DocTestGroupSelector> {
    if argument.trim().is_empty() {
        return NonEmptyVector::single(DocTestGroupSelector::new(""));
    }

    let selectors: Vec<DocTestGroupSelector> = argument
        .split(',')
        .map(|token| {
            if token.trim().is_empty() {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::DoctestEmptyGroup,
                    format!(
                        "{}: empty group name in argument '{argument}', using the default group",
                        kind.as_name()
                    ),
                    span,
                ));
            }
            DocTestGroupSelector::new(token)
        })
        .collect();

    NonEmptyVector::try_from(selectors)
        .unwrap_or_else(|_| NonEmptyVector::single(DocTestGroupSelector::new("")))
}

/// Drops blank lines separating the option block from the content.
fn skip_leading_blank_lines<'a, 'b>(lines: &'a [&'b str]) -> &'a [&'b str] {
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    &lines[start..]
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Parses `body` under `kind`, returning the directive and any diagnostics.
    ///
    /// `pub(super)` because [`super::super::options`]'s own test module also
    /// drives directive parsing through this same helper.
    use rusty_sphinx_ast::Domain;

    pub(in crate::directives::doctest) fn parse(
        kind: DocTestDirectiveKind,
        argument: &str,
        body: &[&str],
    ) -> (Directive, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let directive = parse_doctest_directive(
            kind,
            argument,
            body,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        (directive, diagnostics)
    }

    /// Unwraps the [`DocTestBlock`] a successful parse produced.
    pub(in crate::directives::doctest) fn block_of(directive: Directive) -> DocTestBlock {
        match directive {
            Directive::DocTest(block) => block,
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_parses_a_minimal_doctest_block() {
        // Given
        let body = ["   >>> 1 + 1", "   2"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        let block = block_of(directive);
        assert_eq!(block.content().body(), ">>> 1 + 1\n2");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_preserves_relative_indentation_in_the_body() {
        // Given — Python's meaning depends on it.
        let body = ["   def f():", "       return 1"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::TestCode, "", &body);

        // Then
        assert_eq!(
            block_of(directive).content().body(),
            "def f():\n    return 1"
        );
    }

    #[test]
    fn test_defaults_to_the_default_group_without_an_argument() {
        // Given
        let body = ["   >>> 1"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        assert_eq!(
            block_of(directive).groups().as_slice(),
            &[DocTestGroupSelector::new("")]
        );
    }

    #[test]
    fn test_splits_a_comma_separated_group_argument() {
        // Given — the argument is a list, not a single name.
        let body = ["   print(1)"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::TestCode, "g1, g2", &body);

        // Then
        assert_eq!(
            block_of(directive).groups().as_slice(),
            &[
                DocTestGroupSelector::new("g1"),
                DocTestGroupSelector::new("g2")
            ]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_resolves_a_star_argument_to_all_groups() {
        // Given
        let body = ["   import os"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::TestSetup, "*", &body);

        // Then
        assert_eq!(
            block_of(directive).groups().as_slice(),
            &[DocTestGroupSelector::AllGroups]
        );
    }

    #[test]
    fn test_reports_an_empty_group_token() {
        // Given
        let body = ["   print(1)"];

        // When
        let (_, diagnostics) = parse(DocTestDirectiveKind::TestCode, "g1,,g2", &body);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("empty group name"));
    }

    #[test]
    fn test_degrades_to_unknown_when_the_body_is_empty() {
        // Given — a block with options but no code is unusable.
        let body = ["   :hide:"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::TestCode, "", &body);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("no code in block"))
        );
    }

    #[test]
    fn test_degrades_to_unknown_for_a_body_with_no_lines_at_all() {
        // Given
        let body: [&str; 0] = [];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_diagnostics_are_prefixed_with_the_directive_name() {
        // Given — the benchmark buckets diagnostics by the text before the
        // first colon, so this is what makes them attributable per directive.
        let body = ["   :bogus:", "", "   import os"];

        // When
        let (_, diagnostics) = parse(DocTestDirectiveKind::TestCleanup, "", &body);

        // Then
        assert!(diagnostics[0].message.starts_with("testcleanup:"));
    }

    #[test]
    fn test_skip_leading_blank_lines_drops_only_the_leading_run() {
        // Given
        let lines = ["", "  ", "code", "", "more"];

        // When
        let remaining = skip_leading_blank_lines(&lines);

        // Then
        assert_eq!(remaining, &["code", "", "more"]);
    }

    #[test]
    fn test_skip_leading_blank_lines_handles_an_all_blank_slice() {
        // Given
        let lines = ["", "   "];

        // When
        let remaining = skip_leading_blank_lines(&lines);

        // Then
        assert!(remaining.is_empty());
    }
}
