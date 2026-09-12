//! The last step every diagram goes through: from text to the `PlantUML` file
//! that is compiled.
//!
//! Shared by the two ways a diagram's text comes about — a template that was
//! expanded ([`crate::expand`]) and a flowchart that was generated
//! ([`crate::build_flow`]) — because everything after the text exists is the
//! same for both: the `:config:` preamble, the `@startuml`/`@enduml` markers,
//! the refusal to compile a picture with nothing in it, and the hash that names
//! the SVG. Two copies of this would be two populations of diagram hashes, and
//! the whole point of one pipeline is that there is only ever one.

use rusty_sphinx_ast::HashedContent;

use crate::context::UmlContext;

/// The `PlantUML` preambles a `:config:` may name without the site declaring
/// anything.
///
/// sphinx-needs ships these two in `needs_flow_configs`, so a document writing
/// `:config: toptobottom` is valid there against a `conf.py` that never
/// mentions them — and half of the demo corpus's `.. needflow::` directives do
/// exactly that. Without them such a diagram is refused for naming a preamble
/// nobody declared, which is true of *our* config and useless to the author.
///
/// A name the site declares wins, so a project may redefine either.
const BUILTIN_CONFIGS: [(&str, &str); 2] = [
    ("lefttoright", "left to right direction"),
    ("toptobottom", "top to bottom direction"),
];

/// Why there is no finished diagram.
///
/// Deliberately small and shared: each caller maps it onto its own error type,
/// because a diagnostic code names the construct an author wrote — a flowchart
/// that drew nothing is `entity-flow.empty-result`, not `uml.empty-result`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AssemblyError {
    /// The text holds no diagram content at all.
    DrewNothing,
    /// A `:config:` naming a preamble the site config does not declare.
    UnknownConfig(String),
}

/// `text` as the content to compile, or why there is none.
///
/// An assembly that drew nothing is refused here rather than handed on.
/// `PlantUML` rejects an empty diagram, so compiling one fails the whole build
/// with a syntax error naming a generated file the author never wrote — while
/// the real cause is usually a filter that matched nothing, which is a content
/// problem and belongs in a warning beside the directive.
pub(crate) fn finished(
    text: &str,
    config: Option<&str>,
    ctx: &UmlContext<'_>,
) -> Result<HashedContent, AssemblyError> {
    if drew_nothing(text) {
        return Err(AssemblyError::DrewNothing);
    }
    let preamble = match config {
        Some(name) => Some(
            preamble_named(name, ctx)
                .ok_or_else(|| AssemblyError::UnknownConfig(name.to_string()))?,
        ),
        None => None,
    };
    Ok(HashedContent::new(wrapped(text, preamble)))
}

/// The preamble `name` stands for: the site's, or a sphinx-needs built-in.
///
/// The site's table is consulted first, so declaring `toptobottom` in
/// `[uml_configs]` redefines it rather than colliding with it.
fn preamble_named<'a>(name: &str, ctx: &'a UmlContext<'_>) -> Option<&'a str> {
    if let Some(declared) = ctx.configs.get(name) {
        return Some(declared.as_str());
    }
    BUILTIN_CONFIGS
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, preamble)| *preamble)
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
    use rusty_sphinx_entity::EntitySchema;
    use rusty_sphinx_index::ProjectIndex;

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
    fn test_a_bare_fragment_gets_the_markers_plantuml_requires() {
        // Given
        let text = "A -> B";

        // When
        let wrapped = wrapped(text, None);

        // Then
        assert_eq!(wrapped, "@startuml\nA -> B\n@enduml");
    }

    #[test]
    fn test_text_that_already_opens_with_the_marker_is_untouched() {
        // Given — wrapping one twice would move the hash of every diagram
        // this build has ever compiled
        let text = "@startuml\nA -> B\n@enduml";

        // When
        let wrapped = wrapped(text, None);

        // Then
        assert_eq!(wrapped, text);
    }

    #[test]
    fn test_a_preamble_lands_inside_the_markers_whoever_wrote_them() {
        // Given — PlantUML silently ignores a `skinparam` written outside
        let preamble = Some("skinparam monochrome true");

        // When
        let authored = wrapped("@startuml\nA -> B\n@enduml", preamble);
        let generated = wrapped("A -> B", preamble);

        // Then
        assert_eq!(
            authored,
            "@startuml\nskinparam monochrome true\nA -> B\n@enduml"
        );
        assert_eq!(authored, generated);
    }

    #[test]
    fn test_a_config_naming_no_preamble_is_refused() {
        // Given — a diagram silently missing the styling its author asked for
        // looks finished and is wrong
        let index = ProjectIndex::default();
        let ctx = UmlContext::new(&index, EntitySchema::empty_ref(), "index.rst");

        // When
        let error = finished("A -> B", Some("mono"), &ctx).expect_err("refused");

        // Then
        assert_eq!(error, AssemblyError::UnknownConfig("mono".to_string()));
    }

    #[test]
    fn test_text_with_nothing_in_it_is_refused_before_anything_is_compiled() {
        // Given
        let index = ProjectIndex::default();
        let ctx = UmlContext::new(&index, EntitySchema::empty_ref(), "index.rst");

        // When
        let error = finished("@startuml\n\n@enduml", None, &ctx).expect_err("refused");

        // Then
        assert_eq!(error, AssemblyError::DrewNothing);
    }

    #[test]
    fn test_a_sphinx_needs_builtin_preamble_needs_no_site_config() {
        // Given — sphinx-needs ships `needs_flow_configs` with these two, so a
        // corpus writing `:config: toptobottom` declares nothing anywhere
        let index = ProjectIndex::default();
        let ctx = UmlContext::new(&index, EntitySchema::empty_ref(), "index.rst");

        // When
        let content = finished("A -> B", Some("toptobottom"), &ctx).expect("the built-in resolves");

        // Then
        assert_eq!(
            content.body(),
            "@startuml\ntop to bottom direction\nA -> B\n@enduml"
        );
    }

    #[test]
    fn test_a_site_declaration_redefines_a_built_in_rather_than_colliding() {
        // Given
        let index = ProjectIndex::default();
        let configs = std::collections::BTreeMap::from([(
            "toptobottom".to_string(),
            "skinparam monochrome true".to_string(),
        )]);
        let ctx =
            UmlContext::new(&index, EntitySchema::empty_ref(), "index.rst").with_configs(&configs);

        // When
        let content = finished("A -> B", Some("toptobottom"), &ctx).expect("the site's wins");

        // Then
        assert!(content.body().contains("skinparam monochrome true"));
        assert!(!content.body().contains("top to bottom"));
    }

    #[test]
    fn test_a_name_neither_the_site_nor_sphinx_needs_declares_is_still_refused() {
        // Given
        let index = ProjectIndex::default();
        let ctx = UmlContext::new(&index, EntitySchema::empty_ref(), "index.rst");

        // When
        let error = finished("A -> B", Some("housestyle"), &ctx).expect_err("refused");

        // Then
        assert_eq!(
            error,
            AssemblyError::UnknownConfig("housestyle".to_string())
        );
    }
}
