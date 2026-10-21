//! Loading the project's entity schema for whichever subcommand needs it.
//!
//! Three of them do, for different reasons: `parse` needs the vocabulary to
//! recognise `.. req::` as a directive at all, `index` needs the relations to
//! derive back-links, and `render` needs the labels and the presentation.
//! All three read it through here so they cannot disagree about
//! what a missing `--entity-schema` means.

use anyhow::{Context, Result, anyhow};
use rinx_entity::EntitySchema;
use rinx_renderer::EntityTemplates;
use std::collections::BTreeMap;
use std::fs;

use super::cli_args::{flag_value_opt, flag_values_opt};

/// Reads the schema named by an optional `--entity-schema` flag.
///
/// Absent means a project that declares no entities, which gets the empty
/// schema — under which every entity lookup misses and the whole pipeline
/// behaves exactly as it did before this feature existed.
///
/// # Errors
///
/// Returns an error when the file cannot be read, or when the schema is
/// invalid. A faulty schema is fatal rather than a warning: it is the
/// vocabulary the parser works from, so continuing would report a cascade of
/// unknown-directive diagnostics instead of the one real problem.
pub(super) fn load_entity_schema(args: &[String]) -> Result<EntitySchema> {
    let Some(path) = flag_value_opt(args, "--entity-schema") else {
        return Ok(EntitySchema::empty());
    };
    let text = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read entity schema '{path}'"))?;
    parse_entity_schema(&text, &path)
}

/// Turns schema text into an [`EntitySchema`], or an error naming every fault.
///
/// Split from the file read so it is testable without the filesystem, the same
/// `process_*`/`cmd_*` division every subcommand here follows.
pub(super) fn parse_entity_schema(text: &str, path: &str) -> Result<EntitySchema> {
    rinx_parser::load_entity_schema(text)
        .map_err(|errors| anyhow!("Invalid entity schema '{path}':\n{errors}"))
}

/// The schema's import keys, resolved against wherever `--entity-schema`
/// pointed.
///
/// Reads the flag a second time rather than being handed the path, so a caller
/// that already has an [`EntitySchema`] cannot pass a location it was not
/// loaded from. Without the flag there is no schema and no keys.
pub(super) fn import_keys_from_args(
    args: &[String],
    schema: &EntitySchema,
) -> BTreeMap<String, String> {
    flag_value_opt(args, "--entity-schema")
        .map(|path| schema.resolved_import_keys(&path))
        .unwrap_or_default()
}

/// Reads the per-type entity templates named by `--entity-templates`.
///
/// Variadic, so it must come last on the command line (see `cli_args`).
/// Each value is a path; the template is keyed by the file's basename, which is
/// the name a schema's `template = "..."` refers to. Keeping the *name* in the
/// schema and the *path* on the command line is ADR-001's rule: a sandboxed
/// build relocates files, so a path baked into config would break.
///
/// # Errors
///
/// Returns an error when a named file cannot be read.
pub(super) fn load_entity_templates(args: &[String]) -> Result<EntityTemplates> {
    let paths = flag_values_opt(args, "--entity-templates");
    rinx_renderer::load_entity_templates(paths.iter().map(std::path::Path::new), |path| {
        fs::read_to_string(path)
    })
    .map_err(|(path, error)| {
        anyhow::Error::new(error).context(format!(
            "Failed to read entity template '{}'",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_flag_yields_the_empty_schema() {
        // Given — a project that does not use entities
        let args = vec!["render".to_string()];

        // When
        let schema = load_entity_schema(&args).unwrap();

        // Then
        assert!(schema.is_empty());
    }

    #[test]
    fn test_a_valid_schema_is_loaded() {
        // Given
        let text = "[[entity_type]]\nname = \"req\"\n";

        // When
        let schema = parse_entity_schema(text, "entities.toml").unwrap();

        // Then
        assert!(schema.entity_type("req").is_some());
    }

    #[test]
    fn test_an_invalid_schema_is_fatal_and_names_the_file() {
        // Given
        let text = "[[entity_type]]\nname = \"req\"\n\n[[entity_type]]\nname = \"req\"\n";

        // When
        let error = parse_entity_schema(text, "entities.toml").unwrap_err();

        // Then
        let message = error.to_string();
        assert!(message.contains("entities.toml"), "{message}");
        assert!(message.contains("more than once"), "{message}");
    }

    #[test]
    fn test_a_section_shadowing_a_built_in_directive_is_refused() {
        // Given — the check that needs the parser's own vocabulary
        let text =
            "[[entity_type]]\nname = \"req\"\n\n  [[entity_type.section]]\n  name = \"note\"\n";

        // When
        let error = parse_entity_schema(text, "entities.toml").unwrap_err();

        // Then
        assert!(error.to_string().contains("shadow"), "{error}");
    }

    #[test]
    fn test_a_section_name_no_directive_uses_is_accepted() {
        // Given
        let text = "[[entity_type]]\nname = \"req\"\n\n  [[entity_type.section]]\n  name = \"rationale\"\n";

        // When
        let schema = parse_entity_schema(text, "entities.toml").unwrap();

        // Then
        assert!(
            schema
                .entity_type("req")
                .unwrap()
                .section("rationale")
                .is_some()
        );
    }

    #[test]
    fn test_no_template_flag_yields_no_templates() {
        // Given
        let args = vec!["render".to_string()];

        // When
        let templates = load_entity_templates(&args).unwrap();

        // Then
        assert!(templates.is_empty());
    }

    #[test]
    fn test_import_keys_from_args_needs_the_schema_flag() {
        // Given a command line with no --entity-schema
        let args = vec!["parse".to_string()];

        // When the keys are read
        // Then there are none: without the flag there is no schema to have
        // declared any, so there is no path to resolve against either
        assert!(import_keys_from_args(&args, &EntitySchema::empty()).is_empty());
    }
}
