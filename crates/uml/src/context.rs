//! What a diagram is expanded *against*.

use std::collections::BTreeMap;

use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_index::ProjectIndex;

/// Everything [`expand`](crate::expand) resolves a template's questions with.
///
/// Every field is an input the expansion's bytes may depend on, and that is
/// the rule for adding another: equal contexts must produce equal text, build
/// after build, or an unchanged diagram hashes differently and its compile
/// action misses the cache. Nothing here is mutable and nothing is read from
/// the filesystem — a diagram's text is a pure function of the project.
pub struct UmlContext<'a> {
    /// The project-wide index: the entities a template may ask about, and the
    /// documents it may link to.
    pub index: &'a ProjectIndex,
    /// The entity meta-model, for the type labels and relation names a
    /// template reads.
    pub schema: &'a EntitySchema,
    /// The source path of the document the diagram was written in, which is
    /// what a generated link is made relative to.
    pub doc_path: &'a str,
    /// The named `PlantUML` preambles a `:config:` may ask for, by name.
    ///
    /// Text rather than a path, so it survives a sandbox relocating files, and
    /// read from the site config the render action is already given.
    pub configs: &'a BTreeMap<String, String>,
}

// The entity a diagram sits inside is deliberately *not* a field here. It
// travels on the [`Uml`](rusty_sphinx_ast::Uml) node instead, recorded by the
// parser — the only phase that still knows it — so no later caller has to
// rediscover it by walking outwards from the node, and none can get it wrong.

impl<'a> UmlContext<'a> {
    /// A context for expanding the diagrams of one document.
    #[must_use]
    pub const fn new(index: &'a ProjectIndex, schema: &'a EntitySchema, doc_path: &'a str) -> Self {
        Self {
            index,
            schema,
            doc_path,
            configs: EMPTY_CONFIGS,
        }
    }

    /// The same context, with the site's named `PlantUML` preambles available.
    #[must_use]
    pub const fn with_configs(self, configs: &'a BTreeMap<String, String>) -> Self {
        Self { configs, ..self }
    }
}

/// The empty preamble table a context defaults to.
///
/// A `const` rather than a field built per call, so [`UmlContext::new`] can
/// stay `const` and a caller with no configs allocates nothing.
static EMPTY_CONFIGS: &BTreeMap<String, String> = &BTreeMap::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_carries_the_project_and_the_page() {
        // Given
        let index = ProjectIndex::default();
        let schema = EntitySchema::empty_ref();

        // When
        let ctx = UmlContext::new(&index, schema, "index.rst");

        // Then
        assert_eq!(ctx.doc_path, "index.rst");
        assert!(ctx.index.entities.is_empty());
        assert!(ctx.configs.is_empty());
    }

    #[test]
    fn test_with_configs_makes_the_preambles_available() {
        // Given
        let index = ProjectIndex::default();
        let schema = EntitySchema::empty_ref();
        let configs =
            BTreeMap::from([("mono".to_string(), "skinparam monochrome true".to_string())]);

        // When
        let ctx = UmlContext::new(&index, schema, "index.rst").with_configs(&configs);

        // Then
        assert_eq!(
            ctx.configs.get("mono").map(String::as_str),
            Some("skinparam monochrome true")
        );
    }
}
