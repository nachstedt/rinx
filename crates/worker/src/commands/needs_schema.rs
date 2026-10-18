//! The `needs_schema` subcommand: writes the entity schema a sphinx-needs
//! project's configuration describes.
//!
//! The conversion is `rinx_entity::needs_conf`'s, the one the language server
//! runs over a `conf.py`; this reads it from a `ubproject.toml`, the file
//! `needs_from_toml` names, so a migrating project — and the sphinx-needs
//! benchmark — can write an `entities.toml` once and build against it.
//!
//! What the entity model cannot express is printed to stdout as JSON, one
//! `{"category", "detail"}` object per construct, rather than dropped: that
//! list is as much the output as the schema.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use rinx_entity::needs_conf::{
    DEFAULT_TOML_ROOT, NeedsConfig, ReportEntry, SchemasFile, convert_needs, render_schema_toml,
};

use super::cli_args::{flag_value, flag_value_opt};
use super::entity_schema::parse_entity_schema;

/// The schema `config` (with the `schemas.json` text `schemas`) converts
/// into, as `entities.toml` text, and what could not be carried over.
///
/// # Errors
///
/// Returns an error when `schemas` is no `schemas.json`, or when the
/// converted schema does not load — one need type declared twice, say —
/// since writing it would only fail the build later.
pub(super) fn process_needs_schema(
    config: &NeedsConfig,
    schemas: Option<&str>,
) -> Result<(String, Vec<ReportEntry>)> {
    let schemas = schemas
        .map(SchemasFile::from_json)
        .transpose()
        .context("Failed to read the schemas.json")?;
    let (declarations, report) = convert_needs(config, schemas.as_ref());
    let text = render_schema_toml(&declarations);
    parse_entity_schema(&text, "the converted schema")?;
    Ok((text, report))
}

/// `rinx needs_schema --ubproject <ubproject.toml> [--schemas <schemas.json>]
/// --output <entities.toml>`.
///
/// Without `--schemas`, the `schemas.json` the configuration names in
/// `schema_definitions_from_json` is read, relative to the `ubproject.toml`.
///
/// # Errors
///
/// Returns an error when a flag is missing, a file cannot be read or written,
/// or the configuration cannot be converted.
pub(crate) fn cmd_needs_schema(args: &[String]) -> Result<()> {
    let ubproject = flag_value(args, "--ubproject")?;
    let output = flag_value(args, "--output")?;
    let text =
        fs::read_to_string(&ubproject).with_context(|| format!("Failed to read '{ubproject}'"))?;
    let (config, _) = NeedsConfig::from_toml(&text, DEFAULT_TOML_ROOT).with_context(|| {
        format!("Failed to read the sphinx-needs configuration in '{ubproject}'")
    })?;
    let schemas_path = flag_value_opt(args, "--schemas").or_else(|| {
        config.schema_definitions_from_json.as_ref().map(|written| {
            let base = Path::new(&ubproject).parent().unwrap_or(Path::new(""));
            base.join(written).to_string_lossy().into_owned()
        })
    });
    let schemas = schemas_path
        .map(|path| fs::read_to_string(&path).with_context(|| format!("Failed to read '{path}'")))
        .transpose()?;
    let (schema, report) = process_needs_schema(&config, schemas.as_deref())?;
    fs::write(&output, schema).with_context(|| format!("Failed to write '{output}'"))?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> NeedsConfig {
        NeedsConfig::from_toml(text, DEFAULT_TOML_ROOT)
            .expect("a valid configuration")
            .0
    }

    #[test]
    fn test_process_needs_schema_writes_a_schema_the_build_loads() {
        // Given
        let needs = config("[[needs.types]]\ndirective = \"req\"\ntitle = \"Requirement\"\n");

        // When
        let (text, report) = process_needs_schema(&needs, None).expect("converts");

        // Then
        let schema = parse_entity_schema(&text, "test").expect("the schema loads");
        assert!(schema.entity_type("req").is_some());
        assert!(!report.is_empty(), "the generic `.. need::` is reported");
    }

    #[test]
    fn test_process_needs_schema_applies_the_schemas_json() {
        // Given
        let needs = config("[[needs.types]]\ndirective = \"req\"\n");
        let schemas = r##"{"schemas": [{"id": "r", "select": {"$ref": "#/$defs/type-req"}, "validate": {"local": {"properties": {"id": {"pattern": "^R"}}}}}]}"##;

        // When
        let (text, _) = process_needs_schema(&needs, Some(schemas)).expect("converts");

        // Then
        let schema = parse_entity_schema(&text, "test").expect("the schema loads");
        let pattern = schema
            .entity_type("req")
            .and_then(|t| t.id.pattern.as_ref());
        assert_eq!(pattern.map(rinx_entity::ValuePattern::as_str), Some("^R"));
    }

    #[test]
    fn test_process_needs_schema_refuses_a_schemas_file_that_is_no_json() {
        // Given
        let needs = config("[[needs.types]]\ndirective = \"req\"\n");

        // When
        let result = process_needs_schema(&needs, Some("{"));

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_process_needs_schema_refuses_a_schema_that_does_not_load() {
        // Given one need type declared twice
        let needs =
            config("[[needs.types]]\ndirective = \"req\"\n[[needs.types]]\ndirective = \"req\"\n");

        // When
        let result = process_needs_schema(&needs, None);

        // Then
        let message = format!("{:?}", result.expect_err("refused"));
        assert!(message.contains("req"), "{message}");
    }
}
