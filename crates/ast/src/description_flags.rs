use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The valueless options every object description accepts, as one set.
///
/// Sphinx declares them once, on `ObjectDescription`, so every domain's
/// objects take them — `py:function`, `c:struct` and `std:cmdoption` alike.
/// They are a set of [`DescriptionFlag`] rather than one `bool` per option,
/// so asking for one cannot be confused with asking for its neighbour.
///
/// `py:module` does not carry this set: it is not an object description in
/// Sphinx, and its own flags are [`crate::ModuleFlag`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DescriptionFlags(BTreeSet<DescriptionFlag>);

impl DescriptionFlags {
    /// The set holding exactly `flags`.
    #[must_use]
    pub fn of(flags: impl IntoIterator<Item = DescriptionFlag>) -> Self {
        Self(flags.into_iter().collect())
    }

    /// Whether the author wrote `flag`.
    #[must_use]
    pub fn has(&self, flag: DescriptionFlag) -> bool {
        self.0.contains(&flag)
    }

    /// Records `flag` as written. Repeating an option is harmless, exactly as
    /// it is in docutils.
    pub fn set(&mut self, flag: DescriptionFlag) {
        self.0.insert(flag);
    }
}

/// One of the valueless options Sphinx's `ObjectDescription` declares.
///
/// Sphinx also declares `:no-typesetting:` there, which is not modelled yet:
/// it replaces the whole description with its anchors, which is more than a
/// flag on the rendered markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DescriptionFlag {
    /// `:no-index:` — the object is neither a cross-reference target nor in
    /// the general index. Its body is still rendered, and still indexed.
    NoIndex,
    /// `:no-index-entry:` — no general-index entry; still a target.
    NoIndexEntry,
    /// `:no-contents-entry:` — left out of a local contents listing. rinx
    /// has no such listing for objects yet, so this is recorded only.
    NoContentsEntry,
}

impl DescriptionFlag {
    /// The flag an author spelled, without the surrounding colons, or `None`
    /// when the name is not one of these flags.
    ///
    /// Accepts the pre-Sphinx-7 spellings (`:noindex:`, `:noindexentry:`,
    /// `:nocontentsentry:`), which Sphinx 9.1 still copies over to the new
    /// names.
    #[must_use]
    pub fn from_option_name(name: &str) -> Option<Self> {
        match name {
            "no-index" | "noindex" => Some(Self::NoIndex),
            "no-index-entry" | "noindexentry" => Some(Self::NoIndexEntry),
            "no-contents-entry" | "nocontentsentry" => Some(Self::NoContentsEntry),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_option_name_reads_every_current_spelling() {
        // Given
        let names = ["no-index", "no-index-entry", "no-contents-entry"];

        // When
        let flags: Vec<_> = names
            .iter()
            .map(|name| DescriptionFlag::from_option_name(name))
            .collect();

        // Then
        assert_eq!(
            flags,
            [
                Some(DescriptionFlag::NoIndex),
                Some(DescriptionFlag::NoIndexEntry),
                Some(DescriptionFlag::NoContentsEntry),
            ]
        );
    }

    #[test]
    fn test_from_option_name_reads_every_legacy_spelling() {
        // Given
        let names = ["noindex", "noindexentry", "nocontentsentry"];

        // When
        let flags: Vec<_> = names
            .iter()
            .map(|name| DescriptionFlag::from_option_name(name))
            .collect();

        // Then
        assert_eq!(
            flags,
            [
                Some(DescriptionFlag::NoIndex),
                Some(DescriptionFlag::NoIndexEntry),
                Some(DescriptionFlag::NoContentsEntry),
            ]
        );
    }

    #[test]
    fn test_from_option_name_refuses_other_options_and_values() {
        // Given
        let names = ["module", "final", ":no-index:", "No-Index", ""];

        // When
        let flags: Vec<_> = names
            .iter()
            .map(|name| DescriptionFlag::from_option_name(name))
            .collect();

        // Then
        assert_eq!(flags, [None, None, None, None, None]);
    }

    #[test]
    fn test_set_records_a_flag_once() {
        // Given
        let mut flags = DescriptionFlags::default();

        // When
        flags.set(DescriptionFlag::NoIndex);
        flags.set(DescriptionFlag::NoIndex);

        // Then
        assert!(flags.has(DescriptionFlag::NoIndex));
        assert!(!flags.has(DescriptionFlag::NoIndexEntry));
        assert_eq!(flags, DescriptionFlags::of([DescriptionFlag::NoIndex]));
    }

    #[test]
    fn test_flags_serialize_as_a_plain_list() {
        // Given
        let flags = DescriptionFlags::of([DescriptionFlag::NoIndexEntry, DescriptionFlag::NoIndex]);

        // When
        let json = serde_json::to_string(&flags).expect("serializes");

        // Then — sorted, so an unchanged document's `.ast` stays byte-identical.
        assert_eq!(json, r#"["NoIndex","NoIndexEntry"]"#);
        assert_eq!(
            serde_json::from_str::<DescriptionFlags>(&json).expect("deserializes"),
            flags
        );
    }
}
