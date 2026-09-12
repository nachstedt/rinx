//! Diagram rendering — every spelling of one.
//!
//! What reaches the page is an `<img>` pointing into the site's `_images/`
//! directory, named by the hash of the `PlantUML` text the build compiled. This
//! renderer never opens that file and never runs `PlantUML`: the picture is
//! produced by a build action, and the only thing agreed between the two is
//! the hash.
//!
//! That agreement is the whole design. The hash is computed here from the
//! diagram's *expanded* text — which, for now, is its template verbatim — and
//! computed again, independently, by the action that writes the `.puml` file.
//! Neither reads a sidecar naming the other's answer, in the same way
//! `ImageUri::resolve` keeps the embedder, the validator and this renderer
//! from disagreeing about where a picture lives.

use std::fmt::Write as _;

use rusty_sphinx_ast::{HashedContent, ImageAlign, Uml};
use rusty_sphinx_uml::{UmlContext, expand};

use crate::RenderCtx;
use crate::uml_error::DiagramError;

use super::asset_href::relative_asset_href;

/// The alt text every diagram gets.
///
/// Deliberately generic: the alt text of a diagram whose content is generated
/// cannot describe the picture, and a `:caption:` — which authors write in
/// prose — is rendered as visible text below it, where a screen reader reaches
/// it anyway.
const DIAGRAM_ALT: &str = "PlantUML Diagram";

/// Expands the diagram's template, recording a failure against the directive.
///
/// The expansion goes through `rusty_sphinx_uml` rather than being done here,
/// because the build action that writes the `.puml` file expands the very same
/// template and the two must agree byte for byte — the hash is the SVG's
/// filename, so a renderer with its own idea of the text would emit an `<img>`
/// pointing at a file nothing compiled.
fn expanded_content(uml: &Uml, ctx: &mut RenderCtx) -> Option<HashedContent> {
    let uml_ctx =
        UmlContext::new(ctx.index, ctx.schema, ctx.original_doc_path).with_configs(ctx.uml_configs);
    match expand(uml, &uml_ctx) {
        Ok(content) => {
            // Once per distinct picture: two identical diagrams on one page
            // share one compiled SVG, so they need one source file too.
            if !ctx
                .diagram_sources
                .iter()
                .any(|seen| seen.hash() == content.hash())
            {
                ctx.diagram_sources.push(content.clone());
            }
            Some(content)
        }
        Err(error) => {
            ctx.diagram_errors.push(DiagramError {
                directive: uml.source.as_str().to_string(),
                error,
                span: uml.span,
            });
            None
        }
    }
}

/// The `src` a compiled SVG is served from, relative to the page.
///
/// The hash names the file, so two documents drawing the same diagram share
/// one compiled picture and a diagram that did not change keeps its filename
/// across builds — which is what lets Bazel skip recompiling it.
fn diagram_src(content: &HashedContent, ctx: &RenderCtx<'_>) -> String {
    relative_asset_href(
        std::path::Path::new(&format!("{}.svg", content.hash())),
        ctx.doc_path,
    )
}

/// The classes on the wrapper: the diagram's own marker, its `:align:` and
/// whatever `:class:` added.
fn wrapper_classes(uml: &Uml) -> String {
    let mut classes = vec!["plantuml-diagram".to_string()];
    classes.extend(uml.align.map(ImageAlign::css_class));
    classes.extend(uml.classes.iter().cloned());
    classes.join(" ")
}

/// Renders a diagram directive.
///
/// The markup for an option-free diagram is exactly what this build has always
/// produced, because the overwhelming majority of diagrams carry no options
/// and a changed wrapper would invalidate every cached page for nothing.
pub(super) fn render_uml_directive(html: &mut String, uml: &Uml, ctx: &mut RenderCtx) {
    // A diagram whose template failed to expand was never compiled, so there
    // is no picture to point at. The failure is already recorded against the
    // directive; rendering a broken `<img>` on top of it would say the same
    // thing twice, once unhelpfully.
    let Some(content) = expanded_content(uml, ctx) else {
        return;
    };

    let src = diagram_src(&content, ctx);
    let src = html_escape::encode_double_quoted_attribute(&src);
    let classes = wrapper_classes(uml);
    let classes = html_escape::encode_double_quoted_attribute(&classes);

    let _ = write!(html, "<div class=\"{classes}\"");
    if let Some(name) = &uml.name {
        let id_attr = html_escape::encode_double_quoted_attribute(name.as_str());
        let _ = write!(html, " id=\"{id_attr}\"");
    }
    let _ = writeln!(html, ">");

    let _ = write!(html, "  <img src=\"{src}\" alt=\"{DIAGRAM_ALT}\"");
    if let Some(width) = uml.rendered_width() {
        let style = format!("width: {width}");
        let style = html_escape::encode_double_quoted_attribute(&style);
        let _ = write!(html, " style=\"{style}\"");
    }
    let _ = writeln!(html, " />");

    if let Some(caption) = &uml.caption {
        let text = html_escape::encode_text(caption);
        let _ = writeln!(html, "  <p class=\"caption\">{text}</p>");
    }

    let _ = writeln!(html, "</div>");

    if uml.debug {
        render_debug_source(html, &content);
    }
}

/// Renders the `:debug:` block: the `PlantUML` text that was actually compiled.
///
/// Shown *after* the picture rather than instead of it, because the option
/// exists to explain a diagram that came out wrong — an author needs to see
/// both the result and the source that produced it.
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
    use rusty_sphinx_ast::{LengthOrPercentage, ResolvedLanguage, TargetName, UmlSource};
    use rusty_sphinx_index::ProjectIndex;

    use crate::EmbeddedAssets;
    use crate::blocks::render_test_support::with_ctx_for;

    /// Renders `uml` on the page at `doc_path` and returns the HTML.
    fn render_on(uml: &Uml, doc_path: &str) -> String {
        let index = ProjectIndex::default();
        with_ctx_for(
            &index,
            doc_path,
            ResolvedLanguage::default(),
            &EmbeddedAssets::new(),
            |ctx| {
                let mut html = String::new();
                render_uml_directive(&mut html, uml, ctx);
                html
            },
        )
    }

    /// Renders each of `umls` on one page and returns the diagram sources the
    /// render recorded for the compile action.
    fn sources_of(umls: &[Uml]) -> Vec<HashedContent> {
        let index = ProjectIndex::default();
        with_ctx_for(
            &index,
            "index.rst",
            ResolvedLanguage::default(),
            &EmbeddedAssets::new(),
            |ctx| {
                let mut html = String::new();
                for uml in umls {
                    render_uml_directive(&mut html, uml, ctx);
                }
                ctx.diagram_sources.clone()
            },
        )
    }

    /// Renders `uml` on a root-level page and returns the HTML.
    fn render(uml: &Uml) -> String {
        render_on(uml, "index.rst")
    }

    /// The hash naming the SVG this diagram compiles to.
    ///
    /// Computed through the expander rather than restated here, because a
    /// test with its own idea of the hash would pass while the page pointed
    /// at a file nothing compiled — the exact failure the shared function
    /// exists to prevent.
    fn expected_hash(uml: &Uml) -> String {
        let index = ProjectIndex::default();
        let ctx = UmlContext::new(
            &index,
            rusty_sphinx_entity::EntitySchema::empty_ref(),
            "index.rst",
        );
        expand(uml, &ctx)
            .expect("expansion succeeds")
            .hash()
            .to_string()
    }

    fn diagram() -> Uml {
        Uml::new(UmlSource::PlantUml, "A -> B".to_string())
    }

    #[test]
    fn test_an_option_free_diagram_renders_the_markup_it_always_has() {
        // Given
        let uml = diagram();
        let hash = expected_hash(&uml);

        // When
        let html = render(&uml);

        // Then
        assert_eq!(
            html,
            format!(
                "<div class=\"plantuml-diagram\">\n  <img src=\"_images/{hash}.svg\" \
                 alt=\"PlantUML Diagram\" />\n</div>\n"
            )
        );
    }

    #[test]
    fn test_the_recorded_source_is_the_one_the_img_names() {
        // Given
        let uml = diagram();

        // When
        let sources = sources_of(std::slice::from_ref(&uml));
        let html = render(&uml);

        // Then — the file the compile action gets and the file the page
        // points at are named by one hash, from one render
        assert_eq!(sources.len(), 1);
        assert!(
            html.contains(&format!("{}.svg", sources[0].hash())),
            "{html}"
        );
    }

    #[test]
    fn test_two_identical_diagrams_record_one_source() {
        // Given — they share one compiled SVG, so they need one source file
        let uml = diagram();

        // When
        let sources = sources_of(&[uml.clone(), uml]);

        // Then
        assert_eq!(sources.len(), 1);
    }

    #[test]
    fn test_distinct_diagrams_record_their_sources_in_document_order() {
        // Given
        let first = Uml::new(UmlSource::PlantUml, "A -> B".to_string());
        let second = Uml::new(UmlSource::PlantUml, "C -> D".to_string());

        // When
        let sources = sources_of(&[first, second]);

        // Then
        let bodies: Vec<&str> = sources.iter().map(HashedContent::body).collect();
        assert_eq!(
            bodies,
            ["@startuml\nA -> B\n@enduml", "@startuml\nC -> D\n@enduml"]
        );
    }

    #[test]
    fn test_a_diagram_that_failed_to_expand_records_no_source() {
        // Given — an architecture diagram outside any entity, which is refused
        let uml = Uml::new(UmlSource::EntityArch, "{{ need.id }}".to_string());

        // When
        let sources = sources_of(&[uml]);

        // Then — nothing to compile, so no file to write
        assert!(sources.is_empty());
    }

    #[test]
    fn test_every_spelling_of_the_same_template_renders_one_picture() {
        // Given — the spelling is recorded for diagnostics, and must not reach
        // the rendered page
        let plantuml = render(&Uml::new(UmlSource::PlantUml, "A -> B".to_string()));

        // When
        let uml = render(&Uml::new(UmlSource::Uml, "A -> B".to_string()));

        // Then
        assert_eq!(plantuml, uml);
    }

    #[test]
    fn test_align_and_class_land_on_the_wrapper() {
        // Given
        let uml = Uml {
            align: Some(ImageAlign::Center),
            classes: vec!["wide".to_string()],
            ..diagram()
        };

        // When
        let html = render(&uml);

        // Then
        assert!(
            html.contains("<div class=\"plantuml-diagram align-center wide\">"),
            "{html}"
        );
    }

    #[test]
    fn test_a_named_diagram_carries_an_id_a_ref_can_land_on() {
        // Given
        let uml = Uml {
            name: Some(TargetName::new("my-diagram")),
            ..diagram()
        };

        // When
        let html = render(&uml);

        // Then
        assert!(html.contains("id=\"my-diagram\""), "{html}");
    }

    #[test]
    fn test_a_width_becomes_a_style_with_the_scale_applied() {
        // Given
        let uml = Uml {
            width: Some(LengthOrPercentage::new("400px").expect("a valid length")),
            scale: Some(50),
            ..diagram()
        };

        // When
        let html = render(&uml);

        // Then
        assert!(html.contains("style=\"width: 200px\""), "{html}");
    }

    #[test]
    fn test_a_scale_without_a_width_changes_nothing() {
        // Given — the parser has already reported this; the renderer must not
        // invent a size for it
        let uml = Uml {
            scale: Some(50),
            ..diagram()
        };

        // When
        let html = render(&uml);

        // Then
        assert!(!html.contains("style="), "{html}");
    }

    #[test]
    fn test_a_caption_renders_below_the_picture() {
        // Given
        let uml = Uml {
            caption: Some("How A talks to B".to_string()),
            ..diagram()
        };

        // When
        let html = render(&uml);

        // Then
        let img = html.find("<img").expect("a picture is rendered");
        let caption = html
            .find("<p class=\"caption\">How A talks to B</p>")
            .expect("the caption is rendered");
        assert!(caption > img, "{html}");
    }

    #[test]
    fn test_a_caption_is_escaped() {
        // Given
        let uml = Uml {
            caption: Some("A <-> B & C".to_string()),
            ..diagram()
        };

        // When
        let html = render(&uml);

        // Then
        assert!(html.contains("A &lt;-&gt; B &amp; C"), "{html}");
    }

    #[test]
    fn test_debug_shows_the_compiled_source_after_the_picture() {
        // Given
        let uml = Uml {
            debug: true,
            ..diagram()
        };

        // When
        let html = render(&uml);

        // Then
        let picture = html.find("</div>").expect("a picture is rendered");
        let debug = html
            .find("<pre class=\"plantuml-debug\"><code>@startuml\nA -&gt; B\n@enduml</code></pre>")
            .expect("the source is shown");
        assert!(debug > picture, "{html}");
    }

    #[test]
    fn test_the_src_is_relative_to_the_page_doing_the_linking() {
        // Given — a page two directories down
        let uml = diagram();
        let hash = expected_hash(&uml);

        // When
        let html = render_on(&uml, "team_a/guide/index.rst");

        // Then
        assert!(
            html.contains(&format!("../../_images/{hash}.svg")),
            "{html}"
        );
    }
}
