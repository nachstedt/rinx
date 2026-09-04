use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use super::{NumberedDepth, ToctreeFlag};
use crate::TargetName;

/// The nine options a `.. toctree::` accepts, each already interpreted.
///
/// The four that take a value get their own field; the five bare flags share
/// one [`ToctreeFlag`] set, since they all play the same role — see that
/// type's doc comment.
///
/// Both depth-valued options use `NonZeroUsize`, so "depth 0" — which Sphinx
/// treats as *unset* rather than as a real limit — cannot be constructed. The
/// parser maps `:maxdepth: 0` and `:numbered: 0` to `None` when reading them,
/// which keeps the ambiguity in the one place that sees the source text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToctreeOptions {
    /// `:maxdepth:` — how many levels of the tree to display. `None` is
    /// unlimited, which is also what Sphinx's `:maxdepth: -1` means.
    #[serde(default)]
    pub maxdepth: Option<NonZeroUsize>,
    /// `:numbered:` — assign section numbers to everything below this toctree.
    #[serde(default)]
    pub numbered: Option<NumberedDepth>,
    /// `:caption:` — a heading rendered above this toctree's list.
    #[serde(default)]
    pub caption: Option<String>,
    /// `:name:` — a label making the toctree itself a `:ref:` target.
    #[serde(default)]
    pub name: Option<TargetName>,
    /// The valueless options the author wrote.
    #[serde(default)]
    pub flags: BTreeSet<ToctreeFlag>,
}

impl ToctreeOptions {
    /// Whether the author wrote `flag`.
    #[must_use]
    pub fn has(&self, flag: ToctreeFlag) -> bool {
        self.flags.contains(&flag)
    }

    /// Records `flag` as written. Repeating an option is harmless, exactly as
    /// it is in docutils.
    pub fn set(&mut self, flag: ToctreeFlag) {
        self.flags.insert(flag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options_are_all_unset() {
        // Given / When — a toctree written with no option lines at all.
        let options = ToctreeOptions::default();

        // Then
        assert_eq!(options.maxdepth, None);
        assert_eq!(options.numbered, None);
        assert_eq!(options.caption, None);
        assert_eq!(options.name, None);
        for flag in ToctreeFlag::ALL {
            assert!(!options.has(flag), "{flag:?}");
        }
    }

    #[test]
    fn test_set_records_only_the_flag_given() {
        // Given
        let mut options = ToctreeOptions::default();

        // When
        options.set(ToctreeFlag::Hidden);

        // Then — the neighbouring flag is not accidentally set too.
        assert!(options.has(ToctreeFlag::Hidden));
        assert!(!options.has(ToctreeFlag::IncludeHidden));
    }

    #[test]
    fn test_set_is_idempotent() {
        // Given — docutils tolerates a repeated option.
        let mut options = ToctreeOptions::default();

        // When
        options.set(ToctreeFlag::Glob);
        options.set(ToctreeFlag::Glob);

        // Then
        assert!(options.has(ToctreeFlag::Glob));
        assert_eq!(options.flags.len(), 1);
    }

    #[test]
    fn test_options_round_trip_through_json() {
        // Given
        let mut options = ToctreeOptions {
            maxdepth: NonZeroUsize::new(2),
            numbered: Some(NumberedDepth::Unlimited),
            caption: Some("Contents".to_string()),
            name: Some(TargetName::new("main-toc")),
            flags: BTreeSet::new(),
        };
        for flag in ToctreeFlag::ALL {
            options.set(flag);
        }

        // When
        let json = serde_json::to_string(&options).expect("serializes");
        let restored: ToctreeOptions = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, options);
    }

    #[test]
    fn test_options_deserialize_from_an_empty_object() {
        // Given — every field is `#[serde(default)]`, so an index written by an
        // older build stays readable.
        let json = "{}";

        // When
        let restored: ToctreeOptions = serde_json::from_str(json).expect("deserializes");

        // Then
        assert_eq!(restored, ToctreeOptions::default());
    }
}
