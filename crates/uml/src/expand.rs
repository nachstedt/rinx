//! The expansion itself: a diagram's template to the `PlantUML` text compiled.

use rusty_sphinx_ast::{EntityId, HashedContent, Uml};

use crate::context::UmlContext;
use crate::error::UmlError;
use crate::snapshot::Snapshot;
use crate::template::render;

/// Expands one diagram's template into the `PlantUML` text to compile.
///
/// The single point of truth for what a diagram's bytes are. The render action
/// calls it once per diagram, links the page's `<img>` to the resulting hash
/// and hands the text back for the build to compile — so the picture a page
/// names and the file compiled for it come from one call.
///
/// # Errors
///
/// Returns [`UmlError`] when the template cannot be evaluated. An untemplated
/// diagram cannot fail: there is nothing in it to go wrong.
pub fn expand(uml: &Uml, ctx: &UmlContext<'_>) -> Result<HashedContent, UmlError> {
    if !uml.source.is_templated() {
        // A template with nothing to expand is its own expansion. This is what
        // lets one pipeline serve a plain `.. plantuml::` without changing the
        // hash it has always had — and it is why the snapshot below, which is
        // linear in the size of the project, is never built for one.
        return finished(&uml.template, uml, ctx);
    }

    // An architecture diagram exists to draw the entity it sits in, so one
    // written outside any entity is refused rather than expanded against a
    // `need` bound to nothing — which would render a plausible, empty picture.
    //
    // The enclosing entity comes off the *node*, recorded while parsing — the
    // only phase that still knew it — and never off the context.
    if uml.source.binds_enclosing_entity() && uml.entity.is_none() {
        return Err(UmlError::ArchOutsideEntity);
    }

    let snapshot = Snapshot::build(ctx.index, ctx.schema, ctx.doc_path);
    let text = render(
        &uml.template,
        snapshot,
        &uml.extra,
        uml.entity.as_ref().map(EntityId::as_str),
    )?;
    finished(&text, uml, ctx)
}

/// The expanded text as the content to compile, or why there is none.
///
/// An expansion that drew nothing is refused here rather than handed on.
/// `PlantUML` rejects an empty diagram, so compiling one fails the whole build
/// with a syntax error naming a generated file the author never wrote — while
/// the real cause is usually a `filter()` that matched nothing, which is a
/// content problem and belongs in a warning beside the directive.
fn finished(text: &str, uml: &Uml, ctx: &UmlContext<'_>) -> Result<HashedContent, UmlError> {
    if drew_nothing(text) {
        return Err(UmlError::EmptyDiagram);
    }
    Ok(HashedContent::new(assembled(text, uml, ctx)?))
}

/// Whether `text` holds no diagram content — only whitespace, and possibly the
/// `@startuml`/`@enduml` markers around it.
///
/// The markers have to be discounted because a template usually writes them
/// itself, so the empty case arrives as `@startuml\n\n@enduml` rather than as
/// an empty string.
fn drew_nothing(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .all(|line| line.starts_with("@start") || line.starts_with("@end"))
}

/// The finished `PlantUML` file: the diagram's text, its `:config:` preamble,
/// and the `@startuml`/`@enduml` markers around both.
///
/// Assembled in one place because the three interact — a preamble has to land
/// *inside* the markers, whether the author wrote them or the wrapper added
/// them, and `PlantUML` silently ignores a `skinparam` written outside.
fn assembled(text: &str, uml: &Uml, ctx: &UmlContext<'_>) -> Result<String, UmlError> {
    let preamble = match &uml.config {
        Some(name) => Some(
            ctx.configs
                .get(name)
                .ok_or_else(|| UmlError::UnknownConfig(name.clone()))?
                .as_str(),
        ),
        None => None,
    };
    Ok(wrapped(text, preamble))
}

/// `text` with `@startuml`/`@enduml` around it, and `preamble` inside them.
///
/// Both sphinxcontrib-plantuml and sphinx-needs add the markers when they are
/// missing, and here that earns its place twice over. `PlantUML` refuses a file
/// without them outright, so a diagram written as a bare fragment would
/// otherwise fail the build with a message about the *compiler* rather than
/// about the document. And a fragment is exactly what a `:key:` diagram is:
/// written to be imported into somebody else's `@startuml`, but still a
/// diagram in its own right on the page it was written on.
///
/// A diagram that already opens with the marker keeps its own text untouched
/// when it also has no preamble, so no existing picture — and so no existing
/// hash — moves.
fn wrapped(text: &str, preamble: Option<&str>) -> String {
    let trimmed = text.trim();
    let Some(preamble) = preamble else {
        if trimmed.starts_with("@start") {
            return text.to_string();
        }
        return format!("@startuml\n{trimmed}\n@enduml");
    };

    // The preamble goes directly after the opening marker, which is the only
    // place `PlantUML` reads a `skinparam` from.
    match trimmed.split_once('\n') {
        Some((first, rest)) if first.trim_start().starts_with("@start") => {
            format!("{first}\n{}\n{rest}", preamble.trim())
        }
        _ => format!("@startuml\n{}\n{trimmed}\n@enduml", preamble.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::UmlSource;
    use rusty_sphinx_entity::EntitySchema;
    use rusty_sphinx_index::ProjectIndex;

    /// Runs `body` with a context over an empty project.
    fn with_ctx<R>(body: impl FnOnce(&UmlContext<'_>) -> R) -> R {
        let index = ProjectIndex::default();
        let schema = EntitySchema::empty_ref();
        body(&UmlContext::new(&index, schema, "index.rst"))
    }

    #[test]
    fn test_an_untemplated_diagram_expands_to_itself() {
        // Given — the hash a `.. plantuml::` has always had must not move
        let uml = Uml::new(UmlSource::PlantUml, "A -> B".to_string());

        // When
        let content = with_ctx(|ctx| expand(&uml, ctx)).expect("nothing can fail");

        // Then
        assert_eq!(content.body(), "@startuml\nA -> B\n@enduml");
    }

    #[test]
    fn test_a_diagram_that_already_opens_with_the_marker_is_left_alone() {
        // Given — wrapping one twice would move the hash of every diagram
        // this build has ever compiled
        let body = "@startuml\nA -> B\n@enduml";
        let uml = Uml::new(UmlSource::PlantUml, body.to_string());

        // When
        let content = with_ctx(|ctx| expand(&uml, ctx)).expect("nothing can fail");

        // Then
        assert_eq!(content, HashedContent::new(body.to_string()));
    }

    #[test]
    fn test_expansion_is_deterministic() {
        // Given — the hash is the SVG's filename, so two runs producing
        // different bytes produce a page pointing at a file nothing compiled
        let uml = Uml::new(UmlSource::EntityDiagram, "A -> B".to_string());

        // When
        let first = with_ctx(|ctx| expand(&uml, ctx)).expect("expansion succeeds");
        let second = with_ctx(|ctx| expand(&uml, ctx)).expect("expansion succeeds");

        // Then
        assert_eq!(first.hash(), second.hash());
        assert_eq!(first.body(), second.body());
    }

    #[test]
    fn test_a_filter_matching_nothing_is_reported_rather_than_compiled() {
        // Given — the empty case arrives as `@startuml\n\n@enduml`, which
        // PlantUML refuses; compiling it would fail the whole build with a
        // syntax error naming a file the author never wrote
        let uml = Uml::new(
            UmlSource::EntityDiagram,
            "@startuml\n{% for id in filter('type == \"req\"') %}\n{{ flow(id) }}\n{% endfor %}\n@enduml"
                .to_string(),
        );

        // When — over a project with no entities at all
        let error = with_ctx(|ctx| expand(&uml, ctx)).expect_err("nothing was drawn");

        // Then
        assert_eq!(error, UmlError::EmptyDiagram);
    }

    #[test]
    fn test_a_diagram_with_no_body_at_all_is_reported() {
        // Given
        let uml = Uml::new(UmlSource::PlantUml, String::new());

        // When
        let error = with_ctx(|ctx| expand(&uml, ctx)).expect_err("nothing was drawn");

        // Then
        assert_eq!(error, UmlError::EmptyDiagram);
    }

    #[test]
    fn test_one_line_of_content_is_enough_to_draw() {
        // Given — the check must not mistake a real, small diagram for an
        // empty one
        let uml = Uml::new(
            UmlSource::PlantUml,
            "@startuml\nnode A\n@enduml".to_string(),
        );

        // When / Then
        assert!(with_ctx(|ctx| expand(&uml, ctx)).is_ok());
    }

    #[test]
    fn test_drew_nothing_discounts_only_the_markers() {
        // Given / When / Then
        assert!(drew_nothing(""));
        assert!(drew_nothing("  \n\t\n"));
        assert!(drew_nothing("@startuml\n\n@enduml"));
        assert!(drew_nothing("@startuml\n@enduml"));
        assert!(!drew_nothing("node A"));
        assert!(!drew_nothing("@startuml\nnode A\n@enduml"));
    }

    #[test]
    fn test_a_config_preamble_lands_inside_the_markers() {
        // Given — PlantUML reads a `skinparam` only from inside @startuml
        let configs = std::collections::BTreeMap::from([(
            "mono".to_string(),
            "skinparam monochrome true".to_string(),
        )]);
        let uml = Uml {
            config: Some("mono".to_string()),
            ..Uml::new(
                UmlSource::PlantUml,
                "@startuml\nA -> B\n@enduml".to_string(),
            )
        };
        let index = ProjectIndex::default();
        let ctx =
            UmlContext::new(&index, EntitySchema::empty_ref(), "index.rst").with_configs(&configs);

        // When
        let content = expand(&uml, &ctx).expect("expansion succeeds");

        // Then
        assert_eq!(
            content.body(),
            "@startuml\nskinparam monochrome true\nA -> B\n@enduml"
        );
    }

    #[test]
    fn test_a_config_preamble_is_added_to_an_unwrapped_diagram_too() {
        // Given
        let configs = std::collections::BTreeMap::from([(
            "mono".to_string(),
            "skinparam monochrome true".to_string(),
        )]);
        let uml = Uml {
            config: Some("mono".to_string()),
            ..Uml::new(UmlSource::PlantUml, "A -> B".to_string())
        };
        let index = ProjectIndex::default();
        let ctx =
            UmlContext::new(&index, EntitySchema::empty_ref(), "index.rst").with_configs(&configs);

        // When
        let content = expand(&uml, &ctx).expect("expansion succeeds");

        // Then
        assert_eq!(
            content.body(),
            "@startuml\nskinparam monochrome true\nA -> B\n@enduml"
        );
    }

    #[test]
    fn test_a_config_naming_no_preamble_is_refused() {
        // Given — a diagram silently missing the styling its author asked for
        // looks finished and is wrong
        let uml = Uml {
            config: Some("mono".to_string()),
            ..Uml::new(UmlSource::PlantUml, "A -> B".to_string())
        };

        // When
        let error = with_ctx(|ctx| expand(&uml, ctx)).expect_err("refused");

        // Then
        assert_eq!(error, UmlError::UnknownConfig("mono".to_string()));
    }

    #[test]
    fn test_an_arch_written_outside_an_entity_is_refused() {
        // Given — a `need` bound to nothing would draw a plausible, empty
        // picture, which is worse than saying so
        let uml = Uml::new(UmlSource::EntityArch, "{{ need.id }}".to_string());

        // When
        let error = with_ctx(|ctx| expand(&uml, ctx)).expect_err("refused");

        // Then
        assert_eq!(error, UmlError::ArchOutsideEntity);
    }

    #[test]
    fn test_an_arch_inside_an_entity_draws_it() {
        // Given — the entity comes off the node, recorded while parsing
        let index = crate::snapshot::tests::index_with(vec![(
            "REQ_001",
            crate::snapshot::tests::requirement("reqs.rst", Some("Login")),
        )]);
        let uml = Uml {
            entity: Some(rusty_sphinx_ast::EntityId::new("REQ_001").expect("a valid id")),
            ..Uml::new(UmlSource::EntityArch, "{{ need.title }}".to_string())
        };
        let schema = EntitySchema::empty_ref();
        let ctx = UmlContext::new(&index, schema, "index.rst");

        // When
        let content = expand(&uml, &ctx).expect("expansion succeeds");

        // Then
        assert_eq!(content.body(), "@startuml\nLogin\n@enduml");
    }

    #[test]
    fn test_a_templated_diagram_hashes_its_expansion_not_its_template() {
        // Given — two projects differing only in the entity a template reads
        let template = "{{ need('REQ_001').title }}";
        let first = crate::snapshot::tests::index_with(vec![(
            "REQ_001",
            crate::snapshot::tests::requirement("reqs.rst", Some("Login")),
        )]);
        let second = crate::snapshot::tests::index_with(vec![(
            "REQ_001",
            crate::snapshot::tests::requirement("reqs.rst", Some("Logout")),
        )]);
        let uml = Uml::new(UmlSource::EntityDiagram, template.to_string());
        let schema = EntitySchema::empty_ref();

        // When
        let a = expand(&uml, &UmlContext::new(&first, schema, "index.rst")).unwrap();
        let b = expand(&uml, &UmlContext::new(&second, schema, "index.rst")).unwrap();

        // Then — the SVG's filename follows the picture, not the source text,
        // which is what makes a diagram recompile when the graph it draws moves
        assert_ne!(a.hash(), b.hash());
    }
}
