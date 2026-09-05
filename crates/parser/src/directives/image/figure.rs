//! `.. figure::` — an image, a caption naming it, and a legend explaining it.
//!
//! The image half is [`super::options`]'s work, identical to `.. image::`'s.
//! What is particular to a figure is its body, and the rule docutils gives for
//! reading it: the first block is the caption *if it is a paragraph*, and
//! everything after it is the legend. An author who wants a legend with no
//! caption writes an empty comment (`..`) first, which is the one piece of
//! markup this parser consumes rather than parses.

use rusty_sphinx_ast::{
    DiagnosticCode, Directive, Figure, FigureWidth, ImageUri, InlineNode, Node, Span,
};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::{body_span, join_body_lines};
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::Diagnostic;

use super::image_directive::report_option_conflicts;
use super::options::{ImageContext, parse_common_image_options, report_invalid_length};

const DIRECTIVE: &str = "figure";

/// The two options `.. figure::` adds to the shared image set.
struct FigureOptions {
    figwidth: Option<FigureWidth>,
    figclasses: Vec<String>,
}

/// Consumes `:figwidth:` and `:figclass:` out of the lines the shared image
/// option parser did not recognize, returning the rest untouched.
fn parse_figure_options<'a>(
    option_lines: &[&'a OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (FigureOptions, Vec<&'a OptionLine>) {
    let mut options = FigureOptions {
        figwidth: None,
        figclasses: Vec::new(),
    };
    let mut unrecognized = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "figwidth" => match FigureWidth::new(&line.value) {
                Ok(figwidth) => options.figwidth = Some(figwidth),
                Err(problem) => {
                    report_invalid_length(line, DIRECTIVE, &problem, diagnostics, ctx);
                }
            },
            "figclass" => {
                options.figclasses = line.value.split_whitespace().map(str::to_string).collect();
            }
            _ => unrecognized.push(*line),
        }
    }

    (options, unrecognized)
}

/// The caption and legend a figure's body content splits into.
struct FigureBody {
    caption: Option<Vec<InlineNode>>,
    legend: Vec<Node>,
}

/// Whether `line` is the empty comment docutils reads as "this figure has a
/// legend but no caption".
///
/// Exactly `..` and nothing else: a `.. ` with text after it is an ordinary
/// comment, and a `.. name::` is a directive, neither of which suppresses the
/// caption.
fn is_empty_comment(line: &str) -> bool {
    line.trim() == ".."
}

/// Splits a figure's body into its caption and its legend.
///
/// `lines` are the body's lines with the option block already removed, and
/// `ctx` must already be rebased onto the first of them — every position
/// inside a legend depends on it.
fn split_caption_and_legend(
    lines: &[String],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> FigureBody {
    let first_content = lines.iter().position(|line| !line.trim().is_empty());
    let Some(first_content) = first_content else {
        return FigureBody {
            caption: None,
            legend: Vec::new(),
        };
    };

    // An empty comment suppresses the caption, so everything below it is
    // legend. It is dropped rather than parsed: leaving a `Node::Comment` in
    // the legend would be a node docutils' own tree does not have.
    if is_empty_comment(&lines[first_content]) {
        let legend_start = first_content + 1;
        let legend_lines: Vec<&str> = lines[legend_start..].iter().map(String::as_str).collect();
        let legend_ctx = ctx.nested(legend_start, 0);
        return FigureBody {
            caption: None,
            legend: parse_blocks(&legend_lines, adornment_order, diagnostics, &legend_ctx),
        };
    }

    let content_lines: Vec<&str> = lines[first_content..].iter().map(String::as_str).collect();
    let content_ctx = ctx.nested(first_content, 0);
    let mut nodes = parse_blocks(&content_lines, adornment_order, diagnostics, &content_ctx);

    // Only a *paragraph* becomes the caption. A body opening with a list or a
    // table is all legend, which is docutils' rule and not an approximation of
    // it: a figure whose first block is a table has no caption at all.
    if matches!(nodes.first(), Some(Node::Paragraph(_))) {
        let Node::Paragraph(caption) = nodes.remove(0) else {
            unreachable!("just matched a paragraph")
        };
        FigureBody {
            caption: Some(caption),
            legend: nodes,
        }
    } else {
        FigureBody {
            caption: None,
            legend: nodes,
        }
    }
}

/// Parses a `.. figure::` directive into a [`Directive::Figure`].
///
/// A missing argument degrades to [`Directive::Unknown`] for the same reason
/// `.. image::`'s does: there is no URI to show, and the body alone is not a
/// figure.
pub(in crate::directives) fn parse_figure_directive(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);
    let refs: Vec<&OptionLine> = option_lines.iter().collect();
    let (common, after_image) =
        parse_common_image_options(&refs, DIRECTIVE, ImageContext::Standalone, diagnostics, ctx);
    let (figure_options, unrecognized) = parse_figure_options(&after_image, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    if argument.trim().is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::ImageMissingUri,
            "figure: the directive needs an image path or URL as its argument",
            body_span(body_lines, ctx),
        ));
        return Directive::Unknown {
            name: DIRECTIVE.to_string(),
            argument: String::new(),
            body: join_body_lines(body_lines),
        };
    }

    let mut image = common.with_uri(ImageUri::new(argument));
    image.span = directive_span;
    report_option_conflicts(&image, &option_lines, DIRECTIVE, diagnostics, ctx);

    // The body starts below the option block, so every position inside it is
    // short by that many lines unless the context is rebased first.
    let body_ctx = ctx.nested(body_start, 0);
    let body = split_caption_and_legend(
        &unindented_lines[body_start..],
        adornment_order,
        diagnostics,
        &body_ctx,
    );

    Directive::Figure(Box::new(Figure {
        image,
        figwidth: figure_options.figwidth,
        figclasses: figure_options.figclasses,
        caption: body.caption,
        legend: body.legend,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Domain, ImageAlign, inline_plain_text};

    fn parse(argument: &str, body: &[&str]) -> (Directive, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let mut adornment_order = Vec::new();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let directive = parse_figure_directive(
            argument,
            None,
            body,
            &mut adornment_order,
            &mut diagnostics,
            &ctx,
        );
        (directive, diagnostics)
    }

    fn figure_of(directive: &Directive) -> &Figure {
        match directive {
            Directive::Figure(figure) => figure,
            other => panic!("expected a figure directive, got {other:?}"),
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
    fn test_parses_a_bare_figure() {
        // Given
        let argument = "logo.png";

        // When
        let (directive, diagnostics) = parse(argument, &[]);

        // Then
        let figure = figure_of(&directive);
        assert_eq!(figure.image.uri, ImageUri::new("logo.png"));
        assert!(!figure.has_body());
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_reads_the_first_paragraph_as_the_caption() {
        // Given
        let body = ["   The project logo."];

        // When
        let (directive, diagnostics) = parse("logo.png", &body);

        // Then
        let figure = figure_of(&directive);
        assert_eq!(
            figure.caption.as_deref().map(inline_plain_text),
            Some("The project logo.".to_string())
        );
        assert!(figure.legend.is_empty());
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parses_inline_markup_in_the_caption() {
        // Given
        let body = ["   The **project** logo."];

        // When
        let (directive, _) = parse("logo.png", &body);

        // Then
        let caption = figure_of(&directive)
            .caption
            .clone()
            .expect("should have a caption");
        assert!(
            caption
                .iter()
                .any(|node| matches!(node, InlineNode::Strong(_))),
            "expected bold markup in the caption, got {caption:?}"
        );
    }

    #[test]
    fn test_reads_everything_after_the_caption_as_the_legend() {
        // Given
        let body = [
            "   The project logo.",
            "",
            "   It was drawn in 2019.",
            "",
            "   - and revised twice",
        ];

        // When
        let (directive, _) = parse("logo.png", &body);

        // Then
        let figure = figure_of(&directive);
        assert_eq!(
            figure.caption.as_deref().map(inline_plain_text),
            Some("The project logo.".to_string())
        );
        assert_eq!(figure.legend.len(), 2);
        assert!(matches!(figure.legend[0], Node::Paragraph(_)));
        assert!(matches!(figure.legend[1], Node::BulletList { .. }));
    }

    #[test]
    fn test_an_empty_comment_suppresses_the_caption() {
        // Given
        let body = ["   ..", "", "   This is all legend."];

        // When
        let (directive, _) = parse("logo.png", &body);

        // Then
        let figure = figure_of(&directive);
        assert_eq!(figure.caption, None);
        assert_eq!(figure.legend.len(), 1);
        assert!(matches!(figure.legend[0], Node::Paragraph(_)));
    }

    #[test]
    fn test_a_body_starting_with_a_list_is_all_legend() {
        // Given — docutils only promotes a *paragraph* to a caption
        let body = ["   - one", "   - two"];

        // When
        let (directive, _) = parse("logo.png", &body);

        // Then
        let figure = figure_of(&directive);
        assert_eq!(figure.caption, None);
        assert_eq!(figure.legend.len(), 1);
        assert!(matches!(figure.legend[0], Node::BulletList { .. }));
    }

    #[test]
    fn test_parses_the_figure_only_options() {
        // Given
        let body = ["   :figwidth: 60%", "   :figclass: framed wide"];

        // When
        let (directive, diagnostics) = parse("logo.png", &body);

        // Then
        let figure = figure_of(&directive);
        assert!(matches!(figure.figwidth, Some(FigureWidth::Percentage(_))));
        assert_eq!(
            figure.figclasses,
            vec!["framed".to_string(), "wide".to_string()]
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_parses_figwidth_image() {
        // Given
        let body = ["   :figwidth: image"];

        // When
        let (directive, _) = parse("logo.png", &body);

        // Then
        assert_eq!(
            figure_of(&directive).figwidth,
            Some(FigureWidth::MatchImage)
        );
    }

    #[test]
    fn test_reports_an_invalid_figwidth() {
        // Given
        let body = ["   :figwidth: wide"];

        // When
        let (directive, diagnostics) = parse("logo.png", &body);

        // Then
        assert_eq!(figure_of(&directive).figwidth, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageInvalidLength]
        );
    }

    #[test]
    fn test_accepts_the_shared_image_options_too() {
        // Given
        let body = ["   :align: right", "   :alt: A logo", "", "   Caption."];

        // When
        let (directive, diagnostics) = parse("logo.png", &body);

        // Then
        let figure = figure_of(&directive);
        assert_eq!(figure.image.align, Some(ImageAlign::Right));
        assert_eq!(figure.image.alt.as_deref(), Some("A logo"));
        assert_eq!(
            figure.caption.as_deref().map(inline_plain_text),
            Some("Caption.".to_string())
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_reports_an_unknown_option() {
        // Given
        let body = ["   :bogus: value"];

        // When
        let (_, diagnostics) = parse("logo.png", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
    }

    #[test]
    fn test_reports_a_missing_argument() {
        // Given
        let argument = "";

        // When
        let (directive, diagnostics) = parse(argument, &["   Caption."]);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert_eq!(codes(&diagnostics), vec![DiagnosticCode::ImageMissingUri]);
    }

    #[test]
    fn test_reports_a_scale_with_no_dimension() {
        // Given
        let body = ["   :scale: 50"];

        // When
        let (_, diagnostics) = parse("logo.png", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::ImageScaleNoDimensions]
        );
    }

    #[test]
    fn test_parses_through_the_full_pipeline() {
        // Given
        let input = "\
.. figure:: images/logo.png
   :alt: The logo
   :figwidth: 60%

   The project logo.

   Drawn in 2019.
";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(directive) = &doc.nodes[0] else {
            panic!("expected a directive, got {:?}", doc.nodes[0]);
        };
        let figure = figure_of(directive);
        assert_eq!(figure.image.uri, ImageUri::new("images/logo.png"));
        assert_eq!(figure.image.alt.as_deref(), Some("The logo"));
        assert_eq!(
            figure.caption.as_deref().map(inline_plain_text),
            Some("The project logo.".to_string())
        );
        assert_eq!(figure.legend.len(), 1);
    }

    #[test]
    fn test_a_legend_diagnostic_points_at_its_own_source_line() {
        // Given — the broken role sits in the legend, four lines below the
        // directive, so an unrebased context would report it against the
        // directive's own line instead.
        let input = "\
.. figure:: logo.png
   :alt: The logo

   The caption.

   See :ref:`missing` for details.
";
        let expected_line = u32::try_from(
            input
                .lines()
                .position(|line| line.contains(":ref:`missing`"))
                .expect("fixture must contain the :ref: role")
                + 1,
        )
        .expect("line number fits in u32");

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        let Node::Directive(directive) = &doc.nodes[0] else {
            panic!("expected a directive, got {:?}", doc.nodes[0]);
        };
        let legend = &figure_of(directive).legend;
        let [Node::Paragraph(inlines)] = legend.as_slice() else {
            panic!("expected a single legend paragraph, got {legend:?}");
        };
        let span = inlines
            .iter()
            .find_map(|node| match node {
                InlineNode::Reference { span, .. } => *span,
                _ => None,
            })
            .expect("the :ref: role must carry a span");
        assert_eq!(span.start.line, expected_line);
    }

    #[test]
    fn test_is_empty_comment_accepts_only_a_bare_marker() {
        // Given / When / Then
        assert!(is_empty_comment(".."));
        assert!(is_empty_comment("  ..  "));
        assert!(!is_empty_comment(".. a comment"));
        assert!(!is_empty_comment(".. note::"));
    }
}
