use serde::{Deserialize, Serialize};

/// Which spelling of the bar-chart directive an author wrote.
///
/// `.. entity-bar::` is this build's own name and `.. needbar::` is
/// sphinx-needs', and the two are the same directive — exactly the
/// [`EntityFlowSource`](crate::EntityFlowSource) arrangement, for exactly its
/// reasons: a migrating project keeps its documents, a new one need not adopt
/// another tool's vocabulary, and a diagnostic should quote the name the author
/// actually wrote.
///
/// The spelling deliberately changes nothing about the rendered chart, and no
/// diagnostic code names one: a code names the construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityBarSource {
    /// `.. entity-bar::` — this build's own name.
    EntityBar,
    /// `.. needbar::` — sphinx-needs' name for the same thing.
    NeedBar,
}

impl EntityBarSource {
    /// The directive's own name, as written and as quoted in diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::EntityBar => "entity-bar",
            Self::NeedBar => "needbar",
        }
    }
}

impl std::str::FromStr for EntityBarSource {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "entity-bar" => Ok(Self::EntityBar),
            "needbar" => Ok(Self::NeedBar),
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
        let sources = [EntityBarSource::EntityBar, EntityBarSource::NeedBar];

        // When
        let names: Vec<&str> = sources.iter().map(EntityBarSource::as_str).collect();

        // Then
        assert_eq!(names, ["entity-bar", "needbar"]);
    }

    #[test]
    fn test_parsing_a_name_returns_the_spelling_it_belongs_to() {
        // Given
        let names = ["entity-bar", "needbar"];

        // When
        let parsed: Vec<EntityBarSource> = names
            .iter()
            .map(|name| EntityBarSource::from_str(name).unwrap())
            .collect();

        // Then
        assert_eq!(
            parsed,
            [EntityBarSource::EntityBar, EntityBarSource::NeedBar]
        );
    }

    #[test]
    fn test_an_unrelated_directive_name_is_refused() {
        // Given
        let name = "entity-flow";

        // When
        let parsed = EntityBarSource::from_str(name);

        // Then
        assert_eq!(parsed, Err(()));
    }

    #[test]
    fn test_the_two_directions_are_inverses() {
        // Given — the name a diagnostic quotes must round-trip to the spelling
        // that produced it
        let sources = [EntityBarSource::EntityBar, EntityBarSource::NeedBar];

        // When
        let round_tripped: Vec<EntityBarSource> = sources
            .iter()
            .map(|source| EntityBarSource::from_str(source.as_str()).unwrap())
            .collect();

        // Then
        assert_eq!(round_tripped, sources);
    }
}
