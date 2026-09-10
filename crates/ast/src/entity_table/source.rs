use serde::{Deserialize, Serialize};

/// Which spelling of the listing directive an author wrote.
///
/// `.. entity-table::` is this build's own name and `.. needtable::` is
/// sphinx-needs', and the two are the same directive: a migrating project keeps
/// its documents, and a new one need not adopt another tool's vocabulary. The
/// difference is fully consumed while parsing, so — exactly as with
/// [`TableSource`](crate::TableSource)'s `list-table`/`csv-table` pair — they
/// share one node and record only which name produced it.
///
/// Kept because a diagnostic should quote the directive the author actually
/// wrote. It deliberately does *not* change the rendered output: two documents
/// using different spellings of one directive should not look different.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityTableSource {
    /// `.. entity-table::` — this build's own name.
    EntityTable,
    /// `.. needtable::` — sphinx-needs' name for the same thing.
    NeedTable,
}

impl EntityTableSource {
    /// The directive's own name, as written and as quoted in diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::EntityTable => "entity-table",
            Self::NeedTable => "needtable",
        }
    }
}

impl std::str::FromStr for EntityTableSource {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "entity-table" => Ok(Self::EntityTable),
            "needtable" => Ok(Self::NeedTable),
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
        let sources = [EntityTableSource::EntityTable, EntityTableSource::NeedTable];

        // When
        let names: Vec<&str> = sources.iter().map(EntityTableSource::as_str).collect();

        // Then
        assert_eq!(names, ["entity-table", "needtable"]);
    }

    #[test]
    fn test_parsing_a_name_returns_the_spelling_it_belongs_to() {
        // Given
        let names = ["entity-table", "needtable"];

        // When
        let parsed: Vec<EntityTableSource> = names
            .iter()
            .map(|name| EntityTableSource::from_str(name).unwrap())
            .collect();

        // Then
        assert_eq!(
            parsed,
            [EntityTableSource::EntityTable, EntityTableSource::NeedTable]
        );
    }

    #[test]
    fn test_an_unrelated_directive_name_is_refused() {
        // Given
        let name = "list-table";

        // When
        let parsed = EntityTableSource::from_str(name);

        // Then
        assert_eq!(parsed, Err(()));
    }

    #[test]
    fn test_the_two_directions_are_inverses() {
        // Given — the name a diagnostic quotes must round-trip to the spelling
        // that produced it
        let sources = [EntityTableSource::EntityTable, EntityTableSource::NeedTable];

        // When
        let round_tripped: Vec<EntityTableSource> = sources
            .iter()
            .map(|source| EntityTableSource::from_str(source.as_str()).unwrap())
            .collect();

        // Then
        assert_eq!(round_tripped, sources);
    }
}
