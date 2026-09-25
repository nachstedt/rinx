//! Sphinx's `Display text <target>` syntax, which several unrelated
//! constructs share.
//!
//! It started as a role-only concern — `:ref:`, `:term:` and the
//! domain-object roles all accept it — and lived in `inline/dispatch.rs`
//! accordingly. `.. toctree::` accepts the same form on its entry lines
//! (`Getting started <intro>`), and that parser sits under `directives/`, so
//! the helper is now reached from two sibling trees and belongs flat at the
//! crate root rather than inside either one.

/// Splits content on the optional explicit-title syntax
/// (`Display text <target>`). Returns `None` when there is no explicit title.
///
/// The opening angle bracket is found from the *right*, so a target that
/// itself contains one still splits at the correct place.
pub(crate) fn split_explicit_title(content: &str) -> Option<(String, String)> {
    let angle_start = content.rfind('<')?;
    let angle_end = content[angle_start..].find('>')?;
    let display = content[..angle_start].trim().to_string();
    let target = content[angle_start + 1..angle_start + angle_end]
        .trim()
        .to_string();
    Some((display, target))
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
