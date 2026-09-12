//! The picture a diagram directive puts on the page.
//!
//! Shared by every diagram this build renders — the six spellings of a written
//! `PlantUML` diagram and the generated flowchart — because a diagram's *markup*
//! has nothing to do with where its text came from. An author looking at two
//! pictures should not be able to tell which one was generated, and a page
//! already styled for one should need no new rule for the other.
//!
//! What reaches the page is an `<img>` pointing into the site's `_images/`
//! directory, named by the hash of the `PlantUML` text the build compiled. This
//! renderer never opens that file and never runs `PlantUML`: the picture is
//! produced by a build action, and the only thing agreed between the two is the
//! hash — which is why the text is hashed exactly once, here in the render, and
//! handed back for the same process to write.

use std::fmt::Write as _;

use rusty_sphinx_ast::{HashedContent, ImageAlign, LengthOrPercentage, TargetName};

use crate::RenderCtx;

use super::asset_href::relative_asset_href;

/// The alt text every diagram gets.
///
/// Deliberately generic: the alt text of a diagram whose content is generated
/// cannot describe the picture, and a `:caption:` — which authors write in
/// prose — is rendered as visible text below it, where a screen reader reaches
/// it anyway.
const DIAGRAM_ALT: &str = "PlantUML Diagram";

/// One compiled diagram and the options deciding how it sits on the page.
///
/// Borrowed rather than owned because every field already lives on the node the
/// caller holds; this type exists to say which of a directive's options the
/// *picture* is made of, not to carry them anywhere.
pub(super) struct DiagramFigure<'a> {
    /// The `PlantUML` text whose hash names the compiled SVG.
    pub content: &'a HashedContent,
    /// `:class:` — extra class names on the wrapper.
    pub classes: &'a [String],
    /// `:align:` — horizontal placement.
    pub align: Option<ImageAlign>,
    /// `:width:`, with any `:scale:` already applied by the node.
    pub width: Option<LengthOrPercentage>,
    /// `:caption:` — shown under the picture.
    pub caption: Option<&'a str>,
    /// `:name:` — becomes the wrapper's `id`, so a `:ref:` lands on it.
    pub name: Option<&'a TargetName>,
    /// `:debug:` — also show the `PlantUML` that was compiled.
    pub debug: bool,
}

/// Records this diagram's source for the build to compile, once per picture.
///
/// Two identical diagrams on one page share one compiled SVG, because the hash
/// is the filename — so they need one source file too.
pub(super) fn record_diagram_source(ctx: &mut RenderCtx, content: &HashedContent) {
    if !ctx
        .diagram_sources
        .iter()
        .any(|seen| seen.hash() == content.hash())
    {
        ctx.diagram_sources.push(content.clone());
    }
}

/// Renders one diagram as it appears on the page.
///
/// The markup for an option-free diagram is exactly what this build has always
/// produced, because the overwhelming majority of diagrams carry no options and
/// a changed wrapper would invalidate every cached page for nothing.
pub(super) fn render_diagram_figure(
    html: &mut String,
    figure: &DiagramFigure<'_>,
    ctx: &RenderCtx<'_>,
) {
    let src = diagram_src(figure.content, ctx);
    let src = html_escape::encode_double_quoted_attribute(&src);
    let classes = wrapper_classes(figure);
    let classes = html_escape::encode_double_quoted_attribute(&classes);

    let _ = write!(html, "<div class=\"{classes}\"");
    if let Some(name) = figure.name {
        let id_attr = html_escape::encode_double_quoted_attribute(name.as_str());
        let _ = write!(html, " id=\"{id_attr}\"");
    }
    let _ = writeln!(html, ">");

    let _ = write!(html, "  <img src=\"{src}\" alt=\"{DIAGRAM_ALT}\"");
    if let Some(width) = &figure.width {
        let style = format!("width: {width}");
        let style = html_escape::encode_double_quoted_attribute(&style);
        let _ = write!(html, " style=\"{style}\"");
    }
    let _ = writeln!(html, " />");

    if let Some(caption) = figure.caption {
        let text = html_escape::encode_text(caption);
        let _ = writeln!(html, "  <p class=\"caption\">{text}</p>");
    }

    let _ = writeln!(html, "</div>");

    if figure.debug {
        render_debug_source(html, figure.content);
    }
}

/// The `src` a compiled SVG is served from, relative to the page.
///
/// The hash names the file, so two documents drawing the same diagram share one
/// compiled picture and a diagram that did not change keeps its filename across
/// builds — which is what lets Bazel skip recompiling it.
fn diagram_src(content: &HashedContent, ctx: &RenderCtx<'_>) -> String {
    relative_asset_href(
        std::path::Path::new(&format!("{}.svg", content.hash())),
        ctx.doc_path,
    )
}

/// The classes on the wrapper: the diagram's own marker, its `:align:` and
/// whatever `:class:` added.
fn wrapper_classes(figure: &DiagramFigure<'_>) -> String {
    let mut classes = vec!["plantuml-diagram".to_string()];
    classes.extend(figure.align.map(ImageAlign::css_class));
    classes.extend(figure.classes.iter().cloned());
    classes.join(" ")
}

/// Renders the `:debug:` block: the `PlantUML` text that was actually compiled.
///
/// Shown *after* the picture rather than instead of it, because the option
/// exists to explain a diagram that came out wrong — an author needs to see both
/// the result and the source that produced it. It earns its place more on a
/// flowchart than on a written diagram: that text exists nowhere else.
fn render_debug_source(html: &mut String, content: &HashedContent) {
    let source = html_escape::encode_text(content.body());
    let _ = writeln!(
        html,
        "<pre class=\"plantuml-debug\"><code>{source}</code></pre>"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_diagram_with_no_options_keeps_the_markup_this_build_has_always_emitted() {
        // Given
        let content = HashedContent::new("@startuml\nA -> B\n@enduml".to_string());
        let figure = DiagramFigure {
            content: &content,
            classes: &[],
            align: None,
            width: None,
            caption: None,
            name: None,
            debug: false,
        };

        // When
        let classes = wrapper_classes(&figure);

        // Then
        assert_eq!(classes, "plantuml-diagram");
    }

    #[test]
    fn test_the_wrapper_carries_the_alignment_and_the_authors_own_classes() {
        // Given
        let content = HashedContent::new("@startuml\nA -> B\n@enduml".to_string());
        let classes = vec!["wide".to_string(), "framed".to_string()];
        let figure = DiagramFigure {
            content: &content,
            classes: &classes,
            align: Some(ImageAlign::Center),
            width: None,
            caption: None,
            name: None,
            debug: false,
        };

        // When
        let classes = wrapper_classes(&figure);

        // Then
        assert_eq!(classes, "plantuml-diagram align-center wide framed");
    }

    #[test]
    fn test_the_debug_block_shows_the_text_that_was_compiled() {
        // Given
        let content = HashedContent::new("@startuml\nA -> B\n@enduml".to_string());
        let mut html = String::new();

        // When
        render_debug_source(&mut html, &content);

        // Then
        assert!(html.contains("plantuml-debug"), "{html}");
        assert!(html.contains("A -&gt; B"), "{html}");
    }
}
