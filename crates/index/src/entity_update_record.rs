use serde::{Deserialize, Serialize};

/// One `.. entity-update::`/`.. needextend::` directive as the merged index
/// knows it — per-document data, exactly like [`crate::DocumentToctree`],
/// since the directive's own text is a fact about the document that wrote it.
/// What it *does* is a project-wide operation, applied afterwards by
/// `rusty_sphinx_analyzer::apply_entity_updates`; this only records intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUpdateRecord {
    /// The document that wrote this directive — where a `.. noqa:` for one of
    /// its diagnostics would go.
    pub doc_path: String,
    pub update: rusty_sphinx_ast::EntityUpdate,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> EntityUpdateRecord {
        EntityUpdateRecord {
            doc_path: "specs/boot.rst".to_string(),
            update: rusty_sphinx_ast::EntityUpdate::new(
                rusty_sphinx_ast::EntityUpdateSource::EntityUpdate,
                rusty_sphinx_ast::UpdateTarget {
                    candidate_id: rusty_sphinx_ast::EntityId::new("REQ_001").ok(),
                    filter: None,
                    raw: "REQ_001".to_string(),
                },
            ),
        }
    }

    #[test]
    fn test_a_record_survives_a_serialization_round_trip() {
        // Given
        let original = record();

        // When
        let json = serde_json::to_string(&original).unwrap();
        let restored: EntityUpdateRecord = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(original, restored);
    }
}
