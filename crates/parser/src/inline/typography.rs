//! Smart-typography substitution for plain prose text: `---` becomes an em
//! dash, `--` an en dash, and `...` an ellipsis — mirroring docutils'
//! `smartquotes` transform (on by default in Sphinx).

/// Replaces `---`, `--`, and `...` runs in `text` with their typographic
/// equivalents (em dash `—`, en dash `–`, ellipsis `…`).
///
/// Longer runs are matched first (`---` before `--`) so a triple hyphen
/// isn't consumed as an en dash plus a leftover hyphen. Only applied to
/// plain prose text — callers are responsible for excluding literal/code
/// content, which should never pass through this function.
#[must_use]
pub(super) fn apply_smart_typography(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '-' && chars.get(i + 1) == Some(&'-') && chars.get(i + 2) == Some(&'-') {
            result.push('\u{2014}'); // em dash
            i += 3;
        } else if chars[i] == '-' && chars.get(i + 1) == Some(&'-') {
            result.push('\u{2013}'); // en dash
            i += 2;
        } else if chars[i] == '.'
            && chars.get(i + 1) == Some(&'.')
            && chars.get(i + 2) == Some(&'.')
        {
            result.push('\u{2026}'); // ellipsis
            i += 3;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_smart_typography_converts_triple_hyphen_to_em_dash() {
        // Given
        let text = "wait---no";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "wait\u{2014}no");
    }

    #[test]
    fn test_apply_smart_typography_converts_double_hyphen_to_en_dash() {
        // Given
        let text = "pages 10--20";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "pages 10\u{2013}20");
    }

    #[test]
    fn test_apply_smart_typography_converts_triple_dot_to_ellipsis() {
        // Given
        let text = "Wait... what";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "Wait\u{2026} what");
    }

    #[test]
    fn test_apply_smart_typography_leaves_single_hyphen_untouched() {
        // Given
        let text = "well-known issue";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "well-known issue");
    }

    #[test]
    fn test_apply_smart_typography_leaves_single_or_double_dot_untouched() {
        // Given
        let text = "e.g. a file, or a.b pair";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "e.g. a file, or a.b pair");
    }

    #[test]
    fn test_apply_smart_typography_handles_multiple_occurrences() {
        // Given
        let text = "a--b---c...d";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "a\u{2013}b\u{2014}c\u{2026}d");
    }

    #[test]
    fn test_apply_smart_typography_does_not_leave_leftover_hyphen_after_em_dash() {
        // Given — exactly three hyphens should not become en dash + hyphen
        let text = "---";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "\u{2014}");
    }

    #[test]
    fn test_apply_smart_typography_handles_four_hyphens_as_em_dash_plus_hyphen() {
        // Given
        let text = "----";

        // When
        let result = apply_smart_typography(text);

        // Then
        assert_eq!(result, "\u{2014}-");
    }

    #[test]
    fn test_apply_smart_typography_empty_string_returns_empty() {
        assert_eq!(apply_smart_typography(""), "");
    }
}
