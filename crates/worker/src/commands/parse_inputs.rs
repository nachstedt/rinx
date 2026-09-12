//! Everything a subcommand needs in order to *parse*, bundled.
//!
//! Four settings travel together wherever RST is turned into an AST — the
//! default domain, the loader a file-reading directive goes through, the
//! entity schema that supplies the project's own directive vocabulary, and
//! whether the source is rendered as a Jinja template first. Both `parse` and
//! `preview` take all four, so they are one parameter rather than four, for
//! the same reason `ParseCtx` exists in the parser: the next parse-time
//! setting should not have to touch every signature again.

use anyhow::{Result, anyhow};
use rusty_sphinx_ast as ast;
use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_parser::ParseCtx;

use super::cli_args::flag_values_opt;
use super::parse_files::{DocumentRelativeFiles, parse_ctx};

/// The parse-time configuration one subcommand run works under.
pub(super) struct ParseInputs<'a> {
    /// The domain a bare directive or role resolves to.
    pub default_domain: ast::Domain,
    /// How a file-reading directive reaches the filesystem.
    pub files: &'a DocumentRelativeFiles,
    /// The project's entity meta-model.
    pub schema: &'a EntitySchema,
    /// The names a Jinja template may read, when the library asked for its
    /// sources to be templated, and `None` when it did not.
    pub jinja: Option<&'a [(String, String)]>,
}

impl<'a> ParseInputs<'a> {
    /// Builds the parser context these inputs describe.
    pub(super) fn ctx(&self) -> ParseCtx<'a> {
        let ctx = parse_ctx(self.default_domain, self.files).with_schema(self.schema);
        match self.jinja {
            Some(context) => ctx.with_jinja(context),
            None => ctx,
        }
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

/// Reads the `--jinja` opt-in and the `--jinja-context k=v ...` bindings that
/// `rusty_sphinx_library`'s `jinja` and `jinja_context` attributes pass.
///
/// `None` unless `--jinja` was given: a document is free to contain `{{` and
/// `{%` as text, and most projects mean nothing by them.
///
/// # Errors
///
/// Fails on a binding that is not `name=value`, rather than guessing which
/// half was meant.
pub(super) fn jinja_from_args(args: &[String]) -> Result<Option<Vec<(String, String)>>> {
    if !args.iter().any(|arg| arg == "--jinja") {
        return Ok(None);
    }
    flag_values_opt(args, "--jinja-context")
        .iter()
        .map(|binding| {
            binding
                .split_once('=')
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .ok_or_else(|| {
                    anyhow!("Invalid --jinja-context '{binding}', expected 'name=value'")
                })
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
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
            jinja: None,
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
            jinja: None,
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
            jinja: None,
        };

        // When
        let hash = inputs.schema_hash();

        // Then
        assert_eq!(hash.as_deref(), Some(schema.hash()));
    }

    #[test]
    fn test_jinja_is_off_unless_the_flag_is_given() {
        // Given — a context without the opt-in means nothing
        let args = vec!["--jinja-context".to_string(), "release=3.14".to_string()];

        // When / Then
        assert_eq!(jinja_from_args(&args).unwrap(), None);
    }

    #[test]
    fn test_jinja_reads_every_binding_after_the_flag() {
        // Given
        let args = vec![
            "--jinja".to_string(),
            "--jinja-context".to_string(),
            "release=3.14".to_string(),
            "team=docs".to_string(),
            "--input".to_string(),
            "a.rst".to_string(),
        ];

        // When
        let context = jinja_from_args(&args).unwrap().expect("enabled");

        // Then
        assert_eq!(
            context,
            vec![
                ("release".to_string(), "3.14".to_string()),
                ("team".to_string(), "docs".to_string()),
            ]
        );
    }

    #[test]
    fn test_jinja_with_no_context_is_still_enabled() {
        // Given — the sphinx-needs demo binds nothing; every value it reads
        // comes from a `{% set %}`
        let args = vec!["--jinja".to_string()];

        // When / Then
        assert_eq!(jinja_from_args(&args).unwrap(), Some(Vec::new()));
    }

    #[test]
    fn test_a_binding_without_an_equals_sign_is_refused() {
        // Given
        let args = vec![
            "--jinja".to_string(),
            "--jinja-context".to_string(),
            "release".to_string(),
        ];

        // When
        let error = jinja_from_args(&args).expect_err("refused");

        // Then
        assert!(
            error.to_string().contains("expected 'name=value'"),
            "{error}"
        );
    }

    #[test]
    fn test_the_context_carries_the_jinja_bindings_into_the_parser() {
        // Given
        let files = no_files();
        let schema = EntitySchema::empty();
        let context = [("release".to_string(), "3.14".to_string())];
        let inputs = ParseInputs {
            default_domain: ast::Domain::Py,
            files: &files,
            schema: &schema,
            jinja: Some(&context),
        };

        // When
        let ctx = inputs.ctx();

        // Then
        assert_eq!(ctx.jinja, Some(context.as_slice()));
    }
}
