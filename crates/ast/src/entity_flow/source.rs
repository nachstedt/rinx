use serde::{Deserialize, Serialize};

/// Which spelling of the flowchart directive an author wrote.
///
/// `.. entity-flow::` is this build's own name and `.. needflow::` is
/// sphinx-needs', and the two are the same directive — exactly the
/// [`EntityTableSource`](crate::EntityTableSource) arrangement, for exactly its
/// reasons: a migrating project keeps its documents, a new one need not adopt
/// another tool's vocabulary, and a diagnostic should quote the name the author
/// actually wrote.
///
/// The spelling deliberately changes nothing about the rendered picture, and no
/// diagnostic code names one: a code names the construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityFlowSource {
    /// `.. entity-flow::` — this build's own name.
    EntityFlow,
    /// `.. needflow::` — sphinx-needs' name for the same thing.
    NeedFlow,
}

impl EntityFlowSource {
    /// The directive's own name, as written and as quoted in diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::EntityFlow => "entity-flow",
            Self::NeedFlow => "needflow",
        }
    }
}

impl std::str::FromStr for EntityFlowSource {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "entity-flow" => Ok(Self::EntityFlow),
            "needflow" => Ok(Self::NeedFlow),
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
        let sources = [EntityFlowSource::EntityFlow, EntityFlowSource::NeedFlow];

        // When
        let names: Vec<&str> = sources.iter().map(EntityFlowSource::as_str).collect();

        // Then
        assert_eq!(names, ["entity-flow", "needflow"]);
    }

    #[test]
    fn test_parsing_a_name_returns_the_spelling_it_belongs_to() {
        // Given
        let names = ["entity-flow", "needflow"];

        // When
        let parsed: Vec<EntityFlowSource> = names
            .iter()
            .map(|name| EntityFlowSource::from_str(name).unwrap())
            .collect();

        // Then
        assert_eq!(
            parsed,
            [EntityFlowSource::EntityFlow, EntityFlowSource::NeedFlow]
        );
    }

    #[test]
    fn test_an_unrelated_directive_name_is_refused() {
        // Given
        let name = "entity-table";

        // When
        let parsed = EntityFlowSource::from_str(name);

        // Then
        assert_eq!(parsed, Err(()));
    }

    #[test]
    fn test_the_two_directions_are_inverses() {
        // Given — the name a diagnostic quotes must round-trip to the spelling
        // that produced it
        let sources = [EntityFlowSource::EntityFlow, EntityFlowSource::NeedFlow];

        // When
        let round_tripped: Vec<EntityFlowSource> = sources
            .iter()
            .map(|source| EntityFlowSource::from_str(source.as_str()).unwrap())
            .collect();

        // Then
        assert_eq!(round_tripped, sources);
    }
}
