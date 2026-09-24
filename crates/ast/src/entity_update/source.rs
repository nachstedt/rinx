use serde::{Deserialize, Serialize};

/// Which spelling of the field-mutation directive an author wrote.
///
/// `.. entity-update::` is this build's own name and `.. needextend::` is
/// sphinx-needs', and the two are the same directive — kept as its own name
/// rather than a bridge-only spelling (unlike `.. needimport::`) because
/// nothing here reads a foreign file format or carries a foreign limitation:
/// the argument is this build's own filter grammar, and the field vocabulary
/// is this build's own typed attribute model. See
/// `docs/decisions/019-entity-update.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityUpdateSource {
    /// `.. entity-update::` — this build's own name.
    EntityUpdate,
    /// `.. needextend::` — sphinx-needs' name for the same thing.
    NeedExtend,
}

impl EntityUpdateSource {
    /// The directive's own name, as written and as quoted in diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::EntityUpdate => "entity-update",
            Self::NeedExtend => "needextend",
        }
    }
}

impl std::str::FromStr for EntityUpdateSource {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "entity-update" => Ok(Self::EntityUpdate),
            "needextend" => Ok(Self::NeedExtend),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_each_spelling_names_itself() {
        // Given
        let sources = [
            EntityUpdateSource::EntityUpdate,
            EntityUpdateSource::NeedExtend,
        ];

        // When
        let names: Vec<&str> = sources.iter().map(EntityUpdateSource::as_str).collect();

        // Then
        assert_eq!(names, ["entity-update", "needextend"]);
    }

    #[test]
    fn test_parsing_a_name_returns_the_spelling_it_belongs_to() {
        // Given
        let names = ["entity-update", "needextend"];

        // When
        let parsed: Vec<EntityUpdateSource> = names
            .iter()
            .map(|name| EntityUpdateSource::from_str(name).unwrap())
            .collect();

        // Then
        assert_eq!(
            parsed,
            [
                EntityUpdateSource::EntityUpdate,
                EntityUpdateSource::NeedExtend
            ]
        );
    }

    #[test]
    fn test_an_unrelated_directive_name_is_refused() {
        // Given
        let name = "needtable";

        // When
        let parsed = EntityUpdateSource::from_str(name);

        // Then
        assert_eq!(parsed, Err(()));
    }

    #[test]
    fn test_the_two_directions_are_inverses() {
        // Given
        let sources = [
            EntityUpdateSource::EntityUpdate,
            EntityUpdateSource::NeedExtend,
        ];

        // When
        let round_tripped: Vec<EntityUpdateSource> = sources
            .iter()
            .map(|source| EntityUpdateSource::from_str(source.as_str()).unwrap())
            .collect();

        // Then
        assert_eq!(round_tripped, sources);
    }
}
