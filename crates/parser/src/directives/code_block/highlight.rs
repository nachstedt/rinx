//! `.. highlight::` — the language, line-number threshold and `:force:` that
//! following code blocks inherit.
//!
//! Not a code block itself: it writes no content and renders to nothing. It
//! lives beside the block parsers because it shares their option vocabulary
//! and is the other half of [`rusty_sphinx_ast::CodeLanguage::Inherit`] — a
//! block with no language of its own resolves against whichever of these was
//! last in force.

use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Directive, ResolvedLanguage};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::directives::options::{report_unknown_options, scan_option_lines};
use crate::indent::unindent_body_lines;

use super::options::parse_positive_integer;

/// Parses a `.. highlight::`, which sets the language following blocks
/// inherit.
pub(in crate::directives) fn parse_highlight(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    const DIRECTIVE: &str = "highlight";

    let language = ResolvedLanguage::parse(argument).unwrap_or_else(|_| {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CodeBlockEmptyLanguage,
            format!(
                "{DIRECTIVE}: needs a language argument; following blocks keep the current one"
            ),
            body_span(body_lines, ctx).or_else(|| ctx.line_span(0, "")),
        ));
        // Degrading to the Sphinx default rather than dropping the directive
        // keeps the document renderable, which is the parser's job; the
        // diagnostic is what tells the author.
        ResolvedLanguage::default()
    });

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented_lines);
    let mut linenothreshold = None;
    let mut force = false;
    let mut unrecognized = Vec::new();

    for line in &option_lines {
        match line.name.as_str() {
            "linenothreshold" => {
                linenothreshold = parse_positive_integer(
                    &line.value,
                    "linenothreshold",
                    DIRECTIVE,
                    line,
                    diagnostics,
                    ctx,
                );
            }
            "force" => force = true,
            _ => unrecognized.push(line),
        }
    }
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    Directive::Highlight {
        language,
        linenothreshold,
        force,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{codes, highlight};
    use rusty_sphinx_ast::{DiagnosticCode, Directive, LanguageName, ResolvedLanguage};
    use std::num::NonZeroU32;

    #[test]
    fn test_parse_highlight_reads_the_language() {
        // Given / When
        let (directive, diagnostics) = highlight("Rust", &[]);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Highlight { language, .. } = directive else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(
            language,
            ResolvedLanguage::Named(LanguageName::new("rust").unwrap())
        );
    }

    #[test]
    fn test_parse_highlight_reports_a_missing_language() {
        // Given — unlike a code block, this has no "inherit" to fall back to
        let (directive, diagnostics) = highlight("", &[]);

        // Then — reported, but the document stays renderable
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmptyLanguage]
        );
        let Directive::Highlight { language, .. } = directive else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(language, ResolvedLanguage::default());
    }

    #[test]
    fn test_parse_highlight_reads_linenothreshold() {
        // Given / When
        let (directive, diagnostics) = highlight("python", &["   :linenothreshold: 5"]);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Highlight {
            linenothreshold, ..
        } = directive
        else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(linenothreshold, NonZeroU32::new(5));
    }

    #[test]
    fn test_parse_highlight_reports_a_non_numeric_linenothreshold() {
        // Given / When
        let (directive, diagnostics) = highlight("python", &["   :linenothreshold: many"]);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockInvalidInteger]
        );
        let Directive::Highlight {
            linenothreshold, ..
        } = directive
        else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(linenothreshold, None);
    }

    #[test]
    fn test_parse_highlight_accepts_none_as_a_language() {
        // Given — turning highlighting off for what follows
        let (directive, diagnostics) = highlight("none", &[]);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Highlight { language, .. } = directive else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(language, ResolvedLanguage::None);
    }

    #[test]
    fn test_parse_highlight_reports_an_unknown_option() {
        // Given / When
        let (_, diagnostics) = highlight("python", &["   :nonsense:"]);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
    }
}
