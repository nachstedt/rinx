//! Rendering every spelling of a *written* `PlantUML` diagram.
//!
//! What this module owns is the one thing a written diagram has that a
//! generated one does not: a template, which must be expanded against the
//! entity graph before there is any text to compile. Everything after that —
//! the `<img>`, the wrapper, the caption, the `:debug:` block and the source
//! handed back for the build to compile — is [`super::diagram_figure`], shared
//! with `.. entity-flow::` so two pictures on one page cannot be told apart by
//! how their text came about.
//!
//! The expansion goes through `rinx_uml` rather than being done here,
//! because the hash of the result *is* the compiled SVG's filename: a renderer
//! with its own idea of the text would emit an `<img>` pointing at a file
//! nothing compiled.

use rinx_ast::{HashedContent, Uml};
use rinx_uml::{UmlContext, expand};

use crate::RenderCtx;
use crate::uml_error::DiagramError;

use super::diagram_figure::{DiagramFigure, record_diagram_source, render_diagram_figure};

/// Expands the diagram's template, recording a failure against the directive.
fn expanded_content(uml: &Uml, ctx: &mut RenderCtx) -> Option<HashedContent> {
    let uml_ctx =
        UmlContext::new(ctx.index, ctx.schema, ctx.original_doc_path).with_configs(ctx.uml_configs);
    match expand(uml, &uml_ctx) {
        Ok(content) => {
            record_diagram_source(ctx, &content);
            Some(content)
        }
        Err(error) => {
            ctx.diagram_errors.push(DiagramError {
                directive: uml.source.as_str().to_string(),
                error: error.into(),
                span: uml.span,
            });
            None
        }
    }
}

/// Renders a diagram directive.
pub(super) fn render_uml_directive(html: &mut String, uml: &Uml, ctx: &mut RenderCtx) {
    // A diagram whose template failed to expand was never compiled, so there
    // is no picture to point at. The failure is already recorded against the
    // directive; rendering a broken `<img>` on top of it would say the same
    // thing twice, once unhelpfully.
    let Some(content) = expanded_content(uml, ctx) else {
        return;
    };

    render_diagram_figure(
        html,
        &DiagramFigure {
            content: &content,
            classes: &uml.classes,
            align: uml.align,
            width: uml.rendered_width(),
            caption: uml.caption.as_deref(),
            name: uml.name.as_ref(),
            debug: uml.debug,
        },
        ctx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{ImageAlign, LengthOrPercentage, ResolvedLanguage, TargetName, UmlSource};
    use rinx_index::ProjectIndex;

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
        let ctx = UmlContext::new(&index, rinx_entity::EntitySchema::empty_ref(), "index.rst");
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
