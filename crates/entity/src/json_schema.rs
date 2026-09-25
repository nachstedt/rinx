//! The JSON Schema an editor validates a schema *file* against.
//!
//! TOML has no schema language of its own, so the established route is JSON
//! Schema over TOML's data model — which `taplo` (and through it VS Code's
//! TOML support) applies from a `#:schema` directive or a `.taplo.toml`
//! association.
//!
//! **Derived, never hand-written.** The `Raw*` types in [`crate::load`] *are*
//! the file's shape, down to the rejection of unknown keys; a second
//! hand-maintained copy would drift exactly when it mattered. `schemas/entities.schema.json`
//! is checked in and a test regenerates and compares it.
//!
//! ## What it covers, and what it cannot
//!
//! It validates the **grammar**: table structure, every field name and type,
//! required versus optional, the seven attribute type spellings, the two
//! `split` values, and unknown keys.
//!
//! It cannot validate the **semantics**, because JSON Schema has no way to say
//! "this string must name something declared elsewhere in the document":
//!
//! - a relation's `to`, or a role's `types`, naming a declared entity type
//! - `argument.fields` and `id.from` naming declared attributes
//! - duplicate type, role, attribute, section or relation names
//! - an attribute and a relation sharing an option spelling
//! - a section shadowing a built-in directive (which needs the parser)
//! - back-link label agreement and name clashes
//! - `required` together with `default`
//! - `values` present exactly for the two enum types
//!
//! All of those stay in [`crate::load`], which is the authority. A clean
//! editor therefore does **not** mean a schema that loads — it means one whose
//! shape is right.

use schemars::{Schema, schema_for};

use crate::load::RawSchema;

/// The `#:schema` path a schema file points at, relative to the repository.
pub const SCHEMA_PATH: &str = "schemas/entities.schema.json";

/// Builds the JSON Schema for a schema file.
#[must_use]
pub fn entity_json_schema() -> Schema {
    schema_for!(RawSchema)
}

/// The generated schema as the checked-in file spells it.
///
/// Pretty-printed with a trailing newline, so the checked-in artefact is a
/// well-formed text file and the comparison test can diff it as one.
///
/// # Panics
///
/// Panics only if the generated schema is not serializable, which would be a
/// bug in `schemars` rather than anything a caller can provoke.
#[must_use]
pub fn entity_json_schema_text() -> String {
    let schema = entity_json_schema();
    let mut text = serde_json::to_string_pretty(&schema)
        .expect("a generated JSON Schema is always serializable");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The checked-in artefact this module generates.
    fn checked_in() -> String {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../",
            "schemas/entities.schema.json"
        );
        std::fs::read_to_string(path).unwrap_or_else(|error| {
            panic!(
                "{path} could not be read ({error}); regenerate it with `cargo test -p rinx_entity`"
            )
        })
    }

    #[test]
    fn test_the_checked_in_schema_matches_what_the_types_generate() {
        // Given — the guideline: generate a derived artefact from the same
        // source the code uses, and test that it is still in step
        let generated = entity_json_schema_text();

        // When
        let stored = checked_in();

        // Then
        assert_eq!(
            generated, stored,
            "schemas/entities.schema.json is stale. Regenerate it:\n  \
             cargo run -p rinx_worker -- entity_json_schema > schemas/entities.schema.json"
        );
    }

    #[test]
    fn test_the_schema_describes_the_two_top_level_tables() {
        // Given
        let schema = entity_json_schema_text();

        // When / Then
        assert!(schema.contains("\"entity_type\""), "{schema}");
        assert!(schema.contains("\"role\""), "{schema}");
    }

    #[test]
    fn test_the_schema_offers_every_attribute_type_spelling() {
        // Given — what an editor completes `type = ` with
        let schema = entity_json_schema_text();

        // When / Then
        for spelling in [
            "string",
            "text",
            "int",
            "bool",
            "enum",
            "list<string>",
            "list<enum>",
        ] {
            assert!(
                schema.contains(&format!("\"{spelling}\"")),
                "the schema does not offer `{spelling}`"
            );
        }
    }

    #[test]
    fn test_the_schema_rejects_unknown_keys() {
        // Given — `deny_unknown_fields` is what catches a mistyped `lable`
        let schema = entity_json_schema_text();

        // When / Then
        assert!(
            schema.contains("\"additionalProperties\": false"),
            "the schema does not forbid unknown keys"
        );
    }

    #[test]
    fn test_the_schema_carries_the_documentation_from_the_types() {
        // Given — a doc comment on a Raw field becomes an editor tooltip
        let schema = entity_json_schema_text();

        // When / Then
        assert!(
            schema.contains("The directive name"),
            "field documentation did not reach the schema"
        );
    }
}

#[cfg(test)]
mod validation_tests {
    //! The generated schema, applied to real TOML.
    //!
    //! Two directions matter. Everything the loader accepts must validate,
    //! or the schema would put red squiggles under a correct file. And the
    //! *shape* errors the schema is meant to catch must actually fail, or it
    //! would be decorative.
    //!
    //! What is deliberately **not** asserted: that everything the loader
    //! rejects also fails validation. Most of the loader's rules are
    //! cross-references between parts of the document, which JSON Schema
    //! cannot express — see this module's own documentation.

    use super::*;
    use crate::load::{NoReservedNames, load_schema};

    /// Compiles the generated schema once per test.
    fn validator() -> jsonschema::Validator {
        let schema = serde_json::to_value(entity_json_schema())
            .expect("the generated schema is serializable");
        jsonschema::validator_for(&schema).expect("the generated schema is a valid JSON Schema")
    }

    /// Converts TOML to the JSON value the validator sees.
    fn as_json(toml_text: &str) -> serde_json::Value {
        let value: toml::Value = toml::from_str(toml_text).expect("the fixture should be TOML");
        serde_json::to_value(value).expect("TOML maps onto JSON's data model")
    }

    fn is_valid(toml_text: &str) -> bool {
        validator().is_valid(&as_json(toml_text))
    }

    /// The example the site actually builds with.
    fn shipped_example() -> String {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../",
            "examples/entities/entities.toml"
        );
        std::fs::read_to_string(path).expect("the example schema should be readable")
    }

    #[test]
    fn test_the_shipped_example_validates() {
        // Given — the schema, the example and the loader must stay in step
        let example = shipped_example();

        // When
        let errors: Vec<String> = validator()
            .iter_errors(&as_json(&example))
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect();

        // Then
        assert!(
            errors.is_empty(),
            "the shipped example does not validate:\n{}",
            errors.join("\n")
        );
    }

    #[test]
    fn test_the_shipped_example_also_loads() {
        // Given — validating is not the same as loading, so both are checked
        let example = shipped_example();

        // When
        let loaded = load_schema(&example, &NoReservedNames);

        // Then
        assert!(loaded.is_ok(), "the shipped example does not load");
    }

    #[test]
    fn test_an_empty_document_validates() {
        // Given — a project may declare no entities at all
        assert!(is_valid(""));
    }

    #[test]
    fn test_a_minimal_type_validates() {
        // Given / When / Then
        assert!(is_valid("[[entity_type]]\nname = \"req\"\n"));
    }

    #[test]
    fn test_a_mistyped_key_is_caught() {
        // Given — the mistake `deny_unknown_fields` exists for
        let text = "[[entity_type]]\nname = \"req\"\nlable = \"Requirement\"\n";

        // When / Then
        assert!(!is_valid(text));
    }

    #[test]
    fn test_a_missing_required_key_is_caught() {
        // Given — every type must be named
        assert!(!is_valid("[[entity_type]]\nlabel = \"Requirement\"\n"));
    }

    #[test]
    fn test_an_unknown_attribute_type_is_caught() {
        // Given
        let text = "\
[[entity_type]]
name = \"req\"

  [[entity_type.attribute]]
  name = \"status\"
  type = \"colour\"
";

        // When / Then
        assert!(!is_valid(text));
    }

    #[test]
    fn test_every_attribute_type_spelling_validates() {
        // Given
        let spellings = ["string", "text", "int", "bool", "list<string>"];

        // When / Then
        for spelling in spellings {
            let text = format!(
                "[[entity_type]]\nname = \"req\"\n\n  [[entity_type.attribute]]\n  name = \"a\"\n  type = \"{spelling}\"\n"
            );
            assert!(is_valid(&text), "`{spelling}` should validate");
        }
    }

    #[test]
    fn test_a_wrong_value_type_is_caught() {
        // Given — `required` is a flag, not a word
        let text = "\
[[entity_type]]
name = \"req\"

  [[entity_type.section]]
  name = \"notes\"
  required = \"yes\"
";

        // When / Then
        assert!(!is_valid(text));
    }

    #[test]
    fn test_an_unknown_split_is_caught() {
        // Given
        let text =
            "[[entity_type]]\nname = \"req\"\nargument = { fields = [\"t\"], split = \"space\" }\n";

        // When / Then
        assert!(!is_valid(text));
    }

    #[test]
    fn test_a_semantic_fault_passes_validation_but_fails_loading() {
        // Given — the boundary this schema deliberately does not cross: `to`
        // names an entity type no `[[entity_type]]` declares, which JSON Schema
        // has no way to express
        let text = "\
[[entity_type]]
name = \"req\"

  [[entity_type.relation]]
  name = \"links\"
  to = [\"nowhere\"]
";

        // When
        let validates = is_valid(text);
        let loads = load_schema(text, &NoReservedNames).is_ok();

        // Then — a clean editor does not mean a schema that loads
        assert!(
            validates,
            "the shape is well-formed, so validation should pass"
        );
        assert!(!loads, "the loader is the authority and should refuse it");
    }
}
