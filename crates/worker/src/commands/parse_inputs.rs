//! Everything a subcommand needs in order to *parse*, bundled.
//!
//! Three settings travel together wherever RST is turned into an AST — the
//! default domain, the loader a file-reading directive goes through, and the
//! entity schema that supplies the project's own directive vocabulary. Both
//! `parse` and `preview` take all three, so they are one parameter rather than
//! three, for the same reason `ParseCtx` exists in the parser: the next
//! parse-time setting should not have to touch every signature again.

use rusty_sphinx_ast as ast;
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_parser::ParseCtx;

use super::parse_files::{DocumentRelativeFiles, parse_ctx};

/// The parse-time configuration one subcommand run works under.
pub(super) struct ParseInputs<'a> {
    /// The domain a bare directive or role resolves to.
    pub default_domain: ast::Domain,
    /// How a file-reading directive reaches the filesystem.
    pub files: &'a DocumentRelativeFiles,
    /// The project's entity meta-model.
    pub schema: &'a EntitySchema,
}

impl<'a> ParseInputs<'a> {
    /// Builds the parser context these inputs describe.
    pub(super) fn ctx(&self) -> ParseCtx<'a> {
        parse_ctx(self.default_domain, self.files).with_schema(self.schema)
    }

    /// The hash to stamp on a parsed document, or `None` without a schema.
    ///
    /// `None` rather than the empty schema's hash so a project that does not
    /// use entities writes no field at all, keeping its `.ast` files byte-
    /// identical to what this build produced before entities existed.
    pub(super) fn schema_hash(&self) -> Option<String> {
        (!self.schema.is_empty()).then(|| self.schema.hash().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_files() -> DocumentRelativeFiles {
        DocumentRelativeFiles::for_document("doc.rst")
    }

    #[test]
    fn test_the_context_carries_the_declared_domain_and_schema() {
        // Given
        let files = no_files();
        let schema = rusty_sphinx_entity::load_schema(
            "[[entity_type]]\nname = \"req\"\n",
            &rusty_sphinx_entity::NoReservedNames,
        )
        .unwrap();
        let inputs = ParseInputs {
            default_domain: ast::Domain::C,
            files: &files,
            schema: &schema,
        };

        // When
        let ctx = inputs.ctx();

        // Then
        assert_eq!(ctx.default_domain, ast::Domain::C);
        assert!(ctx.schema.entity_type("req").is_some());
    }

    #[test]
    fn test_a_project_without_entities_stamps_no_hash() {
        // Given — so its `.ast` files stay byte-identical to before
        let files = no_files();
        let schema = EntitySchema::empty();
        let inputs = ParseInputs {
            default_domain: ast::Domain::Py,
            files: &files,
            schema: &schema,
        };

        // When
        let hash = inputs.schema_hash();

        // Then
        assert_eq!(hash, None);
    }

    #[test]
    fn test_a_project_with_entities_stamps_its_schemas_hash() {
        // Given
        let files = no_files();
        let schema = rusty_sphinx_entity::load_schema(
            "[[entity_type]]\nname = \"req\"\n",
            &rusty_sphinx_entity::NoReservedNames,
        )
        .unwrap();
        let inputs = ParseInputs {
            default_domain: ast::Domain::Py,
            files: &files,
            schema: &schema,
        };

        // When
        let hash = inputs.schema_hash();

        // Then
        assert_eq!(hash.as_deref(), Some(schema.hash()));
    }
}
