//! Sphinx's `Display text <target>` syntax, which several unrelated
//! constructs share.
//!
//! It started as a role-only concern — `:ref:`, `:term:` and the
//! domain-object roles all accept it — and lived in `inline/dispatch.rs`
//! accordingly. `.. toctree::` accepts the same form on its entry lines
//! (`Getting started <intro>`), and that parser sits under `directives/`, so
//! the helper is now reached from two sibling trees and belongs flat at the
//! crate root rather than inside either one.

use std::ops::Range;

/// Splits content on the optional explicit-title syntax
/// (`Display text <target>`). Returns `None` when there is no explicit title.
///
/// The opening angle bracket is found from the *right*, so a target that
/// itself contains one still splits at the correct place.
pub(crate) fn split_explicit_title(content: &str) -> Option<(String, String)> {
    let (open, close) = angle_brackets(content)?;
    let display = content[..open].trim().to_string();
    let target = content[open + 1..close].trim().to_string();
    Some((display, target))
}

/// The byte offsets of the explicit title's `<` and its `>`, when `content`
/// has one: the last `<`, and the first `>` after it.
fn angle_brackets(content: &str) -> Option<(usize, usize)> {
    let open = content.rfind('<')?;
    let close = open + content[open..].find('>')?;
    Some((open, close))
}

/// Where a role's target stands in its `content` — what its backquotes hold
/// — as a range of character indices: inside the angle brackets of an
/// explicit title, else the whole content, without the blanks around it.
///
/// The same split every role reads its target with, so an editor rewriting a
/// target rewrites what the parser read.
#[must_use]
pub fn role_target_range(content: &str) -> Range<usize> {
    let (start, end) =
        angle_brackets(content).map_or((0, content.len()), |(open, close)| (open + 1, close));
    let target = &content[start..end];
    let start = start + (target.len() - target.trim_start().len());
    let end = start + target.trim().len();
    let chars = |bytes: usize| content[..bytes].chars().count();
    chars(start)..chars(end)
}

/// Splits content on the optional explicit-title syntax, returning
/// `(display, target)` — both equal to `content` when there is no explicit
/// title.
///
/// This is the form a role wants, where an absent title simply means the
/// target is also the link text. A caller that needs to *know* whether a title
/// was written (as `.. toctree::` does, to tell an authored title from a
/// looked-up one) wants [`split_explicit_title`] instead.
pub(crate) fn split_display_and_target(content: &str) -> (String, String) {
    split_explicit_title(content).unwrap_or_else(|| (content.to_string(), content.to_string()))
}

/// Splits content on the optional explicit-title syntax, returning
/// `(title, target)` with `title` absent when none was written.
///
/// The form a role wants when its link text for a bare target is not the
/// target itself but something only a later phase can look up — `:ref:`,
/// whose bare form shows the title of the section the label points at.
pub(crate) fn split_optional_title(content: &str) -> (Option<String>, String) {
    match split_explicit_title(content) {
        Some((title, target)) => (Some(title), target),
        None => (None, content.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text `role_target_range` finds in `content`.
    fn target_of(content: &str) -> String {
        let range = role_target_range(content);
        content
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect()
    }

    #[test]
    fn test_role_target_range_is_the_whole_content_without_an_explicit_title() {
        // When / Then
        assert_eq!(target_of("install"), "install");
        assert_eq!(role_target_range(" install "), 1..8);
    }

    #[test]
    fn test_role_target_range_is_inside_the_angle_brackets_of_a_title() {
        // When / Then — counted in characters, past the blanks
        assert_eq!(target_of("Mise en place <install>"), "install");
        assert_eq!(role_target_range("Café < install >"), 7..14);
    }

    #[test]
    fn test_role_target_range_splits_where_the_parser_splits() {
        // Given — a title holding a `<` of its own
        let content = "a < b <target>";

        // When / Then
        assert_eq!(
            Some(target_of(content)),
            split_explicit_title(content).map(|(_, target)| target)
        );
    }

    #[test]
    fn test_split_optional_title_keeps_a_written_title() {
        // Given
        let content = "Getting started <intro>";

        // When
        let split = split_optional_title(content);

        // Then
        assert_eq!(
            split,
            (Some("Getting started".to_string()), "intro".to_string())
        );
    }

    #[test]
    fn test_split_optional_title_reports_no_title_for_a_bare_target() {
        // Given
        let content = "intro";

        // When
        let split = split_optional_title(content);

        // Then
        assert_eq!(split, (None, "intro".to_string()));
    }

    #[test]
    fn test_split_explicit_title_splits_display_from_target() {
        // Given
        let content = "Getting started <intro>";

        // When
        let split = split_explicit_title(content);

        // Then
        assert_eq!(
            split,
            Some(("Getting started".to_string(), "intro".to_string()))
        );
    }

    #[test]
    fn test_split_explicit_title_trims_whitespace_around_both_halves() {
        // Given
        let content = "  Getting started   <  intro  >";

        // When
        let split = split_explicit_title(content);

        // Then
        assert_eq!(
            split,
            Some(("Getting started".to_string(), "intro".to_string()))
        );
    }

    #[test]
    fn test_split_explicit_title_returns_none_without_angle_brackets() {
        // Given
        let content = "intro";

        // When
        let split = split_explicit_title(content);

        // Then
        assert_eq!(split, None);
    }

    #[test]
    fn test_split_explicit_title_returns_none_when_the_bracket_is_unclosed() {
        // Given
        let content = "Getting started <intro";

        // When
        let split = split_explicit_title(content);

        // Then
        assert_eq!(split, None);
    }

    #[test]
    fn test_split_explicit_title_splits_at_the_last_opening_bracket() {
        // Given — a display text that itself contains an angle bracket.
        let content = "Vec<T> deref <std/vec>";

        // When
        let split = split_explicit_title(content);

        // Then
        assert_eq!(
            split,
            Some(("Vec<T> deref".to_string(), "std/vec".to_string()))
        );
    }

    #[test]
    fn test_split_explicit_title_yields_an_empty_display_when_none_was_written() {
        // Given — the anonymous form, which several roles accept.
        let content = "<intro>";

        // When
        let split = split_explicit_title(content);

        // Then
        assert_eq!(split, Some((String::new(), "intro".to_string())));
    }

    #[test]
    fn test_split_display_and_target_repeats_the_content_without_a_title() {
        // Given
        let content = "intro";

        // When
        let (display, target) = split_display_and_target(content);

        // Then
        assert_eq!(display, "intro");
        assert_eq!(target, "intro");
    }

    #[test]
    fn test_split_display_and_target_splits_when_a_title_is_written() {
        // Given
        let content = "Getting started <intro>";

        // When
        let (display, target) = split_display_and_target(content);

        // Then
        assert_eq!(display, "Getting started");
        assert_eq!(target, "intro");
    }
}
