use serde::{Deserialize, Serialize};

/// Which spelling of the sequence-diagram directive an author wrote.
///
/// `.. entity-sequence::` is this build's own name and `.. needsequence::` is
/// sphinx-needs', and the two are the same directive — the
/// [`EntityFlowSource`](crate::EntityFlowSource) arrangement, for its reasons:
/// a migrating project keeps its documents, and a diagnostic quotes the name
/// the author actually wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntitySequenceSource {
    /// `.. entity-sequence::` — this build's own name.
    EntitySequence,
    /// `.. needsequence::` — sphinx-needs' name for the same thing.
    NeedSequence,
}

impl EntitySequenceSource {
    /// The directive's own name, as written and as quoted in diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::EntitySequence => "entity-sequence",
            Self::NeedSequence => "needsequence",
        }
    }
}

impl std::str::FromStr for EntitySequenceSource {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "entity-sequence" => Ok(Self::EntitySequence),
            "needsequence" => Ok(Self::NeedSequence),
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
            EntitySequenceSource::EntitySequence,
            EntitySequenceSource::NeedSequence,
        ];

        // When
        let names: Vec<&str> = sources.iter().map(EntitySequenceSource::as_str).collect();

        // Then
        assert_eq!(names, ["entity-sequence", "needsequence"]);
    }

    #[test]
    fn test_an_unrelated_directive_name_is_refused() {
        // Given
        let name = "entity-flow";

        // When
        let parsed = EntitySequenceSource::from_str(name);

        // Then
        assert_eq!(parsed, Err(()));
    }

    #[test]
    fn test_the_two_directions_are_inverses() {
        // Given — the name a diagnostic quotes must round-trip to the spelling
        // that produced it
        let sources = [
            EntitySequenceSource::EntitySequence,
            EntitySequenceSource::NeedSequence,
        ];

        // When
        let round_tripped: Vec<EntitySequenceSource> = sources
            .iter()
            .map(|source| EntitySequenceSource::from_str(source.as_str()).unwrap())
            .collect();

        // Then
        assert_eq!(round_tripped, sources);
    }
}
