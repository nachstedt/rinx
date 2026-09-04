//! The language a code block is highlighted as, from what the author wrote
//! through to what the renderer highlights with.
//!
//! Two types rather than one, and the difference between them is the point.
//! [`CodeLanguage`] is what a directive's argument spells out, and it has an
//! `Inherit` case because an argumentless `.. code-block::` takes its language
//! from the enclosing `.. highlight::`. [`ResolvedLanguage`] is what remains
//! once that inheritance has been applied, and it has no `Inherit` case at
//! all — so a renderer that forgot to resolve one cannot type-check, rather
//! than silently highlighting as the wrong language.
//!
//! `none` and `default` are variants rather than names because Sphinx gives
//! them meanings no grammar has: `none` is *explicitly* unhighlighted, and
//! `default` is "try Python, but never complain when it doesn't fit". An
//! `Option<String>` cannot tell either of them from a language called
//! "none", nor tell an argumentless block from an explicitly unhighlighted
//! one — a distinction Sphinx does draw.
//!
//! The concrete name inside [`LanguageName`] stays an open string rather than
//! a closed enum over every supported language. That set belongs to the
//! highlighting backend, changes when the backend is upgraded, and would have
//! to be mirrored here by hand; enumerating it would also write the backend's
//! vocabulary into every serialized `.ast` file, which is exactly what
//! `docs/decisions/004-math-rendering.md` established must not happen. Whether
//! a name has a grammar behind it is therefore a *render-time* question, and
//! an unknown one degrades to plain text with a warning.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A normalized language name, e.g. `python`, `c++`, `restructuredtext`.
///
/// Opaque with a smart constructor because the name is a lookup key: the
/// parser, the renderer's backend lookup and the rendered `highlight-<lang>`
/// CSS class must all spell it one way, or a block written `Python` and one
/// written `python` become two different things downstream. Normalizing once,
/// where the source text is read, is what stops that.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct LanguageName(String);

/// Why a language name could not be built. Carries no data: there is exactly
/// one way to fail, and the caller already has the text it passed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyLanguageName;

impl fmt::Display for EmptyLanguageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a language name cannot be empty")
    }
}

impl LanguageName {
    /// Creates a name from raw source text, trimming it and lowercasing it.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyLanguageName`] when nothing but whitespace was given,
    /// so that `.. code-block::  ` is reported where it was written rather
    /// than becoming a nameless language the backend will never match.
    pub fn new(raw: &str) -> Result<Self, EmptyLanguageName> {
        let normalized = raw.trim().to_lowercase();
        if normalized.is_empty() {
            return Err(EmptyLanguageName);
        }
        Ok(Self(normalized))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LanguageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for LanguageName {
    type Error = EmptyLanguageName;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

/// Whether a language name means "do not highlight this".
///
/// `none` is Sphinx's spelling; `text` and `plain` are Pygments' aliases for
/// its do-nothing lexer, and Sphinx documents use them constantly. Treating
/// them as names to look a grammar up by would make every such block report an
/// unknown language, which is the opposite of what the author asked for.
fn is_unhighlighted(name: &str) -> bool {
    matches!(name, "none" | "text" | "plain")
}

/// A language as written on a code block, before `.. highlight::` inheritance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeLanguage {
    /// No argument was given, so the enclosing `.. highlight::` decides.
    Inherit,
    /// `none` — the author asked for no highlighting at all.
    None,
    /// `default` — Sphinx's "Python, silently falling back to plain text".
    Default,
    /// A concrete language.
    Named(LanguageName),
}

impl CodeLanguage {
    /// Reads a directive argument. An empty argument is [`Self::Inherit`],
    /// which is why this is infallible where [`LanguageName::new`] is not:
    /// "nothing was written" is a meaningful answer for a code block.
    #[must_use]
    pub fn parse(argument: &str) -> Self {
        match argument.trim().to_lowercase().as_str() {
            "" => Self::Inherit,
            name if is_unhighlighted(name) => Self::None,
            "default" => Self::Default,
            // Already trimmed, lowercased and non-empty by the arm above, so
            // the invariant is established here rather than re-checked.
            other => Self::Named(LanguageName(other.to_string())),
        }
    }

    /// Applies `.. highlight::` inheritance, yielding a language that can no
    /// longer be "inherit".
    ///
    /// Total by construction: every [`Self`] maps to a [`ResolvedLanguage`],
    /// so there is no path on which a caller can skip this step and still
    /// have something to highlight with.
    #[must_use]
    pub fn resolve(&self, current: &ResolvedLanguage) -> ResolvedLanguage {
        match self {
            Self::Inherit => current.clone(),
            Self::None => ResolvedLanguage::None,
            Self::Default => ResolvedLanguage::Default,
            Self::Named(name) => ResolvedLanguage::Named(name.clone()),
        }
    }
}

/// A language after inheritance has been applied — what the renderer actually
/// highlights with. Deliberately has no `Inherit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedLanguage {
    None,
    Default,
    Named(LanguageName),
}

impl ResolvedLanguage {
    /// Reads a `.. highlight::` argument or a `highlight_language` config
    /// value.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyLanguageName`] when no language was named. Unlike
    /// [`CodeLanguage::parse`], an empty value has no meaning here: a
    /// `.. highlight::` exists only to name a language, and a config key that
    /// silently meant "none" would turn a typo into a site with no
    /// highlighting anywhere.
    pub fn parse(argument: &str) -> Result<Self, EmptyLanguageName> {
        match argument.trim().to_lowercase().as_str() {
            "" => Err(EmptyLanguageName),
            name if is_unhighlighted(name) => Ok(Self::None),
            "default" => Ok(Self::Default),
            other => LanguageName::new(other).map(Self::Named),
        }
    }

    /// The name to look a grammar up by, or `None` when nothing should be
    /// highlighted.
    ///
    /// [`Self::Default`] answers `python` because that is what Sphinx's
    /// `default` tries first; that it must *not* warn when Python doesn't fit
    /// is a separate question, answered by [`Self::tolerates_failure`].
    #[must_use]
    pub fn grammar_name(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Default => Some("python"),
            Self::Named(name) => Some(name.as_str()),
        }
    }

    /// Whether failing to highlight is expected rather than worth a warning.
    ///
    /// True only for [`Self::Default`], which means "Python if it fits". A
    /// language the author named explicitly failing to parse is always worth
    /// reporting.
    #[must_use]
    pub const fn tolerates_failure(&self) -> bool {
        matches!(self, Self::Default)
    }

    /// The suffix of the `highlight-<lang>` class Sphinx puts on a block's
    /// wrapper, which themes style per language.
    #[must_use]
    pub fn css_suffix(&self) -> &str {
        match self {
            Self::None => "none",
            Self::Default => "default",
            Self::Named(name) => name.as_str(),
        }
    }
}

impl Default for ResolvedLanguage {
    /// Sphinx's own `highlight_language` default.
    fn default() -> Self {
        Self::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_name_new_trims_and_lowercases() {
        // Given
        let raw = "  PyThon  ";

        // When
        let name = LanguageName::new(raw).expect("a non-empty name is valid");

        // Then
        assert_eq!(name.as_str(), "python");
    }

    #[test]
    fn test_language_name_new_rejects_a_whitespace_only_name() {
        // Given
        let raw = "   ";

        // When
        let result = LanguageName::new(raw);

        // Then
        assert_eq!(result, Err(EmptyLanguageName));
    }

    #[test]
    fn test_language_name_normalizes_two_spellings_to_one_value() {
        // Given — the same language written two ways
        let written_one_way = "Python";
        let written_another = "python ";

        // When
        let one = LanguageName::new(written_one_way).unwrap();
        let other = LanguageName::new(written_another).unwrap();

        // Then — they are one key, not two
        assert_eq!(one, other);
    }

    #[test]
    fn test_language_name_keeps_punctuation_that_belongs_to_the_name() {
        // Given — a language whose name is not alphanumeric
        let raw = "C++";

        // When
        let name = LanguageName::new(raw).unwrap();

        // Then
        assert_eq!(name.as_str(), "c++");
    }

    #[test]
    fn test_language_name_deserialization_rejects_an_empty_name() {
        // Given — a serialized name violating the non-empty invariant
        let json = r#""   ""#;

        // When
        let result: Result<LanguageName, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_language_name_deserialization_renormalizes_on_load() {
        // Given — a serialized name that was never normalized
        let json = r#""PYTHON""#;

        // When
        let name: LanguageName = serde_json::from_str(json).unwrap();

        // Then
        assert_eq!(name.as_str(), "python");
    }

    #[test]
    fn test_code_language_parse_reads_an_empty_argument_as_inherit() {
        // Given — `.. code-block::` with no argument
        let argument = "";

        // When
        let language = CodeLanguage::parse(argument);

        // Then
        assert_eq!(language, CodeLanguage::Inherit);
    }

    #[test]
    fn test_code_language_parse_reads_pygments_no_op_lexers_as_none() {
        // Given — Sphinx documents spell "unhighlighted" three ways
        // When / Then — all mean the same thing, and none is a grammar to
        // look up and fail to find
        for spelling in ["none", "text", "plain", "TEXT"] {
            assert_eq!(
                CodeLanguage::parse(spelling),
                CodeLanguage::None,
                "'{spelling}' should mean no highlighting"
            );
        }
    }

    #[test]
    fn test_resolved_language_parse_reads_pygments_no_op_lexers_as_none() {
        // Given / When / Then — the same vocabulary on a `.. highlight::`
        for spelling in ["none", "text", "plain"] {
            assert_eq!(
                ResolvedLanguage::parse(spelling),
                Ok(ResolvedLanguage::None),
                "'{spelling}' should mean no highlighting"
            );
        }
    }

    #[test]
    fn test_code_language_parse_distinguishes_none_from_inherit() {
        // Given — the distinction an `Option<String>` could not express
        let explicit = CodeLanguage::parse("none");
        let unspecified = CodeLanguage::parse("");

        // Then
        assert_eq!(explicit, CodeLanguage::None);
        assert_eq!(unspecified, CodeLanguage::Inherit);
        assert_ne!(explicit, unspecified);
    }

    #[test]
    fn test_code_language_parse_reads_the_special_values_case_insensitively() {
        // Given — Sphinx's two reserved values, shouted
        // When
        let none = CodeLanguage::parse("NONE");
        let default = CodeLanguage::parse("Default");

        // Then
        assert_eq!(none, CodeLanguage::None);
        assert_eq!(default, CodeLanguage::Default);
    }

    #[test]
    fn test_code_language_parse_reads_a_concrete_language() {
        // Given
        let argument = " Rust ";

        // When
        let language = CodeLanguage::parse(argument);

        // Then
        assert_eq!(
            language,
            CodeLanguage::Named(LanguageName::new("rust").unwrap())
        );
    }

    #[test]
    fn test_code_language_resolve_inherits_the_current_language() {
        // Given — a bare code block under `.. highlight:: rust`
        let language = CodeLanguage::Inherit;
        let current = ResolvedLanguage::Named(LanguageName::new("rust").unwrap());

        // When
        let resolved = language.resolve(&current);

        // Then
        assert_eq!(resolved, current);
    }

    #[test]
    fn test_code_language_resolve_lets_an_explicit_language_win() {
        // Given — an explicit language under a different `.. highlight::`
        let language = CodeLanguage::Named(LanguageName::new("python").unwrap());
        let current = ResolvedLanguage::Named(LanguageName::new("rust").unwrap());

        // When
        let resolved = language.resolve(&current);

        // Then
        assert_eq!(
            resolved,
            ResolvedLanguage::Named(LanguageName::new("python").unwrap())
        );
    }

    #[test]
    fn test_code_language_resolve_lets_an_explicit_none_override_inheritance() {
        // Given — `.. code-block:: none` under `.. highlight:: rust`
        let language = CodeLanguage::None;
        let current = ResolvedLanguage::Named(LanguageName::new("rust").unwrap());

        // When
        let resolved = language.resolve(&current);

        // Then — the block is unhighlighted, not Rust
        assert_eq!(resolved, ResolvedLanguage::None);
    }

    #[test]
    fn test_code_language_resolve_is_total_over_every_variant() {
        // Given — every way a language can be written, and one current value
        let current = ResolvedLanguage::Named(LanguageName::new("rust").unwrap());
        let all = [
            CodeLanguage::Inherit,
            CodeLanguage::None,
            CodeLanguage::Default,
            CodeLanguage::Named(LanguageName::new("python").unwrap()),
        ];

        // When / Then — resolving never has to fall back or fail
        for language in all {
            let resolved = language.resolve(&current);
            match language {
                CodeLanguage::Inherit => assert_eq!(resolved, current),
                CodeLanguage::None => assert_eq!(resolved, ResolvedLanguage::None),
                CodeLanguage::Default => assert_eq!(resolved, ResolvedLanguage::Default),
                CodeLanguage::Named(name) => {
                    assert_eq!(resolved, ResolvedLanguage::Named(name));
                }
            }
        }
    }

    #[test]
    fn test_resolved_language_parse_rejects_an_empty_argument() {
        // Given — a `.. highlight::` naming nothing
        let argument = "  ";

        // When
        let result = ResolvedLanguage::parse(argument);

        // Then — unlike a code block, this has no "inherit" meaning to fall to
        assert_eq!(result, Err(EmptyLanguageName));
    }

    #[test]
    fn test_resolved_language_parse_reads_the_special_values() {
        // Given / When
        let none = ResolvedLanguage::parse("none").unwrap();
        let default = ResolvedLanguage::parse("DEFAULT").unwrap();
        let named = ResolvedLanguage::parse("Rust").unwrap();

        // Then
        assert_eq!(none, ResolvedLanguage::None);
        assert_eq!(default, ResolvedLanguage::Default);
        assert_eq!(
            named,
            ResolvedLanguage::Named(LanguageName::new("rust").unwrap())
        );
    }

    #[test]
    fn test_resolved_language_grammar_name_maps_default_to_python() {
        // Given — Sphinx's `default` means "try Python"
        let language = ResolvedLanguage::Default;

        // When / Then
        assert_eq!(language.grammar_name(), Some("python"));
    }

    #[test]
    fn test_resolved_language_grammar_name_is_absent_for_none() {
        // Given
        let language = ResolvedLanguage::None;

        // When / Then — nothing to look up, so nothing to fail either
        assert_eq!(language.grammar_name(), None);
    }

    #[test]
    fn test_resolved_language_tolerates_failure_only_for_default() {
        // Given — the three kinds of resolved language
        // When / Then — only `default` promises Python without insisting on it
        assert!(ResolvedLanguage::Default.tolerates_failure());
        assert!(!ResolvedLanguage::None.tolerates_failure());
        assert!(
            !ResolvedLanguage::Named(LanguageName::new("python").unwrap()).tolerates_failure(),
            "an explicitly named language failing is worth reporting"
        );
    }

    #[test]
    fn test_resolved_language_css_suffix_names_each_variant() {
        // Given / When / Then
        assert_eq!(ResolvedLanguage::None.css_suffix(), "none");
        assert_eq!(ResolvedLanguage::Default.css_suffix(), "default");
        assert_eq!(
            ResolvedLanguage::Named(LanguageName::new("Python").unwrap()).css_suffix(),
            "python"
        );
    }

    #[test]
    fn test_resolved_language_default_matches_sphinx_config_default() {
        // Given — no `highlight_language` set anywhere
        // When
        let language = ResolvedLanguage::default();

        // Then
        assert_eq!(language, ResolvedLanguage::Default);
    }

    #[test]
    fn test_code_language_serialization_roundtrip() {
        // Given
        let language = CodeLanguage::Named(LanguageName::new("python").unwrap());

        // When
        let json = serde_json::to_string(&language).unwrap();
        let restored: CodeLanguage = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(restored, language);
    }

    #[test]
    fn test_resolved_language_serialization_roundtrip() {
        // Given — each variant, since they serialize differently
        for language in [
            ResolvedLanguage::None,
            ResolvedLanguage::Default,
            ResolvedLanguage::Named(LanguageName::new("c++").unwrap()),
        ] {
            // When
            let json = serde_json::to_string(&language).unwrap();
            let restored: ResolvedLanguage = serde_json::from_str(&json).unwrap();

            // Then
            assert_eq!(restored, language);
        }
    }

    #[test]
    fn test_language_name_displays_its_normalized_form() {
        // Given
        let name = LanguageName::new("  RuSt ").unwrap();

        // When
        let shown = name.to_string();

        // Then
        assert_eq!(shown, "rust");
    }
}
