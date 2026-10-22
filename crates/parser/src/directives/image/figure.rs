//! `.. figure::` — an image, a caption naming it, and a legend explaining it.
//!
//! The image half is [`super::options`]'s work, identical to `.. image::`'s.
//! What is particular to a figure is its body, and the rule docutils gives for
//! reading it: the first block is the caption *if it is a paragraph*, and
//! everything after it is the legend. An author who wants a legend with no
//! caption writes an empty comment (`..`) first, which is the one piece of
//! markup this parser consumes rather than parses.

use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::spec::{Arguments, DirectiveShape, DirectiveSpec};
use rinx_ast::{Directive, Figure, FigureWidth, InlineNode, Node};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::error_node::malformed_node;
use crate::directives::options::{
    DirectiveOptions, OptionLine, classify_options, directive_options,
};
use crate::headings::Adornment;

use super::image_directive::{image_uri, report_option_conflicts};
use super::options::{
    CommonImageOptions, ImageContext, ImageOption, ImageSite, read_image_option,
    report_invalid_length,
};

const DIRECTIVE: &str = "figure";

/// The two options `.. figure::` adds to the shared image set.
struct FigureOptions {
    figwidth: Option<FigureWidth>,
    figclasses: Vec<String>,
}

directive_options! {
    /// The options `.. figure::` takes: the image's nine, and docutils'
    /// `:figwidth:` and `:figclass:`.
    enum FigureOption {
        /// `:figwidth:`.
        Figwidth => (["figwidth"], Length, "The figure's width, or `image` for the picture's."),
        /// `:figclass:`.
        Figclass => (["figclass"], Classes, "Classes for the figure.");
        Image(ImageOption),
    }
}

/// Reads a `.. figure::`'s options: its own and its picture's.
fn parse_figure_options(
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (CommonImageOptions, FigureOptions) {
    let mut common = CommonImageOptions::empty();
    let mut options = FigureOptions {
        figwidth: None,
        figclasses: Vec::new(),
    };
    let image = ImageSite {
        directive: DIRECTIVE,
        context: ImageContext::Standalone,
    };
    for (option, line) in
        classify_options::<FigureOption>(option_lines, DIRECTIVE, diagnostics, ctx)
    {
        match option {
            FigureOption::Figwidth => match FigureWidth::new(&line.value) {
                Ok(figwidth) => options.figwidth = Some(figwidth),
                Err(problem) => {
                    report_invalid_length(line, DIRECTIVE, &problem, diagnostics, ctx);
                }
            },
            FigureOption::Figclass => {
                options.figclasses = line.value.split_whitespace().map(str::to_string).collect();
            }
            FigureOption::Image(option) => {
                read_image_option(option, line, &mut common, image, diagnostics, ctx);
            }
        }
    }
    (common, options)
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

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const FIGURE_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["figure"],
    doc: "A picture with a caption and a legend.",
    shape: DirectiveShape {
        arguments: Arguments::ONE_TEXT,
        takes_options: true,
        takes_content: true,
        content_hint: None,
    },
    options: <FigureOption as DirectiveOptions>::GROUPS,
};

/// Interprets `.. figure::`.
pub(in crate::directives) fn interpret_figure(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_figure_directive(
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses a `.. figure::` directive into a [`Directive::Figure`].
///
/// A missing argument degrades to [`Directive::Malformed`] for the same reason
/// `.. image::`'s does: there is no URI to show, and the body alone is not a
/// figure.
pub(in crate::directives) fn parse_figure_directive(
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let option_lines = &invocation.options;
    let (common, figure_options) = parse_figure_options(option_lines, diagnostics, ctx);

    let Some(uri) = image_uri(invocation) else {
        // The reader has reported the missing argument.
        return malformed_node(
            DIRECTIVE,
            "",
            invocation.source_body,
            "figure: the directive needs an image path or URL as its argument".to_string(),
        );
    };

    let mut image = common.with_uri(uri);
    image.span = invocation.span;
    report_option_conflicts(&image, option_lines, DIRECTIVE, diagnostics, ctx);

    let content = invocation.content();
    let body = split_caption_and_legend(&content.lines, adornment_order, diagnostics, &content.ctx);

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
    use crate::context::ParseCtx;
    use rinx_ast::AssetUri;
    use rinx_ast::{DiagnosticCode, Domain, ImageAlign, inline_plain_text};

    fn parse(argument: &str, body: &[&str]) -> (Directive, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let mut adornment_order = Vec::new();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let written = crate::directives::reader::written_after_blank(body);
        let invocation = crate::directives::reader::test_invocation(
            "figure",
            FIGURE_SPEC.shape,
            argument,
            &written,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_figure_directive(&invocation, &mut adornment_order, &mut diagnostics);
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
        assert_eq!(figure.image.uri, AssetUri::new("logo.png"));
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
        assert!(matches!(directive, Directive::Malformed { .. }));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveMissingArgument]
        );
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
        assert_eq!(figure.image.uri, AssetUri::new("images/logo.png"));
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
