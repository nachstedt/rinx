//! The vocabulary a listing directive's filter, columns and sort key are
//! written in.
//!
//! Lives here, beside the schema, because two phases must agree about it
//! exactly: the **parser** refuses a field no type declares (while it still
//! holds the option line's position), and the **renderer** resolves the same
//! name against an entity record. Two copies of this list would drift, and the
//! drift would show up as a column that validated and then rendered empty.
//!
//! What it does *not* hold is how a field is *resolved* — that needs an entity
//! record, which this crate has never heard of. Only which names exist.

use crate::schema::EntitySchema;

/// The fields every entity has, whatever its type declares.
///
/// Named with sphinx-needs' spellings, because these are what existing filters
/// are written with. Two of them are worth knowing apart:
///
/// - `type` is the *directive name* (`fsr`), which is what a filter compares.
/// - `type_name` is the schema's human label (`Functional Safety Requirement`),
///   which is what a table column shows.
///
/// That pairing reads backwards, and it is sphinx-needs' own, kept so that a
/// migrating project's filters mean here what they meant there.
pub const BUILTIN_FIELDS: [&str; 5] = ["id", "type", "type_name", "title", "docname"];

/// Whether `name` is one of the fields every entity has.
#[must_use]
pub fn is_builtin_field(name: &str) -> bool {
    BUILTIN_FIELDS.contains(&name)
}

impl EntitySchema {
    /// Whether `name` is a field a listing directive may read.
    ///
    /// True for a built-in, and for any attribute, outgoing relation or
    /// derived back-link that *any* declared type has: one table may list
    /// several types at once, so a name only some of them declare is
    /// perfectly good — it is simply missing on the others, which is what
    /// makes `status == "open"` skip an entity with no status rather than
    /// fail.
    #[must_use]
    pub fn declares_field(&self, name: &str) -> bool {
        if is_builtin_field(name) {
            return true;
        }
        self.types().iter().any(|entity_type| {
            entity_type.attribute(name).is_some()
                || entity_type.relation(name).is_some()
                || self
                    .backlinks_for(&entity_type.name)
                    .iter()
                    .any(|backlink| backlink.name == name)
        })
    }

    /// Every field name a listing directive may read, sorted and deduplicated.
    ///
    /// What an unknown-field diagnostic offers the author: a misspelled
    /// `:columns:` entry should be answered with the whole vocabulary, the way
    /// a misspelled entity option already is.
    #[must_use]
    pub fn field_names(&self) -> Vec<String> {
        let mut names: Vec<String> = BUILTIN_FIELDS
            .iter()
            .map(|name| (*name).to_string())
            .collect();
        for entity_type in self.types() {
            names.extend(entity_type.attributes.iter().map(|a| a.name.clone()));
            names.extend(entity_type.relations.iter().map(|r| r.name.clone()));
            names.extend(
                self.backlinks_for(&entity_type.name)
                    .iter()
                    .map(|backlink| backlink.name.clone()),
            );
        }
        names.sort();
        names.dedup();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::{NoReservedNames, load_schema};

    /// A schema with one attribute, one relation and one derived back-link.
    fn schema() -> EntitySchema {
        let text = r#"
[[entity_type]]
name = "req"
[[entity_type.attribute]]
name = "status"
type = "string"

[[entity_type]]
name = "test"
[[entity_type.relation]]
name = "verifies"
to = ["req"]
incoming = "verified_by"
"#;
        load_schema(text, &NoReservedNames).expect("expected this schema to load")
    }

    #[test]
    fn test_every_builtin_field_is_recognised() {
        // Given
        let schema = schema();

        // When
        let recognised: Vec<bool> = BUILTIN_FIELDS
            .iter()
            .map(|name| schema.declares_field(name))
            .collect();

        // Then
        assert_eq!(recognised, [true, true, true, true, true]);
    }

    #[test]
    fn test_a_builtin_is_recognised_without_any_schema_at_all() {
        // Given — a project may filter on `id` or `docname` with no types
        let schema = EntitySchema::empty();

        // When
        let recognised = schema.declares_field("docname");

        // Then
        assert!(recognised);
    }

    #[test]
    fn test_a_declared_attribute_is_a_field() {
        // Given
        let schema = schema();

        // When
        let recognised = schema.declares_field("status");

        // Then
        assert!(recognised);
    }

    #[test]
    fn test_an_outgoing_relation_is_a_field() {
        // Given
        let schema = schema();

        // When
        let recognised = schema.declares_field("verifies");

        // Then
        assert!(recognised);
    }

    #[test]
    fn test_a_derived_backlink_is_a_field() {
        // Given — `verified_by` is declared nowhere; it falls out of
        // `test.verifies` pointing at a `req`
        let schema = schema();

        // When
        let recognised = schema.declares_field("verified_by");

        // Then
        assert!(recognised);
    }

    #[test]
    fn test_a_field_no_type_declares_is_refused() {
        // Given
        let schema = schema();

        // When
        let recognised = schema.declares_field("asil");

        // Then
        assert!(!recognised);
    }

    #[test]
    fn test_a_section_name_is_not_a_field() {
        // Given — sections are documents, not values: they are deliberately
        // absent from the index, so a filter could never read one
        let text = r#"
[[entity_type]]
name = "req"
[[entity_type.section]]
name = "rationale"
"#;
        let schema = load_schema(text, &NoReservedNames).unwrap();

        // When
        let recognised = schema.declares_field("rationale");

        // Then
        assert!(!recognised);
    }

    #[test]
    fn test_the_vocabulary_lists_every_kind_of_field_once() {
        // Given
        let schema = schema();

        // When
        let names = schema.field_names();

        // Then
        assert_eq!(
            names,
            [
                "docname",
                "id",
                "status",
                "title",
                "type",
                "type_name",
                "verified_by",
                "verifies",
            ]
        );
    }

    #[test]
    fn test_the_vocabulary_agrees_with_the_membership_test() {
        // Given — two implementations of one question would drift, so the
        // listing and the check are pinned against each other
        let schema = schema();

        // When
        let all_declared = schema
            .field_names()
            .iter()
            .all(|name| schema.declares_field(name));

        // Then
        assert!(all_declared);
    }
}
