//! Loading the project's entity schema for whichever subcommand needs it.
//!
//! Four of them do, for different reasons: `parse` needs the vocabulary to
//! recognise `.. req::` as a directive at all, `index` needs the relations to
//! derive back-links, and `render`/`preview` need the labels and the
//! presentation. All four read it through here so they cannot disagree about
//! what a missing `--entity-schema` means.

use anyhow::{Context, Result, anyhow};
use rinx_entity::{EntitySchema, ReservedDirectiveNames, load_schema};
use rinx_renderer::EntityTemplates;
use std::collections::BTreeMap;
use std::fs;

use super::cli_args::{flag_value_opt, flag_values_opt};

/// Answers the schema loader's shadowing question by asking the parser.
///
/// The parser owns the directive vocabulary, so this is the one place the two
/// crates are joined: `rinx_entity` deliberately holds no list of
/// built-in directive names, which would drift the moment a new one is added.
struct ParserDirectiveNames;

impl ReservedDirectiveNames for ParserDirectiveNames {
    fn is_reserved(&self, name: &str) -> bool {
        rinx_parser::is_builtin_directive_name(name)
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
        .map(|path| resolve_import_keys(schema, &path))
        .unwrap_or_default()
}

/// Resolves the schema's `[import_keys]` values into paths the parser's file
/// loader can take.
///
/// Two transformations, both of which need to happen exactly once and in a
/// phase that knows where the schema file sits — which is why they are here
/// and not in `rinx_entity`, whose schema keeps the values as written:
///
/// 1. **Anchored at the schema file**, by the rule every other written path in
///    this build follows (`resolve_from_document`): a leading `/` means the
///    source root, anything else is relative to the file that wrote it. That
///    keeps a schema relocatable together with the data files beside it, and
///    takes sphinx-needs' own `/needs_import.json` spelling verbatim.
/// 2. **Prefixed with `/`** afterwards, so the parser can hand it to
///    [`ParseFileLoader`](rinx_parser::ParseFileLoader) with no anchor
///    of its own and get source-root resolution — the same trick
///    `rinx_parser`'s Jinja template loader uses, and for the same
///    reason: a project-level config names a file from the project's root, not
///    from whichever document happens to mention it.
pub(super) fn resolve_import_keys(
    schema: &EntitySchema,
    schema_path: &str,
) -> BTreeMap<String, String> {
    schema
        .import_keys()
        .iter()
        .map(|(alias, written)| {
            let resolved = rinx_ast::resolve_from_document(written, schema_path);
            (alias.clone(), format!("/{}", resolved.to_string_lossy()))
        })
        .collect()
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

    /// A schema declaring `import_keys`, for the resolution tests below.
    fn keyed_schema(pairs: &[(&str, &str)]) -> EntitySchema {
        let table = pairs
            .iter()
            .map(|(alias, path)| format!("{alias} = \"{path}\""))
            .collect::<Vec<_>>()
            .join("\n");
        parse_entity_schema(
            &format!("[import_keys]\n{table}\n[[entity_type]]\nname = \"req\"\n"),
            "entities.toml",
        )
        .expect("the fixture schema is valid")
    }

    #[test]
    fn test_a_bare_import_key_resolves_beside_the_schema_file() {
        // Given a schema nested in the tree, naming a file with no leading `/`
        let schema = keyed_schema(&[("upstream", "needs.json")]);

        // When its keys are resolved
        let resolved = resolve_import_keys(&schema, "examples/entities/entities.toml");

        // Then the file is found beside the schema, which is what lets a
        // schema and its data files move through the tree together
        assert_eq!(
            resolved.get("upstream").map(String::as_str),
            Some("/examples/entities/needs.json")
        );
    }

    #[test]
    fn test_a_slash_prefixed_import_key_resolves_at_the_source_root() {
        // Given sphinx-needs' own spelling, which is source-root-relative
        let schema = keyed_schema(&[("imported_project", "/needs_import.json")]);

        // When its keys are resolved
        let resolved = resolve_import_keys(&schema, "deep/nested/entities.toml");

        // Then the leading `/` wins over the schema's location, so a converted
        // `ubproject.toml` value carries across verbatim
        assert_eq!(
            resolved.get("imported_project").map(String::as_str),
            Some("/needs_import.json")
        );
    }

    #[test]
    fn test_every_resolved_import_key_is_prefixed_for_the_loader() {
        // Given keys written both ways
        let schema = keyed_schema(&[("a", "beside.json"), ("b", "/root.json")]);

        // When they are resolved
        let resolved = resolve_import_keys(&schema, "docs/entities.toml");

        // Then both come out `/`-prefixed, so the parser can hand them to the
        // file loader with no anchor of its own and get source-root resolution
        assert!(
            resolved.values().all(|path| path.starts_with('/')),
            "{resolved:?}"
        );
    }

    #[test]
    fn test_a_schema_without_import_keys_resolves_to_an_empty_map() {
        // Given a schema declaring none
        let schema = parse_entity_schema("[[entity_type]]\nname = \"req\"\n", "entities.toml")
            .expect("valid");

        // When its keys are resolved
        // Then there are none, and every lookup simply misses
        assert!(resolve_import_keys(&schema, "entities.toml").is_empty());
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
