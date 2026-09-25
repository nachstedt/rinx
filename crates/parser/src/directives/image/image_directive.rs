//! `.. image::` — a picture on its own.
//!
//! Everything option-shaped is [`super::options`]'s work; what is left here is
//! the directive's two structural rules: the argument is required, and there
//! is no body.

use rinx_ast::{Diagnostic, DiagnosticCode, Directive, ImageLoading, ImageOptions, ImageUri, Span};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::directives::error_node::malformed_directive;
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::indent::unindent_body_lines;

use super::options::{ImageContext, parse_common_image_options};

pub(in crate::directives) const DIRECTIVE: &str = "image";

/// Parses a `.. image::` directive into a [`Directive::Image`].
///
/// A missing argument degrades to [`Directive::Malformed`] rather than to an
/// image of nothing: there is no URI to put in a `src`, and the parser's job
/// is to record what went wrong and carry on, leaving the build to decide
/// whether that is fatal.
pub(in crate::directives) fn parse_image_directive(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);
    let refs: Vec<&OptionLine> = option_lines.iter().collect();
    let (common, unrecognized) =
        parse_common_image_options(&refs, DIRECTIVE, ImageContext::Standalone, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    report_unexpected_content(&unindented_lines, body_start, diagnostics, ctx);

    if argument.trim().is_empty() {
        return malformed_directive(
            DIRECTIVE,
            "",
            body_lines,
            DiagnosticCode::ImageMissingUri,
            "image: the directive needs an image path or URL as its argument".to_string(),
            body_span(body_lines, ctx),
            diagnostics,
        );
    }

    let mut options = common.with_uri(ImageUri::new(argument));
    options.span = directive_span;
    report_option_conflicts(&options, &option_lines, DIRECTIVE, diagnostics, ctx);
    Directive::Image(Box::new(options))
}

/// Reports body content below an `.. image::`, which takes none.
///
/// docutils raises an error here. Ignoring it silently would be the worse
/// failure: an author who indented a caption under an `.. image::` meant to
/// write a `.. figure::`, and would otherwise see their text simply vanish.
fn report_unexpected_content(
    unindented_lines: &[String],
    body_start: usize,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let content_at = unindented_lines
        .iter()
        .enumerate()
        .skip(body_start)
        .find(|(_, line)| !line.trim().is_empty());
    if let Some((index, line)) = content_at {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::ImageContentNotAllowed,
            "image: the directive takes no content; use figure for a caption or legend",
            ctx.line_span(index, line),
        ));
    }
}

/// Reports the option combinations that only become wrong once the argument
/// is known, so both image directives check them the same way.
pub(in crate::directives) fn report_option_conflicts(
    options: &ImageOptions,
    option_lines: &[OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    if options.has_unusable_scale() {
        report_unusable_scale(option_lines, directive, diagnostics, ctx);
    }
    // Embedding an external URL would mean fetching it during the build, which
    // would make the build depend on the network. Caught here rather than in
    // the renderer because both halves — the `:loading:` and the argument —
    // are already known, and a parse-time diagnostic points at the line the
    // author would have to change.
    if options.loading == ImageLoading::Embed && matches!(options.uri, ImageUri::External(_)) {
        let span = option_line_span("loading", option_lines, ctx).or(options.span);
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::ImageEmbedExternal,
            format!(
                "{directive}: :loading: embed cannot embed the external URL '{}'; \
                 fetching it would make the build depend on the network, so it is linked instead",
                options.uri.as_written()
            ),
            span,
        ));
    }
}

/// The source span of the last option line named `name`, if it was written.
fn option_line_span(name: &str, option_lines: &[OptionLine], ctx: &ParseCtx<'_>) -> Option<Span> {
    option_lines
        .iter()
        .rev()
        .find(|line| line.name == name)
        .and_then(|line| ctx.line_span(line.line_index, &line.raw))
}

/// Reports a `:scale:` that has no `:width:` or `:height:` to apply to.
///
/// Pointed at the `:scale:` line itself, which is the line the author would
/// have to change — hence the search back through the option lines rather
/// than reusing the directive's own span.
pub(in crate::directives) fn report_unusable_scale(
    option_lines: &[OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let span = option_line_span("scale", option_lines, ctx);
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ImageScaleNoDimensions,
        format!(
            "{directive}: :scale: needs a :width: or :height: to scale; this build never reads \
             the image file's own dimensions, so the option was ignored"
        ),
        span,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Domain, ImageAlign, ImageLoading, ImageUri, TargetName};

    /// Parses a directive body given as already-indented source lines.
    fn parse(argument: &str, body: &[&str]) -> (Directive, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let directive = parse_image_directive(argument, None, body, &mut diagnostics, &ctx);
        (directive, diagnostics)
    }

    fn image_of(directive: &Directive) -> &rinx_ast::ImageOptions {
        match directive {
            Directive::Image(options) => options,
            other => panic!("expected an image directive, got {other:?}"),
        }
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
        diagnostics
            .entries()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn test_parses_a_bare_image() {
        // Given
        let argument = "logo.png";

        // When
        let (directive, diagnostics) = parse(argument, &[]);

        // Then
        assert_eq!(image_of(&directive).uri, ImageUri::new("logo.png"));
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parses_every_shared_option() {
        // Given
        let body = [
            "   :alt: A logo",
            "   :height: 40px",
            "   :width: 80px",
            "   :scale: 50",
            "   :align: center",
            "   :target: https://example.com/",
            "   :class: fancy",
            "   :name: the logo",
            "   :loading: lazy",
        ];

        // When
        let (directive, diagnostics) = parse("logo.png", &body);

        // Then
        let options = image_of(&directive);
        assert_eq!(options.alt.as_deref(), Some("A logo"));
        assert_eq!(
            options.rendered_height().map(|height| height.to_string()),
            Some("20px".to_string())
        );
        assert_eq!(
            options.rendered_width().map(|width| width.to_string()),
            Some("40px".to_string())
        );
        assert_eq!(options.align, Some(ImageAlign::Center));
        assert_eq!(options.classes, vec!["fancy".to_string()]);
        assert_eq!(options.name, Some(TargetName::new("the logo")));
        assert_eq!(options.loading, ImageLoading::Lazy);
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_reports_a_missing_argument() {
        // Given
        let argument = "";

        // When
        let (directive, diagnostics) = parse(argument, &[]);

        // Then
        assert!(matches!(directive, Directive::Malformed { .. }));
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::ImageMissingUri]);
    }

    #[test]
    fn test_keeps_the_body_when_degrading_to_unknown() {
        // Given
        let body = ["   :alt: x"];

        // When
        let (directive, _) = parse("", &body);

        // Then
        match directive {
            Directive::Malformed { name, body, .. } => {
                assert_eq!(name, "image");
                assert!(body.contains(":alt: x"));
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn test_reports_unexpected_content() {
        // Given
        let body = ["   :alt: A logo", "", "   A caption belongs on a figure."];

        // When
        let (directive, diagnostics) = parse("logo.png", &body);

        // Then — still an image; the parser degrades rather than discards
        assert_eq!(image_of(&directive).uri, ImageUri::new("logo.png"));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageContentNotAllowed]
        );
    }

    #[test]
    fn test_accepts_a_body_of_only_blank_lines() {
        // Given
        let body = ["   :alt: A logo", "", "   "];

        // When
        let (_, diagnostics) = parse("logo.png", &body);

        // Then
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_reports_a_scale_with_no_dimension() {
        // Given
        let body = ["   :scale: 50"];

        // When
        let (directive, diagnostics) = parse("logo.png", &body);

        // Then
        assert_eq!(image_of(&directive).scale, Some(50));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageScaleNoDimensions]
        );
    }

    #[test]
    fn test_reports_no_scale_problem_when_a_width_is_given() {
        // Given
        let body = ["   :scale: 50", "   :width: 100px"];

        // When
        let (_, diagnostics) = parse("logo.png", &body);

        // Then
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_reports_an_unknown_option() {
        // Given
        let body = ["   :figwidth: image"];

        // When
        let (_, diagnostics) = parse("logo.png", &body);

        // Then — :figwidth: belongs to figure, not image
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
    }

    #[test]
    fn test_reports_embedding_an_external_url() {
        // Given — fetching it would make the build depend on the network
        let body = ["   :loading: embed"];

        // When
        let (directive, diagnostics) = parse("https://example.com/logo.png", &body);

        // Then — the option is kept, so the renderer can say what it fell back to
        assert_eq!(image_of(&directive).loading, ImageLoading::Embed);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageEmbedExternal]
        );
    }

    #[test]
    fn test_reports_no_embed_problem_for_a_project_file() {
        // Given
        let body = ["   :loading: embed"];

        // When
        let (_, diagnostics) = parse("logo.png", &body);

        // Then
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_carries_the_directives_own_span() {
        // Given — a bare image has no body line to borrow a position from
        let input = ".. image:: logo.png\n";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        let rinx_ast::Node::Directive(directive) = &doc.nodes[0] else {
            panic!("expected a directive, got {:?}", doc.nodes[0]);
        };
        let span = image_of(directive)
            .span
            .expect("an image parsed from real source must carry a span");
        assert_eq!(span.start.line, 1);
    }

    #[test]
    fn test_parses_through_the_full_pipeline() {
        // Given
        let input = "\
.. image:: images/logo.png
   :alt: The logo
   :width: 200px
";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        let rinx_ast::Node::Directive(directive) = &doc.nodes[0] else {
            panic!("expected a directive, got {:?}", doc.nodes[0]);
        };
        let options = image_of(directive);
        assert_eq!(options.uri, ImageUri::new("images/logo.png"));
        assert_eq!(options.alt.as_deref(), Some("The logo"));
        assert_eq!(
            options.rendered_width().map(|width| width.to_string()),
            Some("200px".to_string())
        );
    }

    #[test]
    fn test_reads_an_alt_text_wrapped_over_several_lines() {
        // Given — the shape CPython's `pathlib.rst` writes, which without
        // option-value continuation would read the last two lines as content
        let input = "\
.. image:: pathlib-inheritance.png
   :align: center
   :alt: Inheritance diagram showing the classes available in pathlib.
         The most basic class is PurePath, which has three direct
         subclasses.
";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        let rinx_ast::Node::Directive(directive) = &doc.nodes[0] else {
            panic!("expected a directive, got {:?}", doc.nodes[0]);
        };
        assert_eq!(
            image_of(directive).alt.as_deref(),
            Some(
                "Inheritance diagram showing the classes available in pathlib. \
                 The most basic class is PurePath, which has three direct subclasses."
            )
        );
        assert!(
            doc.diagnostics.is_empty(),
            "wrapped option lines must not be reported as content: {:?}",
            doc.diagnostics
        );
    }

    #[test]
    fn test_an_option_diagnostic_points_at_its_own_source_line() {
        // Given
        let input = "\
.. image:: logo.png
   :alt: The logo
   :width: 3rem
";
        let expected_line = u32::try_from(
            input
                .lines()
                .position(|line| line.contains(":width:"))
                .expect("fixture must contain the :width: option")
                + 1,
        )
        .expect("line number fits in u32");

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        let diagnostic = doc
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::ImageInvalidLength)
            .expect("the invalid width must be reported");
        assert_eq!(
            diagnostic.span.expect("must carry a span").start.line,
            expected_line
        );
    }

    #[test]
    fn test_reads_an_external_url_argument() {
        // Given
        let argument = "https://example.com/logo.png";

        // When
        let (directive, _) = parse(argument, &[]);

        // Then
        assert_eq!(
            image_of(&directive).uri,
            ImageUri::External(argument.to_string())
        );
    }
}
