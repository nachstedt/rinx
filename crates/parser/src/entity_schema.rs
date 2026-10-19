//! Loading an entity schema with this parser's directive vocabulary
//! reserved.
//!
//! `rinx_entity` refuses a section that would shadow a built-in directive,
//! but deliberately holds no list of their names, which would drift the
//! moment one is added; the parser owns that list. This is where the two are
//! joined for a written `entities.toml`, so the build and the language
//! server refuse exactly the same schemas. (A converted sphinx-needs
//! configuration passes [`is_builtin_directive_name`] to
//! `rinx_entity::load_raw_schema` the same way.)

use rinx_entity::{EntitySchema, SchemaErrors};

use crate::is_builtin_directive_name;

/// Loads a schema from the text of an `entities.toml`.
///
/// # Errors
///
/// Every fault the schema has, including a section named like a directive
/// this parser dispatches.
pub fn load_entity_schema(text: &str) -> Result<EntitySchema, SchemaErrors> {
    rinx_entity::load_schema(text, &is_builtin_directive_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_entity_schema_loads_a_valid_schema() {
        // Given
        let text = "[[entity_type]]\nname = \"req\"\n";

        // When
        let schema = load_entity_schema(text).expect("a valid schema");

        // Then
        assert!(!schema.is_empty());
    }

    #[test]
    fn test_load_entity_schema_refuses_a_section_shadowing_a_directive() {
        // Given — `note` is an admonition this parser dispatches
        let text = "[[entity_type]]\nname = \"req\"\n[[entity_type.section]]\nname = \"note\"\n";

        // When
        let errors = load_entity_schema(text).expect_err("a shadowing section");

        // Then
        assert!(errors.to_string().contains("note"), "{errors}");
    }
}
