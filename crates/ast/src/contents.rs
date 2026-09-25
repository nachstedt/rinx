use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};

use crate::TargetName;

/// How a heading covered by a `.. contents::` links back to it, from
/// `:backlinks:`.
///
/// Unlike `.. toctree::`'s options this is never absent: docutils always has
/// a concrete backlinks behaviour in force, defaulting to [`Self::Entry`], so
/// this carries no `Option` wrapper the way [`ContentsOptions::depth`] does.
/// Named [`Self::Off`] rather than `None` so it cannot be misread as
/// `Option::None` at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ContentsBacklinks {
    /// Each heading links back to its own entry in the table of contents.
    #[default]
    Entry,
    /// Each heading links back to the table of contents itself.
    Top,
    /// Headings carry no backlink at all.
    Off,
}

impl ContentsBacklinks {
    /// The value as an author spells it in `:backlinks: <value>`.
    #[must_use]
    pub const fn option_value(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Top => "top",
            Self::Off => "none",
        }
    }

    /// The backlinks kind an author spelled, or `None` when `value` is none
    /// of the three docutils accepts. Exact inverse of [`Self::option_value`].
    #[must_use]
    pub fn from_option_value(value: &str) -> Option<Self> {
        match value {
            "entry" => Some(Self::Entry),
            "top" => Some(Self::Top),
            "none" => Some(Self::Off),
            _ => None,
        }
    }
}

/// The options a `.. contents::` accepts, each already interpreted.
///
/// `:local:` is a single bare flag, so — unlike [`crate::ToctreeOptions`]'s
/// five flags — it needs no `BTreeSet`-backed set of its own: a lone `bool`
/// field carries no risk of being confused with a neighbour.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentsOptions {
    /// `:depth:` — how many section levels to list. `None` is unlimited,
    /// exactly as an absent `:maxdepth:` is on a `.. toctree::`.
    #[serde(default)]
    pub depth: Option<NonZeroUsize>,
    /// `:local:` — list only the sections nested under the one this directive
    /// is written in, rather than the whole document.
    #[serde(default)]
    pub local: bool,
    /// `:backlinks:` — how a listed heading links back to this table of
    /// contents, if at all.
    #[serde(default)]
    pub backlinks: ContentsBacklinks,
    /// `:class:` — extra CSS classes for the rendered wrapper.
    #[serde(default)]
    pub classes: Vec<String>,
    /// `:name:` — a label making this table of contents a `:ref:` target.
    #[serde(default)]
    pub name: Option<TargetName>,
}

/// A `.. contents::` directive: a local table of contents built from the
/// section headings of the document it appears in.
///
/// Unlike [`crate::Toctree`] this carries no entries of its own — its
/// argument is a title, not a list of documents — because everything it
/// lists is derived, at render time, from the document's own heading
/// structure (see `rinx_renderer`'s contents block).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contents {
    /// The directive's argument line, if given. `None` means the default
    /// title ("Contents") applies — kept as `None` rather than the default
    /// string itself so a future locale/config-driven default stays
    /// possible without touching every parsed document.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub options: ContentsOptions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backlinks_option_value_and_from_option_value_are_inverses() {
        // Given / When / Then — a half-finished edit to either direction
        // fails here rather than silently making one value unparseable.
        for backlinks in [
            ContentsBacklinks::Entry,
            ContentsBacklinks::Top,
            ContentsBacklinks::Off,
        ] {
            assert_eq!(
                ContentsBacklinks::from_option_value(backlinks.option_value()),
                Some(backlinks),
                "{backlinks:?}"
            );
        }
    }

    #[test]
    fn test_from_option_value_rejects_an_unknown_value() {
        // Given / When / Then
        assert_eq!(ContentsBacklinks::from_option_value("sideways"), None);
    }

    #[test]
    fn test_backlinks_default_is_entry() {
        // Given / When
        let backlinks = ContentsBacklinks::default();

        // Then
        assert_eq!(backlinks, ContentsBacklinks::Entry);
    }

    #[test]
    fn test_default_options_are_all_unset() {
        // Given / When
        let options = ContentsOptions::default();

        // Then
        assert_eq!(options.depth, None);
        assert!(!options.local);
        assert_eq!(options.backlinks, ContentsBacklinks::Entry);
        assert!(options.classes.is_empty());
        assert_eq!(options.name, None);
    }

    #[test]
    fn test_contents_round_trips_through_json() {
        // Given
        let contents = Contents {
            title: Some("Overview".to_string()),
            options: ContentsOptions {
                depth: NonZeroUsize::new(2),
                local: true,
                backlinks: ContentsBacklinks::Top,
                classes: vec!["wide".to_string()],
                name: Some(TargetName::new("main-contents")),
            },
        };

        // When
        let json = serde_json::to_string(&contents).expect("serializes");
        let restored: Contents = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, contents);
    }

    #[test]
    fn test_contents_deserializes_from_an_empty_object() {
        // Given — every field is `#[serde(default)]`, so an index written by
        // an older build stays readable.
        let json = "{}";

        // When
        let restored: Contents = serde_json::from_str(json).expect("deserializes");

        // Then
        assert_eq!(restored, Contents::default());
    }
}
