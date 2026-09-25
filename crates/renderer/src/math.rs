//! The one place `math-core` is called, converting LaTeX to `MathML` Core.
//!
//! Everything outside this module speaks in `&str` LaTeX in and HTML out, so
//! the choice of math backend never leaks into the node renderers — and never
//! into a `.ast` file either, since the conversion happens at render time
//! rather than at parse time.
//!
//! `MathML` is rendered by browsers natively, which is why this is a build-time
//! conversion at all rather than a client-side script: a page carries its own
//! equations, needs no network to display them, and invalid LaTeX becomes a
//! rinx diagnostic instead of a silent failure in someone's browser.
//! See `docs/decisions/004-math-rendering.md`.

use math_core::{LatexToMathML, MathCoreConfig, MathDisplay};
use rinx_ast::{DiagnosticCode, Span};

/// LaTeX the math backend rejected, reported while rendering.
///
/// Not a [`crate::BrokenLink`]: nothing failed to *resolve*: the markup itself
/// is malformed, which is a different thing to tell an author and reports under
/// its own [`DiagnosticCode::MathInvalidLatex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathError {
    /// The backend's own message, e.g. `Unknown command "\bogus".`
    pub message: String,
    /// Where the offending markup was written, when the AST node carried a
    /// position — see [`crate::BrokenLink::span`].
    pub span: Option<Span>,
}

impl MathError {
    /// The diagnostic code this reports under — what a `.. noqa:` names to
    /// suppress it.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        DiagnosticCode::MathInvalidLatex
    }
}

/// What a failed conversion produces: the fallback HTML to put on the page,
/// and the message to warn with.
#[derive(Debug)]
pub(crate) struct MathFailure {
    /// `math-core`'s own error rendering — the source LaTeX with the message
    /// as a tooltip, in a `<span>` for inline math and a `<p>` for a block.
    /// Shown so a page with a broken equation still says what its author
    /// wrote, rather than dropping the content silently.
    pub html: String,
    pub message: String,
}

/// Converts LaTeX to `MathML`.
///
/// Holds a configured converter rather than exposing a free function because
/// building one parses the (currently empty) macro table, which is per-render
/// work, not per-equation work.
pub(crate) struct MathRenderer {
    converter: LatexToMathML,
}

impl MathRenderer {
    pub(crate) fn new() -> Self {
        let config = MathCoreConfig {
            // Embeds the TeX source in an `<annotation>`, so the markup the
            // author wrote survives a copy-paste out of the rendered page.
            annotation: true,
            ..MathCoreConfig::default()
        };
        Self {
            // The only fallible part of building a converter is parsing custom
            // macros, and the config declares none.
            converter: LatexToMathML::new(config)
                .expect("a converter with no custom macros cannot fail to build"),
        }
    }

    /// Renders one equation, returning `MathML` on success and the fallback
    /// rendering plus a message on failure.
    ///
    /// Uses `math-core`'s *local* state deliberately: its global state exists
    /// to number `\begin{equation}` environments across calls, but rinx
    /// numbers equations itself, per document and from the project index (see
    /// `rinx_analyzer`'s `number_equations`). Letting both count would
    /// produce two disagreeing sets of numbers on one page.
    pub(crate) fn to_html(&self, latex: &str, display: MathDisplay) -> Result<String, MathFailure> {
        match self.converter.convert_with_local_state(latex, display) {
            Ok(result) => Ok(result.mathml),
            Err(error) => Err(MathFailure {
                html: error.to_html(latex, display, Some("math-error")),
                message: error.error_message(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_html_renders_inline_math_without_a_display_attribute() {
        // Given
        let renderer = MathRenderer::new();

        // When
        let html = renderer
            .to_html("a + b", MathDisplay::Inline)
            .expect("valid LaTeX should convert");

        // Then
        assert!(html.starts_with("<math>"), "{html}");
        assert!(html.contains("<mi>a</mi>"), "{html}");
    }

    #[test]
    fn test_to_html_marks_block_math_as_display_block() {
        // Given
        let renderer = MathRenderer::new();

        // When
        let html = renderer
            .to_html("a + b", MathDisplay::Block)
            .expect("valid LaTeX should convert");

        // Then
        assert!(html.starts_with("<math display=\"block\">"), "{html}");
    }

    #[test]
    fn test_to_html_embeds_the_latex_source_as_an_annotation() {
        // Given
        let renderer = MathRenderer::new();

        // When
        let html = renderer
            .to_html("x^2", MathDisplay::Inline)
            .expect("valid LaTeX should convert");

        // Then the original markup survives in the output
        assert!(
            html.contains("<annotation encoding=\"application/x-tex\">x^2</annotation>"),
            "{html}"
        );
    }

    #[test]
    fn test_to_html_renders_a_supported_environment() {
        // Given
        let renderer = MathRenderer::new();

        // When
        let html = renderer
            .to_html(
                r"\begin{aligned}a &= b \\ c &= d\end{aligned}",
                MathDisplay::Block,
            )
            .expect("aligned is supported");

        // Then
        assert!(html.contains("<mtable"), "{html}");
    }

    #[test]
    fn test_to_html_reports_unbalanced_braces_with_the_source_kept() {
        // Given
        let renderer = MathRenderer::new();

        // When
        let failure = renderer
            .to_html(r"\frac{1}{2", MathDisplay::Inline)
            .expect_err("unbalanced braces should fail");

        // Then the author is told what is wrong and still shown what they wrote
        assert!(
            failure.message.contains("Expected closing token"),
            "{}",
            failure.message
        );
        assert!(failure.html.contains(r"\frac{1}{2"), "{}", failure.html);
        assert!(failure.html.contains("math-error"), "{}", failure.html);
    }

    #[test]
    fn test_to_html_reports_an_unknown_command() {
        // Given
        let renderer = MathRenderer::new();

        // When
        let failure = renderer
            .to_html(r"\bogus{x}", MathDisplay::Block)
            .expect_err("an unknown command should fail");

        // Then
        assert!(
            failure.message.contains(r"Unknown command"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn test_math_error_reports_under_the_math_code() {
        // Given
        let error = MathError {
            message: "boom".to_string(),
            span: None,
        };

        // When / Then
        assert_eq!(error.code(), DiagnosticCode::MathInvalidLatex);
    }
}
