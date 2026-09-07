//! Resolving an entity role against the project index.
//!
//! Far simpler than the domain-object resolver beside it: an entity id is
//! already fully qualified, so there are no scope tiers to walk and no suffix
//! search to fall back on. What this adds over a bare map lookup is the type
//! check — the thing a declared role exists for.

use rusty_sphinx_entity::EntitySchema;
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use rusty_sphinx_ast::EntityId;

/// What looking up an entity role produced.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EntityResolution<'a> {
    /// The target exists and the role accepts its type.
    Resolved(&'a EntityRecord),
    /// The target exists, but its type is outside the role's declared set.
    /// Carried separately from `Resolved` so the caller can link it *and*
    /// report it, which is what a domain-object type mismatch does.
    TypeMismatch(&'a EntityRecord),
    /// No entity carries that id.
    NotFound,
}

/// Looks entity references up in one project index, under one schema.
pub(crate) struct EntityResolver<'a> {
    index: &'a ProjectIndex,
    schema: &'a EntitySchema,
}

impl<'a> EntityResolver<'a> {
    pub(crate) const fn new(index: &'a ProjectIndex, schema: &'a EntitySchema) -> Self {
        Self { index, schema }
    }

    /// Resolves `target` as written by `role`.
    ///
    /// An unparseable target is a miss rather than an error: the id's
    /// character set was already enforced where entities are *defined*, and a
    /// reference that names something no id could ever be is simply a
    /// reference to nothing.
    pub(crate) fn resolve(&self, role: &str, target: &str) -> EntityResolution<'a> {
        let Ok(id) = EntityId::new(target) else {
            return EntityResolution::NotFound;
        };
        let Some(record) = self.index.entities.get(&id) else {
            return EntityResolution::NotFound;
        };
        match self.schema.role(role) {
            Some(spec) if spec.accepts(&record.type_name) => EntityResolution::Resolved(record),
            // An undeclared role reaching here means the parser matched it
            // against a schema this render is not using — a mismatch already
            // reported by the index phase. Linking is the better degradation.
            None => EntityResolution::Resolved(record),
            Some(_) => EntityResolution::TypeMismatch(record),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_entity::{NoReservedNames, load_schema};
    use std::collections::BTreeMap;

    fn schema() -> EntitySchema {
        load_schema(
            r#"
            [[entity_type]]
            name = "req"

            [[entity_type]]
            name = "spec"

            [[role]]
            name = "req"
            types = ["req"]

            [[role]]
            name = "need"
            "#,
            &NoReservedNames,
        )
        .unwrap()
    }

    fn index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        for (id, type_name) in [("REQ_001", "req"), ("SPEC_003", "spec")] {
            index.entities.insert(
                EntityId::new(id).unwrap(),
                EntityRecord {
                    type_name: type_name.to_string(),
                    doc_path: "specs".to_string(),
                    title: None,
                    attributes: BTreeMap::new(),
                    outgoing: BTreeMap::new(),
                },
            );
        }
        index
    }

    #[test]
    fn test_resolves_a_target_of_an_accepted_type() {
        // Given
        let (index, schema) = (index(), schema());
        let resolver = EntityResolver::new(&index, &schema);

        // When
        let resolution = resolver.resolve("req", "REQ_001");

        // Then
        assert!(matches!(resolution, EntityResolution::Resolved(_)));
    }

    #[test]
    fn test_reports_a_target_of_a_type_the_role_refuses() {
        // Given — `:req:` accepts only a req
        let (index, schema) = (index(), schema());
        let resolver = EntityResolver::new(&index, &schema);

        // When
        let resolution = resolver.resolve("req", "SPEC_003");

        // Then
        assert!(matches!(resolution, EntityResolution::TypeMismatch(_)));
    }

    #[test]
    fn test_an_unconstrained_role_accepts_every_type() {
        // Given
        let (index, schema) = (index(), schema());
        let resolver = EntityResolver::new(&index, &schema);

        // When / Then
        assert!(matches!(
            resolver.resolve("need", "SPEC_003"),
            EntityResolution::Resolved(_)
        ));
        assert!(matches!(
            resolver.resolve("entity", "REQ_001"),
            EntityResolution::Resolved(_)
        ));
    }

    #[test]
    fn test_a_target_no_document_declares_is_not_found() {
        // Given
        let (index, schema) = (index(), schema());
        let resolver = EntityResolver::new(&index, &schema);

        // When
        let resolution = resolver.resolve("req", "NOWHERE");

        // Then
        assert_eq!(resolution, EntityResolution::NotFound);
    }

    #[test]
    fn test_a_target_that_is_not_a_legal_id_is_not_found() {
        // Given
        let (index, schema) = (index(), schema());
        let resolver = EntityResolver::new(&index, &schema);

        // When
        let resolution = resolver.resolve("req", "not a legal id");

        // Then
        assert_eq!(resolution, EntityResolution::NotFound);
    }
}
