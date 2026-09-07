//! Loading the project's entity schema for whichever subcommand needs it.
//!
//! Four of them do, for different reasons: `parse` needs the vocabulary to
//! recognise `.. req::` as a directive at all, `index` needs the relations to
//! derive back-links, and `render`/`preview` need the labels and the
//! presentation. All four read it through here so they cannot disagree about
//! what a missing `--entity-schema` means.

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_entity::{EntitySchema, ReservedDirectiveNames, load_schema};
use rusty_sphinx_renderer::EntityTemplates;
use std::fs;

use super::cli_args::{flag_value_opt, flag_values_opt};

/// Answers the schema loader's shadowing question by asking the parser.
///
/// The parser owns the directive vocabulary, so this is the one place the two
/// crates are joined: `rusty_sphinx_entity` deliberately holds no list of
/// built-in directive names, which would drift the moment a new one is added.
struct ParserDirectiveNames;

impl ReservedDirectiveNames for ParserDirectiveNames {
    fn is_reserved(&self, name: &str) -> bool {
        rusty_sphinx_parser::is_builtin_directive_name(name)
    }
}

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
    load_schema(text, &ParserDirectiveNames)
        .map_err(|errors| anyhow!("Invalid entity schema '{path}':\n{errors}"))
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
    let mut templates = EntityTemplates::new();
    for path in flag_values_opt(args, "--entity-templates") {
        let name = std::path::Path::new(&path)
            .file_name()
            .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
        let source = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read entity template '{path}'"))?;
        templates.insert(name, source);
    }
    Ok(templates)
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
    fn test_the_parser_bridge_reports_a_real_directive_name() {
        // Given
        let reserved = ParserDirectiveNames;

        // When / Then
        assert!(reserved.is_reserved("note"));
        assert!(reserved.is_reserved("toctree"));
        assert!(!reserved.is_reserved("verification-criteria"));
    }
}
