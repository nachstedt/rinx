//! `.. figure::` rendering — an image wrapped in the `<figure>` element that
//! gives it a caption and a legend.
//!
//! The `<img>` itself is [`super::image`]'s, unchanged: a figure differs in
//! what surrounds the picture, not in the picture. What belongs to the figure
//! is the box — its `:figwidth:`, its `:figclass:`, and the `:align:`, which
//! docutils places on the `<figure>` rather than on the image inside it.

use std::fmt::Write as _;

use rinx_ast::{Figure, FigureWidth, ImageAlign};

use crate::RenderCtx;
use crate::inline::render_inline;

use super::dispatch::render_nodes;
use super::image::render_linked_image;

/// The `style` attribute value a `:figwidth:` becomes.
///
/// `:figwidth: image` asks for a box exactly as wide as the picture inside it.
/// docutils answers that by opening the image file and reading its pixel
/// width; CSS can express the same thing directly with `fit-content`, so this
/// build says what it means instead of measuring — and so the option works
/// even though no phase of this build ever opens the file (see
/// `docs/decisions/007-image-assets.md`).
fn figure_width_style(figwidth: &FigureWidth) -> String {
    match figwidth {
        FigureWidth::Length(length) => format!("width: {length}"),
        FigureWidth::Percentage(percentage) => format!("width: {percentage}"),
        FigureWidth::MatchImage => "width: fit-content".to_string(),
    }
}

/// Renders a `.. figure::` directive.
///
/// The element order follows docutils' HTML5 writer: the image, then a single
/// `<figcaption>` holding the caption paragraph and, below it, the legend in
/// its own `<div class="legend">`. A figure with neither is legal and renders
/// as a bare picture in a box.
pub(super) fn render_figure_directive(html: &mut String, figure: &Figure, ctx: &mut RenderCtx) {
    let mut classes: Vec<String> = figure
        .image
        .align
        .map(ImageAlign::css_class)
        .into_iter()
        .collect();
    classes.extend(figure.figclasses.iter().cloned());

    let _ = write!(html, "<figure");
    if let Some(name) = &figure.image.name {
        let id_attr = html_escape::encode_double_quoted_attribute(name.as_str());
        let _ = write!(html, " id=\"{id_attr}\"");
    }
    if !classes.is_empty() {
        let joined = classes.join(" ");
        let class_attr = html_escape::encode_double_quoted_attribute(&joined);
        let _ = write!(html, " class=\"{class_attr}\"");
    }
    if let Some(figwidth) = &figure.figwidth {
        let style = figure_width_style(figwidth);
        let style_attr = html_escape::encode_double_quoted_attribute(&style);
        let _ = write!(html, " style=\"{style_attr}\"");
    }
    let _ = writeln!(html, ">");

    // No alignment class on the image: on a figure the alignment moves the
    // whole box, which is what the class on the `<figure>` above does.
    render_linked_image(html, &figure.image, &[], ctx);
    let _ = writeln!(html);

    if figure.caption.is_some() || !figure.legend.is_empty() {
        let _ = writeln!(html, "<figcaption>");
        if let Some(caption) = &figure.caption {
            let _ = write!(html, "<p>");
            for inline in caption {
                render_inline(html, inline, ctx);
            }
            let _ = writeln!(html, "</p>");
        }
        if !figure.legend.is_empty() {
            let _ = writeln!(html, "<div class=\"legend\">");
            render_nodes(html, &figure.legend, ctx);
            let _ = writeln!(html, "</div>");
        }
        let _ = writeln!(html, "</figcaption>");
    }

    let _ = writeln!(html, "</figure>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::render_test_support::render_directive_html;
    use rinx_ast::{AssetUri, Directive, ImageOptions, InlineNode, Node, TargetName};
    use rinx_index::ProjectIndex;

    fn figure(uri: &str) -> Figure {
        Figure::new(ImageOptions::new(AssetUri::new(uri)))
    }

    fn render(figure: Figure) -> String {
        render_directive_html(
            &Directive::Figure(Box::new(figure)),
            &ProjectIndex::default(),
            "index.rst",
        )
    }

    fn text(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::Text(content.to_string())]
    }

    #[test]
    fn test_renders_a_bare_figure() {
        // Given
        let figure = figure("logo.png");

        // When
        let html = render(figure);

        // Then
        assert!(html.contains("<figure>"), "unexpected: {html}");
        assert!(
            html.contains("<img src=\"_images/logo.png\""),
            "unexpected: {html}"
        );
        assert!(!html.contains("<figcaption>"), "unexpected: {html}");
    }

    #[test]
    fn test_renders_a_caption() {
        // Given
        let mut figure = figure("logo.png");
        figure.caption = Some(text("The project logo."));

        // When
        let html = render(figure);

        // Then
        assert!(
            html.contains("<figcaption>\n<p>The project logo.</p>"),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_inline_markup_in_the_caption() {
        // Given
        let mut figure = figure("logo.png");
        figure.caption = Some(vec![
            InlineNode::Text("The ".to_string()),
            InlineNode::Strong("project".to_string()),
            InlineNode::Text(" logo.".to_string()),
        ]);

        // When
        let html = render(figure);

        // Then
        assert!(
            html.contains("<p>The <strong>project</strong> logo.</p>"),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_a_legend_below_the_caption() {
        // Given
        let mut figure = figure("logo.png");
        figure.caption = Some(text("The logo."));
        figure.legend = vec![Node::Paragraph(text("Drawn in 2019."))];

        // When
        let html = render(figure);

        // Then
        let caption_at = html.find("The logo.").expect("caption must render");
        let legend_at = html.find("Drawn in 2019.").expect("legend must render");
        assert!(caption_at < legend_at, "legend must follow caption: {html}");
        assert!(
            html.contains("<div class=\"legend\">"),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_a_legend_without_a_caption() {
        // Given — what an author writes with a leading empty comment
        let mut figure = figure("logo.png");
        figure.legend = vec![Node::Paragraph(text("All legend."))];

        // When
        let html = render(figure);

        // Then
        assert!(html.contains("<figcaption>"), "unexpected: {html}");
        assert!(
            html.contains("<div class=\"legend\">"),
            "unexpected: {html}"
        );
        assert!(
            !html.contains("<p>All legend.</p>\n</figcaption>"),
            "the legend must not be rendered as a caption: {html}"
        );
    }

    #[test]
    fn test_renders_the_align_class_on_the_figure_not_the_image() {
        // Given
        let mut figure = figure("logo.png");
        figure.image.align = Some(ImageAlign::Center);

        // When
        let html = render(figure);

        // Then
        assert!(
            html.contains("<figure class=\"align-center\">"),
            "unexpected: {html}"
        );
        assert!(
            !html.contains("<img src=\"_images/logo.png\" alt=\"logo.png\" class="),
            "the image itself must carry no align class: {html}"
        );
    }

    #[test]
    fn test_merges_figclass_with_the_align_class() {
        // Given
        let mut figure = figure("logo.png");
        figure.image.align = Some(ImageAlign::Right);
        figure.figclasses = vec!["framed".to_string()];

        // When
        let html = render(figure);

        // Then
        assert!(
            html.contains("class=\"align-right framed\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_a_percentage_figwidth() {
        // Given
        let mut figure = figure("logo.png");
        figure.figwidth = Some(FigureWidth::new("60%").expect("valid"));

        // When
        let html = render(figure);

        // Then
        assert!(html.contains("style=\"width: 60%\""), "unexpected: {html}");
    }

    #[test]
    fn test_renders_a_length_figwidth() {
        // Given
        let mut figure = figure("logo.png");
        figure.figwidth = Some(FigureWidth::new("8cm").expect("valid"));

        // When
        let html = render(figure);

        // Then
        assert!(html.contains("style=\"width: 8cm\""), "unexpected: {html}");
    }

    #[test]
    fn test_renders_figwidth_image_as_fit_content() {
        // Given — docutils reads the file to answer this; CSS says it directly
        let mut figure = figure("logo.png");
        figure.figwidth = Some(FigureWidth::MatchImage);

        // When
        let html = render(figure);

        // Then
        assert!(
            html.contains("style=\"width: fit-content\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_renders_the_name_as_the_figures_id() {
        // Given
        let mut figure = figure("logo.png");
        figure.image.name = Some(TargetName::new("the-figure"));

        // When
        let html = render(figure);

        // Then
        assert!(
            html.contains("<figure id=\"the-figure\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_keeps_the_images_own_options() {
        // Given
        let mut figure = figure("logo.png");
        figure.image.alt = Some("A logo".to_string());
        figure.image.width = Some(rinx_ast::LengthOrPercentage::new("100px").expect("valid"));

        // When
        let html = render(figure);

        // Then
        assert!(html.contains("alt=\"A logo\""), "unexpected: {html}");
        assert!(
            html.contains("style=\"width: 100px\""),
            "unexpected: {html}"
        );
    }

    #[test]
    fn test_figure_width_style_covers_every_form() {
        // Given / When / Then
        assert_eq!(
            figure_width_style(&FigureWidth::new("5em").expect("valid")),
            "width: 5em"
        );
        assert_eq!(
            figure_width_style(&FigureWidth::new("25%").expect("valid")),
            "width: 25%"
        );
        assert_eq!(
            figure_width_style(&FigureWidth::MatchImage),
            "width: fit-content"
        );
    }
}
