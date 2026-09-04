//! Matching a `:glob:` entry's pattern against the project's document names.
//!
//! Sphinx globs with `sphinx.util.matching.patmatch`, whose translation gives
//! `*` and `?` no power to cross a `/` while `**` does. That is exactly
//! `globset`'s `literal_separator(true)`, so the matching is delegated rather
//! than hand-rolled.
//!
//! Two places where `globset` is *wider* than Sphinx are deliberately narrowed
//! here, so a pattern cannot mean one thing to each tool:
//!
//! - `globset` supports `{a,b}` alternation and Sphinx does not, so braces are
//!   escaped to stay literal.
//! - `globset` would let a pattern match the document that wrote it; Sphinx
//!   excludes the owner, and so does [`matching_docnames`].
//!
//! Do not widen either without checking what Sphinx does first.

use globset::{Glob, GlobBuilder};

/// Compiles one toctree glob pattern.
///
/// Returns `None` for a pattern `globset` rejects outright (an unclosed
/// character class, say); the caller reports that as a pattern matching
/// nothing, which is the same outcome the author sees either way.
fn compile(pattern: &str) -> Option<Glob> {
    // Escaping the braces keeps `{a,b}` a literal three-character sequence, as
    // it is in Sphinx, rather than an alternation `globset` would expand.
    let escaped = pattern.replace('{', "[{]").replace('}', "[}]");
    GlobBuilder::new(&escaped)
        .literal_separator(true)
        .build()
        .ok()
}

/// Every docname in `universe` that `pattern` matches, sorted, excluding
/// `owner`.
///
/// Both `universe` and `owner` are extension-less document names, which is
/// what an author's pattern is written against: `api/*` matches `api/client`,
/// not `api/client.rst`.
///
/// The result is sorted because a glob's expansion order is otherwise the
/// arbitrary order documents happened to be indexed in, and that order decides
/// navigation order, page order and section numbers. Sphinx sorts for the same
/// reason.
pub(crate) fn matching_docnames<'a>(
    pattern: &str,
    universe: impl IntoIterator<Item = &'a str>,
    owner: &str,
) -> Vec<String> {
    let Some(glob) = compile(pattern) else {
        return Vec::new();
    };
    let matcher = glob.compile_matcher();

    let mut hits: Vec<String> = universe
        .into_iter()
        .filter(|docname| *docname != owner)
        .filter(|docname| matcher.is_match(docname))
        .map(str::to_string)
        .collect();
    hits.sort_unstable();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIVERSE: [&str; 6] = [
        "index",
        "intro",
        "api/client",
        "api/server",
        "api/deep/internals",
        "guide/setup",
    ];

    fn matches(pattern: &str) -> Vec<String> {
        matching_docnames(pattern, UNIVERSE, "index")
    }

    #[test]
    fn test_star_does_not_cross_a_separator() {
        // Given / When
        let matched = matches("api/*");

        // Then — `api/deep/internals` is one level deeper, so it is excluded.
        assert_eq!(matched, vec!["api/client", "api/server"]);
    }

    #[test]
    fn test_double_star_crosses_separators() {
        // Given / When
        let matched = matches("api/**");

        // Then
        assert_eq!(
            matched,
            vec!["api/client", "api/deep/internals", "api/server"]
        );
    }

    #[test]
    fn test_question_mark_matches_one_character() {
        // Given / When
        let matched = matching_docnames("intr?", UNIVERSE, "index");

        // Then
        assert_eq!(matched, vec!["intro"]);
    }

    #[test]
    fn test_character_class_matches_one_of_its_members() {
        // Given / When
        let matched = matching_docnames("api/[cs]*", UNIVERSE, "index");

        // Then
        assert_eq!(matched, vec!["api/client", "api/server"]);
    }

    #[test]
    fn test_results_are_sorted() {
        // Given — the universe is deliberately not in sorted order.
        let universe = ["b/two", "b/one", "b/three"];

        // When
        let matched = matching_docnames("b/*", universe, "index");

        // Then — expansion order decides nav order and section numbers, so it
        // must not depend on indexing order.
        assert_eq!(matched, vec!["b/one", "b/three", "b/two"]);
    }

    #[test]
    fn test_owner_is_excluded_from_its_own_glob() {
        // Given — a toctree in `index` globbing everything at the top level.
        let universe = ["index", "intro"];

        // When
        let matched = matching_docnames("*", universe, "index");

        // Then — a document must not list itself, which would be a cycle.
        assert_eq!(matched, vec!["intro"]);
    }

    #[test]
    fn test_braces_stay_literal() {
        // Given — Sphinx has no alternation, so this matches a document
        // actually named `a{b,c}` and nothing else.
        let universe = ["ab", "ac", "a{b,c}"];

        // When
        let matched = matching_docnames("a{b,c}", universe, "index");

        // Then
        assert_eq!(matched, vec!["a{b,c}"]);
    }

    #[test]
    fn test_a_pattern_matching_nothing_returns_empty() {
        // Given / When
        let matched = matches("nope/*");

        // Then
        assert!(matched.is_empty());
    }

    #[test]
    fn test_an_uncompilable_pattern_matches_nothing() {
        // Given — an unclosed character class.
        let matched = matches("api/[unclosed");

        // Then — reported to the author as a pattern matching nothing, which
        // is what they observe either way.
        assert!(matched.is_empty());
    }
}
