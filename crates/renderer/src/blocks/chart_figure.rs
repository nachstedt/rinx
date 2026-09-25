//! The figure every inline chart sits in.
//!
//! Deliberately *not* [`super::diagram_figure`], whose whole job is an `<img>`
//! pointing at a hash-named SVG a build action compiled. A chart has no compile
//! step and no hash: its SVG is written straight into the page. The wrapper's
//! shape is copied from that module all the same, so a page already styled
//! for a diagram needs no new rule for a chart, and a reader sees the two
//! placed the same way.
//!
//! Shared by `.. entity-pie::` and `.. entity-bar::`, which differ in what
//! they draw but not in how the drawing is placed on the page.

use std::fmt::Write as _;

use rusty_sphinx_ast::{ImageAlign, LengthOrPercentage, TargetName};

/// Everything about a chart's placement, borrowed from its node.
pub(super) struct ChartFigure<'a> {
    /// The class every chart of this kind carries, whichever name it was
    /// written under — and the prefix of its title and figure classes.
    pub class: &'static str,
    pub title: Option<&'a str>,
    pub caption: Option<&'a str>,
    pub align: Option<ImageAlign>,
    pub classes: &'a [String],
    pub name: Option<&'a TargetName>,
    /// `:width:` with `:scale:` already applied.
    pub width: Option<LengthOrPercentage>,
}

/// Writes `svg` into `html`, wrapped in the chart's figure.
pub(super) fn write_chart_figure(html: &mut String, figure: &ChartFigure<'_>, svg: &str) {
    let class = figure.class;
    let classes = wrapper_classes(figure);
    let classes = html_escape::encode_double_quoted_attribute(&classes);
    let _ = write!(html, "<div class=\"{classes}\"");
    if let Some(name) = figure.name {
        let id_attr = html_escape::encode_double_quoted_attribute(name.as_str());
        let _ = write!(html, " id=\"{id_attr}\"");
    }
    let _ = writeln!(html, ">");

    if let Some(title) = figure.title {
        let text = html_escape::encode_text(title);
        let _ = writeln!(html, "  <p class=\"{class}-title\">{text}</p>");
    }

    let _ = write!(html, "  <div class=\"{class}-figure\"");
    if let Some(width) = &figure.width {
        let style = format!("width: {width}");
        let style = html_escape::encode_double_quoted_attribute(&style);
        let _ = write!(html, " style=\"{style}\"");
    }
    let _ = writeln!(html, ">{svg}</div>");

    if let Some(caption) = figure.caption {
        let text = html_escape::encode_text(caption);
        let _ = writeln!(html, "  <p class=\"caption\">{text}</p>");
    }

    let _ = writeln!(html, "</div>");
}

/// The classes on the wrapper: the chart's own marker, its `:align:` and
/// whatever `:class:` added.
fn wrapper_classes(figure: &ChartFigure<'_>) -> String {
    let mut classes = vec![figure.class.to_string()];
    classes.extend(figure.align.map(ImageAlign::css_class));
    classes.extend(figure.classes.iter().cloned());
    classes.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn figure() -> ChartFigure<'static> {
        ChartFigure {
            class: "entity-bar",
            title: None,
            caption: None,
            align: None,
            classes: &[],
            name: None,
            width: None,
        }
    }

    fn written(figure: &ChartFigure<'_>) -> String {
        let mut html = String::new();
        write_chart_figure(&mut html, figure, "<svg></svg>");
        html
    }

    #[test]
    fn test_the_svg_is_wrapped_in_the_charts_own_classes() {
        // Given
        let figure = figure();

        // When
        let html = written(&figure);

        // Then
        assert!(html.starts_with("<div class=\"entity-bar\">"), "{html}");
        assert!(
            html.contains("<div class=\"entity-bar-figure\"><svg></svg></div>"),
            "{html}"
        );
    }

    #[test]
    fn test_title_and_caption_are_escaped_and_placed_around_the_chart() {
        // Given
        let figure = ChartFigure {
            title: Some("A <b>"),
            caption: Some("Under & over"),
            ..figure()
        };

        // When
        let html = written(&figure);

        // Then
        let title = html.find("entity-bar-title\">A &lt;b&gt;").unwrap();
        let caption = html.find("caption\">Under &amp; over").unwrap();
        assert!(title < caption, "{html}");
    }

    #[test]
    fn test_the_name_alignment_classes_and_width_are_applied() {
        // Given
        let classes = ["wide".to_string()];
        let name = TargetName::new("authors");
        let figure = ChartFigure {
            align: Some(ImageAlign::Right),
            classes: &classes,
            name: Some(&name),
            width: Some(LengthOrPercentage::new("60%").unwrap()),
            ..figure()
        };

        // When
        let html = written(&figure);

        // Then
        assert!(
            html.starts_with("<div class=\"entity-bar align-right wide\" id=\"authors\">"),
            "{html}"
        );
        assert!(html.contains("style=\"width: 60%\""), "{html}");
    }
}
