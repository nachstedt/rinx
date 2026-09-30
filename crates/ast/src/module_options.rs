use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The options a `.. py:module::` accepts, each already interpreted.
///
/// None of them is printed on the page: Sphinx records a module's synopsis,
/// platform and deprecation for the Python Module Index and the tooltip of a
/// `:mod:` link, and nowhere else — see ADR-032. The two that take a value
/// get their own field; the bare flags share one [`ModuleFlag`] set, as
/// [`crate::ToctreeOptions`]' do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleOptions {
    /// `:platform:` — comma-separated platform identifiers
    /// (e.g. `"Unix, Windows"`), kept as written.
    #[serde(default)]
    pub platform: Option<String>,
    /// `:synopsis:` — one sentence on what the module is for. A raw string,
    /// never inline-parsed, exactly as Sphinx reads it.
    #[serde(default)]
    pub synopsis: Option<String>,
    /// The valueless options the author wrote.
    #[serde(default)]
    pub flags: BTreeSet<ModuleFlag>,
}

impl ModuleOptions {
    /// Whether the author wrote `flag`.
    #[must_use]
    pub fn has(&self, flag: ModuleFlag) -> bool {
        self.flags.contains(&flag)
    }

    /// Records `flag` as written. Repeating an option is harmless, exactly as
    /// it is in docutils.
    pub fn set(&mut self, flag: ModuleFlag) {
        self.flags.insert(flag);
    }
}

/// One of `.. py:module::`'s valueless options that has an effect.
///
/// Sphinx also accepts `:no-contents-entry:` and `:no-typesetting:` on a
/// module, but its `PyModule` never reads them, so they are not modelled here
/// — the parser consumes and drops them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModuleFlag {
    /// `:deprecated:` — shown as a qualifier in the module index and in the
    /// `:mod:` tooltip.
    Deprecated,
    /// `:no-index:` (legacy `:noindex:`) — the module is neither a
    /// cross-reference target nor listed in any index; it only sets the
    /// current module for what follows.
    NoIndex,
    /// `:no-index-entry:` — no general-index entry; the module is still a
    /// target and still listed in the module index.
    NoIndexEntry,
}

impl ModuleFlag {
    /// The flag an author spelled, without the surrounding colons, or `None`
    /// when the name is not a flag option with an effect. Accepts
    /// `noindex`, the pre-Sphinx-7 spelling Sphinx 9.1 still copies over.
    #[must_use]
    pub fn from_option_name(name: &str) -> Option<Self> {
        match name {
            "deprecated" => Some(Self::Deprecated),
            "no-index" | "noindex" => Some(Self::NoIndex),
            "no-index-entry" => Some(Self::NoIndexEntry),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options_are_all_unset() {
        // Given / When — a module written with no option lines at all.
        let options = ModuleOptions::default();

        // Then
        assert_eq!(options.platform, None);
        assert_eq!(options.synopsis, None);
        for flag in [
            ModuleFlag::Deprecated,
            ModuleFlag::NoIndex,
            ModuleFlag::NoIndexEntry,
        ] {
            assert!(!options.has(flag), "{flag:?}");
        }
    }

    #[test]
    fn test_set_records_only_the_flag_given() {
        // Given
        let mut options = ModuleOptions::default();

        // When
        options.set(ModuleFlag::NoIndex);

        // Then — the neighbouring flag is not accidentally set too.
        assert!(options.has(ModuleFlag::NoIndex));
        assert!(!options.has(ModuleFlag::NoIndexEntry));
    }

    #[test]
    fn test_from_option_name_accepts_both_no_index_spellings() {
        // Given / When / Then
        assert_eq!(
            ModuleFlag::from_option_name("no-index"),
            Some(ModuleFlag::NoIndex)
        );
        assert_eq!(
            ModuleFlag::from_option_name("noindex"),
            Some(ModuleFlag::NoIndex)
        );
    }

    #[test]
    fn test_from_option_name_maps_deprecated_and_no_index_entry() {
        // Given / When / Then
        assert_eq!(
            ModuleFlag::from_option_name("deprecated"),
            Some(ModuleFlag::Deprecated)
        );
        assert_eq!(
            ModuleFlag::from_option_name("no-index-entry"),
            Some(ModuleFlag::NoIndexEntry)
        );
    }

    #[test]
    fn test_from_option_name_refuses_options_without_an_effect() {
        // Given — accepted by Sphinx's `PyModule` but never read by it.
        // When / Then
        assert_eq!(ModuleFlag::from_option_name("no-contents-entry"), None);
        assert_eq!(ModuleFlag::from_option_name("no-typesetting"), None);
        assert_eq!(ModuleFlag::from_option_name("synopsis"), None);
    }
}
