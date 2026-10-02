//! Everything a subcommand needs in order to *parse*, bundled.
//!
//! Six settings travel together wherever RST is turned into an AST — the
//! default domain, the default role, the loader a file-reading directive goes
//! through, the entity schema that supplies the project's own directive
//! vocabulary, the import keys it declares, and whether the source is
//! rendered as a Jinja template first. Both `parse` and `preview` take all
//! six, so they are one parameter rather than six, for
//! the same reason `ParseCtx` exists in the parser: the next parse-time
//! setting should not have to touch every signature again.

use anyhow::{Result, anyhow};
use rinx_ast as ast;
use rinx_entity::EntitySchema;
use rinx_parser::{DefaultRole, ParseCtx};
use std::collections::BTreeMap;

use super::cli_args::{flag_value_opt, flag_values_opt};
use super::parse_files::{DocumentRelativeFiles, parse_ctx};

/// The parse-time configuration one subcommand run works under.
pub(super) struct ParseInputs<'a> {
    /// The domain a bare directive or role resolves to.
    pub default_domain: ast::Domain,
    /// The role a bare `` `text` `` is read as until a `.. default-role::`
    /// says otherwise.
    pub default_role: &'a DefaultRole,
    /// How a file-reading directive reaches the filesystem.
    pub files: &'a DocumentRelativeFiles,
    /// The project's entity meta-model.
    pub schema: &'a EntitySchema,
    /// The names a Jinja template may read, when the library asked for its
    /// sources to be templated, and `None` when it did not.
    pub jinja: Option<&'a [(String, String)]>,
    /// The schema's `[import_keys]`, already resolved to source-root-relative
    /// paths — what a `.. needimport::` looks its argument up in.
    ///
    /// Beside the schema rather than taken from it because the resolution
    /// needs the schema *file's* location, which only the caller that read it
    /// knows; see `entity_schema::resolve_import_keys`.
    pub import_keys: &'a BTreeMap<String, String>,
}

impl<'a> ParseInputs<'a> {
    /// Builds the parser context these inputs describe.
    pub(super) fn ctx(&self) -> ParseCtx<'a> {
        let ctx = parse_ctx(self.default_domain, self.files)
            .with_schema(self.schema)
            .with_import_keys(self.import_keys)
            .with_default_role(self.default_role);
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

/// Reads the `--default-role` flag `rinx_library`'s `default_role` attribute
/// passes, defaulting to docutils' `title-reference` when it is absent.
///
/// Checked against `schema` because an entity role the schema declares is a
/// role like any other, and may be the default.
///
/// # Errors
///
/// Fails on a name no role answers to: the setting belongs to the build, so a
/// typo in it is a configuration fault rather than one more warning — every
/// bare `` `text` `` of the library would otherwise silently become a title.
pub(super) fn default_role_from_args(
    args: &[String],
    schema: &EntitySchema,
) -> Result<DefaultRole> {
    let Some(name) = flag_value_opt(args, "--default-role") else {
        return Ok(DefaultRole::TITLE_REFERENCE);
    };
    // The domain plays no part in whether a name is a role: a bare `func` is
    // one in either, it only resolves differently.
    let ctx = ParseCtx::with_domain(ast::Domain::Py).with_schema(schema);
    DefaultRole::parse(&name, &ctx).map_err(|error| {
        anyhow!("Invalid --default-role '{name}': {error}; check the rinx_library's `default_role`")
    })
}

/// Reads the `--jinja` opt-in and the `--jinja-context k=v ...` bindings that
/// `rinx_library`'s `jinja` and `jinja_context` attributes pass.
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

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_default_role_from_args_defaults_to_title_reference() {
        // Given / When
        let role = default_role_from_args(&[], &EntitySchema::empty()).unwrap();

        // Then
        assert_eq!(role, DefaultRole::TITLE_REFERENCE);
    }

    #[test]
    fn test_default_role_from_args_reads_a_role() {
        // Given
        let ctx = ParseCtx::with_domain(ast::Domain::Py);

        // When
        let role =
            default_role_from_args(&args(&["--default-role", "any"]), &EntitySchema::empty())
                .unwrap();

        // Then
        assert_eq!(role, DefaultRole::parse("any", &ctx).unwrap());
    }

    #[test]
    fn test_default_role_from_args_names_the_attribute_on_an_unknown_role() {
        // Given / When
        let error =
            default_role_from_args(&args(&["--default-role", "nope"]), &EntitySchema::empty())
                .unwrap_err();

        // Then
        assert!(error.to_string().contains("default_role"), "{error}");
        assert!(error.to_string().contains("':nope:'"), "{error}");
    }

    #[test]
    fn test_default_role_from_args_accepts_an_entity_role() {
        // Given
        let schema = rinx_entity::load_schema(
            "[[entity_type]]\nname = \"req\"\n\n[[role]]\nname = \"req\"\n",
            &rinx_entity::NoReservedNames,
        )
        .unwrap();

        // When / Then
        assert!(default_role_from_args(&args(&["--default-role", "req"]), &schema).is_ok());
    }

    fn no_files() -> DocumentRelativeFiles {
        DocumentRelativeFiles::for_document("doc.rst")
    }

    #[test]
    fn test_the_context_carries_the_declared_domain_and_schema() {
        // Given
        let files = no_files();
        let schema = rinx_entity::load_schema(
            "[[entity_type]]\nname = \"req\"\n",
            &rinx_entity::NoReservedNames,
        )
        .unwrap();
        let inputs = ParseInputs {
            default_domain: ast::Domain::C,
            default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
            files: &files,
            schema: &schema,
            jinja: None,
            import_keys: &BTreeMap::new(),
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
            default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
            files: &files,
            schema: &schema,
            jinja: None,
            import_keys: &BTreeMap::new(),
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
        let schema = rinx_entity::load_schema(
            "[[entity_type]]\nname = \"req\"\n",
            &rinx_entity::NoReservedNames,
        )
        .unwrap();
        let inputs = ParseInputs {
            default_domain: ast::Domain::Py,
            default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
            files: &files,
            schema: &schema,
            jinja: None,
            import_keys: &BTreeMap::new(),
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
            default_role: &rinx_parser::DefaultRole::TITLE_REFERENCE,
            files: &files,
            schema: &schema,
            jinja: Some(&context),
            import_keys: &BTreeMap::new(),
        };

        // When
        let ctx = inputs.ctx();

        // Then
        assert_eq!(ctx.jinja, Some(context.as_slice()));
    }
}
