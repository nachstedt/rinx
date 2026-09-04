use serde::{Deserialize, Serialize};

/// One of `.. toctree::`'s valueless options.
///
/// Five of the nine options are bare flags — present or absent, carrying no
/// value — so they are modelled as one set rather than as five `bool` fields.
/// They all play the same role, and a struct of five booleans invites callers
/// to read the wrong one; asking a set whether it holds
/// [`ToctreeFlag::Hidden`] cannot be confused with asking about
/// [`ToctreeFlag::IncludeHidden`] the way two adjacent `bool` fields can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ToctreeFlag {
    /// `:titlesonly:` — list only document titles, omitting the sections
    /// within each document.
    TitlesOnly,
    /// `:reversed:` — reverse the entry order, applied after glob expansion so
    /// that navigation, page order and section numbers all agree.
    Reversed,
    /// `:hidden:` — contribute to the navigation tree, page order and section
    /// numbering, but render nothing in the page body.
    Hidden,
    /// `:includehidden:` — when descending into a referenced document, also
    /// expand that document's own `:hidden:` toctrees.
    IncludeHidden,
    /// `:glob:` — entries may be wildcard patterns.
    Glob,
}

impl ToctreeFlag {
    /// The option name as an author writes it, without the surrounding colons.
    #[must_use]
    pub const fn option_name(self) -> &'static str {
        match self {
            Self::TitlesOnly => "titlesonly",
            Self::Reversed => "reversed",
            Self::Hidden => "hidden",
            Self::IncludeHidden => "includehidden",
            Self::Glob => "glob",
        }
    }

    /// The flag an author spelled, or `None` when the name is not a flag
    /// option. Exact inverse of [`Self::option_name`].
    #[must_use]
    pub fn from_option_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|flag| flag.option_name() == name)
    }

    /// Every flag, so a caller can enumerate them without repeating the list.
    pub const ALL: [Self; 5] = [
        Self::TitlesOnly,
        Self::Reversed,
        Self::Hidden,
        Self::IncludeHidden,
        Self::Glob,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_option_name_and_from_option_name_are_inverses() {
        // Given / When / Then — a half-finished edit to either direction
        // fails here rather than silently making one flag unparseable.
        for flag in ToctreeFlag::ALL {
            assert_eq!(
                ToctreeFlag::from_option_name(flag.option_name()),
                Some(flag),
                "{flag:?}"
            );
        }
    }

    #[test]
    fn test_all_lists_every_flag_exactly_once() {
        // Given / When
        let mut names: Vec<&str> = ToctreeFlag::ALL
            .iter()
            .map(|flag| flag.option_name())
            .collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();

        // Then
        assert_eq!(names.len(), unique, "ALL contains a duplicate");
    }

    #[test]
    fn test_from_option_name_rejects_a_valued_option() {
        // Given — `:maxdepth:` takes a value, so it is not a flag.
        let name = "maxdepth";

        // When / Then
        assert_eq!(ToctreeFlag::from_option_name(name), None);
    }

    #[test]
    fn test_flag_round_trips_through_json() {
        // Given
        let flag = ToctreeFlag::IncludeHidden;

        // When
        let json = serde_json::to_string(&flag).expect("serializes");
        let restored: ToctreeFlag = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, flag);
    }
}
