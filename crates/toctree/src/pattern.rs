//! Sphinx's glob patterns: `sphinx.util.matching._translate_pattern`,
//! ported.
//!
//! Sphinx matches a toctree's `:glob:` entries and a project's
//! `exclude_patterns` and `include_patterns` with one translation of a
//! shell-style pattern into a regular expression, and so does this crate:
//!
//! - `*` matches anything but a `/`, and `**` matches anything at all — in
//!   any position, not only as a whole path component (`**.ipynb_checkpoints`
//!   is a pattern projects write);
//! - `?` matches one character but a `/`;
//! - `[…]` is a character class, `[!…]` its negation, which never matches a
//!   `/`; a `[` with no `]` after it is a literal `[`;
//! - every other character stands for itself — `{a,b}` included, since
//!   Sphinx has no alternation.
//!
//! A pattern matches from the start of a name to its end. The translation is
//! ported rather than delegated to a glob library because the libraries
//! disagree with it exactly where it is unusual — `globset` rejects a `**`
//! inside a component — and a pattern meaning one thing to each tool would
//! include a file Sphinx excludes.

use regex::Regex;

/// One compiled Sphinx glob pattern.
#[derive(Debug, Clone)]
pub struct SphinxPattern {
    /// `None` should the translation ever not compile, which then matches
    /// nothing — the outcome an author sees for any typo in a pattern.
    regex: Option<Regex>,
}

impl SphinxPattern {
    /// Compiles `pattern`. Every pattern compiles, as every pattern does for
    /// Sphinx; one that matches nothing simply matches nothing.
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        // The translation escapes everything it does not produce itself, so
        // it compiles but for a class Python refuses too (`[z-a]`).
        Self {
            regex: Regex::new(&translate(pattern)).ok(),
        }
    }

    /// Whether the whole of `name` matches.
    #[must_use]
    pub fn is_match(&self, name: &str) -> bool {
        self.regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(name))
    }
}

/// The regular expression `pattern` stands for, anchored at both ends.
fn translate(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut translated = String::from("^");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '*' if chars.get(i) == Some(&'*') => {
                i += 1;
                translated.push_str(".*");
            }
            '*' => translated.push_str("[^/]*"),
            '?' => translated.push_str("[^/]"),
            '[' => match class_end(&chars, i) {
                Some(end) => {
                    translated.push_str(&class(&chars[i..end]));
                    i = end + 1;
                }
                None => translated.push_str("\\["),
            },
            c => translated.push_str(&regex::escape(&c.to_string())),
        }
    }
    translated.push('$');
    translated
}

/// Where the class whose `[` precedes `start` closes: Sphinx lets a leading
/// `!`, and then a leading `]`, belong to the class.
fn class_end(chars: &[char], start: usize) -> Option<usize> {
    let mut j = start;
    if chars.get(j) == Some(&'!') {
        j += 1;
    }
    if chars.get(j) == Some(&']') {
        j += 1;
    }
    while j < chars.len() && chars[j] != ']' {
        j += 1;
    }
    (j < chars.len()).then_some(j)
}

/// The class written `members` between its brackets, as a `regex` class.
///
/// Sphinx copies the members into a Python class nearly verbatim; the
/// characters `regex` gives a meaning Python does not — a nested `[`, and
/// the set operators `&&`, `--` and `~~` — are escaped so they stay members.
fn class(members: &[char]) -> String {
    let mut class = String::from("[");
    let mut rest = members;
    match rest.first() {
        // A negated class never matches a separator.
        Some('!') => {
            class.push_str("^/");
            rest = &rest[1..];
        }
        Some('^') => {
            class.push_str("\\^");
            rest = &rest[1..];
        }
        _ => {}
    }
    for (index, &c) in rest.iter().enumerate() {
        match c {
            '\\' => class.push_str("\\\\"),
            '[' | '&' | '~' => {
                class.push('\\');
                class.push(c);
            }
            // A `-` after a `-` would start a difference.
            '-' if index > 0 && rest[index - 1] == '-' => class.push_str("\\-"),
            c => class.push(c),
        }
    }
    class.push(']');
    class
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, name: &str) -> bool {
        SphinxPattern::new(pattern).is_match(name)
    }

    #[test]
    fn test_translate_writes_sphinxs_expression() {
        // When / Then
        assert_eq!(translate("a*?**.b"), "^a[^/]*[^/].*\\.b$");
        assert_eq!(translate("[!x][^y]"), "^[^/x][\\^y]$");
        assert_eq!(translate("[x"), "^\\[x$");
    }

    #[test]
    fn test_class_end_lets_a_leading_bracket_belong_to_the_class() {
        // Given
        let chars: Vec<char> = "[]a]".chars().collect();

        // When / Then
        assert_eq!(class_end(&chars, 1), Some(3));
        assert_eq!(class_end(&"[!]".chars().collect::<Vec<_>>(), 1), None);
    }

    #[test]
    fn test_star_matches_within_one_component() {
        // When / Then
        assert!(matches("api/*", "api/client"));
        assert!(!matches("api/*", "api/deep/internals"));
        assert!(matches("*.rst", "index.rst"));
        assert!(!matches("*.rst", "guide/setup.rst"));
    }

    #[test]
    fn test_double_star_matches_across_components_in_any_position() {
        // When / Then
        assert!(matches("api/**", "api/deep/internals"));
        assert!(matches("**/_sources", "a/b/_sources"));
        assert!(!matches("**/_sources", "_sources"));
        assert!(matches(
            "**.ipynb_checkpoints",
            "notebooks/.ipynb_checkpoints"
        ));
        assert!(matches("*.lproj/**", "en.lproj/a/b"));
    }

    #[test]
    fn test_question_mark_matches_one_character_but_a_separator() {
        // When / Then
        assert!(matches("intr?", "intro"));
        assert!(!matches("a?b", "a/b"));
        assert!(!matches("intr?", "intr"));
    }

    #[test]
    fn test_a_pattern_matches_the_whole_name() {
        // When / Then
        assert!(matches("_build", "_build"));
        assert!(!matches("_build", "_build/html"));
        assert!(!matches("build", "_build"));
    }

    #[test]
    fn test_character_classes_match_their_members() {
        // When / Then
        assert!(matches("api/[cs]*", "api/server"));
        assert!(!matches("api/[cs]*", "api/token"));
        assert!(matches("[a-c]x", "bx"));
        assert!(matches("[]a]", "]"));
        // As in Python: `[!]` is a negated class closed at once, then `a]`.
        assert!(!matches("[!]a]", "b"));
        assert!(matches("[!]a]", "ba]"));
    }

    #[test]
    fn test_a_negated_class_never_matches_a_separator() {
        // When / Then
        assert!(matches("a[!x]b", "ayb"));
        assert!(!matches("a[!x]b", "axb"));
        assert!(!matches("a[!x]b", "a/b"));
    }

    #[test]
    fn test_a_caret_is_a_member_of_a_class() {
        // When / Then
        assert!(matches("[^a]", "^"));
        assert!(matches("[^a]", "a"));
        assert!(!matches("[^a]", "b"));
    }

    #[test]
    fn test_a_bracket_without_its_close_is_literal() {
        // When / Then
        assert!(matches("api/[unclosed", "api/[unclosed"));
        assert!(!matches("api/[unclosed", "api/u"));
    }

    #[test]
    fn test_braces_and_regex_characters_are_literal() {
        // When / Then
        assert!(matches("a{b,c}", "a{b,c}"));
        assert!(!matches("a{b,c}", "ab"));
        assert!(matches("a.b+(c)|$", "a.b+(c)|$"));
        assert!(!matches("a.b", "axb"));
    }

    #[test]
    fn test_characters_regex_treats_as_set_operators_stay_members() {
        // When / Then
        assert!(matches("[&&]", "&"));
        assert!(matches("[~~]", "~"));
        assert!(matches("[+--]", ","));
        assert!(!matches("[z-a]", "z"), "a reversed range matches nothing");
        assert!(matches("[[]", "["));
        assert!(matches("[\\]", "\\"));
    }
}
