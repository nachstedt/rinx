//! Reading a directive's class option: the whitespace-separated list of names
//! every `:class:`-like option holds.
//!
//! Shared rather than one copy per directive because four directives read the
//! same list, and three of them once carried identical private copies.

/// Splits a class option's value into its names.
///
/// Takes the names as written. docutils' own `class_option` would normalize
/// them as [`normalize_class_name`] does; the sphinx-design directives that
/// use this have always passed them through, so that is left to them.
pub(in crate::directives) fn split_classes(value: &str) -> Vec<String> {
    value.split_whitespace().map(str::to_string).collect()
}

/// Normalizes one class name as docutils' `make_id` does: lowercased, every
/// run of characters other than ASCII letters and digits turned into one
/// hyphen, and leading digits and hyphens and trailing hyphens dropped — so
/// `Foo_Bar` becomes `foo-bar`.
///
/// Returns `None` when nothing is left, which docutils refuses as a class
/// name. Unlike docutils, a non-ASCII letter is not first decomposed to its
/// ASCII base (`é` to `e`); it separates, like any other punctuation.
pub(in crate::directives) fn normalize_class_name(name: &str) -> Option<String> {
    let mut normalized = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
    }
    let normalized = normalized
        .trim_start_matches(|character: char| character == '-' || character.is_ascii_digit())
        .trim_end_matches('-');
    (!normalized.is_empty()).then(|| normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_classes_splits_on_any_whitespace() {
        // Given
        let value = "  one\ttwo   three ";

        // When
        let classes = split_classes(value);

        // Then
        assert_eq!(classes, vec!["one", "two", "three"]);
    }

    #[test]
    fn test_split_classes_keeps_names_as_written() {
        // Given / When / Then
        assert_eq!(split_classes("Foo_Bar"), vec!["Foo_Bar"]);
    }

    #[test]
    fn test_normalize_class_name_lowercases_and_hyphenates() {
        // Given / When / Then — the case Sphinx was measured on
        assert_eq!(normalize_class_name("Foo_Bar").as_deref(), Some("foo-bar"));
    }

    #[test]
    fn test_normalize_class_name_collapses_runs_of_punctuation() {
        // Given / When / Then
        assert_eq!(normalize_class_name("a..__b").as_deref(), Some("a-b"));
    }

    #[test]
    fn test_normalize_class_name_drops_leading_digits_and_trailing_hyphens() {
        // Given / When / Then
        assert_eq!(normalize_class_name("-12abc-").as_deref(), Some("abc"));
    }

    #[test]
    fn test_normalize_class_name_refuses_a_name_with_nothing_left() {
        // Given / When / Then
        assert_eq!(normalize_class_name("123"), None);
        assert_eq!(normalize_class_name("--"), None);
    }
}
